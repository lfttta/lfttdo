import { useId, useState } from 'react';
import { AspectRatio } from '@astryxdesign/core/AspectRatio';
import { Button } from '@astryxdesign/core/Button';
import { Card } from '@astryxdesign/core/Card';
import { ChatToolCalls } from '@astryxdesign/core/Chat';
import { CodeBlock } from '@astryxdesign/core/CodeBlock';
import { Grid } from '@astryxdesign/core/Grid';
import { HStack } from '@astryxdesign/core/HStack';
import { Icon } from '@astryxdesign/core/Icon';
import { RadioList, RadioListItem } from '@astryxdesign/core/RadioList';
import { Text } from '@astryxdesign/core/Text';
import { TextInput } from '@astryxdesign/core/TextInput';
import { useToast } from '@astryxdesign/core/Toast';
import { Token } from '@astryxdesign/core/Token';
import { VStack } from '@astryxdesign/core/VStack';
import { DocumentTextIcon, FolderIcon, QuestionMarkCircleIcon } from '@heroicons/react/24/outline';

/**
 * 工具调用、权限、提问、图片与文件产物的统一渲染。
 *
 * 数据形状来自 docs/agent-ui-contract.md §4.1：
 * - tool 块没有 id，React key 用 useId + 数组下标组合。
 * - permission / question 的 id 全局唯一，直接作为 key。
 * - text / code 块由 MessageTurn 渲染，这里跳过。
 */

const RISK_META = {
  read: { label: '只读', color: 'gray' },
  write: { label: '写入', color: 'orange' },
  exec: { label: '执行', color: 'red' },
};

const DECISION_META = {
  once: { label: '已放行', color: 'green', text: '已放行（仅本次）。' },
  session: { label: '已放行', color: 'green', text: '已放行（本会话内）。' },
  deny: { label: '已拒绝', color: 'red', text: '已拒绝，不会执行这一步。' },
};

function formatArgs(args) {
  try {
    return JSON.stringify(args ?? {}, null, 2);
  } catch {
    return String(args);
  }
}

function PermissionCard({ permission, onDecide, disabled = false }) {
  const risk = RISK_META[permission.risk] ?? RISK_META.read;
  const resolved = permission.decision ? DECISION_META[permission.decision] : null;

  return (
    <Card elevation="low" padding={3}>
      <VStack gap={2}>
        <HStack gap={2} align="center" wrap="wrap">
          <Text type="label">需要你的许可</Text>
          <Token label={risk.label} color={risk.color} size="sm" />
        </HStack>

        <Text type="code">{permission.tool}</Text>

        <CodeBlock
          code={formatArgs(permission.args)}
          language="json"
          size="sm"
          hasCopyButton
          maxHeight="12rem"
        />

        {resolved ? (
          <HStack gap={2} align="center" wrap="wrap">
            <Token label={resolved.label} color={resolved.color} size="sm" />
            <Text type="supporting" color="secondary">
              {resolved.text}
            </Text>
          </HStack>
        ) : (
          <HStack gap={2} wrap="wrap">
            <Button
              label="仅本次"
              variant="primary"
              size="sm"
              isDisabled={disabled}
              onClick={() => onDecide?.(permission.id, 'once')}
            />
            <Button
              label="本会话内都允许"
              variant="secondary"
              size="sm"
              isDisabled={disabled}
              onClick={() => onDecide?.(permission.id, 'session')}
            />
            <Button
              label="拒绝"
              variant="destructive"
              size="sm"
              isDisabled={disabled}
              onClick={() => onDecide?.(permission.id, 'deny')}
            />
          </HStack>
        )}
      </VStack>
    </Card>
  );
}

