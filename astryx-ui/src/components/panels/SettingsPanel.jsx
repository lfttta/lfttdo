import { useEffect, useRef, useState } from 'react';
import { Drawer, DrawerHeader } from '@astryxdesign/core/Drawer';
import {
  Layout,
  LayoutContent,
  LayoutFooter,
  LayoutPanel,
  StackItem,
} from '@astryxdesign/core/Layout';
import { VStack } from '@astryxdesign/core/VStack';
import { HStack } from '@astryxdesign/core/HStack';
import { Card } from '@astryxdesign/core/Card';
import { Button } from '@astryxdesign/core/Button';
import { IconButton } from '@astryxdesign/core/IconButton';
import { Heading } from '@astryxdesign/core/Heading';
import { Text } from '@astryxdesign/core/Text';
import { TextInput } from '@astryxdesign/core/TextInput';
import { TextArea } from '@astryxdesign/core/TextArea';
import { Switch } from '@astryxdesign/core/Switch';
import { Selector } from '@astryxdesign/core/Selector';
import { Slider } from '@astryxdesign/core/Slider';
import { RadioList, RadioListItem } from '@astryxdesign/core/RadioList';
import { List, ListItem } from '@astryxdesign/core/List';
import { Token } from '@astryxdesign/core/Token';
import { Banner } from '@astryxdesign/core/Banner';
import { useToast } from '@astryxdesign/core/Toast';
import {
  FolderIcon,
  PencilIcon,
  TrashIcon,
  XMarkIcon,
} from '@heroicons/react/24/outline';

const SECTIONS = [
  { id: 'model', label: '模型' },
  { id: 'appearance', label: '外观' },
  { id: 'permissions', label: '权限' },
  { id: 'persona', label: '人设' },
  { id: 'memories', label: '长期记忆' },
  { id: 'roots', label: '文件访问' },
  { id: 'storage', label: '储存' },
];

const KIND_LABEL = { fact: '事实', preference: '偏好', todo: '待办' };
const KIND_COLOR = { fact: 'blue', preference: 'green', todo: 'orange' };

const KIND_OPTIONS = [
  { value: 'fact', label: '事实' },
  { value: 'preference', label: '偏好' },
  { value: 'todo', label: '待办' },
];

// 强调色是传给 App runtime theme 的数据值，不是组件内联样式。
const ACCENT_OPTIONS = [
  { value: '#7c8cff', label: '靛蓝' },
  { value: '#22d3ee', label: '青色' },
  { value: '#f472b6', label: '粉色' },
  { value: '#34d399', label: '绿色' },
];

const PERMISSION_LABEL = {
  rules: '规则',
  ask: '全部询问',
  trusted: '信任模式',
};

const DUE_DATE_PATTERN = /^\d{4}-\d{2}-\d{2}$/;

function kindLabel(kind) {
  return KIND_LABEL[kind] ?? kind;
}

function kindColor(kind) {
  return KIND_COLOR[kind] ?? 'gray';
}

function parseDueAt(rawValue) {
  const value = rawValue.trim();
  if (!value) {
    return { ok: true, value: null };
  }
  if (!DUE_DATE_PATTERN.test(value)) {
    return { ok: false, value: null };
  }
  return { ok: true, value };
}

function SectionIntro({ title, description }) {
  return (
    <VStack gap={1}>
      <Heading level={3}>{title}</Heading>
      <Text type="supporting" color="secondary">
        {description}
      </Text>
    </VStack>
  );
}

