import { describe, expect, it } from 'vitest';

import {
  dueDateToMs,
  dueMsToDate,
  hooksFromLfttdo,
  mcpServersFromLfttdo,
  memoriesFromLfttdo,
  memoryKindToWire,
  rawSettingsOf,
  sessionRowsFromLfttdo,
  settingsFromLfttdo,
  settingsFromMapped,
  settingsWithModel,
  skillsFromLfttdo,
} from './mapping';

describe('settings 映射', () => {
  it('settings_get → 界面 settings：ask_all 映射成 ask（面板的三档值）', () => {
    const mapped = settingsFromLfttdo({
      base_url: 'http://127.0.0.1:11434/v1',
      model: 'qwen-plus',
      system_prompt: '中文',
      budget_tokens: 32000,
      font_size: 16,
      accent: '',
      permission_mode: 'ask_all',
      plan_mode: true,
      roots: ['D:\\work'],
      api_key_masked: 'sk-***',
    });
    expect(mapped).toEqual({
      endpoint: 'http://127.0.0.1:11434/v1',
      model: 'qwen-plus',
      persona: '中文',
      contextBudget: 32000,
      fontSize: 16,
      accent: '#7c8cff',
      permissionMode: 'ask',
      planMode: true,
      roots: ['D:\\work'],
      hasApiKey: true,
    });
  });

  it('settingsFromMapped 保留 raw 的 carry-through 字段，只覆盖界面出现的键', () => {
    const raw = {
      base_url: 'old',
      model: 'old-model',
      temperature: 0.4,
      memory_enabled: false,
      shells: [{ id: 'pwsh' }],
      default_shell: 'pwsh',
    };
    const wire = settingsFromMapped(raw, {
      endpoint: 'http://new/v1',
      model: 'new-model',
      persona: '人设',
      contextBudget: 64000,
      fontSize: 17,
      accent: '#22d3ee',
      permissionMode: 'ask',
      planMode: true,
      roots: ['D:\\a'],
    });
    expect(wire).toEqual({
      base_url: 'http://new/v1',
      model: 'new-model',
      temperature: 0.4,
      memory_enabled: false,
      shells: [{ id: 'pwsh' }],
      default_shell: 'pwsh',
      system_prompt: '人设',
      budget_tokens: 64000,
      font_size: 17,
      accent: '#22d3ee',
      permission_mode: 'ask_all',
      plan_mode: true,
      roots: ['D:\\a'],
    });
  });

  it('settingsWithModel 只换 model；rawSettingsOf 剥掉 view 层字段', () => {
    const view = { model: 'a', base_url: 'u', api_key_masked: 'sk-***', corrupt_backup: null };
    expect(settingsWithModel(rawSettingsOf(view), 'b')).toEqual({ model: 'b', base_url: 'u' });
  });
});

describe('面板视图映射', () => {
  it('skills：folder/argument_hint 转面板形状', () => {
    const rows = skillsFromLfttdo({
      root: 'C:\\skills',
      skills: [
        { name: 'commit', description: '提交', folder: 'commit-skill', argument_hint: '[范围]', enabled: true },
        { name: 'review', description: '审查', folder: 'review-skill', argument_hint: null, enabled: false },
      ],
    });
    expect(rows).toEqual([
      { name: 'commit', description: '提交', folder: 'commit-skill', argumentHint: '[范围]', enabled: true },
      { name: 'review', description: '审查', folder: 'review-skill', argumentHint: '', enabled: false },
    ]);
  });

  it('mcp：ready→connected、auto→allow，transport 留空不编造', () => {
    const rows = mcpServersFromLfttdo([
      { name: 'a', status: 'ready', tools: 3, trust: 'auto', reason: null },
      { name: 'b', status: 'connecting', tools: 0, trust: 'ask', reason: null },
      { name: 'c', status: 'disconnected', tools: 0, trust: 'block', reason: '超时' },
    ]);
    expect(rows).toEqual([
      { name: 'a', transport: '', status: 'connected', tools: 3, trust: 'allow', reason: null },
      { name: 'b', transport: '', status: 'connecting', tools: 0, trust: 'ask', reason: null },
      { name: 'c', transport: '', status: 'disconnected', tools: 0, trust: 'block', reason: '超时' },
    ]);
  });

  it('hooks：digest 变成信任键 id，scope 变 source', () => {
    const view = hooksFromLfttdo({
      config_path: 'C:\\data\\hooks.json',
      enabled: true,
      config_ok: false,
      workspace_enabled: true,
      workspace: 'D:\\ws',
      hooks: [
        {
          event: 'PreToolUse',
          matcher: 'fs.write',
          command: 'node guard.mjs',
          args: ['--strict'],
          timeout_ms: 5000,
          digest: 'u:abc',
          trusted: true,
          scope: 'user',
        },
      ],
    });
    expect(view).toEqual({
      configPath: 'C:\\data\\hooks.json',
      configOk: false,
      workspaceEnabled: true,
      workspace: 'D:\\ws',
      hooks: [
        {
          id: 'u:abc',
          event: 'PreToolUse',
          command: 'node guard.mjs',
          matcher: 'fs.write',
          args: ['--strict'],
          timeoutMs: 5000,
          trusted: true,
          source: 'user',
        },
      ],
    });
  });

  it('memories：prospective→todo、due_at 毫秒→YYYY-MM-DD', () => {
    const rows = memoriesFromLfttdo([
      {
        id: 7,
        kind: 'prospective',
        body: '交周报',
        due_at: dueDateToMs('2026-11-20'),
        is_due: false,
        user_edited: true,
        reinforcements: 2,
        valid: true,
      },
      { id: 8, kind: 'preference', body: '中文', due_at: null, is_due: false, user_edited: false, reinforcements: 0, valid: true },
    ]);
    expect(rows[0]).toMatchObject({ id: '7', kind: 'todo', body: '交周报', dueAt: '2026-11-20', userEdited: true });
    expect(rows[1]).toMatchObject({ id: '8', kind: 'preference', dueAt: null });
  });

  it('memoryKindToWire 把 todo 送回 prospective', () => {
    expect(memoryKindToWire('todo')).toBe('prospective');
    expect(memoryKindToWire('fact')).toBe('fact');
    expect(memoryKindToWire('preference')).toBe('preference');
  });

  it('到期日往返：dueDateToMs → dueMsToDate 同一天', () => {
    expect(dueMsToDate(dueDateToMs('2026-11-20'))).toBe('2026-11-20');
    expect(dueDateToMs('不是日期')).toBeNull();
    expect(dueDateToMs('')).toBeNull();
    expect(dueMsToDate(null)).toBeNull();
  });

  it('sessionRows：毫秒时间戳转 ISO，空标题回落「新对话」', () => {
    const rows = sessionRowsFromLfttdo([
      { id: 's1', title: '', updated_at: 1_700_000_000_000 },
    ]);
    expect(rows[0]).toEqual({
      id: 's1',
      title: '新对话',
      updatedAt: new Date(1_700_000_000_000).toISOString(),
      messages: [],
    });
  });
});