function QuestionCard({ question, onAnswer, disabled = false }) {
  const [selected, setSelected] = useState('');
  const [custom, setCustom] = useState('');
  const options = Array.isArray(question.options) ? question.options : [];
  const isAnswered = question.answer !== null && question.answer !== undefined;
  const answer = custom.trim() || selected;

  return (
    <Card elevation="low" padding={3}>
      <VStack gap={2}>
        <Text type="label">模型提问</Text>
        <Text as="p">{question.question}</Text>

        {isAnswered ? (
          <HStack gap={2} align="center" wrap="wrap">
            <Token label="已回答" color="green" size="sm" />
            <Text type="supporting" color="secondary">
              {question.answer}
            </Text>
          </HStack>
        ) : (
          <>
            <RadioList
              label="选项"
              isLabelHidden
              value={selected}
              onChange={(value) => {
                setSelected(value);
                setCustom('');
              }}
              size="sm"
            >
              {options.map((option) => (
                <RadioListItem key={option} label={option} value={option} />
              ))}
            </RadioList>

            <TextInput
              label="自定义回答"
              placeholder="也可自由输入回答"
              value={custom}
              onChange={setCustom}
              size="sm"
            />

            <HStack>
              <Button
                label="提交"
                variant="primary"
                size="sm"
                isDisabled={!answer || disabled}
                onClick={() => onAnswer?.(question.id, answer)}
              />
            </HStack>
          </>
        )}
      </VStack>
    </Card>
  );
}

function PendingQuestionNote({ question, onReopen, disabled = false }) {
  return (
    <Card elevation="none" padding={2}>
      <HStack gap={2} align="center" wrap="wrap">
        <Icon icon={QuestionMarkCircleIcon} size="md" color="secondary" />
        <VStack gap={0}>
          <Text type="label">模型提问 · 待回答</Text>
          <Text type="supporting" color="secondary">
            {question.question}
          </Text>
        </VStack>
        <Button
          label="继续回答"
          variant="secondary"
          size="sm"
          isDisabled={disabled}
          onClick={() => onReopen?.(question.id)}
        />
      </HStack>
    </Card>
  );
}

function ImageCard({ image, hasCaption = true }) {
  return (
    <VStack gap={1}>
      <AspectRatio ratio={16 / 10} fit="cover">
        <img
          src={image.src}
          alt={image.alt || '工具返回的图片'}
          style={{
            width: '100%',
            height: '100%',
            objectFit: 'cover',
            borderRadius: 'var(--radius-element)',
          }}
        />
      </AspectRatio>
      {hasCaption ? (
        <Text type="supporting" color="secondary">
          工具返回的图片
        </Text>
      ) : null}
    </VStack>
  );
}

function ImageGroup({ images }) {
  const shown = images.slice(0, 4);

  if (shown.length === 1) {
    return (
      <VStack maxWidth="20rem" width="100%">
        <ImageCard image={shown[0]} />
      </VStack>
    );
  }

  return (
    <VStack gap={1}>
      <Grid columns={2} gap={2}>
        {shown.map((image, index) => (
          <ImageCard
            key={`${image.src}-${index}`}
            image={image}
            hasCaption={false}
          />
        ))}
      </Grid>
      <Text type="supporting" color="secondary">
        {images.length > shown.length
          ? '工具返回的图片（仅显示前 4 张）'
          : '工具返回的图片'}
      </Text>
    </VStack>
  );
}

function FileCard({ file, onReveal }) {
  return (
    <Card elevation="none" padding={2}>
      <HStack gap={2} align="center" wrap="wrap">
        <Icon icon={DocumentTextIcon} size="md" color="secondary" />
        <VStack gap={0}>
          <Text type="label">{file.name}</Text>
          <Token
            label={file.path}
            size="sm"
            icon={<Icon icon={FolderIcon} size="xsm" />}
            onClick={onReveal}
          />
        </VStack>
        <Button
          label="打开位置"
          variant="ghost"
          size="sm"
          onClick={onReveal}
        />
      </HStack>
    </Card>
  );
}

