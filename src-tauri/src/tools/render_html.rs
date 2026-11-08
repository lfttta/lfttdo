//! render_html：把模型产出的静态 HTML 存进会话输出目录，交给界面沙箱内嵌渲染。
//!
//! 这是 Gen UI 的第一块砖：模型用它交付图表、对比表、样式卡片这类「看的东西」，
//! 界面在工具卡里用 `sandbox=""` 的 iframe 显示（禁脚本、禁表单、独立源）——
//! 交付物是**静态** HTML/CSS/内联 SVG，不含可执行 JavaScript；需要用户做选择
//! 时走 question 工具。文件按内容哈希命名（同内容不重复占盘），与 output_bound
//! 的落盘同一套习惯。
use crate::agent::types::{Risk, ToolOutput};
use crate::tools::fs_write::{atomic_write, display_path};
use crate::tools::registry::{Tool, ToolCtx, ToolError, ToolMeta};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub const NAME: &str = "render_html";

/// 单次渲染的 HTML 上限：256 KiB 与 write_file 对齐 —— 超大的静态页面没有查看
/// 价值，通常是模型把整个文档错误地塞了进来，明确拒绝而不是静默截断。
pub const MAX_HTML_BYTES: usize = 256 * 1024;

/// 标题只是给人看的说明文本，进不了文件名（文件名一律内容哈希，防注入）。
const MAX_TITLE_CHARS: usize = 80;

pub struct RenderHtmlTool;

