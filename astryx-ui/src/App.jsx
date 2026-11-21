import { useEffect, useMemo, useRef, useState } from 'react';
import { AppShell } from '@astryxdesign/core/AppShell';
import { Button } from '@astryxdesign/core/Button';
import { VStack } from '@astryxdesign/core/VStack';
import { ChatLayout } from '@astryxdesign/core/Chat';
import { Spinner } from '@astryxdesign/core/Spinner';
import { Text } from '@astryxdesign/core/Text';
import { useToast } from '@astryxdesign/core/Toast';
import { defineTheme, Theme } from '@astryxdesign/core/theme';

import { themes, defaultThemeSlug } from '../astryx-themes';
import {
  initialSessions,
  initialSettings,
  memories as initialMemories,
  skills as initialSkills,
  mcpServers as initialMcpServers,
  hooks as initialHooks,
  fileMentions,
  models,
  newSession,
} from './data/mockData';
import { client, errorMessage, isTauri } from './agent/lfttdo/client';
import {
  hooksFromLfttdo,
  mcpServersFromLfttdo,
  memoriesFromLfttdo,
  memoryKindToWire,
  dueDateToMs,
  rawSettingsOf,
  sessionRowsFromLfttdo,
  settingsFromLfttdo,
  settingsFromMapped,
  skillsFromLfttdo,
} from './agent/lfttdo/mapping';
import { useAgent } from './hooks/useAgent';
import { SessionSidebar } from './components/SessionSidebar';
import { WorkspaceBar } from './components/WorkspaceBar';
import { ChatPane } from './components/ChatPane';
import { Composer } from './components/Composer';
import { StatusBar } from './components/StatusBar';
import { QuestionDialog } from './components/QuestionDialog';
import SettingsPanel from './components/panels/SettingsPanel';
import SkillsPanel from './components/panels/SkillsPanel';
import McpPanel from './components/panels/McpPanel';
import HooksPanel from './components/panels/HooksPanel';

const ACCENT_VALUES = new Set(['#7c8cff', '#22d3ee', '#f472b6', '#34d399']);

const DEFAULT_PERSONA = '你是 LFTTDO，一个严谨、简洁的中文 Agent 助手。先确认目标，再给出可执行的步骤。';

const CHOSEN_WORKSPACE = 'D:\\Projects\\demo\\agent-ui';

function makeInitialMcpJson(servers) {
  const mcpServers = {};
  (servers ?? []).forEach((server) => {
    const base = {
      transport: server.transport,
      trust: server.trust,
    };
    if (server.name === 'filesystem') {
      mcpServers[server.name] = {
        ...base,
        command: 'npx',
        args: [
          '-y',
          '@modelcontextprotocol/server-filesystem',
          initialSettings.roots[0] ?? '.',
        ],
      };
      return;
    }
    if (server.name === 'github') {
      mcpServers[server.name] = {
        ...base,
        url: 'https://api.githubcopilot.com/mcp/',
      };
      return;
    }
    if (server.name === 'postgres') {
      mcpServers[server.name] = {
        ...base,
        command: 'npx',
        args: ['-y', '@modelcontextprotocol/server-postgres'],
      };
      return;
    }
    mcpServers[server.name] = {
      ...base,
      command: 'npx',
      args: [`-y`, `@modelcontextprotocol/server-${server.name}`],
    };
  });
  return JSON.stringify({ mcpServers }, null, 2);
}

function validateMcpConfig(text) {
  try {
    const parsed = JSON.parse(text);
    const hasServers =
      parsed &&
      typeof parsed === 'object' &&
      !Array.isArray(parsed) &&
      parsed.mcpServers &&
      typeof parsed.mcpServers === 'object' &&
      !Array.isArray(parsed.mcpServers);
    if (!hasServers) {
      return {
        status: 'invalid',
        message: 'JSON 解析失败：缺少 mcpServers 对象',
      };
    }
    return { status: 'valid', message: '配置格式正确' };
  } catch (error) {
    return {
      status: 'invalid',
      message: `JSON 解析失败：${error.message}`,
    };
  }
}

function decisionLabel(decision) {
  if (decision === 'deny') return '已拒绝';
  if (decision === 'once') return '已放行（仅本次）';
  if (decision === 'session') return '已放行（本会话内）';
  return '待处理';
}

/** 设计 5.5：删除当前会话后切到最近更新的一条，而不是数组顺序里的第一条。 */
function mostRecentSession(list) {
  return [...(list ?? [])].sort(
    (a, b) => (Date.parse(b.updatedAt) || 0) - (Date.parse(a.updatedAt) || 0)
  )[0];
}

function serializeSessionMarkdown(session) {
  const lines = [`# ${session.title}`, ''];

  (session.messages ?? []).forEach((message) => {
    lines.push(`## ${message.role === 'user' ? '用户' : '助手'}`, '');

    (message.blocks ?? []).forEach((block) => {
      if (!block) return;

      switch (block.type) {
        case 'text':
          lines.push(block.text || '', '');
          break;
        case 'code':
          lines.push(
            `\`\`\`${block.language || 'text'}`,
            block.code || '',
            '```',
            ''
          );
          break;
        case 'tool':
          lines.push(
            `- 工具：\`${block.name}\` → \`${block.target}\``,
            `  - 状态：${block.status}`
          );
          if (block.duration) {
            lines.push(`  - 耗时：${block.duration}ms`);
          }
          if (block.result) {
            lines.push('', '```text', block.result, '```');
          }
          if (block.errorMessage) {
            lines.push('', `错误：${block.errorMessage}`);
          }
          lines.push('');
          break;
        case 'permission':
          lines.push(
            `- 权限请求：\`${block.tool}\`（${block.risk}）`,
            `  - 结果：${decisionLabel(block.decision)}`,
            ''
          );
          break;
        case 'question':
          lines.push(
            `- 提问：${block.question}`,
            `  - 回答：${block.answer ?? '未回答'}`,
            ''
          );
          break;
        case 'image':
          lines.push(`- 图片：${block.alt || '附件'}`, '');
          break;
        case 'file':
          lines.push(`- 文件：\`${block.path}\`（${block.name}）`, '');
          break;
        default:
          break;
      }
    });
  });

  return lines.join('\n');
}

