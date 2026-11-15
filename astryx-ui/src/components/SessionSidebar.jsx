import { useEffect, useMemo, useState } from 'react';
import {
  SideNav,
  SideNavCollapseButton,
  SideNavHeading,
  SideNavItem,
  SideNavSection,
} from '@astryxdesign/core/SideNav';
import { Button } from '@astryxdesign/core/Button';
import { IconButton } from '@astryxdesign/core/IconButton';
import { MoreMenu } from '@astryxdesign/core/MoreMenu';
import { Text } from '@astryxdesign/core/Text';
import { TextInput } from '@astryxdesign/core/TextInput';
import { HStack } from '@astryxdesign/core/HStack';
import { VStack } from '@astryxdesign/core/VStack';
import {
  ArrowDownTrayIcon,
  BoltIcon,
  ChatBubbleLeftRightIcon,
  CheckIcon,
  Cog6ToothIcon,
  PencilIcon,
  PlusIcon,
  ServerStackIcon,
  SparklesIcon,
  TrashIcon,
  XMarkIcon,
} from '@heroicons/react/24/outline';

const PANEL_ENTRIES = [
  { id: 'settings', label: '设置', icon: Cog6ToothIcon },
  { id: 'skills', label: '技能', icon: SparklesIcon },
  { id: 'mcp', label: 'MCP', icon: ServerStackIcon },
  { id: 'hooks', label: 'Hooks', icon: BoltIcon },
];

export function SessionSidebar({
  sessions,
  activeId,
  collapsed,
  onToggleCollapse,
  onSelect,
  onCreate,
  onRename,
  onDelete,
  onExport,
  onOpenPanel,
}) {
  const [renamingId, setRenamingId] = useState(null);
  const [deletingId, setDeletingId] = useState(null);
  const [draft, setDraft] = useState('');

  const sortedSessions = useMemo(
    () =>
      [...sessions].sort(
        (a, b) =>
          (Date.parse(b.updatedAt) || 0) - (Date.parse(a.updatedAt) || 0)
      ),
    [sessions]
  );

  useEffect(() => {
    if (collapsed) {
      setRenamingId(null);
      setDeletingId(null);
      setDraft('');
    }
  }, [collapsed]);

  function startRename(session) {
    setDeletingId(null);
    setRenamingId(session.id);
    setDraft(session.title);
  }

  function cancelRename() {
    setRenamingId(null);
    setDraft('');
  }

  function commitRename(sessionId) {
    const title = draft.trim();
    if (!title) return;
    onRename(sessionId, title);
    setRenamingId(null);
    setDraft('');
  }

  function startDelete(session) {
    setRenamingId(null);
    setDeletingId(session.id);
  }

  function cancelDelete() {
    setDeletingId(null);
  }

  function confirmDelete(sessionId) {
    onDelete(sessionId);
    setDeletingId(null);
  }

  const panelButtons = PANEL_ENTRIES.map((entry) => {
    const PanelIcon = entry.icon;
    return (
      <IconButton
        key={entry.id}
        label={entry.label}
        tooltip={entry.label}
        icon={<PanelIcon />}
        variant="ghost"
        size="sm"
        onClick={() => onOpenPanel(entry.id)}
      />
    );
  });

  const topContent = collapsed ? (
    <VStack gap={2} align="center" width="100%">
      <IconButton
        label="新建对话"
        tooltip="新建对话"
        icon={<PlusIcon />}
        variant="primary"
        onClick={onCreate}
      />
      <SideNavCollapseButton label="展开侧栏" size="sm" />
    </VStack>
  ) : (
    <Button
      label="新建对话"
      variant="primary"
      icon={<PlusIcon />}
      width="100%"
      onClick={onCreate}
    />
  );

  const footerIcons = collapsed ? (
    <VStack gap={1} align="center">
      {panelButtons}
    </VStack>
  ) : (
    <HStack gap={1} align="center" justify="between" width="100%">
      {panelButtons}
    </HStack>
  );

  return (
    <SideNav
      header={
        <SideNavHeading
          heading="LFTTDO"
          subheading="Agent 工作台"
          icon={<SparklesIcon />}
          headerEndContent={
            <SideNavCollapseButton label="收起侧栏" size="sm" />
          }
        />
      }
      topContent={topContent}
      footerIcons={footerIcons}
      collapsible={{
        isCollapsed: collapsed,
        onCollapsedChange: () => onToggleCollapse(),
        hasButton: false,
        buttonLabel: collapsed ? '展开侧栏' : '收起侧栏',
      }}
      resizable={{ defaultWidth: 240, minWidth: 200, maxWidth: 320 }}
    >
      {sortedSessions.length > 0 && (
        <SideNavSection title="会话列表">
          {sortedSessions.map((session) => {
            const isRenaming = !collapsed && renamingId === session.id;
            const isDeleting = !collapsed && deletingId === session.id;

            if (isRenaming) {
              return (
                <HStack
                  key={session.id}
                  gap={1}
                  align="center"
                  width="100%"
                >
                  <TextInput
                    label="重命名"
                    isLabelHidden
                    size="sm"
                    value={draft}
                    onChange={setDraft}
                    onEnter={() => commitRename(session.id)}
                    hasAutoFocus
                    width="100%"
                  />
                  <IconButton
                    label="提交重命名"
                    tooltip="提交重命名"
                    icon={<CheckIcon />}
                    variant="ghost"
                    size="sm"
                    onClick={() => commitRename(session.id)}
                  />
                  <IconButton
                    label="取消"
                    tooltip="取消"
                    icon={<XMarkIcon />}
                    variant="ghost"
                    size="sm"
                    onClick={cancelRename}
                  />
                </HStack>
              );
            }

            if (isDeleting) {
              return (
                <VStack key={session.id} gap={2} width="100%">
                  <Text type="label">删除会话？</Text>
                  <HStack gap={2} wrap="wrap">
                    <Button
                      label="确认删除"
                      variant="destructive"
                      size="sm"
                      onClick={() => confirmDelete(session.id)}
                    />
                    <Button
                      label="取消"
                      variant="ghost"
                      size="sm"
                      onClick={cancelDelete}
                    />
                  </HStack>
                </VStack>
              );
            }

            return (
              <SideNavItem
                key={session.id}
                label={session.title}
                icon={collapsed ? <ChatBubbleLeftRightIcon /> : undefined}
                isSelected={session.id === activeId}
                onClick={() => {
                  if (renamingId !== null) cancelRename();
                  if (deletingId !== null) cancelDelete();
                  onSelect(session.id);
                }}
                actions={
                  <MoreMenu
                    label={`会话操作：${session.title}`}
                    size="sm"
                    items={[
                      {
                        label: '重命名',
                        icon: <PencilIcon />,
                        onClick: () => startRename(session),
                      },
                      {
                        label: '导出',
                        icon: <ArrowDownTrayIcon />,
                        onClick: () => onExport(session.id),
                      },
                      { type: 'divider' },
                      {
                        label: '删除',
                        variant: 'destructive',
                        icon: <TrashIcon />,
                        onClick: () => startDelete(session),
                      },
                    ]}
                  />
                }
              />
            );
          })}
        </SideNavSection>
      )}
    </SideNav>
  );
}

export default SessionSidebar;
