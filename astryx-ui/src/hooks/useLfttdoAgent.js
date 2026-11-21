import { useCallback, useEffect, useRef, useState } from 'react';

import { client, errorMessage, listenAgent } from '../agent/lfttdo/client.js';
import { emptyProjection, projectEvent } from '../agent/lfttdo/projection.js';

/**
 * 真实 LFTTDO agent 钩子：与 useMockAgent 相同的返回 API（契约 §7.13），
 * 但消息的唯一来源是 Tauri 的 `agent` 事件通道，发送走 `send_message`。
 *
 * 投影状态（projectionsRef）是每个会话消息的**权威副本**：直播事件会把
 * `state.messages` 整体写回会话，所以乐观插入的用户消息必须同时进 React state
 * 和投影状态，否则第一条 `turn_start` 会把它冲掉。
 */

function nowIso() {
  return new Date().toISOString();
}

function titleFor(text) {
  return String(text ?? '').replace(/\s+/g, ' ').slice(0, 16);
}

/** 界面 pendingImage（dataURL）→ 线上 image block；解析失败就丢弃，不让发送卡死。 */
function imagesToBlocks(images) {
  return (Array.isArray(images) ? images : []).flatMap((image) => {
    const match = /^data:image\/(png|jpeg);base64,(.+)$/i.exec(String(image?.url ?? ''));
    if (!match) return [];
    return [{ type: 'image', data: match[2], mime: match[1].toLowerCase() }];
  });
}

/** 线上 blocks → 乐观用户消息的界面块（图片给 dataURL 预览）。 */
function localBlocks(blocks) {
  return blocks.flatMap((block) => {
    if (block.type === 'text') return [{ type: 'text', text: block.text }];
    if (block.type === 'image') {
      // mime 是裸 "png"/"jpeg"（Rust ContentBlock），data URL 要补 image/ 前缀。
      return [
        { type: 'image', src: `data:image/${block.mime};base64,${block.data}`, alt: '附件图片' },
      ];
    }
    return [];
  });
}

/** 权限卡三态 → Rust Decision 的线上形状（events.rs 的手写反序列化只认这两个形状）。 */
function decisionToWire(decision) {
  if (decision === 'once') return { allow: true, scope: 'once' };
  if (decision === 'session') return { allow: true, scope: 'session' };
  return { allow: false, reason: '用户拒绝' };
}

function removeFromLastUser(messages) {
  let cut = -1;
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    if (messages[i].role === 'user') {
      cut = i;
      break;
    }
  }
  return cut >= 0 ? messages.slice(0, cut) : messages;
}

function removeLastAssistant(messages) {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    if (messages[i].role === 'assistant') {
      return [...messages.slice(0, i), ...messages.slice(i + 1)];
    }
  }
  return messages;
}

