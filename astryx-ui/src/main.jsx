import { createRoot } from 'react-dom/client';
import { Theme } from '@astryxdesign/core/theme';
import { InternationalizationProvider } from '@astryxdesign/core/i18n';
import zhCN from '@astryxdesign/core/locales/zh-CN.generated.js';

// 基础样式：重置 + 组件样式，必须在主题之前引入
import '@astryxdesign/core/reset.css';
import '@astryxdesign/core/astryx.css';

// 主题：由 npx astryx theme add lfttdo-noir 生成（内部引入主题 CSS 和字体）
import { themes, defaultThemeSlug } from '../astryx-themes';

import App from './App';

createRoot(document.getElementById('root')).render(
  <Theme theme={themes[defaultThemeSlug]} mode="dark">
    <InternationalizationProvider locale="zh-CN" messages={{ 'zh-CN': zhCN }}>
      <App />
    </InternationalizationProvider>
  </Theme>
);