#[async_trait]
impl Tool for RenderHtmlTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: NAME.to_string(),
            description: format!(
                "渲染一段**静态** HTML/CSS/SVG 供用户查看：界面会在聊天里内嵌显示\
                 （沙箱禁用 JavaScript 与表单 —— 不要写 <script>，它不会执行；需要\
                 用户做选择时改用 question 工具）。适合内联 SVG 图表、对比表、\
                 样式化卡片。整段 <!DOCTYPE html> 页面或 <body> 片段皆可；外部 \
                 https 资源（图片/字体）可以引用。上限 {MAX_HTML_BYTES} 字节；\
                 同内容重复渲染落同一个文件，不占额外空间。"
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "html": {
                        "type": "string",
                        "description": "要渲染的完整 HTML 文本（静态，禁脚本）"
                    },
                    "title": {
                        "type": "string",
                        "description": "可选：这张卡片的标题，如「Q3 销售对比」"
                    }
                },
                "required": ["html"]
            }),
            risk: Risk::Write,
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
        if ctx.cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        let html = input
            .get("html")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::BadInput("render_html 需要字符串参数 html".into()))?;
        if html.trim().is_empty() {
            return Err(ToolError::BadInput("html 不能为空".into()));
        }
        if html.len() > MAX_HTML_BYTES {
            return Err(ToolError::BadInput(format!(
                "html 超过上限（{} 字节 > {} 字节）；拆成多张卡片或精简样式",
                html.len(),
                MAX_HTML_BYTES
            )));
        }
        let title = input
            .get("title")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| {
                if t.chars().count() > MAX_TITLE_CHARS {
                    t.chars().take(MAX_TITLE_CHARS).collect::<String>()
                } else {
                    t.to_string()
                }
            });

        // 落点：会话输出目录的 rendered/ 子目录 —— 与 images/ 同级、同一套
        // 「会话产物」语义；write_root 缺席时退回第一个 root（同 output_bound）。
        let base = ctx
            .write_root
            .clone()
            .or_else(|| ctx.roots.first().cloned())
            .ok_or_else(|| {
                ToolError::Failed("没有可用的输出目录（write_root 与 roots 都为空）".into())
            })?;
        // 规范化基底：canonicalize 会给出 `\\?\` 前缀的verbatim路径，而
        // `ensure_within_write_root` 拿同样规范化的 write_root 做 `starts_with` ——
        // 两边必须同一形态，否则裸路径会被误判成越界（真机复现过）。
        let base = base.canonicalize().map_err(|e| {
            ToolError::Failed(format!("输出目录无法解析 {}: {e}", display_path(&base)))
        })?;
        let dir = base.join("rendered");
        let mut hasher = DefaultHasher::new();
        html.hash(&mut hasher);
        let path = dir.join(format!("{:016x}.html", hasher.finish()));
        ctx.ensure_within_write_root(&path)?;
        std::fs::create_dir_all(&dir).map_err(|e| {
            ToolError::Failed(format!("创建渲染目录失败 {}: {e}", display_path(&dir)))
        })?;
        atomic_write(&path, html)?;

        let size = html.len();
        let head = match &title {
            Some(t) => format!("已渲染「{t}」"),
            None => "已渲染页面".into(),
        };
        Ok(ToolOutput::text(format!(
            "{head}（{size} 字节，静态 HTML，界面已内嵌显示）→ {}\n\
             文件已存到会话输出目录；要修改请整体重新渲染（同内容覆盖同一文件）。",
            display_path(&path)
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use tokio_util::sync::CancellationToken;

    fn ctx(write_root: Option<PathBuf>) -> ToolCtx {
        ToolCtx {
            http: reqwest::Client::new(),
            cancel: CancellationToken::new(),
            roots: write_root.clone().into_iter().collect(),
            write_root,
            skills_dir: None,
            skills_disabled: vec![],
            subagents: None,
            events: None,
        }
    }

    async fn run(input: Value, write_root: Option<PathBuf>) -> Result<ToolOutput, ToolError> {
        RenderHtmlTool.execute(input, &ctx(write_root)).await
    }

    fn text_of(out: ToolOutput) -> String {
        match out {
            ToolOutput::Text { text } => text,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_tool_meta_is_render_html_write_with_a_pinned_schema() {
        let m = RenderHtmlTool.meta();
        assert_eq!(m.name, "render_html");
        assert_eq!(m.risk, Risk::Write);
        assert!(m.description.contains("沙箱"), "描述要说清沙箱语义");
        let required = m.input_schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 1, "只有 html 必填");
    }

    #[tokio::test]
    async fn execute_writes_the_file_and_names_the_absolute_path() {
        let tmp = tempfile::tempdir().unwrap();
        let out = run(
            json!({"html": "<p>你好</p>", "title": "测试卡片"}),
            Some(tmp.path().to_path_buf()),
        )
        .await
        .unwrap();
        let text = text_of(out);
        assert!(text.contains("已渲染「测试卡片」"), "{text}");
        assert!(text.contains("rendered"), "{text}");
        // 输出里的绝对路径真实存在、内容逐字节一致。
        let base = tmp.path().to_str().unwrap();
        let start = text.find(base).unwrap();
        let end = text[start..].find('\n').unwrap_or(text.len() - start);
        let line = &text[start..start + end];
        let path = Path::new(line.trim());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "<p>你好</p>");
        assert!(
            path.to_str()
                .unwrap()
                .replace('\\', "/")
                .contains("rendered/")
        );
    }

    #[tokio::test]
    async fn identical_html_lands_on_the_same_file() {
        let tmp = tempfile::tempdir().unwrap();
        let input = json!({"html": "<b>same</b>"});
        let a = text_of(
            run(input.clone(), Some(tmp.path().to_path_buf()))
                .await
                .unwrap(),
        );
        let b = text_of(run(input, Some(tmp.path().to_path_buf())).await.unwrap());
        let path_of = |t: &str| {
            let start = t.find(tmp.path().to_str().unwrap()).unwrap();
            let end = t[start..].find('\n').unwrap_or(t.len() - start);
            t[start..start + end].to_string()
        };
        assert_eq!(path_of(&a), path_of(&b), "同内容哈希同名，不重复占盘");
    }

    #[tokio::test]
    async fn oversize_html_is_bad_input_not_a_truncation() {
        let tmp = tempfile::tempdir().unwrap();
        let big = "x".repeat(MAX_HTML_BYTES + 1);
        let err = run(json!({"html": big}), Some(tmp.path().to_path_buf()))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::BadInput(_)), "{err:?}");
    }

    #[tokio::test]
    async fn empty_html_is_bad_input() {
        let tmp = tempfile::tempdir().unwrap();
        let err = run(json!({"html": "   "}), Some(tmp.path().to_path_buf()))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::BadInput(_)), "{err:?}");
    }

    #[tokio::test]
    async fn no_output_dir_is_a_clear_failure() {
        let err = run(json!({"html": "<p>x</p>"}), None).await.unwrap_err();
        assert!(matches!(err, ToolError::Failed(_)), "{err:?}");
    }

    #[tokio::test]
    async fn an_already_cancelled_ctx_returns_cancelled() {
        let tmp = tempfile::tempdir().unwrap();
        let c = ctx(Some(tmp.path().to_path_buf()));
        c.cancel.cancel();
        let err = RenderHtmlTool
            .execute(json!({"html": "<p>x</p>"}), &c)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Cancelled), "{err:?}");
    }
}
