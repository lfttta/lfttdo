# LFTTDO Astryx P2 Implementation Plan: Real Text and Sessions

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In the `LFTTDO Astryx` edition, replace the mock agent with the real LFTTDO Tauri agent for text and sessions: boot loads real settings/sessions, sending goes through `send_message`, streaming text and tool/permission/question cards come from the `agent` event channel, and session create/rename/delete/switch hit real commands. The browser (non-Tauri) keeps the mock path unchanged.

**Architecture:** New adapter layer inside `astryx-ui/src/agent/lfttdo/` (client + mapping + projection) behind a `useAgent` switch. `App.jsx` gains a boot gate and real session handlers. No Rust changes; `src-tauri` is read-only.

**Tech Stack:** Tauri v2 IPC (`@tauri-apps/api`), React 19, Vitest 5.

**Spec:** `docs/superpowers/specs/2026-11-15-astryx-ui-integration-design.md` §7–§11

## Global Constraints

- Rust (`src-tauri/**`) and the default frontend (`src/`) are read-only. The IPC shapes below are observed from Rust source, not invented.
- Wire shapes (from `src-tauri/src/agent/events.rs`, `src/ipc.rs`, `src/store/db.rs`):
  - Event payload: `{ sessionId, event }` on channel `agent`; `event` is tagged `{"t": "turn_start"|"text_delta"|...}` with snake_case inner keys (`input_tokens`, `stop_reason`, `call_id`, error's `where`).
  - `settings_get` → flattened `Settings` (snake_case: `base_url`, `model`, `system_prompt`, `budget_tokens`, `font_size`, `accent`, `permission_mode` = `rules|ask_all|trusted`, `plan_mode`, `roots`, …) plus `api_key_masked`, `corrupt_backup`.
  - `session_list` → `[{id, title, updated_at(ms)}]`; `session_messages(id)` → `AgentEvent[]` ending with `done` (replay: `user_message` + `turn_start` + `text_delta`/`tool_call` + `tool_result`, turn number resets at each real user message).
  - Tauri v2 converts JS camelCase args to Rust snake_case (`sessionId` → `session_id`).
- Browser mode must remain fully mock: no IPC import-time side effects; `isTauri()` gates everything.
- No raw `<div>`/`<span>`, no bare hex/px; Astryx components/tokens only.
- Never show mock data as real data: in Tauri mode, panels/exports/workspace/images/file-mentions that are not wired yet must be gated behind a toast, not silently use mock content.
- Commit messages and comments in Chinese. Branch `feat/astryx-ui` only.

## Scope Boundary (P2 vs later)

In scope: `settings_get`, `session_list/create/rename/delete/messages`, `send_message`, `interrupt`, `respond`, `question_respond`, `session_retry_last`, `settings_set` (model switching only, full-object carry-through), full event projection for all 15 event types, boot/hydration/error states.

Out of scope (gated with a toast in Tauri mode): settings panel persistence beyond model, images (`normalize_image`), workspace (`session_workspace_set`/`session_output_view`), export (`session_export`), file reveal, `@` file mentions (`session_search_files`), skills/MCP/Hooks/memory panels, API key.

---

## File Structure

```text
astryx-ui/src/agent/lfttdo/client.js       IPC wrappers + isTauri + listenAgent
astryx-ui/src/agent/lfttdo/mapping.js      settings/session row mapping (pure)
astryx-ui/src/agent/lfttdo/projection.js   AgentEvent → messages/usage reducer (pure)
astryx-ui/src/agent/lfttdo/projection.test.js
astryx-ui/src/hooks/useLfttdoAgent.js      real hook (same return API as useMockAgent)
astryx-ui/src/hooks/useAgent.js            isTauri() switch between mock and real
astryx-ui/src/App.jsx                      boot gate + real session/model handlers
astryx-ui/package.json                     + vitest, "test": "vitest run"
```

## Task 1: Adapter layer (client, mapping, projection)

**Files:** Create `client.js`, `mapping.js`, `projection.js`, `projection.test.js`; modify `astryx-ui/package.json`.

**Interfaces:**
- Consumes: Tauri commands and `agent` events as listed above.
- Produces: `client` command functions; `settingsFromLfttdo(view)`, `settingsToLfttdo(mapped, rawView, patch)`; `emptyProjection()`, `projectEvent(state, event, { live, model })` returning `{ messages, usage, streaming, toasts, fatalError }`; tool blocks carry optional `id` (`call_id`).

- [ ] Write unit tests first (`projection.test.js`): text stream (`turn_start`+`text_delta`+`done` builds one assistant message), tool pair (`tool_call` running → `tool_result` complete by `call_id`, replay `ms:0` keeps duration 0), replay (`user_message` + two `turn_start` → separate assistant bubbles; `done` ends replay, no streaming), permission pair (`permission_request` card pending; `permission_resolved` allow once/session/deny mapping; no self-resolution), question (`question` block pending; no answer event), usage (`usage` event → contextTokens/budget), error (`fatal` → fatalError; non-fatal → toast), live turn_start closes previous streaming bubble.
- [ ] Implement `client.js` / `mapping.js` / `projection.js` until tests pass: `npm --prefix astryx-ui run test`.
- [ ] Commit: `feat: 增加 LFTTDO IPC 适配层与事件投影`.

## Task 2: useLfttdoAgent hook and App wiring

**Files:** Create `src/hooks/useLfttdoAgent.js`, `src/hooks/useAgent.js`; modify `src/App.jsx`.

**Interfaces:**
- Produces hook return `{ send, stop, isStreaming, streamingSessionId, respondPermission, answerQuestion, retryLast }` (mock-compatible); App uses `client.session*` directly for session CRUD and `settingsToLfttdo` for model switching.

Key behaviors (spec §7, §10):
- Boot: parallel `settings_get` + `session_list`; empty list → `session_create`; loading/error/retry states; raw SettingsView kept for carry-through.
- Hydration: on first activation of a session call `session_messages`, project with `live:false`; sessions already streaming skip replay.
- Events: one `listen("agent")` subscription; dispatch by `payload.sessionId` to any session; unknown id → refresh `session_list`; `usage` events update global usage; projection toasts → `showToast`.
- Send: optimistic user message; `send_message` reject → remove it + toast; retry re-sends returned blocks without re-appending.
- Permission: `respond(requestId, decision)` with `{allow,scope}`/`{allow,reason}`; card state changes only on `permission_resolved`.
- Question: `question_respond` true → mark answer + local record; false → toast + dismiss.
- Stop: `interrupt()`; local state changes only on `done{stop_reason:"interrupted"}`.
- Model switch: persist full Settings with only `model` patched; failure → toast + revert.
- Tauri mode gates: panels open → toast “该面板将在后续阶段接入真实数据”; images/workspace/export/file-mentions → toast; `models` list = current model + static suggestions.

- [ ] Implement; run `npm --prefix astryx-ui run test` and `npm --prefix astryx-ui run build`.
- [ ] Commit: `feat: 接入真实 agent 文本流与会话管理`.

## Task 3: Acceptance and regression

- [ ] `npm run test:edition` (11/11), `npx vitest run` + `npx tsc --noEmit` + `npm run lint:css` (old frontend unchanged), `npm --prefix astryx-ui run test`, build + `astryx doctor --built dist` (`fail: 0`).
- [ ] Rust regression untouched: `cargo test --all-targets` etc. only if any Rust file changed (none expected).
- [ ] Tauri manual run (`npm run tauri:astryx`): real settings load, real session list, send a message → streamed reply, switch session → history replay, stop button interrupts, model switch survives restart. Record results.
- [ ] Push branch via GitHub Git Data API per AGENTS.md and verify blob SHAs.
