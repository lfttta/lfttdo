import { useMemo, useRef } from 'react';
import {
  ChatComposer,
  ChatComposerDrawer,
  ChatComposerInput,
} from '@astryxdesign/core/Chat';
import { Button } from '@astryxdesign/core/Button';
import { DropdownMenu } from '@astryxdesign/core/DropdownMenu';
import { IconButton } from '@astryxdesign/core/IconButton';
import { HStack } from '@astryxdesign/core/HStack';
import { VStack } from '@astryxdesign/core/VStack';
import { Text } from '@astryxdesign/core/Text';
import { ProgressBar } from '@astryxdesign/core/ProgressBar';
import { TypeaheadItem, createStaticSource } from '@astryxdesign/core/Typeahead';
import {
  AtSymbolIcon,
  CpuChipIcon,
  PhotoIcon,
  SparklesIcon,
  XMarkIcon,
} from '@heroicons/react/24/outline';

const PLACEHOLDER = '说点什么…（Enter 发送，Shift+Enter 换行）';

/**
 * 输入坞：ChatComposer + ChatComposerInput。
 * - drawer 显示待发送图片缩略图；
 * - headerActions 放「添加图片」与「@ 文件」；
 * - headerContext 显示上下文占用条；
 * - footerActions 显示只读的模型标签（换模型在设置面板）。
 *
 * `/` 菜单只列启用的技能；`@` 菜单使用 fileMentions 的相对路径，最多 20 条。
 */
