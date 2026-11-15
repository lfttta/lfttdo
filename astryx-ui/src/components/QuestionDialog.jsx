import { useEffect, useState } from 'react';
import { Dialog, DialogHeader } from '@astryxdesign/core/Dialog';
import { Layout, LayoutContent, LayoutFooter } from '@astryxdesign/core/Layout';
import { VStack } from '@astryxdesign/core/VStack';
import { HStack } from '@astryxdesign/core/HStack';
import { Text } from '@astryxdesign/core/Text';
import { Button } from '@astryxdesign/core/Button';
import { RadioList, RadioListItem } from '@astryxdesign/core/RadioList';
import { TextInput } from '@astryxdesign/core/TextInput';

/**
 * 模型提问的居中弹窗。
 * 只在有待回答的提问时打开；「稍后回答」或关闭按钮把问题交还消息流里的内联卡片。
 */
export function QuestionDialog({ question, isOpen, onOpenChange, onAnswer }) {
  const [selected, setSelected] = useState('');
  const [custom, setCustom] = useState('');
  const questionId = question?.id ?? null;

  useEffect(() => {
    setSelected('');
    setCustom('');
  }, [questionId]);

  const options = Array.isArray(question?.options) ? question.options : [];
  const answer = custom.trim() || selected;

  const handleSubmit = () => {
    if (!question || !answer) return;
    onAnswer?.(question.id, answer);
  };

  return (
    <Dialog
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      width={520}
      purpose="form"
    >
      <Layout
        header={
          <DialogHeader
            title="模型提问"
            subtitle="回答后继续当前任务"
            onOpenChange={onOpenChange}
          />
        }
        content={
          <LayoutContent>
            <VStack gap={3}>
              <Text as="p">{question?.question ?? ''}</Text>

              {options.length > 0 ? (
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
              ) : null}

              <TextInput
                label="自定义回答"
                placeholder="也可以直接输入回答"
                value={custom}
                onChange={setCustom}
                size="sm"
              />
            </VStack>
          </LayoutContent>
        }
        footer={
          <LayoutFooter hasDivider>
            <HStack gap={2} hAlign="end" width="100%">
              <Button
                label="稍后回答"
                variant="ghost"
                onClick={() => onOpenChange?.(false)}
              />
              <Button
                label="提交"
                variant="primary"
                isDisabled={!answer}
                onClick={handleSubmit}
              />
            </HStack>
          </LayoutFooter>
        }
      />
    </Dialog>
  );
}

export default QuestionDialog;
