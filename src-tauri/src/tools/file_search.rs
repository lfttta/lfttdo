//! G43：@文件提及的搜索层。给输入框的 @ 浮层供数：在给定的根目录集合里按
//! 文件名子串找文件，返回绝对路径（浅层优先）。
//!
//! 与 `glob` 工具的分工：glob 是给**模型**按模式精确列文件（`**/*.rs` 语义），
//! 这里是给**人**边打字边模糊找 —— 子串匹配、跨根、限条数、跳过依赖目录。
//! 两套语义硬塞一个函数里，迟早有一边被另一边的剪枝规则误伤。

use std::path::{Path, PathBuf};

/// 递归剪枝：这些目录名永远不进去（依赖树与版本库，几十万文件的深坑；
/// `.lfttdo` 是本应用自己的账本目录，同理）。
const SKIPPED_DIRS: [&str; 4] = [".git", "node_modules", "target", ".lfttdo"];
/// 递归深度上限：@提及找的是「项目里的文件」，8 层足够够到绝大多数源码。
const MAX_DEPTH: usize = 8;
/// 单根扫描的条目数上限（防御：异常的符号链接环 / 巨型扁平目录不至于卡死 IPC）。
const MAX_ENTRIES_PER_ROOT: usize = 4_000;
/// 默认返回条数：浮层一屏能看完的量，调用方可以传自己的值。
pub const DEFAULT_LIMIT: usize = 20;

/// 在 `roots` 里按文件名（或相对路径）子串找文件。query 为空 = 不筛，
/// 直接给浅层的前 `limit` 个 —— 用户刚敲 `@` 还没打字时浮层也得有东西可选。
/// 匹配不区分 ASCII 大小写；只返回**文件**，目录本身不进结果。
pub fn search(roots: &[PathBuf], query: &str, limit: usize) -> Vec<String> {
    let q = query.to_lowercase();
    let limit = limit.max(1);
    let mut hits: Vec<(usize, String)> = Vec::new();
    for root in roots {
        if hits.len() >= limit {
            break;
        }
        if !root.is_dir() {
            continue;
        }
        let mut budget = MAX_ENTRIES_PER_ROOT;
        walk(root, root, 0, &q, &mut hits, &mut budget, limit);
    }
    hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    hits.truncate(limit);
    hits.into_iter().map(|(_, p)| p).collect()
}

fn walk(
    root: &Path,
    dir: &Path,
    depth: usize,
    q: &str,
    hits: &mut Vec<(usize, String)>,
    budget: &mut usize,
    limit: usize,
) {
    if depth > MAX_DEPTH || hits.len() >= limit || *budget == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    // 排序让结果稳定（测试要逐字断言；用户看到的顺序也不该随文件系统心跳变）。
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if hits.len() >= limit || *budget == 0 {
            return;
        }
        *budget -= 1;
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if SKIPPED_DIRS.contains(&name.as_str()) || name.starts_with('.') {
                continue;
            }
            walk(root, &path, depth + 1, q, hits, budget, limit);
        } else {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_lowercase();
            if q.is_empty() || name.to_lowercase().contains(q) || rel.contains(q) {
                hits.push((depth, path.to_string_lossy().into_owned()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(rel: &str) -> PathBuf {
        let p = Path::new(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "x").unwrap();
        p.to_path_buf()
    }

    #[test]
    fn finds_by_name_substring_across_roots_and_prefers_shallow() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        write(&root.join("a_main.rs").to_string_lossy());
        write(&root.join("nested/deep/b_main.rs").to_string_lossy());
        write(&root.join("unrelated.txt").to_string_lossy());
        let other = tempfile::tempdir().unwrap();
        write(&other.path().join("c_main.rs").to_string_lossy());

        let hits = search(
            &[root.to_path_buf(), other.path().to_path_buf()],
            "main",
            10,
        );
        assert_eq!(hits.len(), 3, "两个根都该被搜到：{hits:?}");
        // 两个根的直接文件（同 depth 0）之间的先后由路径字典序决定 —— tempdir
        // 名是随机的，钉死具体先后等于掷骰子（实测翻过车）。只钉**层次**不变量：
        // 嵌套两层深的 b_main 必须排在两个根下直接文件之后。
        assert!(hits.iter().any(|h| h.ends_with("a_main.rs")), "{hits:?}");
        assert!(hits.iter().any(|h| h.ends_with("c_main.rs")), "{hits:?}");
        assert!(hits[2].ends_with("b_main.rs"), "{hits:?}");
    }

    #[test]
    fn skips_dependency_dirs_and_hidden_entries() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        write(&root.join("node_modules/pkg/dep.rs").to_string_lossy());
        write(&root.join(".git/hooks/hook.sh").to_string_lossy());
        write(&root.join("target/debug/out.rs").to_string_lossy());
        write(&root.join("src/real.rs").to_string_lossy());

        let hits = search(&[root.to_path_buf()], "rs", 50);
        assert_eq!(hits.len(), 1, "依赖与隐藏目录整个剪掉：{hits:?}");
        assert!(hits[0].ends_with("real.rs"));
    }

    #[test]
    fn empty_query_lists_the_first_files_and_limit_caps_the_result() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        for i in 0..8 {
            write(&root.join(format!("f{i}.txt")).to_string_lossy());
        }
        let hits = search(&[root.to_path_buf()], "", 3);
        assert_eq!(hits.len(), 3, "空 query 也给前 limit 个：{hits:?}");
        assert!(hits[0].ends_with("f0.txt"), "{hits:?}");
    }

    #[test]
    fn a_missing_root_is_skipped_not_an_error() {
        let hits = search(&[PathBuf::from("Z:/definitely/not/here")], "x", 5);
        assert!(hits.is_empty());
    }
}
