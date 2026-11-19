import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/**
 * LFTTDO Tauri 内核的命令封装（spec §8）。
 * 参数名走 Tauri v2 的 camelCase → snake_case 自动转换；
 * 错误一律是 Rust 侧的 String，errorMessage 统一归一化。
 */

export function isTauri() {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export const client = {
  settingsGet: () => invoke('settings_get'),
  // settings_set 只接受完整 Settings（spec §11.2）：调用方必须从 rawSettings 展开补齐，
  // 不能只发局部键，否则 carry-through 字段会被 Rust 的 serde(default) 静默回默认。
  settingsSet: (settings) => invoke('settings_set', { settings }),
  sessionList: () => invoke('session_list'),
  sessionCreate: () => invoke('session_create'),
  sessionRename: (id, title) => invoke('session_rename', { id, title }),
  sessionDelete: (id) => invoke('session_delete', { id }),
  sessionMessages: (id) => invoke('session_messages', { id }),
  sendMessage: (sessionId, blocks) => invoke('send_message', { sessionId, blocks }),
  interrupt: () => invoke('interrupt'),
  respond: (requestId, decision) => invoke('respond', { requestId, decision }),
  questionRespond: (id, choice) => invoke('question_respond', { id, choice }),
  retryLast: (id) => invoke('session_retry_last', { id }),

  // ── P3：设置 / 面板 / 图片 / 工作区 ──
  settingsSetApiKey: (apiKey) => invoke('settings_set_api_key', { apiKey }),

  skillsList: () => invoke('skills_list'),
  skillsSetEnabled: (folder, enabled) => invoke('skills_set_enabled', { folder, enabled }),
  skillsOpenFolder: () => invoke('skills_open_folder'),

  mcpServers: () => invoke('mcp_servers'),
  mcpSetTrust: (name, trust) => invoke('mcp_set_trust', { name, trust }),
  mcpReconnect: (name) => invoke('mcp_reconnect', { name }),
  mcpDisconnect: (name) => invoke('mcp_disconnect', { name }),
  mcpConfigStatus: () => invoke('mcp_config_status'),
  mcpConfigRead: () => invoke('mcp_config_read'),
  mcpConfigSave: (text) => invoke('mcp_config_save', { text }),

  // sessionId 缺省 = 只看用户作用域（Rust 里 Option）。
  hooksList: (sessionId) => invoke('hooks_list', { sessionId: sessionId ?? null }),
  hooksSetTrusted: (digest, trusted) => invoke('hooks_set_trusted', { digest, trusted }),
  hooksSetWorkspaceEnabled: (enabled) => invoke('hooks_set_workspace_enabled', { enabled }),
  hooksOpenFolder: (sessionId, workspace) =>
    invoke('hooks_open_folder', { sessionId: sessionId ?? null, workspace: workspace ?? false }),

  memoriesList: () => invoke('memories_list'),
  // dueAt 是毫秒或 null（Rust `Option<i64>`）。
  memoryAdd: (kind, body, dueAt) => invoke('memory_add', { kind, body, dueAt }),
  memoryUpdateBody: (id, body) => invoke('memory_update_body', { id, body }),
  memoriesDelete: (id) => invoke('memories_delete', { id }),
  memoriesClear: () => invoke('memories_clear'),
  memoryProfile: () => invoke('memory_profile'),

  /**
   * 图片归一化：字节必须作为**原始 invoke 载荷**整个传出去（旧前端的教训：
   * 包一层 `{ bytes }` 会进 `InvokeBody::Json`，Rust 的 raw body 解析直接报错）。
   */
  normalizeImage: (bytes) => invoke('normalize_image', bytes),

  sessionSearchFiles: (sessionId, query) => invoke('session_search_files', { sessionId, query }),
  sessionOutputView: (id) => invoke('session_output_view', { id }),
  sessionWorkspaceSet: (id, path) => invoke('session_workspace_set', { id, path }),
  sessionExport: (sessionId) => invoke('session_export', { sessionId }),
  pickFolder: () => invoke('pick_folder'),
  revealPath: (sessionId, path) => invoke('reveal_path', { sessionId, path }),
  openUrl: (url) => invoke('open_url', { url }),
};

/**
 * 订阅唯一的 agent 事件通道。payload 是 `{ sessionId, event }`（ipc.rs:63 的
 * AgentPayload 是全仓库唯一 camelCase 的结构体）。
 */
export function listenAgent(handler) {
  return listen('agent', (event) => handler(event.payload));
}

export function errorMessage(error) {
  if (error == null) return '未知错误';
  if (typeof error === 'string') return error;
  if (typeof error === 'object' && typeof error.message === 'string') return error.message;
  return String(error);
}
