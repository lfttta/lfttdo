# LFTTDO Astryx Version Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship an independent `LFTTDO Astryx` Tauri edition that loads the Astryx UI from `astryx-ui/` while leaving the default LFTTDO frontend and Rust core untouched; Phase 1 ends with the new edition running the existing mock agent.

**Architecture:** Add a second Tauri config overlay (`src-tauri/tauri.astryx.conf.json`) and root scripts. The new edition uses its own product name, bundle identifier, data directory and Vite dev server. `astryx-ui/` is a copy of the existing Astryx prototype. No Rust changes in this plan.

**Tech Stack:** Tauri v2, React 19, Vite 8, Astryx 0.7.16, Node 24, Rust 1.99.

**Spec:** `docs/superpowers/specs/2026-11-15-astryx-ui-integration-design.md`

## Global Constraints

- Default edition must stay unchanged: `src-tauri/tauri.conf.json`, `src/`, and all Rust code are read-only in this plan.
- New edition: `productName = "LFTTDO Astryx"`, `identifier = "com.qwa.do.astryx"`.
- Default edition remains `npm run tauri dev` on `http://localhost:1420`.
- New edition uses `npm run tauri:astryx` on `http://localhost:1420`. The port is shared because `src-tauri/src/web_guard.rs` allows dev navigation only on 1420; the two editions cannot run `dev` at the same time, but their builds, identifiers and data directories are independent.
- New edition data directory is `%APPDATA%\com.qwa.do.astryx\`; it must not read or write `%APPDATA%\com.qwa.do\`.
- Rust builds run from the ASCII worktree `D:\aido\lfttdo-astryx`, never from a path containing Chinese characters.
- Commit messages and code comments are Chinese. Do not push to `main`.
- Astryx UI rules: no raw `<div>`/`<span>` layout, no bare hex/px, use Astryx components and tokens. Run `cd astryx-ui && npx astryx doctor --built dist`.
- New dependencies are allowed only inside `astryx-ui/`; the old root frontend keeps its dependency set.
- This plan is Phase 1 only. Do not implement P2–P5 here.

## Review Focus

1. **Default edition still loads the original UI after the overlay exists.** Test: `scripts/astryx-edition-config.check.mjs` asserts the base config still points at `../dist` and port 1420; Task 2 also launches it.
2. **New edition loads the Astryx UI, not a blank/unstyled page.** Test: Task 2's manual launch sends one mock message; Task 3 runs the production build and Astryx doctor.
3. **Theme assets survive the copy.** Test: `scripts/astryx-ui-copy.check.mjs` asserts `astryx-themes.js`, `themes/lfttdo-noir`, and `src/themes/lfttdo-noir` exist; doctor confirms built styles.
4. **Shared dev port.** Test: config check asserts both editions use 1420 and `strictPort`; launch them one at a time. The Rust navigation whitelist only allows 1420, so 5173 would render a blank window.
5. **Data isolation between editions.** Test: Task 2 records `%APPDATA%\com.qwa.do` mtime, launches only the Astryx edition, and asserts the old directory was not touched and the new one was created.

---

## File Structure

```text
astryx-ui/                                  copied Astryx prototype (new)
  package.json                              name + @tauri-apps/api
  vite.config.js                            fixed port 1420
  src/…                                     existing UI source
  astryx-themes.js                          existing generated theme module
  themes/lfttdo-noir/                       existing built theme package
  src/themes/lfttdo-noir/                   existing app theme copy

scripts/astryx-ui-copy.check.mjs            new file/asset assertions
scripts/astryx-edition-config.check.mjs     base/overlay/scripts assertions

src-tauri/tauri.astryx.conf.json            new overlay; default config untouched
package.json                                new Astryx scripts only
```

## Task 1: Copy the Astryx UI and make it build independently

**Files:**
- Create: `astryx-ui/` (copied from `D:\aido\Astryx-界面制作\Astryx测试1`)
- Modify: `astryx-ui/package.json`
- Modify: `astryx-ui/vite.config.js`
- Test: `scripts/astryx-ui-copy.check.mjs`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `astryx-ui/` with `npm run dev` on 1420 and `npm run build` producing `astryx-ui/dist/`; package name `lfttdo-astryx-ui`; `@tauri-apps/api` available for Phase 2.

- [ ] **Step 1: Write the failing copy check**

Create `scripts/astryx-ui-copy.check.mjs`:

```js
import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const root = process.cwd();
const ui = join(root, 'astryx-ui');

test('astryx-ui contains the prototype entry and theme assets', () => {
  for (const path of [
    'index.html',
    'src/App.jsx',
    'astryx-themes.js',
    'src/themes/lfttdo-noir',
    'themes/lfttdo-noir',
  ]) {
    assert.ok(existsSync(join(ui, path)), `missing ${path}`);
  }
});

