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

test('astryx-ui dev server uses the navigation-whitelisted 1420 port', () => {
  const vite = readFileSync(join(ui, 'vite.config.js'), 'utf8');
  assert.match(vite, /port:\s*1420/);
  assert.match(vite, /strictPort:\s*true/);
});
