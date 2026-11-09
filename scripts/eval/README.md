# G45 会话级回归评测

外部启动器 + 案例集 + 证据归档。**不是 CI 的一部分**（CI 无模型、无网络），
是「要发版/要大改时手动跑一遍」的回归门。

## 责任边界

照 ZCode `usage/13-automation.md` 的取向：**LFTTDO 只当执行器**。案例集、
环境隔离、资源预算、评分全部在这些脚本里；产品代码不为评测加任何开关
（除了已有的正常入口）。

## 用法

```bash
npm run tauri build            # 评测用发布版二进制，不替你构建
node scripts/eval/run.mjs --dry           # 先看将跑哪些用例
node scripts/eval/run.mjs                 # 跑全部
node scripts/eval/run.mjs --case boot-clean-profile
```

## 证据

每次运行落 `scripts/eval/runs/<时间戳>/`：

- `fingerprint.json`：git SHA（含是否脏工作区）、版本、平台、node 版本、时间
- `profiles/<case>/`：该用例的独立数据目录（HOME/APPDATA 全指到这里）
- `results.json`：每条用例的结论

`runs/` 不进 git（`.gitignore`），但**结论要能对回代码**：指纹是评测报告的一部分。

## 当前范围与已知缺口（如实标注）

这一版钉住的是**启动与隔离**：全新 profile 起得来、二次启动能复用、退出时进程树
被回收。真正的「会话行为断言」（prompt → 期望工具序列 / 产物内容）需要
**headless 入口**（不开窗口跑一轮并把事件流吐到 stdout）—— 那是 G45 的下一步，
这里不假装做到（用 GUI 模拟点击做断言的代价远高于它的信号价值）。

## 加一条用例

在 `cases.json` 里加一项：

```json
{ "name": "mcp-boot", "prompt": "（描述）", "why": "为什么值得回归", "settle_ms": 5000 }
```

`seed` 字段（可选）指向 `scripts/eval/` 下的一个目录，会被复制进该用例的 profile。
