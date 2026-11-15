import { HStack } from '@astryxdesign/core/HStack';
import { VStack } from '@astryxdesign/core/VStack';
import { Text } from '@astryxdesign/core/Text';
import { ProgressBar } from '@astryxdesign/core/ProgressBar';

/**
 * 状态栏：本轮输入/输出 token 与上下文占用条。
 * 占用超过 85% 时进度条转危险色，并在下一行提示自动压缩。
 */
export function StatusBar({ usage, contextBudget }) {
  const current = usage ?? { inputTokens: 0, outputTokens: 0, contextTokens: 0 };
  const budget = Math.max(1, Number(contextBudget) || 1);
  const ratio = current.contextTokens / budget;
  const percent = Math.min(100, Math.max(0, Math.round(ratio * 100)));
  const isFull = percent > 85;

  return (
    <VStack gap={1} width="100%">
      <HStack
        justify="between"
        align="center"
        paddingInline={4}
        paddingBlockStart={1}
        paddingBlockEnd={2}
        width="100%"
      >
        <Text type="supporting" color="secondary" aria-live="polite">
          本轮 token：输入 {current.inputTokens} · 输出 {current.outputTokens}
        </Text>
        <HStack gap={2} align="center">
          <Text type="supporting" color="secondary">
            {percent}%
          </Text>
          <VStack width="7.5rem">
            <ProgressBar
              label="上下文占用"
              isLabelHidden
              value={Math.min(current.contextTokens, budget)}
              max={budget}
              variant={isFull ? 'error' : 'accent'}
            />
          </VStack>
        </HStack>
      </HStack>
      {isFull ? (
        <HStack paddingInline={4} paddingBlockEnd={2}>
          <Text type="supporting" style={{ color: 'var(--color-text-error)' }}>
            上下文较满：下次发送将自动压缩较早的对话
          </Text>
        </HStack>
      ) : null}
    </VStack>
  );
}

export default StatusBar;
