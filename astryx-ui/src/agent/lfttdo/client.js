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
