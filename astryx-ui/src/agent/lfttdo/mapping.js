/**
 * LFTTDO 线上数据 → Astryx 界面数据的纯映射（spec §11）。
 * 全部函数无副作用：输入是 Rust 序列化的 snake_case 形状，输出是 mockData 同构的
 * camelCase 界面形状；不知道的字段原样保留，绝不猜默认值以外的语义。
 */

/**
 * SettingsView → 界面 settings。
 * view 是「Settings 平铺字段 + api_key_masked + corrupt_backup」（ipc.rs:3139）。
 * accent 原样保留：空串是 Rust 的「跟随主题预设」语义（settings.rs:380），
 * 不能编造成具体颜色——否则任何一次设置写回都会把跟随预设永久写死成固定色。
 */
export function settingsFromLfttdo(view) {
  const mode = view?.permission_mode;
  return {
    endpoint: view?.base_url ?? '',
    model: view?.model ?? '',
    persona: view?.system_prompt ?? '',
    contextBudget: view?.budget_tokens ?? 120000,
    fontSize: view?.font_size ?? 15,
    accent: view?.accent ?? '',
    permissionMode:
      mode === 'ask_all' ? 'ask' : mode === 'trusted' ? 'trusted' : 'rules',
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

// ───────────────────── P3：设置写回与面板视图映射 ─────────────────────

/**
 * 界面 settings → 完整 Settings（settings_set 的入参）。
 * raw 必须来自 rawSettingsOf：未在界面出现的字段（温度、压缩线、shell 表、记忆配置……）
 * 全部原样带回，缺一个就会被 Rust 的 serde(default) 静默回默认。
 */
export function settingsFromMapped(rawSettings, mapped) {
  const mode = mapped?.permissionMode;
  return {
    ...(rawSettings ?? {}),
    base_url: mapped?.endpoint ?? '',
    model: mapped?.model ?? '',
    system_prompt: mapped?.persona ?? '',
    budget_tokens: mapped?.contextBudget ?? 32000,
    font_size: mapped?.fontSize ?? 15,
    accent: mapped?.accent ?? '',
    permission_mode:
      mode === 'ask' ? 'ask_all' : mode === 'trusted' ? 'trusted' : 'rules',
    plan_mode: Boolean(mapped?.planMode),
    roots: Array.isArray(mapped?.roots) ? mapped.roots.map((root) => String(root)) : [],
  };
}

/** SkillView → 面板行；folder 是启用开关的键（不是 name）。 */
export function skillsFromLfttdo(view) {
  return (view?.skills ?? []).map((skill) => ({
    name: skill?.name ?? '',
    description: skill?.description ?? '',
    folder: skill?.folder ?? '',
    argumentHint: skill?.argument_hint ?? '',
    enabled: Boolean(skill?.enabled),
  }));
}

/** McpServerView → 面板行：status ready↔connected、trust auto↔allow 只在这一处翻译。 */
export function mcpServersFromLfttdo(rows) {
  return (Array.isArray(rows) ? rows : []).map((server) => ({
    name: server?.name ?? '',
    // 线上没有 transport 字段：面板的 transport 徽章留空，不编造。
    transport: '',
    status: server?.status === 'ready' ? 'connected' : server?.status ?? 'disconnected',
    tools: server?.tools ?? 0,
    trust: server?.trust === 'auto' ? 'allow' : server?.trust ?? 'ask',
    reason: server?.reason ?? null,
  }));
}

/** HooksView → 面板行：digest 是信任键（已带作用域前缀），scope 是来源。 */
export function hooksFromLfttdo(view) {
  return {
    configPath: view?.config_path ?? '',
    configOk: view?.config_ok ?? true,
    workspaceEnabled: Boolean(view?.workspace_enabled),
    workspace: view?.workspace ?? null,
    hooks: (view?.hooks ?? []).map((hook) => ({
      id: hook?.digest ?? '',
      event: hook?.event ?? '',
      command: hook?.command ?? '',
      matcher: hook?.matcher ?? '',
      args: Array.isArray(hook?.args) ? hook.args : [],
      timeoutMs: hook?.timeout_ms ?? 0,
      trusted: Boolean(hook?.trusted),
      source: hook?.scope ?? 'user',
    })),
  };
}

/** 待办到期日 'YYYY-MM-DD' → UTC 当天末尾的毫秒值（memory_add 的 dueAt）。 */
export function dueDateToMs(value) {
  const text = String(value ?? '').trim();
  if (!/^\d{4}-\d{2}-\d{2}$/.test(text)) return null;
  // Rust 约定前端「按 UTC 换算后传下」（ipc.rs:2996）：带 Z 后缀解析成 UTC，
  // 与 due_date_label（按 UTC 日）同口径，负时区机器上也不会分叉。
  const ms = Date.parse(`${text}T23:59:59.999Z`);
  return Number.isNaN(ms) ? null : ms;
}

function datePart(date) {
  // 统一按 UTC 取日期，与 dueDateToMs 往返一致。
  const year = date.getUTCFullYear();
  const month = String(date.getUTCMonth() + 1).padStart(2, '0');
  const day = String(date.getUTCDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

/** MemoryView.due_at（毫秒）→ 'YYYY-MM-DD'（UTC 口径，与写入侧互逆）。 */
export function dueMsToDate(ms) {
  if (ms == null) return null;
  const date = new Date(Number(ms));
  if (Number.isNaN(date.getTime())) return null;
  return datePart(date);
}

/** MemoryView → 面板行：kind prospective↔todo 只在这一处翻译。 */
export function memoriesFromLfttdo(rows) {
  return (Array.isArray(rows) ? rows : []).map((row) => ({
    id: String(row?.id ?? ''),
    kind: row?.kind === 'prospective' ? 'todo' : row?.kind ?? 'fact',
    body: row?.body ?? '',
    dueAt: dueMsToDate(row?.due_at),
    isDue: Boolean(row?.is_due),
    userEdited: Boolean(row?.user_edited),
    reinforcements: row?.reinforcements ?? 0,
    valid: row?.valid ?? true,
  }));
}

/** 面板 kind → 线上 kind（todo → prospective；Rust 白名单 fact/preference/prospective）。 */
export function memoryKindToWire(kind) {
  return kind === 'todo' ? 'prospective' : kind === 'preference' ? 'preference' : 'fact';
}
