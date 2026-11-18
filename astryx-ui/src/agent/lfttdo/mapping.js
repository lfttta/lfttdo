/**
 * LFTTDO 线上数据 → Astryx 界面数据的纯映射（spec §11）。
 * 全部函数无副作用：输入是 Rust 序列化的 snake_case 形状，输出是 mockData 同构的
 * camelCase 界面形状；不知道的字段原样保留，绝不猜默认值以外的语义。
 */

/** mock 的默认强调色：Rust accent 空串 = 跟随主题预设，映射成 Astryx 原默认。 */
const FALLBACK_ACCENT = '#7c8cff';

/**
 * SettingsView → 界面 settings。
 * view 是「Settings 平铺字段 + api_key_masked + corrupt_backup」（ipc.rs:3139）。
 */
export function settingsFromLfttdo(view) {
  const mode = view?.permission_mode;
  return {
    endpoint: view?.base_url ?? '',
    model: view?.model ?? '',
    persona: view?.system_prompt ?? '',
    contextBudget: view?.budget_tokens ?? 120000,
    fontSize: view?.font_size ?? 15,
    accent: view?.accent || FALLBACK_ACCENT,
    permissionMode:
      mode === 'ask_all' ? 'askAll' : mode === 'trusted' ? 'trusted' : 'rules',
    planMode: Boolean(view?.plan_mode),
    roots: Array.isArray(view?.roots) ? view.roots.map((root) => String(root)) : [],
    hasApiKey: view?.api_key_masked != null,
  };
}

/**
 * 剥掉 SettingsView 的两个 view 层字段，得到可用于 settings_set 的完整 Settings。
 * settings_set 的入参必须带全量字段（serde(default) 会把缺的静默回默认），
 * 所以这里保留原始对象，改字段时展开它（spec §11.2）。
 */
export function rawSettingsOf(view) {
  const { api_key_masked: masked, corrupt_backup: backup, ...settings } = view ?? {};
  return { ...settings };
}

/** 返回换过 model 的完整 Settings 副本；raw 必须来自 rawSettingsOf。 */
export function settingsWithModel(rawSettings, model) {
  return { ...rawSettings, model };
}

/** SessionRow → 界面会话行；updated_at 是毫秒，界面统一 ISO 字符串。 */
export function sessionRowsFromLfttdo(rows) {
  return (Array.isArray(rows) ? rows : []).map((row) => ({
    id: row?.id ?? '',
    title: row?.title || '新对话',
    updatedAt: new Date(row?.updated_at ?? Date.now()).toISOString(),
    messages: [],
  }));
}