export default function SettingsPanel({
  isOpen,
  onClose,
  settings,
  onSettingChange,
  memories = [],
  onAddMemory,
  onUpdateMemory,
  onDeleteMemory,
  onClearMemories,
  onImportPersona,
  onExportPersona,
}) {
  const showToast = useToast();
  const contentRef = useRef(null);

  const [activeSection, setActiveSection] = useState('model');
  const [apiKey, setApiKey] = useState('');
  const [confirmClearKey, setConfirmClearKey] = useState(false);
  const [newRoot, setNewRoot] = useState('');
  const [newMemory, setNewMemory] = useState({
    kind: 'fact',
    body: '',
    dueAt: '',
  });
  const [editingId, setEditingId] = useState(null);
  const [editDraft, setEditDraft] = useState({
    kind: 'fact',
    body: '',
    dueAt: '',
  });
  const [confirmClearMemories, setConfirmClearMemories] = useState(false);

  const editingMemory = memories.find((memory) => memory.id === editingId) ?? null;

  useEffect(() => {
    if (!isOpen) {
      setApiKey('');
      setConfirmClearKey(false);
      setEditingId(null);
      setConfirmClearMemories(false);
    }
  }, [isOpen]);

  function handleClose() {
    setApiKey('');
    setConfirmClearKey(false);
    setEditingId(null);
    setConfirmClearMemories(false);
    onClose();
  }

  function handleDrawerOpenChange(open) {
    if (!open) {
      handleClose();
    }
  }

  function selectSection(sectionId) {
    setActiveSection(sectionId);
    contentRef.current?.scrollTo({ top: 0 });
  }

  function handleSaveKey() {
    const key = apiKey.trim();
    if (!key) {
      return;
    }
    onSettingChange('hasApiKey', true);
    setApiKey('');
    setConfirmClearKey(false);
    showToast({ body: '已保存 API Key（演示）' });
  }

  function handleClearKey() {
    onSettingChange('hasApiKey', false);
    setApiKey('');
    setConfirmClearKey(false);
    showToast({ body: '已清除 API Key' });
  }

  function handleAddRoot() {
    const root = newRoot.trim();
    if (!root) {
      return;
    }
    if (settings.roots.includes(root)) {
      showToast({ body: '这个目录已经在可访问列表里', type: 'error' });
      return;
    }
    onSettingChange('roots', [...settings.roots, root]);
    setNewRoot('');
    showToast({ body: '已添加可访问目录' });
  }

  function handleRemoveRoot(root) {
    onSettingChange(
      'roots',
      settings.roots.filter((item) => item !== root)
    );
    showToast({ body: '已移除可访问目录' });
  }

  function handleAddMemory() {
    const body = newMemory.body.trim();
    if (!body) {
      return;
    }
    const due = parseDueAt(newMemory.dueAt);
    if (!due.ok) {
      showToast({ body: '到期日格式应为 YYYY-MM-DD', type: 'error' });
      return;
    }
    onAddMemory({ kind: newMemory.kind, body, dueAt: due.value });
    setNewMemory((prev) => ({ ...prev, body: '', dueAt: '' }));
  }

  function beginEditMemory(memory) {
    setEditingId(memory.id);
    setEditDraft({
      kind: memory.kind,
      body: memory.body,
      dueAt: memory.dueAt ?? '',
    });
  }

  function handleSaveMemory() {
    if (!editingMemory) {
      return;
    }
    const body = editDraft.body.trim();
    if (!body) {
      showToast({ body: '记忆内容不能为空', type: 'error' });
      return;
    }
    const due = parseDueAt(editDraft.dueAt);
    if (!due.ok) {
      showToast({ body: '到期日格式应为 YYYY-MM-DD', type: 'error' });
      return;
    }
    onUpdateMemory(editingMemory.id, {
      kind: editDraft.kind,
      body,
      dueAt: due.value,
    });
    setEditingId(null);
    showToast({ body: '已更新长期记忆' });
  }

  function handleDeleteMemory(id) {
    onDeleteMemory(id);
    if (editingId === id) {
      setEditingId(null);
    }
    setConfirmClearMemories(false);
    showToast({ body: '已删除长期记忆' });
  }

  function handleClearMemories() {
    setEditingId(null);
    setConfirmClearMemories(false);
    onClearMemories();
  }

  function handleRestorePersona() {
    onSettingChange('persona', '');
    showToast({ body: '已恢复内置人设' });
  }

  function renderModelSection() {
    return (
      <VStack gap={5}>
        <SectionIntro
          title="模型"
          description="配置本地或兼容 OpenAI 接口的模型服务；改动会立即影响输入区的模型标签。"
        />

        <VStack gap={3}>
          <TextInput
            label="接口地址（base_url）"
            value={settings.endpoint}
            onChange={(value) => onSettingChange('endpoint', value)}
          />
          <TextInput
            label="模型名"
            value={settings.model}
            onChange={(value) => onSettingChange('model', value)}
          />
        </VStack>

        <VStack gap={3}>
          <TextInput
            label="API Key"
            type="password"
            value={apiKey}
            onChange={setApiKey}
            onEnter={handleSaveKey}
            placeholder={
              settings.hasApiKey ? '已配置，留空不修改' : '未配置'
            }
            description="Key 只保存在本次演示的内存里，不会写入磁盘。"
          />
          <HStack gap={2} wrap="wrap">
            <Button
              label="保存"
              variant="primary"
              size="sm"
              isDisabled={!apiKey.trim()}
              onClick={handleSaveKey}
            />
            {settings.hasApiKey ? (
              <Button
                label="清除已存的 key"
                variant="destructive"
                size="sm"
                onClick={() => setConfirmClearKey(true)}
              />
            ) : (
              <Button
                label="清除已存的 key"
                variant="destructive"
                size="sm"
                isDisabled
              />
            )}
          </HStack>

          {confirmClearKey ? (
            <Banner
              status="warning"
              title="确认清除已存的 key？"
              description="清除后需要重新填写 API Key，当前会话不会自动恢复。"
              endContent={
                <HStack gap={2}>
                  <Button
                    label="确认清除"
                    variant="destructive"
                    size="sm"
                    onClick={handleClearKey}
                  />
                  <Button
                    label="取消"
                    variant="ghost"
                    size="sm"
                    onClick={() => setConfirmClearKey(false)}
                  />
                </HStack>
              }
            />
          ) : null}

          <HStack gap={2} align="center">
            <Text type="supporting" color="secondary">
              当前：
            </Text>
            <Token
              label={settings.hasApiKey ? '已配置' : '未配置'}
              color={settings.hasApiKey ? 'green' : 'gray'}
              size="sm"
            />
          </HStack>
        </VStack>

        <VStack gap={2}>
          <Slider
            label="上下文预算"
            min={20000}
            max={200000}
            step={10000}
            value={settings.contextBudget}
            onChange={(value) => onSettingChange('contextBudget', value)}
            onChangeEnd={(value) =>
              showToast({
                body: `上下文预算已设为 ${Math.round(value / 1000)}k`,
              })
            }
            formatValue={(value) => `${Math.round(value / 1000)}k`}
            valueDisplay="text"
            width="100%"
          />
          <Text type="supporting" color="secondary">
            上下文预算决定状态栏占用条的分母；占用超过 85% 时会提示压缩较早的对话。
          </Text>
        </VStack>
      </VStack>
    );
  }

  function renderAppearanceSection() {
    return (
      <VStack gap={5}>
        <SectionIntro
          title="外观"
          description="界面主题固定为深色；字号和强调色会立即作用到整个应用。"
        />

        <VStack gap={2}>
          <Text type="label">主题</Text>
          <HStack gap={2} align="center" wrap="wrap">
            <Token label="lfttdo-noir" color="purple" size="sm" />
            <Text type="supporting" color="secondary">
              当前主题：lfttdo-noir（深色固定，不可切换）
            </Text>
          </HStack>
        </VStack>

        <Slider
          label="字号"
          min={13}
          max={18}
          step={1}
          value={settings.fontSize}
          onChange={(value) => onSettingChange('fontSize', value)}
          onChangeEnd={(value) =>
            showToast({ body: `字号已设为 ${value}` })
          }
          formatValue={(value) => `${value} 号`}
          valueDisplay="text"
          width="100%"
        />

        <Selector
          label="强调色"
          value={settings.accent}
          onChange={(value) => {
            onSettingChange('accent', value);
            const option = ACCENT_OPTIONS.find(
              (item) => item.value === value
            );
            showToast({ body: `强调色已切换为${option?.label ?? value}` });
          }}
          options={ACCENT_OPTIONS}
          description="强调色用于按钮、链接、焦点环和选中状态。"
          width="100%"
        />

        <Text type="supporting" color="secondary">
          字号和强调色即时生效；刷新页面后回到初始值。
        </Text>
      </VStack>
    );
  }

  function renderPermissionsSection() {
    return (
      <VStack gap={5}>
        <SectionIntro
          title="权限"
          description="控制写入与命令执行前的确认方式。读取操作始终自动放行。"
        />

        <RadioList
          label="权限模式"
          value={settings.permissionMode}
          onChange={(value) => {
            onSettingChange('permissionMode', value);
            showToast({
              body: `权限模式已切换为${PERMISSION_LABEL[value] ?? value}`,
            });
          }}
        >
          <RadioListItem
            value="rules"
            label="规则（默认）"
            description="读取自动放行；写入和命令逐条询问。"
          />
          <RadioListItem
            value="ask"
            label="全部询问"
            description="写入和命令每次都弹权限卡。"
          />
          <RadioListItem
            value="trusted"
            label="信任模式"
            description="自动放行，风险自担。"
          />
        </RadioList>

        <Switch
          label="Plan 模式"
          description="开启后写入和命令会被直接拒绝，直到退出。"
          value={settings.planMode}
          onChange={(checked) => {
            onSettingChange('planMode', checked);
            showToast({
              body: checked
                ? '已开启 Plan 模式：写入和命令会被拒绝'
                : '已退出 Plan 模式',
            });
          }}
        />

        <Text type="supporting" color="secondary">
          规则模式只对写入和执行询问；信任模式会跳过权限卡，请只在可信项目里使用。
        </Text>
      </VStack>
    );
  }

  function renderPersonaSection() {
    return (
      <VStack gap={5}>
        <SectionIntro
          title="人设"
          description="人设会作为系统提示词附加到之后的每轮对话。"
        />

        <TextArea
          label="人设（系统提示词）"
          rows={6}
          value={settings.persona}
          onChange={(value) => onSettingChange('persona', value)}
          placeholder="例如：你是一名严谨的前端工程师，回答默认使用中文。"
          width="100%"
        />

        <HStack gap={2} wrap="wrap">
          <Button
            label="从 .txt 导入"
            variant="secondary"
            size="sm"
            onClick={onImportPersona}
          />
          <Button
            label="导出 .txt"
            variant="secondary"
            size="sm"
            onClick={onExportPersona}
          />
          <Button
            label="恢复内置"
            variant="ghost"
            size="sm"
            onClick={handleRestorePersona}
          />
        </HStack>

        <Text type="supporting" color="secondary">
          当前 {settings.persona.trim().length} 字；导入和导出只做演示，不修改项目文件。
        </Text>
      </VStack>
    );
  }

  function renderMemoriesSection() {
    return (
      <VStack gap={5}>
        <SectionIntro
          title="长期记忆"
          description="模型会在之后的对话里参考这些内容；可以随时修改或删除。"
        />

        {memories.length === 0 ? (
          <Text type="supporting" color="secondary">
            还没有长期记忆
          </Text>
        ) : (
          <List density="compact" hasDividers>
            {memories.map((memory) => (
              <ListItem
                key={memory.id}
                label={memory.body}
                description={
                  memory.dueAt
                    ? `${kindLabel(memory.kind)} · 到期 ${memory.dueAt}`
                    : kindLabel(memory.kind)
                }
                startContent={
                  <Token
                    label={kindLabel(memory.kind)}
                    color={kindColor(memory.kind)}
                    size="sm"
                  />
                }
                endContent={
                  <HStack gap={1}>
                    <IconButton
                      label="编辑记忆"
                      icon={<PencilIcon />}
                      size="sm"
                      variant="ghost"
                      onClick={() => beginEditMemory(memory)}
                    />
                    <IconButton
                      label="删除记忆"
                      icon={<TrashIcon />}
                      size="sm"
                      variant="ghost"
                      onClick={() => handleDeleteMemory(memory.id)}
                    />
                  </HStack>
                }
                isSelected={editingId === memory.id}
                align="start"
              />
            ))}
          </List>
        )}

        {editingMemory ? (
          <Card elevation="low" padding={3}>
            <VStack gap={3}>
              <HStack justify="between" align="center">
                <Text type="label">编辑记忆</Text>
                <IconButton
                  label="取消编辑"
                  icon={<XMarkIcon />}
                  size="sm"
                  variant="ghost"
                  onClick={() => setEditingId(null)}
                />
              </HStack>
              <Selector
                label="类型"
                value={editDraft.kind}
                onChange={(value) =>
                  setEditDraft((prev) => ({ ...prev, kind: value }))
                }
                options={KIND_OPTIONS}
                width="100%"
              />
              <TextArea
                label="内容"
                rows={3}
                value={editDraft.body}
                onChange={(value) =>
                  setEditDraft((prev) => ({ ...prev, body: value }))
                }
                width="100%"
              />
              <TextInput
                label="到期日（可选）"
                value={editDraft.dueAt}
                onChange={(value) =>
                  setEditDraft((prev) => ({ ...prev, dueAt: value }))
                }
                placeholder="YYYY-MM-DD"
              />
              <HStack gap={2}>
                <Button
                  label="保存修改"
                  variant="primary"
                  size="sm"
                  onClick={handleSaveMemory}
                />
                <Button
                  label="取消"
                  variant="ghost"
                  size="sm"
                  onClick={() => setEditingId(null)}
                />
              </HStack>
            </VStack>
          </Card>
        ) : null}

        <Card elevation="none" padding={3}>
          <VStack gap={3}>
            <Text type="label">新增记忆</Text>
            <Selector
              label="类型"
              value={newMemory.kind}
              onChange={(value) =>
                setNewMemory((prev) => ({ ...prev, kind: value }))
              }
              options={KIND_OPTIONS}
              width="100%"
            />
            <TextInput
              label="内容"
              value={newMemory.body}
              onChange={(value) =>
                setNewMemory((prev) => ({ ...prev, body: value }))
              }
              placeholder="例如：发布前必须跑一遍构建"
              onEnter={handleAddMemory}
            />
            <TextInput
              label="到期日（可选）"
              value={newMemory.dueAt}
              onChange={(value) =>
                setNewMemory((prev) => ({ ...prev, dueAt: value }))
              }
              placeholder="YYYY-MM-DD"
            />
            <Button
              label="新增"
              variant="secondary"
              size="sm"
              isDisabled={!newMemory.body.trim()}
              onClick={handleAddMemory}
            />
          </VStack>
        </Card>

        {memories.length > 0 ? (
          confirmClearMemories ? (
            <Banner
              status="warning"
              title="确认清空全部记忆？"
              description="清空后无法恢复，之后的对话不会再引用这些内容。"
              endContent={
                <HStack gap={2}>
                  <Button
                    label="确认清空"
                    variant="destructive"
                    size="sm"
                    onClick={handleClearMemories}
                  />
                  <Button
                    label="取消"
                    variant="ghost"
                    size="sm"
                    onClick={() => setConfirmClearMemories(false)}
                  />
                </HStack>
              }
            />
          ) : (
            <Button
              label="清空全部记忆"
              variant="destructive"
              size="sm"
              onClick={() => setConfirmClearMemories(true)}
            />
          )
        ) : null}
      </VStack>
    );
  }

  function renderRootsSection() {
    return (
      <VStack gap={5}>
        <SectionIntro
          title="文件访问"
          description="可访问目录会成为 @ 提及和文件工具的根目录。"
        />

        {settings.roots.length === 0 ? (
          <Text type="supporting" color="secondary">
            还没有可访问目录
          </Text>
        ) : (
          <List density="compact" hasDividers>
            {settings.roots.map((root) => (
              <ListItem
                key={root}
                label={root}
                startContent={<FolderIcon />}
                endContent={
                  <IconButton
                    label={`移除 ${root}`}
                    icon={<XMarkIcon />}
                    size="sm"
                    variant="ghost"
                    onClick={() => handleRemoveRoot(root)}
                  />
                }
              />
            ))}
          </List>
        )}

        <HStack gap={2} align="end" width="100%">
          <StackItem size="fill">
            <TextInput
              label="新增可访问目录"
              value={newRoot}
              onChange={setNewRoot}
              onEnter={handleAddRoot}
              placeholder={'例如 D:\\Projects\\demo'}
              width="100%"
            />
          </StackItem>
          <Button
            label="添加"
            variant="secondary"
            size="sm"
            isDisabled={!newRoot.trim()}
            onClick={handleAddRoot}
          />
        </HStack>

        <Text type="supporting" color="secondary">
          仅演示，不访问真实文件系统。
        </Text>
      </VStack>
    );
  }

  function renderStorageSection() {
    return (
      <VStack gap={5}>
        <SectionIntro
          title="储存"
          description="演示数据只保存在浏览器内存，刷新后回到预置状态。"
        />

        <HStack gap={2} align="center" wrap="wrap">
          <Text type="supporting" color="secondary">
            默认储存位置：
          </Text>
          <Token label="浏览器内存（演示）" color="gray" size="sm" />
        </HStack>

        <HStack gap={2} wrap="wrap">
          <Button
            label="打开数据目录"
            variant="secondary"
            size="sm"
            onClick={() =>
              showToast({ body: '已在资源管理器中打开数据目录' })
            }
          />
          <Button
            label="打开工作区目录"
            variant="ghost"
            size="sm"
            onClick={() =>
              showToast({ body: '已在资源管理器中打开工作区目录' })
            }
          />
        </HStack>
      </VStack>
    );
  }

  function renderSection() {
    switch (activeSection) {
      case 'appearance':
        return renderAppearanceSection();
      case 'permissions':
        return renderPermissionsSection();
      case 'persona':
        return renderPersonaSection();
      case 'memories':
        return renderMemoriesSection();
      case 'roots':
        return renderRootsSection();
      case 'storage':
        return renderStorageSection();
      case 'model':
      default:
        return renderModelSection();
    }
  }

  return (
    <Drawer
      isOpen={isOpen}
      onOpenChange={handleDrawerOpenChange}
      label="设置"
      side="end"
      width="min(45rem, 92vw)"
      padding={4}
      purpose="info"
    >
      <Layout
        header={
          <DrawerHeader
            title="设置"
            subtitle="模型、外观、权限与长期记忆"
            onOpenChange={handleDrawerOpenChange}
            hasDivider
          />
        }
        start={
          <LayoutPanel width="9rem" padding={0} hasDivider label="设置分区">
            <VStack gap={1} padding={1}>
              {SECTIONS.map((section) => (
                <Button
                  key={section.id}
                  label={section.label}
                  variant={
                    activeSection === section.id ? 'secondary' : 'ghost'
                  }
                  size="sm"
                  width="100%"
                  onClick={() => selectSection(section.id)}
                />
              ))}
            </VStack>
          </LayoutPanel>
        }
        content={
          <LayoutContent label="设置内容" ref={contentRef}>
            {renderSection()}
          </LayoutContent>
        }
        footer={
          <LayoutFooter hasDivider>
            <HStack justify="between" align="center" gap={3} wrap="wrap">
              <Text type="supporting" color="secondary">
                改动即时生效；刷新页面后回到初始值。
              </Text>
              <Button
                label="关闭"
                variant="ghost"
                size="sm"
                onClick={handleClose}
              />
            </HStack>
          </LayoutFooter>
        }
      />
    </Drawer>
  );
}
