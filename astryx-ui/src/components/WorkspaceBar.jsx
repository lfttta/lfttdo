import { Button } from '@astryxdesign/core/Button';
import { HStack } from '@astryxdesign/core/HStack';
import { Text } from '@astryxdesign/core/Text';
import { Token } from '@astryxdesign/core/Token';
import { FolderIcon } from '@heroicons/react/24/outline';

export function WorkspaceBar({
  workspacePath,
  onChoose,
  onClear,
  onReveal,
}) {
  const hasWorkspace = Boolean(workspacePath);

  return (
    <HStack
      align="center"
      justify="between"
      gap={3}
      paddingInline={4}
      paddingBlock={2}
      width="100%"
      wrap="wrap"
    >
      <HStack align="center" gap={2} wrap="wrap">
        {hasWorkspace ? (
          <>
            <Text type="label" color="secondary">
              工作文件夹
            </Text>
            <Token
              label={workspacePath}
              icon={<FolderIcon />}
              onClick={onReveal}
              description="点击在资源管理器中显示"
              size="md"
            />
          </>
        ) : (
          <Text type="label" color="secondary">
            默认位置
          </Text>
        )}
      </HStack>

      <HStack align="center" gap={2} wrap="wrap">
        <Button
          label="选择文件夹"
          variant="secondary"
          size="sm"
          onClick={onChoose}
        />
        <Button
          label="清除"
          variant="ghost"
          size="sm"
          onClick={onClear}
          isDisabled={!hasWorkspace}
        />
      </HStack>
    </HStack>
  );
}

export default WorkspaceBar;