export function Composer({
  value,
  onChange,
  onSend,
  onStop,
  isStreaming,
  pendingImages,
  onAddFiles,
  onRemoveImage,
  skills,
  files,
  models,
  modelName,
  onModelChange,
  usage,
  contextBudget,
}) {
  const fileInputRef = useRef(null);
  const inputHandleRef = useRef(null);

  const skillSource = useMemo(() => {
    const items = (skills ?? [])
      .filter((skill) => skill.enabled)
      .map((skill) => ({
        id: skill.name,
        label: `/${skill.name}`,
        auxiliaryData: {
          description: skill.description,
          argumentHint: skill.argumentHint,
        },
      }));
    return createStaticSource(items);
  }, [skills]);

  const fileSource = useMemo(() => {
    const items = (files ?? [])
      .slice(0, 20)
      .map((path) => ({ id: path, label: path }));
    return createStaticSource(items);
  }, [files]);

  const triggers = useMemo(
    () => [
      {
        character: '/',
        menuLabel: '技能快捷菜单',
        emptySearchText: '没有匹配的技能',
        searchSource: skillSource,
        renderItem: (item) => (
          <TypeaheadItem
            item={item}
            description={[item.auxiliaryData?.description, item.auxiliaryData?.argumentHint]
              .filter(Boolean)
              .join(' ')}
          />
        ),
        onSelect: (item) => `使用「${item.id}」技能：`,
      },
      {
        character: '@',
        menuLabel: '文件提及菜单',
        emptySearchText: '没有匹配的文件',
        searchSource: fileSource,
        renderItem: (item) => <TypeaheadItem item={item} />,
        onSelect: (item) => `@${item.id}`,
      },
    ],
    [skillSource, fileSource]
  );

  const images = pendingImages ?? [];
  const currentUsage = usage ?? { inputTokens: 0, outputTokens: 0, contextTokens: 0 };
  const budget = Math.max(1, Number(contextBudget) || 1);
  const ratio = currentUsage.contextTokens / budget;
  const percent = Math.min(100, Math.round(ratio * 100));

  const handleSubmit = (text) => {
    const trimmed = String(text ?? '').trim();
    if (!trimmed) return;
    onSend(trimmed);
  };

  const handleFileInput = (event) => {
    const selected = Array.from(event.target.files ?? []);
    if (selected.length > 0) onAddFiles(selected);
    event.target.value = '';
  };

  // 把触发字符送进输入框并聚焦，再补发一次原生 input 事件，
  // 让 ChatComposerInput 的 trigger 菜单立即按字符打开。
  // trigger 只在文本开头或空白之后才会激活菜单（useTriggerMenu 的
  // isTriggerBoundary）；草稿末尾是普通字符时先补一个空格。
  // 走组件自己的 insertText 而不是模拟按键：光标落在文本节点里，
  // useTriggerMenu 才读得到触发字符。
  const insertTrigger = (character) => {
    const input = inputHandleRef.current;
    if (!input) return;
    input.focus();
    const currentValue = input.getValue();
    const needsSeparator =
      currentValue.length > 0 && !/[ \t\n]$/.test(currentValue);
    input.insertText(`${needsSeparator ? ' ' : ''}${character}`);
    const editable = document.activeElement;
    if (editable && editable.getAttribute?.('contenteditable') === 'true') {
      editable.dispatchEvent(new Event('input', { bubbles: true }));
    }
  };

  const handleMentionClick = () => insertTrigger('@');
  const handleSkillClick = () => insertTrigger('/');

  const drawer =
    images.length > 0 ? (
      <ChatComposerDrawer count={images.length} label="图片">
        <HStack gap={2} wrap="wrap">
          {images.map((image) => (
            <VStack key={image.id} gap={1} width="4.5rem">
              <img
                src={image.url}
                alt={image.name}
                style={{
                  width: '100%',
                  height: '4.5rem',
                  objectFit: 'cover',
                  borderRadius: 'var(--radius-element)',
                }}
              />
              <IconButton
                label={`移除 ${image.name}`}
                icon={<XMarkIcon />}
                size="sm"
                variant="ghost"
                onClick={() => onRemoveImage(image.id)}
              />
            </VStack>
          ))}
        </HStack>
      </ChatComposerDrawer>
    ) : undefined;

  const headerActions = (
    <HStack gap={1} align="center">
      <Button
        label="添加图片"
        variant="ghost"
        size="sm"
        icon={<PhotoIcon />}
        onClick={() => fileInputRef.current?.click()}
      />
      <Button
        label="@ 文件"
        variant="ghost"
        size="sm"
        icon={<AtSymbolIcon />}
        onClick={handleMentionClick}
      />
      <Button
        label="/ 技能"
        variant="ghost"
        size="sm"
        icon={<SparklesIcon />}
        onClick={handleSkillClick}
      />
      <input
        ref={fileInputRef}
        type="file"
        accept="image/*"
        multiple
        hidden
        onChange={handleFileInput}
      />
    </HStack>
  );

  const headerContext = (
    <HStack gap={2} align="center">
      <Text type="supporting" color="secondary">
        上下文
      </Text>
      <VStack width="7.5rem">
        <ProgressBar
          label="上下文占用"
          isLabelHidden
          value={Math.min(currentUsage.contextTokens, budget)}
          max={budget}
          variant={ratio > 0.85 ? 'error' : 'accent'}
        />
      </VStack>
      <Text type="supporting" color="secondary">
        {percent}%
      </Text>
    </HStack>
  );

  const modelOptions = Array.from(
    new Set([modelName, ...(models ?? [])].filter(Boolean))
  );
  const footerActions = (
    <DropdownMenu
      button={{
        label: modelName,
        variant: 'ghost',
        size: 'sm',
        icon: <CpuChipIcon width="1em" height="1em" />,
        isDisabled: isStreaming,
      }}
      items={modelOptions.map((model) => ({
        id: model,
        label: model,
        endContent: model === modelName ? '当前' : undefined,
        onClick: () => onModelChange?.(model),
      }))}
      placement="above"
      alignment="start"
      menuWidth={240}
    />
  );

  return (
    <ChatComposer
      value={value}
      onChange={onChange}
      onSubmit={handleSubmit}
      onStop={onStop}
      isStopShown={isStreaming}
      elevation="low"
      placeholder={PLACEHOLDER}
      input={
        <ChatComposerInput
          handleRef={inputHandleRef}
          triggers={triggers}
          onFiles={onAddFiles}
          hasHistory
          maxRows={6}
          label="消息输入"
          placeholder={PLACEHOLDER}
          enterKeyHint="send"
        />
      }
      drawer={drawer}
      headerActions={headerActions}
      headerContext={headerContext}
      footerActions={footerActions}
    />
  );
}

export default Composer;
