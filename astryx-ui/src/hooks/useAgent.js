import { useMockAgent } from './useMockAgent';
import { useLfttdoAgent } from './useLfttdoAgent';
import { isTauri } from '../agent/lfttdo/client.js';

/**
 * 入口选择（spec §12.1）：Tauri 里走真实 LFTTDO 内核，浏览器里保持 mock。
 * isTauri() 在一次生命周期内不会变，所以同一挂载始终走同一分支，
 * 不违反 hook 规则；两个钩子共享同一套返回 API（契约 §7.13）。
 */
export function useAgent(args) {
  if (isTauri()) return useLfttdoAgent(args);
  return useMockAgent(args);
}

export default useAgent;
