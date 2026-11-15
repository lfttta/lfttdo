import { useEffect, useRef } from 'react';
import { Drawer, DrawerHeader } from '@astryxdesign/core/Drawer';
import { Layout, LayoutContent, LayoutFooter } from '@astryxdesign/core/Layout';
import { VStack } from '@astryxdesign/core/VStack';
import { HStack } from '@astryxdesign/core/HStack';
import { List, ListItem } from '@astryxdesign/core/List';
import { Button } from '@astryxdesign/core/Button';
import { Heading } from '@astryxdesign/core/Heading';
import { Text } from '@astryxdesign/core/Text';
import { TextArea } from '@astryxdesign/core/TextArea';
import { Selector } from '@astryxdesign/core/Selector';
import { Banner } from '@astryxdesign/core/Banner';
import { Token } from '@astryxdesign/core/Token';

const STATUS_META = {
  connected: { label: '已连接', color: 'green' },
  connecting: { label: '连接中…', color: 'yellow' },
  disconnected: { label: '未连接', color: 'gray' },
};

const TRANSPORT_COLOR = {
  stdio: 'blue',
  'streamable-http': 'purple',
};

const TRUST_OPTIONS = [
  { value: 'allow', label: '本服务器全部允许' },
  { value: 'ask', label: '每次询问' },
  { value: 'block', label: '禁止' },
];

export default function McpPanel({
  isOpen,
  onClose,
  servers = [],
  onSetTrust,
  jsonText,
  onJsonChange,
  onValidate,
  onSave,
  validation = { status: 'idle', message: '' },
}) {
  const jsonRef = useRef(null);
  const connectedCount = servers.filter(
    (server) => server.status === 'connected'
  ).length;

  function handleOpenChange(open) {
    if (!open) {
      onClose();
    }
  }

  // App 的 mcpJson 初值在契约里既可能是字符串，也可能是已解析的对象；
  // TextArea 只接受字符串，这里统一兜底成带缩进的 JSON。
  const editorValue =
    typeof jsonText === 'string'
      ? jsonText
      : JSON.stringify(jsonText ?? { mcpServers: {} }, null, 2);

  useEffect(() => {
    if (isOpen && typeof jsonText !== 'string') {
      onJsonChange(editorValue);
    }
  }, [isOpen, jsonText, editorValue, onJsonChange]);

  return (
    <Drawer
      isOpen={isOpen}
      onOpenChange={handleOpenChange}
      label="MCP"
      side="end"
      width="min(45rem, 92vw)"
      padding={4}
      purpose="info"
    >
      <Layout
        header={
          <DrawerHeader
            title="MCP 服务器"
            subtitle="连接状态、信任档与 mcp.json"
            onOpenChange={handleOpenChange}
            endContent={
              <Text type="supporting" color="secondary">
                已连接 {connectedCount} / {servers.length}
              </Text>
            }
            hasDivider
          />
        }
        content={
          <LayoutContent label="MCP 配置">
            <VStack gap={5}>
              <Banner
                status="warning"
                title="安全提示"
                description="配置文件可能包含 env/headers，不要在共享屏幕时打开。"
              />

              <VStack gap={2}>
                <Heading level={3}>服务器</Heading>

                {servers.length === 0 ? (
                  <VStack gap={2} hAlign="start">
                    <Text type="supporting" color="secondary">
                      还没有配置服务器。点击「编辑配置」直接写 mcp.json。
                    </Text>
                    <Button
                      label="编辑配置"
                      variant="secondary"
                      size="sm"
                      onClick={() => jsonRef.current?.focus()}
                    />
                  </VStack>
                ) : (
                  <List density="compact" hasDividers>
                    {servers.map((server) => {
                      const status = STATUS_META[server.status] ?? {
                        label: server.status,
                        color: 'gray',
                      };

                      return (
                        <ListItem
                          key={server.name}
                          label={server.name}
                          description={
                            <Token
                              label={server.transport}
                              color={
                                TRANSPORT_COLOR[server.transport] ?? 'gray'
                              }
                              size="sm"
                            />
                          }
                          startContent={
                            <Token
                              label={status.label}
                              color={status.color}
                              size="sm"
                            />
                          }
                          endContent={
                            <Selector
                              label={`${server.name} 的信任档`}
                              isLabelHidden
                              size="sm"
                              value={server.trust}
                              onChange={(value) =>
                                onSetTrust(server.name, value)
                              }
                              options={TRUST_OPTIONS}
                              width="10.5rem"
                            />
                          }
                          align="center"
                        />
                      );
                    })}
                  </List>
                )}
              </VStack>

              <VStack gap={2}>
                <Heading level={3}>编辑 mcp.json</Heading>
                <Text type="supporting" color="secondary">
                  校验只检查 JSON 结构与 mcpServers 字段；保存后重启才会重新连接。
                </Text>
                <TextArea
                  ref={jsonRef}
                  label="mcp.json"
                  rows={14}
                  value={editorValue}
                  onChange={onJsonChange}
                  hasSpellCheck={false}
                />
                <HStack gap={2} wrap="wrap">
                  <Button
                    label="校验"
                    variant="secondary"
                    size="sm"
                    onClick={onValidate}
                  />
                  <Button
                    label="保存并重启"
                    variant="primary"
                    size="sm"
                    onClick={onSave}
                  />
                </HStack>

                {validation.status === 'valid' ? (
                  <Banner
                    status="success"
                    title="配置格式正确"
                    description={validation.message}
                  />
                ) : null}

                {validation.status === 'invalid' ? (
                  <Banner
                    status="error"
                    title="配置格式有误"
                    description={validation.message}
                  />
                ) : null}
              </VStack>
            </VStack>
          </LayoutContent>
        }
        footer={
          <LayoutFooter hasDivider>
            <HStack justify="end">
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
