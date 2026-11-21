import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const readJson = (path) => JSON.parse(readFileSync(path, 'utf8'));

test('default LFTTDO build points at the astryx-ui frontend (promoted 2026-11-21)', () => {
  const base = readJson('src-tauri/tauri.conf.json');
  assert.equal(base.productName, 'LFTTDO');
  assert.equal(base.identifier, 'com.qwa.do');
  assert.equal(base.build.devUrl, 'http://localhost:1420');
  assert.equal(base.build.beforeDevCommand, 'npm run dev:astryx');
  assert.equal(base.build.beforeBuildCommand, 'npm run build:astryx');
  assert.equal(base.build.frontendDist, '../astryx-ui/dist');
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

test('default frontend keeps the Rust window title and adds no window sizing', () => {
  const main = readFileSync('astryx-ui/src/main.jsx', 'utf8');
  assert.doesNotMatch(
    main,
    /setTitle\(/,
    'after promotion the title stays the Rust-hardcoded LFTTDO',
  );
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
