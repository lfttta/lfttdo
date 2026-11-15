# Astryx 配置测试项目

一个最小可运行的 Astryx 项目，用来验证设计系统是否配置正确，并作为后续做界面的起点。

## 启动

```bash
npm install        # 首次
npm run dev        # 开发预览，默认 http://localhost:5173
npm run build      # 生产构建，产物在 dist/
npm run preview    # 预览构建产物
```

## 目录

| 路径 | 作用 |
|---|---|
| `src/main.jsx` | 入口：引基础 CSS、套 `Theme`、挂载 App |
| `src/App.jsx` | 演示页：Button 各 variant / 尺寸 / 状态 / 输入交互 |
| `astryx-themes.js` | 主题注册（`astryx theme add neutral` 生成，勿手改） |
| `AGENTS.md`、`.claude/CLAUDE.md` | `astryx init` 生成的 AI 助手说明 |
| `preview.png` | 实际渲染截图 |

## 配置要点

入口必须按顺序引入样式并包主题：

```jsx
import '@astryxdesign/core/reset.css';
import '@astryxdesign/core/astryx.css';
import { themes, defaultThemeSlug } from '../astryx-themes';
import { Theme } from '@astryxdesign/core/theme';

<Theme theme={themes[defaultThemeSlug]}><App /></Theme>
```

组件按单入口引入，不要把整包引进来：

```jsx
import { Button } from '@astryxdesign/core/Button';
<Button label="查询" variant="primary" onClick={handleQuery} />
```

排版用 `VStack` / `HStack`，不写 `<div>`；间距先用组件 props（`gap`、`padding`），需要时用 token（`var(--color-*)`、`var(--spacing-*)`）。

## 常用命令

```bash
npx astryx component Button     # 查组件参数与示例
npx astryx component --list     # 列出全部组件
npx astryx docs                 # 文档主题
npx astryx docs tokens          # 颜色/间距/圆角 token
npx astryx template --list      # 整页模板
npx astryx doctor --built dist  # 交付前自检
```

## 已验证

- `npm run build` 成功（Vite 8.3.8）
- `npx astryx doctor --built dist`：0 fail
- 浏览器实测：页面渲染、四种 Button variant、输入查询、异步 Loading/完成态均正常
