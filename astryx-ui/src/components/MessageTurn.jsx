import { Avatar } from '@astryxdesign/core/Avatar';
import { Banner } from '@astryxdesign/core/Banner';
import { Button } from '@astryxdesign/core/Button';
import {
  ChatMessage,
  ChatMessageBubble,
  ChatMessageMetadata,
} from '@astryxdesign/core/Chat';
import { CodeBlock } from '@astryxdesign/core/CodeBlock';
import { Collapsible } from '@astryxdesign/core/Collapsible';
import { HStack } from '@astryxdesign/core/HStack';
import { Markdown } from '@astryxdesign/core/Markdown';
import { Text } from '@astryxdesign/core/Text';
import { Timestamp } from '@astryxdesign/core/Timestamp';
import { useToast } from '@astryxdesign/core/Toast';
import { Token } from '@astryxdesign/core/Token';
import { VStack } from '@astryxdesign/core/VStack';
import {
  ArrowPathIcon,
  DocumentDuplicateIcon,
  FolderIcon,
  PhotoIcon,
} from '@heroicons/react/24/outline';
import { ToolCards } from './ToolCards';

/**
 * 单条消息。用户消息在右侧 filled 气泡；助手消息在左侧 ghost 气泡，
 * 带头像、模型名与时间元数据。工具类块交给 ToolCards。
 */

const TOOL_BLOCK_TYPES = new Set([
  'tool',
  'permission',
  'question',
  'image',
  'file',
]);

const NOTICE_STATUSES = new Set(['info', 'warning', 'error', 'success']);

function isToolBlock(block) {
  return block != null && TOOL_BLOCK_TYPES.has(block.type);
}

function ThinkingBlock({ block }) {
  const rawDuration = typeof block.duration === 'number' ? block.duration : null;
  const seconds =
    rawDuration !== null && rawDuration > 0
      ? Math.max(1, Math.round(rawDuration > 100 ? rawDuration / 1000 : rawDuration))
      : null;

  return (
    <Collapsible
      defaultIsOpen={false}
      trigger={
        <Text type="supporting" color="secondary">
          {seconds ? `思考过程 · ${seconds}s` : '思考过程'}
        </Text>
      }
    >
      <Text type="supporting" color="secondary" display="block">
        {block.text}
      </Text>
    </Collapsible>
  );
}

function NoticeBlock({ block }) {
  const rawStatus = block.status ?? block.level;
  const status = NOTICE_STATUSES.has(rawStatus) ? rawStatus : 'info';

  return (
    <Banner
      container="section"
      status={status}
      title={block.title || (status === 'error' ? '出错了' : '通知')}
      description={block.text || block.description}
    />
  );
}

function buildUserContent(blocks) {
  const items = [];
  let index = 0;

  while (index < blocks.length) {
    const block = blocks[index];

    if (!block) {
      index += 1;
      continue;
    }

    if (block.type === 'text') {
      items.push(
        <Markdown key={`text-${index}`} headingLevelStart={3} contentWidth="100%">
          {block.text}
        </Markdown>
      );
      index += 1;
      continue;
    }

    if (block.type === 'code') {
      items.push(
        <CodeBlock
          key={`code-${index}`}
          code={block.code}
          language={block.language || 'plaintext'}
          title={block.title}
          hasCopyButton
          size="sm"
          maxHeight="16rem"
        />
      );
      index += 1;
      continue;
    }

    if (block.type === 'image') {
      const groupStart = index;
      const attachments = [];

      while (index < blocks.length && blocks[index]?.type === 'image') {
        attachments.push(blocks[index]);
        index += 1;
      }

      items.push(
        <HStack key={`attachments-${groupStart}`} gap={2} wrap="wrap">
          {attachments.map((image, offset) => (
            <Token
              key={`attachment-${groupStart}-${offset}`}
              label={image.alt || '图片附件'}
              icon={<PhotoIcon />}
              size="sm"
            />
          ))}
        </HStack>
      );
      continue;
    }

    if (block.type === 'file') {
      items.push(
        <Token
          key={`file-${index}`}
          label={block.path}
          icon={<FolderIcon />}
          size="sm"
        />
      );
      index += 1;
      continue;
    }

    index += 1;
  }

  return items;
}

