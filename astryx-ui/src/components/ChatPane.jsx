import { ChatMessageList, ChatSystemMessage } from '@astryxdesign/core/Chat';
import { EmptyState } from '@astryxdesign/core/EmptyState';
import { Timestamp } from '@astryxdesign/core/Timestamp';
import { MessageTurn } from './MessageTurn';

/**
 * 消息流容器。只返回 ChatMessageList：滚动、底栏、贴底都由父级
 * ChatLayout 提供，token 与上下文占用条由 StatusBar 渲染。
 */

function toDayKey(value) {
  const date = new Date(value);

  if (Number.isNaN(date.getTime())) {
    return 'unknown';
  }

  return `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()}`;
}

export function ChatPane({
  session,
  isStreaming,
  settings,
  // 保留参数：usage / contextBudget 由 StatusBar 渲染，这里只接收。
  usage,
  contextBudget,
  onPermission,
  onQuestionAnswer,
  onReopenQuestion,
  hidePendingQuestion,
  onRetry,
}) {
  const messages = Array.isArray(session?.messages) ? session.messages : [];
  const rows = [];
  let currentDay = null;

  messages.forEach((message, index) => {
    const nextDay = toDayKey(message.createdAt);

    if (nextDay !== currentDay) {
      rows.push(
        <ChatSystemMessage
          key={`day-${message.id ?? index}`}
          variant="divider"
        >
          <Timestamp value={message.createdAt} format="date" />
        </ChatSystemMessage>
      );
      currentDay = nextDay;
    }

    rows.push(
      <MessageTurn
        key={message.id ?? `message-${index}`}
        turn={message}
        isLast={index === messages.length - 1}
        isStreaming={isStreaming && index === messages.length - 1}
        settings={settings}
        onPermission={onPermission}
        onQuestionAnswer={onQuestionAnswer}
        onReopenQuestion={onReopenQuestion}
        hidePendingQuestion={hidePendingQuestion}
        onRetry={onRetry}
      />
    );
  });

  return (
    <ChatMessageList
      density="spacious"
      gap={3}
      isStreaming={isStreaming}
      emptyState={
        <EmptyState
          title="开始一段新对话"
          description="有什么都可以聊，也能拖张图进来。"
        />
      }
    >
      {rows}
    </ChatMessageList>
  );
}

export default ChatPane;
