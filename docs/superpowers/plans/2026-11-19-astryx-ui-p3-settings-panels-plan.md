# LFTTDO Astryx P3 Implementation Plan: Settings Persistence, Real Panels, Images, Workspace

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In the `LFTTDO Astryx` edition, make the Settings panel persist through real commands (endpoint, model, API key, appearance, permission mode, plan mode, persona, roots, memories), wire the Skills/MCP/Hooks panels to real commands, and enable images, per-session workspace, export, file reveal and `@` file mentions. Browser mode stays mock.

**Architecture:** Extend the existing adapter layer (`client.js`, `mapping.js`) with the remaining IPC commands and view→panel mappers; remove the P2 toast gates in `App.jsx`; small optional props on `SettingsPanel` (API key callbacks, memory-edit capability, footer note). No Rust changes.

**Tech Stack:** Tauri v2 IPC, React 19, Vitest 5.

**Spec:** `docs/superpowers/specs/2026-11-15-astryx-ui-integration-design.md` §10.6–§10.8, §11, §12 items 4–8

## Global Constraints

- Rust and the default frontend are read-only.
- Wire shapes (observed from `src-tauri/src/ipc.rs` and the native frontend's `src/ipc/commands.ts`):
  - `skills_list` → `{root, skills: [{name, description, folder, argument_hint, enabled}]}`; `skills_set_enabled({folder, enabled})`; `skills_open_folder()`.
  - `mcp_servers` → `[{name, status: connecting|ready|disconnected, tools, trust: auto|ask|block, reason}]`; `mcp_set_trust({name, trust})`; `mcp_reconnect({name}) → tools count`; `mcp_disconnect({name})`; `mcp_config_read` → camelCase `{path, exists, text, parseError, skipped}`; `mcp_config_save({text})`; `mcp_config_status`.
  - `hooks_list({sessionId})` → `{config_path, enabled, config_ok, workspace_enabled, workspace, hooks: [{event, matcher, command, args, timeout_ms, digest, trusted, scope}]}`; `hooks_set_trusted({digest, trusted})`; `hooks_set_workspace_enabled({enabled})`; `hooks_open_folder({sessionId, workspace})`.
  - `memories_list` → `MemoryView[]` (snake_case, `id: number`, `kind: fact|preference|prospective`, `due_at: ms|null`, `is_due`, `user_edited`, `reinforcements`); `memory_add({kind, body, dueAt}) → id` (dueAt in **ms**); `memory_update_body({id, body})` — **body only**; `memories_delete({id})`; `memories_clear()`; `memory_profile() → string`.
  - `normalize_image` takes the **raw ArrayBuffer as the whole invoke payload** (not `{bytes}`) and returns an image `ContentBlock` (`{type:'image', data, mime}`).
  - `session_search_files({sessionId, query}) → string[]` (searches settings.roots + session output dir); `pick_folder() → string|null`; `session_output_view({id}) → {workspace, output_dir}`; `session_workspace_set({id, path})`; `session_export({sessionId}) → path|null`; `reveal_path({sessionId, path})`; `open_url({url})`; `settings_set_api_key({apiKey})` (empty string clears).
- Panel value translations at the App boundary only: MCP trust `auto ↔ allow` (Rust `parse_trust` only knows auto/ask/block; the panel labels `allow` as 本服务器全部允许), MCP status `ready ↔ connected`, memory kind `prospective ↔ todo`, permission mode `ask_all ↔ ask`.
- `settings_set` always submits the full Settings object built from the last raw view (carry-through); text-field edits persist debounced (~600 ms), discrete actions persist immediately.
- The panel must never display a value the backend would silently drop: memory editing in real mode can change body only (`memoryEditing='body'` hides kind/dueAt in the edit form); MCP servers have no transport field (render an empty token).
- Replay timestamps stay projection-time (Rust change, out of scope; noted in P2 report).
- Browser mode stays mock. Commit messages and comments in Chinese.

---

## File Structure

```text
astryx-ui/src/agent/lfttdo/client.js        + panel/image/workspace/export commands
astryx-ui/src/agent/lfttdo/mapping.js       + view→panel mappers, settingsFromMapped, ask_all fix
astryx-ui/src/agent/lfttdo/mapping.test.js  new unit tests
astryx-ui/src/App.jsx                       remove gates; real handlers for panels/settings/images/workspace
astryx-ui/src/components/panels/SettingsPanel.jsx  + onSaveApiKey/onClearApiKey/memoryEditing/footer note
```

## Task 1: Adapter extension and mappers

- [ ] Extend `client.js` with all commands listed above (`normalizeImage` passes the ArrayBuffer through).
- [ ] Extend `mapping.js`: fix `ask_all ↔ ask`; `settingsFromMapped(raw, mapped)` (endpoint/model/persona/budget/font/accent/permission/plan/roots over raw carry-through); mappers `skillsFromLfttdo`, `mcpServersFromLfttdo` (trust `auto→allow`, status `ready→connected`, `transport: ''`), `hooksFromLfttdo` (`id: digest`, `source: scope`), `memoriesFromLfttdo` (`kind prospective→todo`, `dueAt: 'YYYY-MM-DD'|null`), `memoryKindToWire` (todo→prospective), `dueDateToMs`/`dueMsToDate`.
- [ ] Unit tests: mappers round-trip, trust/status/kind translations, settingsFromMapped keeps carry-through fields, due-date conversions.
- [ ] Commit: `feat: 扩展 IPC 适配层与面板数据映射`.

## Task 2: App wiring + SettingsPanel props

- [ ] `SettingsPanel`: optional `onSaveApiKey`/`onClearApiKey` (used instead of `hasApiKey` local echo), optional `memoryEditing='full'|'body'`, optional footer note prop.
- [ ] `App`: remove the panel/export/workspace/image/mentions gates; load memories at boot and panels on open; debounced settings persist; real handlers for memory CRUD, skills toggle, MCP trust/save/reconnect, hooks trust/workspace, workspace choose/clear/reveal via `pick_folder`/`session_workspace_set`/`session_output_view`/`reveal_path`, export via `session_export`, images via `normalize_image`, `@` mentions via `session_search_files(activeId, '')` refreshed on session/workspace change.
- [ ] Build + tests + doctor.
- [ ] Commit: `feat: 设置持久化与真实面板数据`.

## Task 3: Acceptance

- [ ] `npm --prefix astryx-ui run test`, build, `doctor --built dist` 0 fail, `test:edition` 11/11, old frontend regression unchanged.
- [ ] Tauri manual run with the local SSE fixture: settings edit survives restart (settings.json check), API key save/clear (masked feedback), memories add/edit-body/delete/clear, skills/MCP/hooks panels open with real (possibly empty) lists, image attach + send through `normalize_image`, workspace choose/clear with the real folder picker, export (native dialog), `@` file mentions after setting roots, model switch still persists.
- [ ] Push via GitHub Git Data API and verify blob SHAs.