test('astryx-ui is the independent edition package', () => {
  const pkg = JSON.parse(readFileSync(join(ui, 'package.json'), 'utf8'));
  assert.equal(pkg.name, 'lfttdo-astryx-ui');
  assert.ok(pkg.dependencies['@tauri-apps/api'], 'missing @tauri-apps/api');
});

test('prototype-only agent and artifact files are not copied', () => {
  for (const path of ['.claude', '.mcp.json', 'AGENTS.md', 'ERROR', 'preview.png']) {
    assert.ok(!existsSync(join(ui, path)), `unexpected ${path}`);
  }
});

test('astryx-ui dev server is fixed to 1420', () => {
  const vite = readFileSync(join(ui, 'vite.config.js'), 'utf8');
  assert.match(vite, /port:\s*1420/);
  assert.match(vite, /strictPort:\s*true/);
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `node --test scripts/astryx-ui-copy.check.mjs`

Expected: FAIL with `ENOENT` for `astryx-ui/package.json` (or `missing index.html`).

- [ ] **Step 3: Copy the prototype, excluding build artifacts**

Run from `D:\aido\lfttdo-astryx` in PowerShell:

```powershell
robocopy "D:\aido\Astryx-界面制作\Astryx测试1" "D:\aido\lfttdo-astryx\astryx-ui" /E /XD node_modules dist artifacts .zcode .claude /XF 使用说明.md AGENTS.md ERROR preview.png .mcp.json
if ($LASTEXITCODE -ge 8) { throw "robocopy failed: $LASTEXITCODE" }
```

Expected: robocopy exit code 1–7 (success); `astryx-ui/index.html` and `astryx-ui/src/App.jsx` exist.

- [ ] **Step 4: Set the package name and Tauri API dependency**

In `astryx-ui/package.json`, change `"name"` to `"lfttdo-astryx-ui"` and add to `dependencies`:

```json
"@tauri-apps/api": "^2"
```

Do not change React, Vite, Astryx, or Heroicons versions.

- [ ] **Step 5: Pin the Vite dev server to 1420**

Replace the `defineConfig` body in `astryx-ui/vite.config.js` with:

```js
export default defineConfig({
  plugins: [react()],
  server: {
    port: 1420,
    strictPort: true,
  },
});
```

- [ ] **Step 6: Confirm build artifacts are ignored**

Run:

```bash
git check-ignore -v astryx-ui/node_modules astryx-ui/dist
```

Expected: both paths are ignored. If either is not ignored, add `astryx-ui/node_modules/` and `astryx-ui/dist/` to the root `.gitignore`, then rerun the command.

- [ ] **Step 7: Run the copy check to verify it passes**

Run: `node --test scripts/astryx-ui-copy.check.mjs`

Expected: `# pass 4`, `# fail 0`.

- [ ] **Step 8: Install and build the copied UI**

Run:

```bash
npm --prefix astryx-ui install
npm --prefix astryx-ui run build
```

Expected: `vite v8.x building for production...` then `✓ built in …`; `astryx-ui/dist/index.html` exists.

- [ ] **Step 9: Run the Astryx gate**

Run: `cd astryx-ui && npx astryx doctor --built dist`

Expected: `fail: 0`. Warnings about external fonts or the local CLI path are acceptable.

- [ ] **Step 10: Commit**

```bash
git add astryx-ui scripts/astryx-ui-copy.check.mjs .gitignore
git commit -m "feat: 复制 Astryx 前端为独立版本目录"
```

---

## Task 2: Add the independent Tauri overlay and launch scripts

**Files:**
- Create: `src-tauri/tauri.astryx.conf.json`
- Modify: `package.json`
- Create: `scripts/astryx-edition-config.check.mjs`
- Test: `scripts/astryx-edition-config.check.mjs`

**Interfaces:**
- Consumes: `astryx-ui/` and its `dev`/`build` scripts from Task 1.
- Produces: `npm run tauri:astryx` (dev), `npm run tauri:astryx:build` (build), `npm run dev:astryx`, `npm run build:astryx`, `npm run test:edition`.

- [ ] **Step 1: Write the failing edition config check**

Create `scripts/astryx-edition-config.check.mjs`:

```js
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const readJson = (path) => JSON.parse(readFileSync(path, 'utf8'));

test('default LFTTDO edition keeps its original build settings', () => {
  const base = readJson('src-tauri/tauri.conf.json');
  assert.equal(base.productName, 'LFTTDO');
  assert.equal(base.build.devUrl, 'http://localhost:1420');
  assert.equal(base.build.frontendDist, '../dist');
});

test('Astryx overlay is a separate edition', () => {
  const overlay = readJson('src-tauri/tauri.astryx.conf.json');
  assert.equal(overlay.productName, 'LFTTDO Astryx');
  assert.equal(overlay.identifier, 'com.qwa.do.astryx');
  assert.equal(overlay.build.devUrl, 'http://localhost:1420');
  assert.equal(overlay.build.beforeDevCommand, 'npm run dev:astryx');
  assert.equal(overlay.build.frontendDist, '../astryx-ui/dist');
});

test('Astryx dev URL passes the Rust navigation whitelist', () => {
  const overlay = readJson('src-tauri/tauri.astryx.conf.json');
  const guard = readFileSync('src-tauri/src/web_guard.rs', 'utf8');
  assert.equal(overlay.build.devUrl, 'http://localhost:1420');
  assert.match(guard, /Some\(1420\)/, 'web_guard.rs must allow the Astryx dev port');
});

test('Astryx overlay leaves window creation to Rust', () => {
  const overlay = readJson('src-tauri/tauri.astryx.conf.json');
  assert.equal(
    (overlay.app?.windows ?? []).length,
    0,
    'overlay must not create windows: src-tauri/src/ipc.rs creates label "main" in setup',
  );
});

test('Astryx overlay keeps the default security posture', () => {
  const overlay = readJson('src-tauri/tauri.astryx.conf.json');
  assert.equal(overlay.app, undefined, 'overlay must not add capabilities or windows');
});

test('Astryx UI applies the edition window title at runtime', () => {
  const main = readFileSync('astryx-ui/src/main.jsx', 'utf8');
  assert.match(main, /setTitle\('LFTTDO Astryx'\)/);
  assert.doesNotMatch(main, /setSize\(/);
});

test('root scripts expose both editions', () => {
  const pkg = readJson('package.json');
  assert.equal(pkg.scripts['dev:astryx'], 'npm --prefix astryx-ui run dev');
  assert.equal(pkg.scripts['build:astryx'], 'npm --prefix astryx-ui run build');
  assert.equal(
    pkg.scripts['tauri:astryx'],
    'tauri dev --config src-tauri/tauri.astryx.conf.json',
  );
  assert.equal(
    pkg.scripts['tauri:astryx:build'],
    'tauri build --config src-tauri/tauri.astryx.conf.json',
  );
  assert.equal(
    pkg.scripts['test:edition'],
    'node --test scripts/astryx-ui-copy.check.mjs scripts/astryx-edition-config.check.mjs',
  );
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `node --test scripts/astryx-edition-config.check.mjs`

Expected: FAIL with `ENOENT` for `src-tauri/tauri.astryx.conf.json` (or missing script).

- [ ] **Step 3: Create the overlay config**

Create `src-tauri/tauri.astryx.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "LFTTDO Astryx",
  "identifier": "com.qwa.do.astryx",
  "build": {
    "beforeDevCommand": "npm run dev:astryx",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build:astryx",
    "frontendDist": "../astryx-ui/dist"
  }
}
```

Do not add `app.windows` or inline `app.security.capabilities` to the overlay:

- `src-tauri/src/ipc.rs` creates the only window (`label = "main"`, title `LFTTDO`, 800×600) in `setup`; adding `app.windows` would create a second window.
- Inline capability objects in `app.security.capabilities` are not emitted into the generated ACL; the title change must rely on the existing `core:default` capability. Do not grant window-size permissions just to resize the app.
- `astryx-ui/src/main.jsx` changes the title at runtime with `getCurrentWindow().setTitle('LFTTDO Astryx')`. Window size stays 800×600; the user can maximize.

Do not edit `src-tauri/tauri.conf.json`.

- [ ] **Step 4: Add the root scripts**

In root `package.json`, add:

```json
"dev:astryx": "npm --prefix astryx-ui run dev",
"build:astryx": "npm --prefix astryx-ui run build",
"test:edition": "node --test scripts/astryx-ui-copy.check.mjs scripts/astryx-edition-config.check.mjs",
"tauri:astryx": "tauri dev --config src-tauri/tauri.astryx.conf.json",
"tauri:astryx:build": "tauri build --config src-tauri/tauri.astryx.conf.json"
```

Keep all existing scripts unchanged.

- [ ] **Step 5: Install root dependencies**

Run from the repository root:

```bash
npm install --no-audit --no-fund
```

Expected: `@tauri-apps/cli`, `vitest`, and the existing frontend dependencies are installed. This step is local-only; do not commit `node_modules/`. If `package-lock.json` changes, inspect why before committing; the old frontend dependency set must stay unchanged.

- [ ] **Step 6: Run the config check to verify it passes**

Run: `node --test scripts/astryx-edition-config.check.mjs`

Expected: `# pass 7`, `# fail 0`.

- [ ] **Step 7: Verify the Tauri CLI accepts the overlay**

Run:

```bash
npm run tauri:astryx -- --help
```

Expected: Tauri CLI help prints without a config parse error. If `--config` is rejected in this position, change the two scripts to `tauri --config src-tauri/tauri.astryx.conf.json dev` / `... build`, update the config check, and rerun.

- [ ] **Step 8: Record the default data directory state**

In PowerShell:

```powershell
$old = "$env:APPDATA\com.qwa.do"
if (Test-Path $old) { (Get-Item $old).LastWriteTimeUtc } else { "missing" }
```

Record the output in the task handoff. Do not modify `%APPDATA%\com.qwa.do`.

- [ ] **Step 9: Launch the Astryx edition and verify the window**

Run: `npm run tauri:astryx`

Expected:

- Rust compiles successfully; the first build may take several minutes.
- Window title is `LFTTDO Astryx`.
- The Astryx dark-glass UI renders, not a blank page.
- Sending `你好` produces the existing mock response (this phase does not call a real agent).

Close the window. Then check:

```powershell
Test-Path "$env:APPDATA\com.qwa.do.astryx"
(Get-Item "$env:APPDATA\com.qwa.do").LastWriteTimeUtc
```

Expected: `True`; the old directory's timestamp equals the recorded value. If the new directory was not created, inspect the Tauri log and report it; do not fake the check.

- [ ] **Step 10: Launch the default edition and verify it is unchanged**

Run: `npm run tauri dev`

Expected: the original LFTTDO UI loads on port 1420, with its original title and look. Close it.

- [ ] **Step 11: Commit**

```bash
git add package.json src-tauri/tauri.astryx.conf.json scripts/astryx-edition-config.check.mjs
git commit -m "feat: 增加 LFTTDO Astryx 独立 Tauri 配置"
```

---

## Task 3: Phase 1 acceptance and regression

**Files:**
- Modify: none expected.
- Test: root checks, Astryx build, old frontend regression, Tauri manual smoke.

**Interfaces:**
- Consumes: Task 1's `astryx-ui/`, Task 2's overlay and scripts.
- Produces: a verified Phase 1 branch state suitable for pushing.

- [ ] **Step 1: Run both edition checks together**

Run: `npm run test:edition`

Expected: all tests pass; no file/asset or config assertion fails.

- [ ] **Step 2: Rebuild the Astryx UI and run its gate**

Run:

```bash
npm --prefix astryx-ui run build
(cd astryx-ui && npx astryx doctor --built dist)
```

Expected: build succeeds; doctor reports `fail: 0`.

- [ ] **Step 3: Run the old frontend regression**

Run:

```bash
npx vitest run
npx tsc --noEmit
npm run lint:css
```

Expected: Vitest passes at least the 354-test baseline; `tsc` and Stylelint exit 0. These files were not changed; any failure is a regression to investigate.

- [ ] **Step 4: Run the Rust regression**

Run:

```bash
taskkill /IM lfttdo.exe /F 2>$null
cd src-tauri
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Expected: all pass. No Rust files were changed; this confirms the existing core still builds.

- [ ] **Step 5: Record the two-edition smoke result**

In the handoff, record:

- `npm run tauri:astryx`: window title, visible Astryx UI, mock reply received.
- `npm run tauri dev`: original UI loaded.
- `%APPDATA%\com.qwa.do.astryx` created.
- `%APPDATA%\com.qwa.do` timestamp unchanged.
- Any warnings from the first Rust build.

- [ ] **Step 6: Commit any verification-only changes**

If no files changed, skip this step. If documentation or scripts changed during verification:

```bash
git add <changed files>
git commit -m "test: 验证 LFTTDO Astryx 独立版本"
```

- [ ] **Step 7: Push the branch**

Follow `AGENTS.md`: push via the GitHub Git Data API (do not use a direct `git push`). Verify:

- Remote branch head is the local branch head.
- Every new file is present in the remote tree.
- Remote blob SHA equals local `git rev-parse HEAD:<path>`.

Do not push to `main`.

---

## Follow-up Plans (not in this plan)

Write a separate plan for each phase after Phase 1 is accepted:

- **P2:** `useLfttdoAgent`, event projection, real text streaming, settings/session bootstrap.
- **P3:** tool cards, permission cards, question dialog, stop, retry.
- **P4:** settings write-back, images, workspace, export, file reveal.
- **P5:** skills/MCP/Hooks/memory panels, full tests, packaging.

Each follow-up plan must state the Phase 1 interfaces it consumes and the new interfaces it produces.
