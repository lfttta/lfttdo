import { Drawer, DrawerHeader } from '@astryxdesign/core/Drawer';
import { Layout, LayoutContent, LayoutFooter } from '@astryxdesign/core/Layout';
import { VStack } from '@astryxdesign/core/VStack';
import { HStack } from '@astryxdesign/core/HStack';
import { List, ListItem } from '@astryxdesign/core/List';
import { Switch } from '@astryxdesign/core/Switch';
import { Button } from '@astryxdesign/core/Button';
import { Text } from '@astryxdesign/core/Text';
import { useToast } from '@astryxdesign/core/Toast';
import { ArrowPathIcon, SparklesIcon } from '@heroicons/react/24/outline';

export default function SkillsPanel({
  isOpen,
  onClose,
  skills = [],
  onToggleSkill,
  onUseSkill,
  // 真实模式传入：刷新现读磁盘，打开技能文件夹走 Rust。
  onRefresh,
  onOpenFolder,
  description = '停用的技能不会出现在输入框的 / 菜单里；刷新只会重新读取演示技能目录。',
}) {
  const showToast = useToast();
  const enabledCount = skills.filter((skill) => skill.enabled).length;

  function handleOpenChange(open) {
    if (!open) {
      onClose();
    }
  }

  return (
    <Drawer
      isOpen={isOpen}
      onOpenChange={handleOpenChange}
      label="技能"
      side="end"
      width="min(28.75rem, 92vw)"
      padding={4}
      purpose="info"
    >
      <Layout
        header={
          <DrawerHeader
            title="技能"
            subtitle="启用后会出现在输入框的 / 菜单"
            onOpenChange={handleOpenChange}
            endContent={
              <Text type="supporting" color="secondary">
                已启用 {enabledCount} / {skills.length}
              </Text>
            }
            hasDivider
          />
        }
        content={
          <LayoutContent label="技能列表">
            <VStack gap={4}>
              <Text type="supporting" color="secondary">
                {description}
              </Text>

              {skills.length === 0 ? (
                <Text type="supporting" color="secondary">
                  还没有技能。
                </Text>
              ) : (
                <List density="compact" hasDividers>
                  {skills.map((skill) => (
                    <ListItem
                      key={skill.name}
                      label={<Text type="code">/{skill.name}</Text>}
                      description={`${skill.description}${
                        skill.argumentHint ? ` · ${skill.argumentHint}` : ''
                      }`}
                      startContent={
                        <SparklesIcon
                          style={{
                            color: skill.enabled
                              ? 'var(--color-icon-accent)'
                              : 'var(--color-icon-disabled)',
                          }}
                        />
                      }
                      endContent={
                        <HStack gap={2} align="center">
                          <Switch
                            label={`启用 ${skill.name}`}
                            isLabelHidden
                            size="sm"
                            value={skill.enabled}
                            onChange={(checked) =>
                              onToggleSkill(skill.name, checked)
                            }
                          />
                          <Button
                            label="用这个技能"
                            variant="secondary"
                            size="sm"
                            onClick={() => onUseSkill(skill.name)}
                          />
                        </HStack>
                      }
                      align="start"
                    />
                  ))}
                </List>
              )}
            </VStack>
          </LayoutContent>
        }
        footer={
          <LayoutFooter hasDivider>
            <HStack gap={2} wrap="wrap" justify="end">
              <Button
                label="刷新"
                variant="secondary"
                size="sm"
                icon={<ArrowPathIcon />}
                onClick={
                  onRefresh ??
                  (() => showToast({ body: '已刷新技能目录' }))
                }
              />
              <Button
                label="打开技能文件夹"
                variant="ghost"
                size="sm"
                onClick={
                  onOpenFolder ??
                  (() =>
                    showToast({ body: '已在资源管理器中打开技能文件夹' }))
                }
              />
              <Button
                label="关闭"
                variant="ghost"
                size="sm"
                onClick={onClose}
              />
            </HStack>
          </LayoutFooter>
        }
      />
    </Drawer>
  );
}
