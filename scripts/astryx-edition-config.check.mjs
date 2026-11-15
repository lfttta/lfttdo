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
  assert.equal(overlay.build.devUrl, 'http://localhost:5173');
  assert.equal(overlay.build.beforeDevCommand, 'npm run dev:astryx');
  assert.equal(overlay.build.frontendDist, '../astryx-ui/dist');
});

test('Astryx overlay leaves window creation to Rust', () => {
  const overlay = readJson('src-tauri/tauri.astryx.conf.json');
  assert.equal(
    (overlay.app?.windows ?? []).length,
    0,
    'overlay must not create windows: src-tauri/src/ipc.rs creates label "main" in setup',
  );
});

test('Astryx overlay grants the window title permission', () => {
  const overlay = readJson('src-tauri/tauri.astryx.conf.json');
  const capabilities = overlay.app?.security?.capabilities ?? [];
  assert.ok(capabilities.includes('default'), 'missing default capability');
  const titleCapability = capabilities.find(
    (capability) => typeof capability === 'object' && capability.identifier === 'astryx-window-title',
  );
  assert.ok(titleCapability, 'missing astryx-window-title capability');
  assert.deepEqual(titleCapability.windows, ['main']);
  assert.ok(titleCapability.permissions.includes('core:window:allow-set-title'));
});

test('Astryx UI applies the edition window title at runtime', () => {
  const main = readFileSync('astryx-ui/src/main.jsx', 'utf8');
  assert.match(main, /setTitle\('LFTTDO Astryx'\)/);
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