export function useLfttdoAgent({ sessions, settings, setSessions, setUsage, showToast }) {
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const sessionsRef = useRef(sessions);
  sessionsRef.current = sessions;

  const [isStreaming, setIsStreaming] = useState(false);
  const [streamingSessionId, setStreamingSessionId] = useState(null);
  const isStreamingRef = useRef(false);
  const streamingSessionRef = useRef(null);

  /** 每个会话的投影状态：消息的权威副本。 */
  const projectionsRef = useRef(new Map());
  /** 已加载过历史的会话；同一会话只 `session_messages` 一次（spec §7.3）。 */
  const hydratedRef = useRef(new Set());
  /** 发送那一刻的模型（spec §7.4：写入助手消息，流式中不可切换）。 */
  const modelRef = useRef(settings?.model ?? null);

  const setStreaming = useCallback((next, sessionId) => {
    isStreamingRef.current = next;
    setIsStreaming(next);
    if (next) {
      streamingSessionRef.current = sessionId ?? null;
      setStreamingSessionId(sessionId ?? null);
    } else {
      streamingSessionRef.current = null;
      setStreamingSessionId(null);
    }
  }, []);

  /** 把某会话的投影消息写回 React state；done 时顺带把 updatedAt 推到现在。 */
  const writeBack = useCallback((sessionId, state, { touch = false } = {}) => {
    setSessions((prev) =>
      prev.map((session) =>
        session.id === sessionId
          ? {
              ...session,
              messages: state.messages,
              updatedAt: touch ? nowIso() : session.updatedAt,
            }
          : session
      )
    );
  }, [setSessions]);

  /** 未知 sessionId：刷新一次列表，不凭空造会话（spec §7.2）。 */
  const refreshSessions = useCallback(async () => {
    try {
      const rows = await client.sessionList();
      setSessions((prev) =>
        rows
          .map((row) => ({
            id: row.id,
            title: row.title || '新对话',
            updatedAt: new Date(row.updated_at ?? Date.now()).toISOString(),
            messages: prev.find((session) => session.id === row.id)?.messages ?? [],
          }))
      );
    } catch (error) {
      showToast?.({ body: `刷新会话列表失败：${errorMessage(error)}`, type: 'error' });
    }
  }, [setSessions, showToast]);

  const handleEvent = useCallback(
    (payload) => {
      const sessionId = payload?.sessionId;
      const event = payload?.event;
      if (!sessionId || !event) return;

      const known = sessionsRef.current.some((session) => session.id === sessionId);
      if (!known) {
        refreshSessions();
        return;
      }

      const before = projectionsRef.current.get(sessionId) ?? emptyProjection();
      const after = projectEvent(before, event, { live: true, model: modelRef.current });
      projectionsRef.current.set(sessionId, after);

      if (event.t === 'turn_start') setStreaming(true, sessionId);
      if (event.t === 'done' && streamingSessionRef.current === sessionId) {
        setStreaming(false);
        // Rust 会在首轮后生成会话标题（db.rs:1098），收尾时同步一次列表。
        refreshSessions();
      }

      if (event.t === 'usage' && after.usage) {
        setUsage(after.usage);
      }

      (after.toasts ?? []).forEach((toast) => showToast?.(toast));
      if (after.fatalError) {
        showToast?.({ body: after.fatalError, type: 'error' });
      }

      writeBack(sessionId, after, { touch: event.t === 'done' });
    },
    [refreshSessions, setStreaming, setUsage, showToast, writeBack]
  );

  // 只订阅一次；payload.sessionId 分发，非 active 会话也更新（spec §7.2）。
  useEffect(() => {
    let disposed = false;
    let unlisten = null;
    listenAgent((payload) => handleEvent(payload))
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch((error) => {
        showToast?.({ body: `事件通道订阅失败：${errorMessage(error)}`, type: 'error' });
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
    // handleEvent 的依赖都是稳定 ref/回调；订阅必须只发生一次。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** 首次激活会话时拉一次历史并用同一投影函数重放（spec §7.1）。 */
  const hydrateSession = useCallback(
    async (sessionId) => {
      if (!sessionId || hydratedRef.current.has(sessionId)) return;
      const session = sessionsRef.current.find((item) => item.id === sessionId);
      if (!session) return;
      if (session.messages.length > 0) {
        // 已有内容（流式中或刚发送过）：不重放，避免覆盖实时事件。
        hydratedRef.current.add(sessionId);
        return;
      }
      hydratedRef.current.add(sessionId);
      try {
        const events = await client.sessionMessages(sessionId);
        // 拉取期间该会话开始直播：放弃重放，保留直播状态。
        if (streamingSessionRef.current === sessionId) return;
        let state = emptyProjection();
        for (const item of Array.isArray(events) ? events : []) {
          state = projectEvent(state, item, { live: false });
        }
        projectionsRef.current.set(sessionId, state);
        writeBack(sessionId, state);
      } catch (error) {
        hydratedRef.current.delete(sessionId);
        showToast?.({ body: `加载历史失败：${errorMessage(error)}`, type: 'error' });
      }
    },
    [showToast, writeBack]
  );

  /** 删除会话后清掉它的投影与 hydration 记录。 */
  const forgetSession = useCallback((sessionId) => {
    if (!sessionId) return;
    projectionsRef.current.delete(sessionId);
    hydratedRef.current.delete(sessionId);
  }, []);

  /**
   * 发送一组线上 blocks：乐观插入用户消息 → send_message → 失败回滚。
   * retry 也走这里（blocks 来自 session_retry_last），语义一致。
   */
  const sendBlocks = useCallback(
    async (sessionId, blocks) => {
      if (!Array.isArray(blocks) || blocks.length === 0) return;
      if (isStreamingRef.current) return;
      const session = sessionsRef.current.find((item) => item.id === sessionId);
      if (!session) return;

      const userId = `local-user-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
      const userMessage = {
        id: userId,
        role: 'user',
        createdAt: nowIso(),
        blocks: localBlocks(blocks),
      };

      // 投影状态与 React state 同时插入：直播事件不会带这条用户消息（R68）。
      const before = projectionsRef.current.get(sessionId) ?? emptyProjection();
      projectionsRef.current.set(sessionId, {
        ...before,
        messages: [...before.messages, userMessage],
      });
      setSessions((prev) =>
        prev.map((item) =>
          item.id === sessionId
            ? { ...item, updatedAt: nowIso(), messages: [...item.messages, userMessage] }
            : item
        )
      );

      try {
        await client.sendMessage(sessionId, blocks);
      } catch (error) {
        projectionsRef.current.set(sessionId, {
          ...(projectionsRef.current.get(sessionId) ?? emptyProjection()),
          messages: (projectionsRef.current.get(sessionId)?.messages ?? []).filter(
            (message) => message.id !== userId
          ),
        });
        setSessions((prev) =>
          prev.map((item) =>
            item.id === sessionId
              ? {
                  ...item,
                  messages: item.messages.filter((message) => message.id !== userId),
                }
              : item
          )
        );
        showToast?.({ body: `发送失败：${errorMessage(error)}`, type: 'error' });
      }
    },
    [setSessions, showToast]
  );

  const send = useCallback(
    (text, sessionId, images = []) => {
      const trimmed = String(text ?? '').trim();
      if (!trimmed || isStreamingRef.current) return;
      if (!sessionsRef.current.some((session) => session.id === sessionId)) return;
      modelRef.current = settingsRef.current?.model ?? null;
      const blocks = [{ type: 'text', text: trimmed }, ...imagesToBlocks(images)];
      sendBlocks(sessionId, blocks);
    },
    [sendBlocks]
  );

  const stop = useCallback(() => {
    // 不立刻清消息流：等 done{stop_reason:"interrupted"} 收尾（spec §10.2）。
    client
      .interrupt()
      .catch((error) => showToast?.({ body: `停止失败：${errorMessage(error)}`, type: 'error' }));
  }, [showToast]);

  const respondPermission = useCallback(
    (permissionId, decision) => {
      if (!permissionId) return;
      // 卡片状态只由 permission_resolved 事件改（spec §10.3）；返回 false 说明请求已失效。
      client
        .respond(permissionId, decisionToWire(decision))
        .then((closed) => {
          if (closed === false) {
            showToast?.({ body: '该权限请求已失效', type: 'error' });
          }
        })
        .catch((error) => {
          showToast?.({ body: `权限提交失败：${errorMessage(error)}`, type: 'error' });
        });
    },
    [showToast]
  );

  const answerQuestion = useCallback(
    async (questionId, answer) => {
      const trimmed = String(answer ?? '').trim();
      if (!questionId || !trimmed) return false;
      try {
        const ok = await client.questionRespond(questionId, trimmed);
        if (!ok) {
          showToast?.({ body: '这道提问已失效', type: 'error' });
          return false;
        }
        // 本地把该块标记已答 + 追加一条本地用户记录（仅界面，不写库；spec §10.4）。
        const stamp = nowIso();
        setSessions((prev) =>
          prev.map((session) => {
            let marked = false;
            const messages = session.messages.map((message) => {
              let messageChanged = false;
              const blocks = message.blocks.map((block) => {
                if (
                  block.type !== 'question' ||
                  block.id !== questionId ||
                  block.answer != null
                ) {
                  return block;
                }
                messageChanged = true;
                return { ...block, answer: trimmed };
              });
              if (!messageChanged) return message;
              marked = true;
              return { ...message, blocks };
            });
            if (!marked) return session;
            return {
              ...session,
              updatedAt: stamp,
              messages: [
                ...messages,
                {
                  id: `local-user-${Date.now()}`,
                  role: 'user',
                  createdAt: stamp,
                  blocks: [{ type: 'text', text: trimmed }],
                },
              ],
            };
          })
        );
        return true;
      } catch (error) {
        showToast?.({ body: `回答提交失败：${errorMessage(error)}`, type: 'error' });
        return false;
      }
    },
    [setSessions, showToast]
  );

  const retryLast = useCallback(
    async (sessionId) => {
      if (isStreamingRef.current) return;
      try {
        const blocks = await client.retryLast(sessionId);
        if (!Array.isArray(blocks) || blocks.length === 0) {
          showToast?.({ body: '没有可重试的轮次' });
          return;
        }
        // Rust 已截断库里最后一轮；本地把最后一条用户消息及其后的内容去掉，再按普通发送重放。
        const before = projectionsRef.current.get(sessionId);
        if (before) {
          projectionsRef.current.set(sessionId, {
            ...before,
            messages: removeFromLastUser(before.messages),
          });
        }
        setSessions((prev) =>
          prev.map((session) =>
            session.id === sessionId
              ? { ...session, messages: removeFromLastUser(session.messages) }
              : session
          )
        );
        modelRef.current = settingsRef.current?.model ?? null;
        await sendBlocks(sessionId, blocks);
      } catch (error) {
        showToast?.({ body: `重试失败：${errorMessage(error)}`, type: 'error' });
      }
    },
    [sendBlocks, showToast]
  );

  return {
    send,
    stop,
    isStreaming,
    streamingSessionId,
    respondPermission,
    answerQuestion,
    retryLast,
    hydrateSession,
    forgetSession,
  };
}

export default useLfttdoAgent;