function downloadBlob(filename, blob) {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  anchor.rel = 'noopener';
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  // 让浏览器先开始下载，再释放 object URL。
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

export default function App() {
  const [sessions, setSessions] = useState(() =>
    initialSessions.map((session) => ({
      ...session,
      messages: session.messages.map((message) =>
        message.role === 'assistant' && !message.model
          ? { ...message, model: initialSettings.model }
          : message
      ),
    }))
  );
  const [activeId, setActiveId] = useState(initialSessions[0]?.id ?? '');
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [openPanel, setOpenPanel] = useState(null);
  const [settings, setSettings] = useState(initialSettings);
  const [memories, setMemories] = useState(initialMemories);
  const [skills, setSkills] = useState(initialSkills);
  const [mcpServers, setMcpServers] = useState(initialMcpServers);
  const [hooks, setHooks] = useState(initialHooks);
  const [workspacePath, setWorkspacePath] = useState(
    initialSettings.roots[0] ?? null
  );
  const [drafts, setDrafts] = useState({});
  const [pendingImages, setPendingImages] = useState([]);
  const [usage, setUsage] = useState({
    inputTokens: 0,
    outputTokens: 0,
    contextTokens: 0,
  });
  const [mcpJson, setMcpJson] = useState(() =>
    makeInitialMcpJson(initialMcpServers)
  );
  const [mcpValidation, setMcpValidation] = useState({
    status: 'idle',
    message: '',
  });
  const [workspaceHooksEnabled, setWorkspaceHooksEnabled] = useState(false);
  const [dismissedQuestionId, setDismissedQuestionId] = useState(null);

  // Tauri 里走真实内核：启动要先加载 settings/sessions，加载失败给出可重试错误。
  const real = isTauri();
  const rawSettingsRef = useRef(null);
  const [boot, setBoot] = useState(real ? { status: 'loading' } : { status: 'ready' });
  // 真实模式的 @ 文件提及列表（session_search_files 的结果，跟会话/工作文件夹走）。
  const [realFileMentions, setRealFileMentions] = useState([]);
  // 设置文本框逐键触发 onSettingChange：落盘做防抖，离散动作仍即时保存。
  const persistTimerRef = useRef(null);
  // 防抖回调拿到的是旧闭包：settings 的真相放 ref，落盘时现读。
  const settingsRef = useRef(settings);
  settingsRef.current = settings;

  const showToast = useToast();
  const agent = useAgent({ sessions, settings, setSessions, setUsage, showToast });

  const activeSession =
    sessions.find((session) => session.id === activeId) ??
    sessions[0] ??
    null;
  const composerValue = drafts[activeId] ?? '';
  const activeStreaming = agent.streamingSessionId === activeId;

  const pendingQuestion = (() => {
    if (!activeSession) return null;
    for (let m = activeSession.messages.length - 1; m >= 0; m -= 1) {
      const message = activeSession.messages[m];
      if (message.stopped) continue;
      const blocks = Array.isArray(message.blocks) ? message.blocks : [];
      for (let b = blocks.length - 1; b >= 0; b -= 1) {
        const block = blocks[b];
        if (block && block.type === 'question' && block.answer == null) {
          return block;
        }
      }
    }
    return null;
  })();
  const questionDialogOpen =
    Boolean(pendingQuestion) && pendingQuestion.id !== dismissedQuestionId;

  // 状态栏分母：真实模式由 usage 事件携带 budget（spec §11.3），mock/未加载时用设置兜底。
  const contextBudget = usage?.contextBudget ?? settings.contextBudget;

  // 真实模式的模型下拉 = 当前模型 + 静态建议列表，不伪造可用性（spec §10.7）。
  const modelOptions = real
    ? Array.from(new Set([settings.model, ...models].filter(Boolean)))
    : models;

  async function bootReal() {
    setBoot({ status: 'loading' });
    try {
      const [view, rows, memoryRows, skillView, mcpRows, mcpConfig, hookView] =
        await Promise.all([
          client.settingsGet(),
          client.sessionList(),
          client.memoriesList(),
          client.skillsList(),
          client.mcpServers(),
          client.mcpConfigRead(),
          // 启动时还没有 activeId：hooks 先给用户作用域，面板打开时再带会话刷新。
          client.hooksList(null),
        ]);
      rawSettingsRef.current = rawSettingsOf(view);
      const mappedSettings = settingsFromLfttdo(view);
      let nextSessions = sessionRowsFromLfttdo(rows);
      if (nextSessions.length === 0) {
        // 空库先建一个会话，避免空态（spec §7.1）。
        const id = await client.sessionCreate();
        nextSessions = [{ id, title: '新对话', updatedAt: new Date().toISOString(), messages: [] }];
      }
      const mappedHooks = hooksFromLfttdo(hookView);
      setSettings(mappedSettings);
      setSessions(nextSessions);
      setActiveId(mostRecentSession(nextSessions).id);
      setWorkspacePath(mappedSettings.roots[0] ?? null);
      setMemories(memoriesFromLfttdo(memoryRows));
      // 面板/斜杠菜单的数据启动就铺真实值，避免首帧顶着 mock 冒充真实（评审 P1-2）。
      setSkills(skillsFromLfttdo(skillView));
      setMcpServers(mcpServersFromLfttdo(mcpRows));
      setMcpJson(mcpConfig?.text ?? '{"mcpServers": {}}');
      setHooks(mappedHooks.hooks);
      setWorkspaceHooksEnabled(mappedHooks.workspaceEnabled);
      setUsage({ inputTokens: 0, outputTokens: 0, contextTokens: 0 });
      setBoot({ status: 'ready' });
    } catch (error) {
      setBoot({ status: 'error', message: errorMessage(error) });
    }
  }

  useEffect(() => {
    if (real) bootReal();
    // 启动只跑一次；bootReal 内部自己 setState。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 卸载时清掉未触发的防抖落盘定时器（评审 P2-8）。
  useEffect(
    () => () => {
      if (persistTimerRef.current) {
        clearTimeout(persistTimerRef.current);
        persistTimerRef.current = null;
      }
    },
    []
  );

  // 首次激活某个会话时拉一次历史（spec §7.1/§7.3）；同一会话只加载一次。
  const hydrateSession = agent.hydrateSession;
  useEffect(() => {
    if (!real || boot.status !== 'ready') return;
    hydrateSession?.(activeId);
  }, [real, boot.status, activeId, hydrateSession]);

  // 真实模式：工作文件夹与 @ 文件提及都跟着会话走（spec §10.6 / §12.5）。
  // 用 activeIdRef 做序守卫：快速切换会话时，迟到的旧会话响应不许覆盖新会话（评审 P2-9）。
  const activeIdRef = useRef(activeId);
  activeIdRef.current = activeId;

  const refreshWorkspaceReal = async (sessionId) => {
    if (!sessionId) return;
    try {
      const view = await client.sessionOutputView(sessionId);
      if (activeIdRef.current !== sessionId) return;
      setWorkspacePath(view.workspace ?? null);
    } catch (error) {
      if (activeIdRef.current !== sessionId) return;
      showToast({ body: `读取工作文件夹失败：${errorMessage(error)}`, type: 'error' });
    }
  };

  const refreshFileMentionsReal = async (sessionId) => {
    if (!sessionId) return;
    try {
      const files = await client.sessionSearchFiles(sessionId, '');
      if (activeIdRef.current !== sessionId) return;
      setRealFileMentions(files);
    } catch (error) {
      // 文件提及是辅助数据：失败静默留空，不挡对话。
      if (activeIdRef.current !== sessionId) return;
      setRealFileMentions([]);
      showToast({ body: `文件提及刷新失败：${errorMessage(error)}`, type: 'error' });
    }
  };

  useEffect(() => {
    if (!real || boot.status !== 'ready' || !activeId) return;
    refreshWorkspaceReal(activeId);
    refreshFileMentionsReal(activeId);
    // 两个刷新都只依赖会话与启动状态。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [real, boot.status, activeId]);

  useEffect(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return undefined;

    const mediaQuery = window.matchMedia('(max-width: 899px)');
    const applyCollapsed = (isNarrow) => {
      setSidebarCollapsed(isNarrow);
    };

    applyCollapsed(mediaQuery.matches);
    const handleChange = (event) => applyCollapsed(event.matches);
    mediaQuery.addEventListener('change', handleChange);

    return () => mediaQuery.removeEventListener('change', handleChange);
  }, []);

  const runtimeTheme = useMemo(
    () =>
      defineTheme({
        name: `${defaultThemeSlug}-runtime`,
        extends: themes[defaultThemeSlug],
        typography: { scale: { base: settings.fontSize, ratio: 1.2 } },
        tokens: {
          // 空串是 Rust 的「跟随主题预设」语义：不覆盖任何 accent 变量，让主题预设生效。
          ...(settings.accent
            ? {
                '--color-accent': settings.accent,
                '--color-text-accent': settings.accent,
                '--color-icon-accent': settings.accent,
                '--focus-outline-color': settings.accent,
              }
            : {}),
        },
      }),
    [settings.fontSize, settings.accent]
  );

  function setDraftFor(sessionId, value) {
    setDrafts((prev) => ({ ...prev, [sessionId]: value }));
  }

  function handleToggleCollapse() {
    setSidebarCollapsed((prev) => !prev);
  }

  function handleSelectSession(sessionId) {
    setActiveId(sessionId);
    setOpenPanel(null);
  }

  async function handleCreateSession() {
    if (!real) {
      const next = newSession('新对话');
      setSessions((prev) => [next, ...prev]);
      setActiveId(next.id);
      setDraftFor(next.id, '');
      setPendingImages([]);
      setUsage({ inputTokens: 0, outputTokens: 0, contextTokens: 0 });
      return;
    }
    try {
      const id = await client.sessionCreate();
      setSessions((prev) => [
        { id, title: '新对话', updatedAt: new Date().toISOString(), messages: [] },
        ...prev,
      ]);
      setActiveId(id);
      setDraftFor(id, '');
      setPendingImages([]);
    } catch (error) {
      showToast({ body: `新建会话失败：${errorMessage(error)}`, type: 'error' });
    }
  }

  async function refreshSessionsReal() {
    try {
      const rows = await client.sessionList();
      let mapped = [];
      setSessions((prev) => {
        mapped = sessionRowsFromLfttdo(rows).map((row) => ({
          ...row,
          messages: prev.find((session) => session.id === row.id)?.messages ?? [],
        }));
        return mapped;
      });
      return mapped;
    } catch (error) {
      showToast({ body: `刷新会话列表失败：${errorMessage(error)}`, type: 'error' });
      return null;
    }
  }

  async function handleRenameSession(sessionId, title) {
    if (!real) {
      setSessions((prev) =>
        prev.map((session) =>
          session.id === sessionId ? { ...session, title } : session
        )
      );
      return;
    }
    try {
      await client.sessionRename(sessionId, title);
      await refreshSessionsReal();
    } catch (error) {
      showToast({ body: `重命名失败：${errorMessage(error)}`, type: 'error' });
    }
  }

  async function handleDeleteSession(sessionId) {
    if (!real) {
      const remaining = sessions.filter((session) => session.id !== sessionId);

      if (remaining.length === 0) {
        const fresh = newSession('新对话');
        setSessions([fresh]);
        setActiveId(fresh.id);
        setDrafts({ [fresh.id]: '' });
      } else {
        setSessions(remaining);
        if (sessionId === activeId || !remaining.some((session) => session.id === activeId)) {
          setActiveId(mostRecentSession(remaining).id);
        }
        setDrafts((prev) => {
          const next = { ...prev };
          delete next[sessionId];
          return next;
        });
      }

      setPendingImages([]);
      return;
    }

    try {
      await client.sessionDelete(sessionId);
      agent.forgetSession?.(sessionId);
      setDrafts((prev) => {
        const next = { ...prev };
        delete next[sessionId];
        return next;
      });
      setPendingImages([]);
      const remaining = await refreshSessionsReal();
      if (!remaining) return;
      if (remaining.length === 0) {
        // 空库回到「始终有一个会话」的启动态（spec §7.1）。
        const id = await client.sessionCreate();
        const fresh = { id, title: '新对话', updatedAt: new Date().toISOString(), messages: [] };
        setSessions([fresh]);
        setActiveId(id);
        return;
      }
      if (sessionId === activeId || !remaining.some((session) => session.id === activeId)) {
        setActiveId(mostRecentSession(remaining).id);
      }
    } catch (error) {
      showToast({ body: `删除会话失败：${errorMessage(error)}`, type: 'error' });
    }
  }

  async function handleExportSession(sessionId) {
    if (real) {
      // Rust 弹原生保存对话框（spec §10.6），返回保存路径或 null（用户取消）。
      try {
        const path = await client.sessionExport(sessionId);
        showToast({ body: path ? `已导出：${path}` : '导出已取消' });
      } catch (error) {
        showToast({ body: `导出失败：${errorMessage(error)}`, type: 'error' });
      }
      return;
    }
    const session = sessions.find((item) => item.id === sessionId);
    if (!session) {
      showToast({ body: '导出失败', type: 'error' });
      return;
    }

    const safeTitle = String(session.title || '会话').replace(
      /[\\/:*?"<>|]/g,
      '-'
    );
    const filename = `${safeTitle}.md`;

    try {
      const markdown = serializeSessionMarkdown(session);
      downloadBlob(
        filename,
        new Blob([markdown], { type: 'text/markdown;charset=utf-8' })
      );
      showToast({ body: `已导出：${filename}` });
    } catch {
      showToast({ body: '导出失败', type: 'error' });
    }
  }

  function handleOpenPanel(nextPanel) {
    if (real) {
      // 面板数据每次打开都现读（skills.rs 的纪律：磁盘是唯一真相源）。
      if (nextPanel === 'skills') refreshSkillsReal();
      if (nextPanel === 'mcp') refreshMcpReal();
      if (nextPanel === 'hooks') refreshHooksReal();
    }
    setOpenPanel((current) => (current === nextPanel ? null : nextPanel));
  }

  function handleClosePanel() {
    setOpenPanel(null);
  }

  async function handleChooseWorkspace() {
    if (real) {
      try {
        const picked = await client.pickFolder();
        if (!picked) return;
        await client.sessionWorkspaceSet(activeId, picked);
        await refreshWorkspaceReal(activeId);
        await refreshFileMentionsReal(activeId);
        showToast({ body: '已设置会话工作文件夹' });
      } catch (error) {
        showToast({ body: `设置工作文件夹失败：${errorMessage(error)}`, type: 'error' });
      }
      return;
    }
    setWorkspacePath(CHOSEN_WORKSPACE);
    showToast({ body: '已选择工作文件夹' });
  }

  async function handleClearWorkspace() {
    if (real) {
      try {
        await client.sessionWorkspaceSet(activeId, null);
        await refreshWorkspaceReal(activeId);
        await refreshFileMentionsReal(activeId);
        showToast({ body: '已清除会话工作文件夹' });
      } catch (error) {
        showToast({ body: `清除工作文件夹失败：${errorMessage(error)}`, type: 'error' });
      }
      return;
    }
    setWorkspacePath(null);
    showToast({ body: '已清除工作文件夹' });
  }

  function handleRevealWorkspace() {
    if (real) {
      if (!workspacePath) {
        showToast({ body: '这个会话还没有工作文件夹' });
        return;
      }
      client
        .revealPath(activeId, workspacePath)
        .catch((error) => showToast({ body: `定位失败：${errorMessage(error)}`, type: 'error' }));
      return;
    }
    showToast({ body: '已在资源管理器中显示' });
  }

  async function handleModelChange(model) {
    if (!real) {
      handleSettingChange('model', model);
      showToast({ body: `已切换到 ${model}` });
      return;
    }
    // 真实模式必须落盘，否则下一轮模型调用仍是旧值，界面在撒谎（spec §10.7）。
    const raw = rawSettingsRef.current;
    if (!raw) {
      showToast({ body: '设置尚未加载完成，切换未保存', type: 'error' });
      return;
    }
    handleSettingChange('model', model);
    try {
      const next = settingsFromMapped(raw, { ...settingsRef.current, model });
      await client.settingsSet(next);
      rawSettingsRef.current = next;
      showToast({ body: `已切换到 ${model}` });
    } catch (error) {
      handleSettingChange('model', raw.model ?? settings.model);
      showToast({ body: `模型切换失败：${errorMessage(error)}`, type: 'error' });
    }
  }

  function handleSettingChange(key, value) {
    setSettings((prev) => {
      let nextValue = value;
      if (key === 'fontSize') {
        nextValue = Math.min(18, Math.max(13, Number(value) || 15));
      }
      if (key === 'contextBudget') {
        nextValue = Math.min(200000, Math.max(20000, Number(value) || 120000));
      }
      if (key === 'roots' && !Array.isArray(value)) {
        return prev;
      }
      if (key === 'accent' && value !== '' && !ACCENT_VALUES.has(value)) {
        return prev;
      }
      return { ...prev, [key]: nextValue };
    });
    // 真实模式：文本框逐键触发，落盘做 600ms 防抖（composer 换档走 handleModelChange
    // 即时保存，这里再排一次防抖是同一份数据的幂等重写）；hasApiKey 不是 Settings 字段，
    // API Key 只走 settings_set_api_key。
    if (real && key !== 'hasApiKey') {
      if (persistTimerRef.current) clearTimeout(persistTimerRef.current);
      persistTimerRef.current = setTimeout(() => {
        persistTimerRef.current = null;
        persistSettingsReal();
      }, 600);
    }
  }

  async function persistSettingsReal() {
    const raw = rawSettingsRef.current;
    if (!raw) return;
    try {
      const next = settingsFromMapped(raw, settingsRef.current);
      await client.settingsSet(next);
      rawSettingsRef.current = next;
    } catch (error) {
      showToast({ body: `设置保存失败：${errorMessage(error)}`, type: 'error' });
    }
  }

  async function refreshMemoriesReal() {
    try {
      setMemories(memoriesFromLfttdo(await client.memoriesList()));
    } catch (error) {
      showToast({ body: `记忆列表刷新失败：${errorMessage(error)}`, type: 'error' });
    }
  }

  function refreshSkillsReal() {
    client
      .skillsList()
      .then((view) => setSkills(skillsFromLfttdo(view)))
      .catch((error) => showToast({ body: `技能列表刷新失败：${errorMessage(error)}`, type: 'error' }));
  }

  function refreshMcpReal() {
    client
      .mcpServers()
      .then((rows) => setMcpServers(mcpServersFromLfttdo(rows)))
      .catch((error) => showToast({ body: `MCP 列表刷新失败：${errorMessage(error)}`, type: 'error' }));
    client
      .mcpConfigRead()
      .then((view) => setMcpJson(view.text ?? ''))
      .catch((error) => showToast({ body: `MCP 配置读取失败：${errorMessage(error)}`, type: 'error' }));
  }

  function refreshHooksReal() {
    client
      .hooksList(activeId)
      .then((view) => {
        const mapped = hooksFromLfttdo(view);
        setHooks(mapped.hooks);
        setWorkspaceHooksEnabled(mapped.workspaceEnabled);
        // config_ok=false 是 Rust 明确要求上界面的警告（ipc.rs:2556）：文件坏了，hooks 不生效。
        if (view?.config_ok === false) {
          showToast({
            body: 'hooks.json 解析失败，当前 hooks 不生效；请检查该文件',
            type: 'error',
          });
        }
      })
      .catch((error) => showToast({ body: `Hooks 列表刷新失败：${errorMessage(error)}`, type: 'error' }));
  }

  async function handleAddMemory({ kind, body, dueAt }) {
    const text = String(body ?? '').trim();
    if (!text) return;
    if (real) {
      try {
        await client.memoryAdd(memoryKindToWire(kind), text, dueDateToMs(dueAt));
        await refreshMemoriesReal();
        showToast({ body: '已记住' });
      } catch (error) {
        showToast({ body: `新增记忆失败：${errorMessage(error)}`, type: 'error' });
      }
      return;
    }
    setMemories((prev) => [
      {
        id: `memory-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
        kind,
        body: text,
        dueAt: dueAt || null,
      },
      ...prev,
    ]);
    showToast({ body: '已记住' });
  }

  async function handleUpdateMemory(id, patch) {
    if (real) {
      // Rust 只暴露正文修改（memory_update_body）：面板在真实模式隐藏类型/到期日编辑。
      try {
        await client.memoryUpdateBody(Number(id), String(patch.body ?? ''));
        await refreshMemoriesReal();
        showToast({ body: '记忆已更新' });
      } catch (error) {
        showToast({ body: `记忆更新失败：${errorMessage(error)}`, type: 'error' });
      }
      return;
    }
    setMemories((prev) =>
      prev.map((memory) => (memory.id === id ? { ...memory, ...patch } : memory))
    );
  }

  async function handleDeleteMemory(id) {
    if (real) {
      try {
        await client.memoriesDelete(Number(id));
        await refreshMemoriesReal();
        showToast({ body: '已删除记忆' });
      } catch (error) {
        showToast({ body: `删除记忆失败：${errorMessage(error)}`, type: 'error' });
      }
      return;
    }
    setMemories((prev) => prev.filter((memory) => memory.id !== id));
  }

  async function handleClearMemories() {
    if (real) {
      try {
        await client.memoriesClear();
        await refreshMemoriesReal();
        showToast({ body: '已清空长期记忆' });
      } catch (error) {
        showToast({ body: `清空记忆失败：${errorMessage(error)}`, type: 'error' });
      }
      return;
    }
    setMemories([]);
    showToast({ body: '已清空长期记忆' });
  }

  async function flushPendingPersist() {
    if (!persistTimerRef.current) return;
    clearTimeout(persistTimerRef.current);
    persistTimerRef.current = null;
    await persistSettingsReal();
  }

  async function handleSaveApiKey(key) {
    // 先把防抖窗口内的未落盘编辑冲掉，再回读设置整体刷新（评审 P2-10）：
    // 否则 settingsGet 的回读会把刚输入未保存的字段滚回磁盘值。
    try {
      await flushPendingPersist();
      await client.settingsSetApiKey(key);
      const view = await client.settingsGet();
      rawSettingsRef.current = rawSettingsOf(view);
      setSettings(settingsFromLfttdo(view));
      showToast({ body: '已保存 API Key' });
    } catch (error) {
      showToast({ body: `API Key 保存失败：${errorMessage(error)}`, type: 'error' });
      throw error;
    }
  }

  async function handleClearApiKey() {
    try {
      await flushPendingPersist();
      await client.settingsSetApiKey('');
      const view = await client.settingsGet();
      rawSettingsRef.current = rawSettingsOf(view);
      setSettings(settingsFromLfttdo(view));
      showToast({ body: '已清除 API Key' });
    } catch (error) {
      showToast({ body: `API Key 清除失败：${errorMessage(error)}`, type: 'error' });
      throw error;
    }
  }

  function handleImportPersona() {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.txt,text/plain';
    input.addEventListener(
      'change',
      () => {
        const file = input.files?.[0];
        if (!file) return;
        const reader = new FileReader();
        reader.onload = () => {
          if (typeof reader.result === 'string') {
            handleSettingChange('persona', reader.result);
            showToast({ body: '已导入人设' });
          } else {
            showToast({ body: '导入失败', type: 'error' });
          }
        };
        reader.onerror = () => showToast({ body: '导入失败', type: 'error' });
        reader.readAsText(file);
      },
      { once: true }
    );
    input.click();
  }

  function handleExportPersona() {
    const persona = settings.persona?.trim()
      ? settings.persona
      : DEFAULT_PERSONA;
    try {
      downloadBlob(
        'persona.txt',
        new Blob([persona], { type: 'text/plain;charset=utf-8' })
      );
      showToast({ body: '已导出人设' });
    } catch {
      showToast({ body: '导出失败', type: 'error' });
    }
  }

  async function handleToggleSkill(name, enabled) {
    if (real) {
      // 启用键是 folder（skills_set_enabled 的键），不是显示名。
      const skill = skills.find((item) => item.name === name);
      if (!skill?.folder) {
        showToast({ body: '找不到这条技能的目录名', type: 'error' });
        return;
      }
      try {
        await client.skillsSetEnabled(skill.folder, enabled);
        refreshSkillsReal();
        showToast({ body: enabled ? `已启用 ${name}` : `已停用 ${name}` });
      } catch (error) {
        showToast({ body: `技能切换失败：${errorMessage(error)}`, type: 'error' });
      }
      return;
    }
    setSkills((prev) =>
      prev.map((skill) =>
        skill.name === name ? { ...skill, enabled } : skill
      )
    );
  }

  function handleUseSkill(name) {
    const insertion = `使用「${name}」技能：`;
    setDrafts((prev) => {
      const current = prev[activeId] ?? '';
      return {
        ...prev,
        [activeId]: current ? `${current} ${insertion}` : insertion,
      };
    });
    setOpenPanel(null);
    showToast({ body: insertion });
  }

  async function handleSetMcpTrust(name, trust) {
    if (real) {
      // 面板的 allow 在 Rust 侧叫 auto（parse_trust 只认 auto/ask/block）。
      try {
        await client.mcpSetTrust(name, trust === 'allow' ? 'auto' : trust);
        refreshMcpReal();
        showToast({ body: `${name} 信任档已更新` });
      } catch (error) {
        showToast({ body: `信任档更新失败：${errorMessage(error)}`, type: 'error' });
      }
      return;
    }
    setMcpServers((prev) =>
      prev.map((server) =>
        server.name === name ? { ...server, trust } : server
      )
    );
  }

  function handleMcpJsonChange(text) {
    setMcpJson(text);
    setMcpValidation({ status: 'idle', message: '' });
  }

  function handleMcpValidate() {
    setMcpValidation(validateMcpConfig(mcpJson));
  }

  function handleMcpSave() {
    const result = validateMcpConfig(mcpJson);
    setMcpValidation(result);
    if (result.status === 'valid') {
      if (real) {
        // 真实保存走 mcp_config_save：Rust 校验形状并落盘，重启后重新连接。
        client
          .mcpConfigSave(mcpJson)
          .then(() =>
            showToast({
              body: '配置已保存，重启应用后生效。当前会话仍按启动时的连接与工具运行。',
            })
          )
          .catch((error) =>
            showToast({ body: `配置保存失败：${errorMessage(error)}`, type: 'error' })
          );
        return;
      }
      showToast({
        body: '配置已保存，重启应用后生效。当前会话仍按启动时的连接与工具运行。',
      });
    }
  }

  async function handleToggleHookTrust(id, trusted) {
    if (real) {
      // id 就是 digest（含作用域前缀），原样递回（hooks.rs 的信任键）。
      try {
        await client.hooksSetTrusted(id, trusted);
        refreshHooksReal();
        showToast({ body: trusted ? '已信任这条 hook' : '已撤销信任' });
      } catch (error) {
        showToast({ body: `信任状态更新失败：${errorMessage(error)}`, type: 'error' });
      }
      return;
    }
    setHooks((prev) =>
      prev.map((hook) => (hook.id === id ? { ...hook, trusted } : hook))
    );
  }

  async function handleToggleWorkspaceHooks(enabled) {
    if (real) {
      try {
        await client.hooksSetWorkspaceEnabled(enabled);
        refreshHooksReal();
        showToast({
          body: enabled ? '已启用工作区级 hooks' : '已停用工作区级 hooks',
        });
      } catch (error) {
        showToast({ body: `工作区 hooks 切换失败：${errorMessage(error)}`, type: 'error' });
      }
      return;
    }
    setWorkspaceHooksEnabled(enabled);
    showToast({
      body: enabled ? '已启用工作区级 hooks' : '已停用工作区级 hooks',
    });
  }

  async function handleAddFiles(files) {
    if (real) {
      // 每张图的原始字节交给 Rust 归一化（spec §10.8）：Rust 是唯一的图片闸门。
      const images = Array.from(files ?? []).filter(
        (file) => file && file.type && file.type.startsWith('image/')
      );
      if (images.length === 0) return;

      const room = Math.max(0, 4 - pendingImages.length);
      const accepted = images.slice(0, room);
      const ignored = images.length - accepted.length;
      if (ignored > 0) {
        showToast({ body: `一次最多 4 张，已忽略 ${ignored} 张`, type: 'error' });
      }

      for (const file of accepted) {
        try {
          const bytes = await file.arrayBuffer();
          const block = await client.normalizeImage(bytes);
          if (block?.type !== 'image') {
            throw new Error('归一化结果不是图片块');
          }
          setPendingImages((prev) => {
            if (prev.length >= 4) return prev;
            return [
              ...prev,
              {
                id: `pending-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
                name: file.name || '图片',
                // Rust ContentBlock 的 mime 是裸 "png"/"jpeg"，data URL 必须补 image/ 前缀，
                // 否则发送端的解析正则吃不掉、<img> 预览也渲染不出来（评审 P0-1）。
                url: `data:image/${block.mime};base64,${block.data}`,
              },
            ];
          });
        } catch (error) {
          showToast({ body: `图片处理失败：${file.name}：${errorMessage(error)}`, type: 'error' });
        }
      }
      return;
    }
    const images = Array.from(files ?? []).filter(
      (file) => file && file.type && file.type.startsWith('image/')
    );
    if (images.length === 0) return;

    const room = Math.max(0, 4 - pendingImages.length);
    const accepted = images.slice(0, room);
    const ignored = images.length - accepted.length;

    if (ignored > 0) {
      showToast({
        body: `一次最多 4 张，已忽略 ${ignored} 张`,
        type: 'error',
      });
    }

    accepted.forEach((file) => {
      const reader = new FileReader();
      reader.onload = () => {
        if (typeof reader.result !== 'string') return;
        const url = reader.result;
        setPendingImages((prev) => {
          if (prev.length >= 4) return prev;
          return [
            ...prev,
            {
              id: `pending-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
              name: file.name || '图片',
              url,
            },
          ];
        });
      };
      reader.onerror = () => {
        showToast({ body: `读取图片失败：${file.name}`, type: 'error' });
      };
      reader.readAsDataURL(file);
    });
  }

  function handleRemoveImage(id) {
    setPendingImages((prev) => prev.filter((image) => image.id !== id));
  }

  function handleSend(text) {
    const trimmed = String(text ?? composerValue).trim();
    if (!trimmed) return;
    if (agent.isStreaming) {
      if (agent.streamingSessionId !== activeId) {
        showToast({ body: '另一个会话正在生成，先停止再发送' });
      }
      return;
    }

    agent.send(trimmed, activeId, pendingImages);
    setDraftFor(activeId, '');
    setPendingImages([]);
  }

  function handleStop() {
    agent.stop();
  }

  function handleRetry() {
    agent.retryLast(activeId);
  }

  function handlePermission(permissionId, decision) {
    agent.respondPermission(permissionId, decision);
  }

  function handleOpenDataDir() {
    if (real) {
      // hooks_open_folder 的缺省语义就是打开用户数据目录（ipc.rs:2693）。
      client
        .hooksOpenFolder(null, false)
        .catch((error) => showToast({ body: `打开数据目录失败：${errorMessage(error)}`, type: 'error' }));
      return;
    }
    showToast({ body: '已在资源管理器中打开数据目录' });
  }

  function handleOpenWorkspaceDir() {
    if (real) {
      if (!workspacePath) {
        showToast({ body: '这个会话还没有工作文件夹' });
        return;
      }
      client
        .hooksOpenFolder(activeId, true)
        .catch((error) => showToast({ body: `打开工作区目录失败：${errorMessage(error)}`, type: 'error' }));
      return;
    }
    showToast({ body: '已在资源管理器中打开工作区目录' });
  }

  async function handleQuestionAnswer(questionId, answer) {
    const ok = await agent.answerQuestion(questionId, answer);
    // 真实模式返回 false = 这道提问已失效：关掉弹窗，不再重开（spec §10.4）。
    setDismissedQuestionId(ok === false ? questionId : null);
  }

  // 启动门（spec §7.1）：加载失败可重试；所有 hook 已在上面执行完，这里才允许提前返回。
  if (boot.status === 'loading') {
    return (
      <Theme theme={runtimeTheme} mode="dark">
        <AppShell height="fill" contentPadding={0} variant="elevated">
          <VStack height="100%" align="center" justify="center" gap={3}>
            <Spinner size="lg" shade="subtle" label="正在连接 LFTTDO 内核…" />
          </VStack>
        </AppShell>
      </Theme>
    );
  }
  if (boot.status === 'error') {
    return (
      <Theme theme={runtimeTheme} mode="dark">
        <AppShell height="fill" contentPadding={0} variant="elevated">
          <VStack height="100%" align="center" justify="center" gap={3} padding={5}>
            <Text type="body">连接 LFTTDO 内核失败：{boot.message}</Text>
            <Button label="重试" variant="secondary" onClick={bootReal} />
          </VStack>
        </AppShell>
      </Theme>
    );
  }

  return (
    <Theme theme={runtimeTheme} mode="dark">
      <AppShell
        height="fill"
        contentPadding={0}
        variant="elevated"
        sideNav={
          <SessionSidebar
            sessions={sessions}
            activeId={activeId}
            collapsed={sidebarCollapsed}
            onToggleCollapse={handleToggleCollapse}
            onSelect={handleSelectSession}
            onCreate={handleCreateSession}
            onRename={handleRenameSession}
            onDelete={handleDeleteSession}
            onExport={handleExportSession}
            onOpenPanel={handleOpenPanel}
          />
        }
      >
        <VStack height="100%" gap={0} minHeight={0}>
          <WorkspaceBar
            workspacePath={workspacePath}
            onChoose={handleChooseWorkspace}
            onClear={handleClearWorkspace}
            onReveal={handleRevealWorkspace}
          />
          <ChatLayout
            density="spacious"
            composer={
              <Composer
                value={composerValue}
                onChange={(value) => setDraftFor(activeId, value)}
                onSend={handleSend}
                onStop={handleStop}
                isStreaming={agent.isStreaming}
                pendingImages={pendingImages}
                onAddFiles={handleAddFiles}
                onRemoveImage={handleRemoveImage}
                skills={skills}
                files={real ? realFileMentions : fileMentions}
                models={modelOptions}
                modelName={settings.model}
                onModelChange={handleModelChange}
                usage={usage}
                contextBudget={contextBudget}
              />
            }
          >
            <ChatPane
              session={activeSession}
              isStreaming={activeStreaming}
              settings={settings}
              usage={usage}
              contextBudget={contextBudget}
              onPermission={handlePermission}
              onQuestionAnswer={handleQuestionAnswer}
              onReopenQuestion={() => setDismissedQuestionId(null)}
              hidePendingQuestion={questionDialogOpen}
              onRetry={handleRetry}
            />
          </ChatLayout>
          <StatusBar usage={usage} contextBudget={contextBudget} />
        </VStack>
      </AppShell>

      <SettingsPanel
        isOpen={openPanel === 'settings'}
        onClose={handleClosePanel}
        settings={settings}
        onSettingChange={handleSettingChange}
        memories={memories}
        onAddMemory={handleAddMemory}
        onUpdateMemory={handleUpdateMemory}
        onDeleteMemory={handleDeleteMemory}
        onClearMemories={handleClearMemories}
        onImportPersona={handleImportPersona}
        onExportPersona={handleExportPersona}
        onSaveApiKey={real ? handleSaveApiKey : undefined}
        onClearApiKey={real ? handleClearApiKey : undefined}
        memoryEditing={real ? 'body' : 'full'}
        persistNote={real ? '改动即时生效并写入设置文件。' : undefined}
        storageLocation={
          real ? '应用数据目录（LFTTDO Astryx 独立目录）' : undefined
        }
        onOpenDataDir={handleOpenDataDir}
        onOpenWorkspaceDir={handleOpenWorkspaceDir}
      />

      <SkillsPanel
        isOpen={openPanel === 'skills'}
        onClose={handleClosePanel}
        skills={skills}
        onToggleSkill={handleToggleSkill}
        onUseSkill={handleUseSkill}
        onRefresh={real ? () => refreshSkillsReal() : undefined}
        onOpenFolder={
          real
            ? () =>
                client
                  .skillsOpenFolder()
                  .catch((error) =>
                    showToast({ body: `打开技能目录失败：${errorMessage(error)}`, type: 'error' })
                  )
            : undefined
        }
        description={
          real
            ? '停用的技能不会出现在输入框的 / 菜单里；列表每次现读数据目录下的技能文件夹。'
            : undefined
        }
      />

      <McpPanel
        isOpen={openPanel === 'mcp'}
        onClose={handleClosePanel}
        servers={mcpServers}
        onSetTrust={handleSetMcpTrust}
        jsonText={mcpJson}
        onJsonChange={handleMcpJsonChange}
        onValidate={handleMcpValidate}
        onSave={handleMcpSave}
        validation={mcpValidation}
      />

      <HooksPanel
        isOpen={openPanel === 'hooks'}
        onClose={handleClosePanel}
        hooks={hooks}
        onToggleTrust={handleToggleHookTrust}
        workspaceHooksEnabled={workspaceHooksEnabled}
        onToggleWorkspaceHooks={handleToggleWorkspaceHooks}
        trustNote={
          real
            ? '信任按声明内容的 SHA-256 digest 记录（见每行的信任键）；hooks.json 一改，旧 digest 自动失效回到未信任。'
            : undefined
        }
        fingerprintSuffix={real ? '' : undefined}
        useRealDigest={real}
        onOpenConfig={
          real
            ? () =>
                client
                  .hooksOpenFolder(activeId, false)
                  .catch((error) =>
                    showToast({ body: `打开 hooks 目录失败：${errorMessage(error)}`, type: 'error' })
                  )
            : undefined
        }
        onOpenDataDir={
          real
            ? () =>
                client
                  .hooksOpenFolder(null, false)
                  .catch((error) =>
                    showToast({ body: `打开数据目录失败：${errorMessage(error)}`, type: 'error' })
                  )
            : undefined
        }
        onOpenWorkspaceDir={
          real
            ? () => handleOpenWorkspaceDir()
            : undefined
        }
      />

      <QuestionDialog
        question={pendingQuestion}
        isOpen={questionDialogOpen}
        onOpenChange={(open) => {
          if (!open) setDismissedQuestionId(pendingQuestion?.id ?? null);
        }}
        onAnswer={handleQuestionAnswer}
      />
    </Theme>
  );
}
