import { Fragment, useEffect, useState } from 'react';
import { Drawer, DrawerHeader } from '@astryxdesign/core/Drawer';
import { Layout, LayoutContent, LayoutFooter } from '@astryxdesign/core/Layout';
import { VStack } from '@astryxdesign/core/VStack';
import { HStack } from '@astryxdesign/core/HStack';
import { List, ListItem } from '@astryxdesign/core/List';
import { Switch } from '@astryxdesign/core/Switch';
import { Button } from '@astryxdesign/core/Button';
import { Heading } from '@astryxdesign/core/Heading';
import { Text } from '@astryxdesign/core/Text';
import { Banner } from '@astryxdesign/core/Banner';
import { Token } from '@astryxdesign/core/Token';
import { useToast } from '@astryxdesign/core/Toast';

const SOURCE_META = {
  user: { label: '用户作用域', color: 'blue' },
  workspace: { label: '工作区级', color: 'purple' },
};

// 只用于界面展示的稳定指纹，不是真实的 SHA-256 计算。
function displayFingerprint(hook) {
  const source = `${hook.event}:${hook.command}`;
  let hash = 2166136261;
  for (let index = 0; index < source.length; index += 1) {
    hash ^= source.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  const hex = (hash >>> 0).toString(16).padStart(8, '0');
  return `${hex.slice(0, 4)}…${hex.slice(-4)}`;
}

export default function HooksPanel({
  isOpen,
  onClose,
  hooks = [],
  onToggleTrust,
  workspaceHooksEnabled,
  onToggleWorkspaceHooks,
  // 真实模式传入：信任说明、指纹不再是「演示」，页脚打开动作走 Rust。
  trustNote = '信任状态只保存在本次演示中；界面不会执行 hook 命令，也不会读取真实文件。',
  fingerprintSuffix = '（演示）',
  onOpenConfig,
  onOpenDataDir,
  onOpenWorkspaceDir,
}) {
  const showToast = useToast();
  const [confirmingWorkspace, setConfirmingWorkspace] = useState(false);
  const [confirmingHookId, setConfirmingHookId] = useState(null);

  const trustedCount = hooks.filter((hook) => hook.trusted).length;

  useEffect(() => {
    if (!isOpen) {
      setConfirmingWorkspace(false);
      setConfirmingHookId(null);
    }
  }, [isOpen]);

  function handleClose() {
    setConfirmingWorkspace(false);
    setConfirmingHookId(null);
    onClose();
  }

  function handleOpenChange(open) {
    if (!open) {
      handleClose();
    }
  }

  function handleWorkspaceToggle(next) {
    if (!next) {
      setConfirmingWorkspace(false);
      onToggleWorkspaceHooks(false);
      return;
    }
    if (workspaceHooksEnabled) {
      return;
    }
    setConfirmingWorkspace(true);
  }

  function confirmWorkspaceHooks() {
    onToggleWorkspaceHooks(true);
    setConfirmingWorkspace(false);
  }

  function handleTrust(hook) {
    onToggleTrust(hook.id, true);
    showToast({ body: `已信任 ${hook.event} hook` });
  }

  function confirmRevoke() {
    if (!confirmingHookId) {
      return;
    }
    onToggleTrust(confirmingHookId, false);
    setConfirmingHookId(null);
    showToast({ body: '已撤销 hook 信任' });
  }

  return (
    <Drawer
      isOpen={isOpen}
      onOpenChange={handleOpenChange}
      label="Hooks"
      side="end"
      width="min(28.75rem, 92vw)"
      padding={4}
      purpose="info"
    >
      <Layout
        header={
          <DrawerHeader
            title="Hooks"
            subtitle="事件、命令与信任状态"
            onOpenChange={handleOpenChange}
            endContent={
              <Text type="supporting" color="secondary">
                已信任 {trustedCount} / {hooks.length}
              </Text>
            }
            hasDivider
          />
        }
        content={
          <LayoutContent label="Hooks 配置">
            <VStack gap={5}>
              <VStack gap={3}>
                <Switch
                  label="启用工作区级 hooks"
                  description={`当前：${
                    workspaceHooksEnabled ? '已启用' : '未启用'
                  }`}
                  value={workspaceHooksEnabled}
                  onChange={handleWorkspaceToggle}
                />

                {confirmingWorkspace ? (
                  <Banner
                    status="warning"
                    title="确认启用工作区级 hooks？"
                    description="工作区 hooks 会执行仓库里的命令，请确认来源可信。"
                    endContent={
                      <HStack gap={2}>
                        <Button
                          label="确认启用"
                          variant="destructive"
                          size="sm"
                          onClick={confirmWorkspaceHooks}
                        />
                        <Button
                          label="取消"
                          variant="ghost"
                          size="sm"
                          onClick={() => setConfirmingWorkspace(false)}
                        />
                      </HStack>
                    }
                  />
                ) : null}
              </VStack>

              <VStack gap={2}>
                <HStack justify="between" align="center">
                  <Heading level={3}>Hook 列表</Heading>
                  <Text type="supporting" color="secondary">
                    用户与工作区共 {hooks.length} 条
                  </Text>
                </HStack>

                {hooks.length === 0 ? (
                  <Text type="supporting" color="secondary">
                    还没有配置 hook。
                  </Text>
                ) : (
                  <List density="compact" hasDividers>
                    {hooks.map((hook) => {
                      const source = SOURCE_META[hook.source] ?? {
                        label: hook.source,
                        color: 'gray',
                      };

                      return (
                        <Fragment key={hook.id}>
                          <ListItem
                            label={<Text type="code">{hook.event}</Text>}
                            description={
                              <Text type="code" color="secondary">
                                {hook.command} · 声明 SHA-256：
                                {displayFingerprint(hook)}
                                {fingerprintSuffix}
                              </Text>
                            }
                            startContent={
                              <Token
                                label={source.label}
                                color={source.color}
                                size="sm"
                              />
                            }
                            endContent={
                              hook.trusted ? (
                                <HStack gap={1} align="center">
                                  <Token
                                    label="信任"
                                    color="green"
                                    size="sm"
                                  />
                                  <Button
                                    label="撤销信任"
                                    variant="ghost"
                                    size="sm"
                                    onClick={() =>
                                      setConfirmingHookId(hook.id)
                                    }
                                  />
                                </HStack>
                              ) : (
                                <Button
                                  label="信任"
                                  variant="secondary"
                                  size="sm"
                                  onClick={() => handleTrust(hook)}
                                />
                              )
                            }
                            align="start"
                          />

                          {confirmingHookId === hook.id ? (
                            <ListItem
                              label={<Text type="label">撤销信任？</Text>}
                              description={`${hook.event} · ${hook.command}`}
                              startContent={
                                <Token
                                  label="待确认"
                                  color="orange"
                                  size="sm"
                                />
                              }
                              endContent={
                                <HStack gap={1} align="center">
                                  <Button
                                    label="确认撤销"
                                    variant="destructive"
                                    size="sm"
                                    onClick={confirmRevoke}
                                  />
                                  <Button
                                    label="取消"
                                    variant="ghost"
                                    size="sm"
                                    onClick={() =>
                                      setConfirmingHookId(null)
                                    }
                                  />
                                </HStack>
                              }
                              align="start"
                            />
                          ) : null}
                        </Fragment>
                      );
                    })}
                  </List>
                )}
              </VStack>

              <Text type="supporting" color="secondary">
                {trustNote}
              </Text>
            </VStack>
          </LayoutContent>
        }
        footer={
          <LayoutFooter hasDivider>
            <HStack gap={2} wrap="wrap" justify="end">
              <Button
                label="打开 hooks.json"
                variant="secondary"
                size="sm"
                onClick={
                  onOpenConfig ??
                  (() =>
                    showToast({ body: '已在资源管理器中打开 hooks.json' }))
                }
              />
              <Button
                label="打开数据目录"
                variant="ghost"
                size="sm"
                onClick={
                  onOpenDataDir ??
                  (() =>
                    showToast({ body: '已在资源管理器中打开数据目录' }))
                }
              />
              <Button
                label="打开工作区目录"
                variant="ghost"
                size="sm"
                onClick={
                  onOpenWorkspaceDir ??
                  (() =>
                    showToast({ body: '已在资源管理器中打开工作区目录' }))
                }
              />
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
