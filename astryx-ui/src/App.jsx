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
  rawSettingsOf,
  sessionRowsFromLfttdo,
  settingsFromLfttdo,
  settingsWithModel,
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
      const [view, rows] = await Promise.all([client.settingsGet(), client.sessionList()]);
      rawSettingsRef.current = rawSettingsOf(view);
      const mappedSettings = settingsFromLfttdo(view);
      let nextSessions = sessionRowsFromLfttdo(rows);
      if (nextSessions.length === 0) {
        // 空库先建一个会话，避免空态（spec §7.1）。
        const id = await client.sessionCreate();
        nextSessions = [{ id, title: '新对话', updatedAt: new Date().toISOString(), messages: [] }];
      }
      setSettings(mappedSettings);
      setSessions(nextSessions);
      setActiveId(mostRecentSession(nextSessions).id);
      setWorkspacePath(mappedSettings.roots[0] ?? null);
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

  // 首次激活某个会话时拉一次历史（spec §7.1/§7.3）；同一会话只加载一次。
  const hydrateSession = agent.hydrateSession;
  useEffect(() => {
    if (!real || boot.status !== 'ready') return;
    hydrateSession?.(activeId);
  }, [real, boot.status, activeId, hydrateSession]);

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
          '--color-accent': settings.accent,
          '--color-text-accent': settings.accent,
          '--color-icon-accent': settings.accent,
          '--focus-outline-color': settings.accent,
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

  function handleExportSession(sessionId) {
    if (real) {
      // 导出走 Rust 的原生保存对话框（spec §10.6），P4 接入；不用 Blob 下载冒充。
      showToast({ body: '导出将在后续阶段接入真实保存' });
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
      // 技能/MCP/Hooks/设置面板的数据都还没接真实命令（spec §12.7），
      // 打开只会展示 mock 冒充真实数据，所以统一拦截。
      showToast({ body: '该面板将在后续阶段接入真实数据' });
      return;
    }
    setOpenPanel((current) => (current === nextPanel ? null : nextPanel));
  }

  function handleClosePanel() {
    setOpenPanel(null);
  }

  function handleChooseWorkspace() {
    if (real) {
      showToast({ body: '工作文件夹将在后续阶段接入' });
      return;
    }
    setWorkspacePath(CHOSEN_WORKSPACE);
    showToast({ body: '已选择工作文件夹' });
  }

  function handleClearWorkspace() {
    if (real) {
      showToast({ body: '工作文件夹将在后续阶段接入' });
      return;
    }
    setWorkspacePath(null);
    showToast({ body: '已清除工作文件夹' });
  }

  function handleRevealWorkspace() {
    if (real) {
      showToast({ body: '文件定位将在后续阶段接入' });
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
      const next = settingsWithModel(raw, model);
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
      if (key === 'accent' && !ACCENT_VALUES.has(value)) {
        return prev;
      }
      return { ...prev, [key]: nextValue };
    });
  }

  function handleAddMemory({ kind, body, dueAt }) {
    const text = String(body ?? '').trim();
    if (!text) return;
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

  function handleUpdateMemory(id, patch) {
    setMemories((prev) =>
      prev.map((memory) => (memory.id === id ? { ...memory, ...patch } : memory))
    );
  }

  function handleDeleteMemory(id) {
    setMemories((prev) => prev.filter((memory) => memory.id !== id));
  }

  function handleClearMemories() {
    setMemories([]);
    showToast({ body: '已清空长期记忆' });
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

  function handleToggleSkill(name, enabled) {
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

  function handleSetMcpTrust(name, trust) {
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
      showToast({
        body: '配置已保存，重启应用后生效。当前会话仍按启动时的连接与工具运行。',
      });
    }
  }

  function handleToggleHookTrust(id, trusted) {
    setHooks((prev) =>
      prev.map((hook) => (hook.id === id ? { ...hook, trusted } : hook))
    );
  }

  function handleToggleWorkspaceHooks(enabled) {
    setWorkspaceHooksEnabled(enabled);
    showToast({
      body: enabled ? '已启用工作区级 hooks' : '已停用工作区级 hooks',
    });
  }

  function handleAddFiles(files) {
    if (real) {
      // 图片要先过 Rust 的 normalize_image（spec §10.8），P4 接入。
      showToast({ body: '图片附件将在后续阶段接入' });
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
                files={real ? [] : fileMentions}
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
      />

      <SkillsPanel
        isOpen={openPanel === 'skills'}
        onClose={handleClosePanel}
        skills={skills}
        onToggleSkill={handleToggleSkill}
        onUseSkill={handleUseSkill}
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