function buildAssistantContent(blocks, {
  isStreaming,
  isStopped,
  onPermission,
  onQuestionAnswer,
  onReopenQuestion,
  hidePendingQuestion,
}) {
  const items = [];
  let lastTextIndex = -1;

  blocks.forEach((block, index) => {
    if (block?.type === 'text') {
      lastTextIndex = index;
    }
  });

  let index = 0;

  while (index < blocks.length) {
    const block = blocks[index];

    if (!block) {
      index += 1;
      continue;
    }

    if (block.type === 'text') {
      items.push(
        <Markdown
          key={`text-${index}`}
          headingLevelStart={3}
          contentWidth="100%"
          isStreaming={Boolean(isStreaming && index === lastTextIndex)}
        >
          {block.text}
        </Markdown>
      );
      index += 1;
      continue;
    }

    if (block.type === 'code') {
      items.push(
        <CodeBlock
          key={`code-${index}`}
          code={block.code}
          language={block.language || 'plaintext'}
          title={block.title}
          hasCopyButton
          size="sm"
          maxHeight="16rem"
        />
      );
      index += 1;
      continue;
    }

    if (block.type === 'thinking') {
      items.push(<ThinkingBlock key={`thinking-${index}`} block={block} />);
      index += 1;
      continue;
    }

    if (block.type === 'notice') {
      items.push(<NoticeBlock key={`notice-${index}`} block={block} />);
      index += 1;
      continue;
    }

    if (isToolBlock(block)) {
      const runStart = index;
      const segment = [];

      while (index < blocks.length && isToolBlock(blocks[index])) {
        segment.push(blocks[index]);
        index += 1;
      }

      items.push(
        <ToolCards
          key={`tools-${runStart}`}
          blocks={segment}
          onPermission={onPermission}
          onQuestionAnswer={onQuestionAnswer}
          onReopenQuestion={onReopenQuestion}
          disabled={Boolean(isStopped)}
          hidePendingQuestion={Boolean(hidePendingQuestion)}
        />
      );
      continue;
    }

    index += 1;
  }

  return items;
}

export function MessageTurn({
  turn,
  isLast,
  isStreaming,
  settings,
  onPermission,
  onQuestionAnswer,
  onReopenQuestion,
  hidePendingQuestion,
  onRetry,
}) {
  const showToast = useToast();
  const blocks = Array.isArray(turn.blocks) ? turn.blocks : [];
  const isUser = turn.role === 'user';
  const textBlocks = blocks.filter(
    (block) => block?.type === 'text' && block.text
  );
  const canCopy = !isUser && textBlocks.length > 0;
  const canRetry = !isUser && isLast && !isStreaming && !turn.stopped;
  const timestamp = <Timestamp value={turn.createdAt} format="time" />;
  const modelName = turn.model ?? settings?.model;

  async function handleCopy() {
    try {
      const text = textBlocks.map((block) => block.text).join('\n\n');
      await navigator.clipboard.writeText(text);
      showToast({ body: '已复制' });
    } catch {
      showToast({ body: '复制失败', type: 'error' });
    }
  }

  if (isUser) {
    return (
      <ChatMessage sender="user">
        <ChatMessageBubble
          metadata={
            <ChatMessageMetadata timestamp={timestamp} status="sent" />
          }
        >
          <VStack gap={2}>
            {buildUserContent(blocks)}
            {turn.stopped ? (
              <Text type="supporting" color="secondary">
                已停止
              </Text>
            ) : null}
          </VStack>
        </ChatMessageBubble>
      </ChatMessage>
    );
  }

  return (
    <ChatMessage sender="assistant" avatar={<Avatar name="LFTTDO" size="sm" />}>
      <ChatMessageBubble
        variant="ghost"
        name="LFTTDO"
        metadata={
          <ChatMessageMetadata
            timestamp={timestamp}
            footer={
              modelName ? (
                <Text type="supporting" color="secondary">
                  {modelName}
                </Text>
              ) : undefined
            }
          />
        }
      >
        <VStack gap={2}>
          {buildAssistantContent(blocks, {
            isStreaming,
            isStopped: Boolean(turn.stopped),
            onPermission,
            onQuestionAnswer,
            onReopenQuestion,
            hidePendingQuestion,
          })}
          {turn.stopped ? (
            <Text type="supporting" color="secondary">
              已停止
            </Text>
          ) : null}
        </VStack>
      </ChatMessageBubble>

      {canCopy || canRetry ? (
        <HStack gap={1} align="center" wrap="wrap">
          {canCopy ? (
            <Button
              label="复制"
              variant="ghost"
              size="sm"
              icon={<DocumentDuplicateIcon />}
              onClick={handleCopy}
            />
          ) : null}
          {canRetry ? (
            <Button
              label="重试这一轮"
              variant="ghost"
              size="sm"
              icon={<ArrowPathIcon />}
              onClick={onRetry}
            />
          ) : null}
        </HStack>
      ) : null}
    </ChatMessage>
  );
}

export default MessageTurn;