// ChatToolCalls 的 duration 是展示字符串（例如 "1.2s"），
// 而 ToolBlock.duration 按契约是毫秒数，映射时在这里转换。
function formatDuration(duration) {
  const value = Number(duration);
  if (!Number.isFinite(value) || value <= 0) return undefined;
  return value >= 1000 ? `${(value / 1000).toFixed(1)}s` : `${Math.round(value)}ms`;
}

function buildToolCalls(run, idPrefix, runStart) {
  return run.map((tool, index) => ({
    key: `${idPrefix}-tool-${runStart}-${index}`,
    name: tool.name,
    status: tool.status,
    target: tool.target,
    duration: formatDuration(tool.duration),
    additions: tool.additions,
    deletions: tool.deletions,
    errorMessage: tool.errorMessage || undefined,
    resultDetail: tool.result ? (
      <CodeBlock
        code={tool.result}
        language="text"
        size="sm"
        hasCopyButton
        maxHeight="16rem"
      />
    ) : undefined,
  }));
}

export function ToolCards({
  blocks,
  onPermission,
  onQuestionAnswer,
  onReopenQuestion,
  disabled = false,
  hidePendingQuestion = false,
}) {
  const showToast = useToast();
  const idPrefix = useId();

  if (!Array.isArray(blocks) || blocks.length === 0) {
    return null;
  }

  const handleReveal = () => showToast({ body: '已在资源管理器中显示' });

  const items = [];
  let cursor = 0;

  while (cursor < blocks.length) {
    const block = blocks[cursor];

    if (!block) {
      cursor += 1;
      continue;
    }

    if (block.type === 'text' || block.type === 'code') {
      cursor += 1;
      continue;
    }

    if (block.type === 'tool') {
      const runStart = cursor;
      const run = [];

      while (cursor < blocks.length && blocks[cursor]?.type === 'tool') {
        run.push(blocks[cursor]);
        cursor += 1;
      }

      items.push(
        <ChatToolCalls
          key={`tool-run-${runStart}`}
          calls={buildToolCalls(run, idPrefix, runStart)}
        />
      );
      continue;
    }

    if (block.type === 'image') {
      const groupStart = cursor;
      const images = [];

      while (cursor < blocks.length && blocks[cursor]?.type === 'image') {
        images.push(blocks[cursor]);
        cursor += 1;
      }

      items.push(
        <ImageGroup key={`image-group-${groupStart}`} images={images} />
      );
      continue;
    }

    if (block.type === 'permission') {
      items.push(
        <PermissionCard
          key={`permission-${block.id ?? cursor}`}
          permission={block}
          onDecide={onPermission}
          disabled={disabled}
        />
      );
      cursor += 1;
      continue;
    }

    if (block.type === 'question') {
      const isAnswered = block.answer !== null && block.answer !== undefined;
      // 待回答的提问由居中弹窗承载；弹窗被「稍后回答」收起时，
      // 消息流只留一个紧凑的回退入口，点击可重新打开弹窗。
      if (!isAnswered) {
        if (!hidePendingQuestion) {
          items.push(
            <PendingQuestionNote
              key={`question-${block.id ?? cursor}`}
              question={block}
              onReopen={onReopenQuestion}
              disabled={disabled}
            />
          );
        }
        cursor += 1;
        continue;
      }
      items.push(
        <QuestionCard
          key={`question-${block.id ?? cursor}`}
          question={block}
          onAnswer={onQuestionAnswer}
          disabled={disabled}
        />
      );
      cursor += 1;
      continue;
    }

    if (block.type === 'file') {
      items.push(
        <FileCard
          key={`file-${block.path}-${cursor}`}
          file={block}
          onReveal={handleReveal}
        />
      );
      cursor += 1;
      continue;
    }

    cursor += 1;
  }

  if (items.length === 0) {
    return null;
  }

  return <VStack gap={2}>{items}</VStack>;
}

export default ToolCards;
