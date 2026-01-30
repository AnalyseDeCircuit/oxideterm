import { useState, useRef, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { cn } from '../../lib/utils';
import { validateFileName } from '../../lib/pathUtils';

interface IdeInlineInputProps {
  defaultValue?: string;
  placeholder?: string;
  onConfirm: (value: string) => void;
  onCancel: () => void;
  className?: string;
  selectBaseName?: boolean; // 重命名时只选中不含扩展名的部分
}

export function IdeInlineInput({
  defaultValue = '',
  placeholder,
  onConfirm,
  onCancel,
  className,
  selectBaseName = false,
}: IdeInlineInputProps) {
  const { t } = useTranslation();
  const [value, setValue] = useState(defaultValue);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const confirmingRef = useRef(false);

  // 自动聚焦
  useEffect(() => {
    if (inputRef.current) {
      inputRef.current.focus();

      if (selectBaseName && defaultValue) {
        // 选中不含扩展名的部分
        const dotIndex = defaultValue.lastIndexOf('.');
        if (dotIndex > 0) {
          inputRef.current.setSelectionRange(0, dotIndex);
        } else {
          inputRef.current.select();
        }
      } else {
        inputRef.current.select();
      }
    }
  }, [defaultValue, selectBaseName]);

  // 获取翻译后的错误消息
  const getErrorMessage = useCallback(
    (errorKey: string | null) => {
      if (!errorKey) return null;
      // 错误键格式如 "ide.validation.nameEmpty"
      return t(errorKey, errorKey.split('.').pop() || errorKey);
    },
    [t]
  );

  // 实时验证
  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const newValue = e.target.value;
      setValue(newValue);
      setError(validateFileName(newValue));
    },
    []
  );

  // 确认
  const handleConfirm = useCallback(() => {
    if (confirmingRef.current) return;
    if (error || !value.trim()) return;

    confirmingRef.current = true;
    onConfirm(value.trim());
  }, [value, error, onConfirm]);

  // 键盘事件
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        e.stopPropagation();
        handleConfirm();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        onCancel();
      }
    },
    [handleConfirm, onCancel]
  );

  // 失焦时取消
  const handleBlur = useCallback(() => {
    // 短暂延迟，允许 Enter 键或按钮点击先处理
    setTimeout(() => {
      if (!confirmingRef.current) {
        onCancel();
      }
    }, 150);
  }, [onCancel]);

  const displayError = getErrorMessage(error);

  return (
    <div className="relative inline-block w-full">
      <input
        ref={inputRef}
        type="text"
        value={value}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        onBlur={handleBlur}
        placeholder={placeholder}
        spellCheck={false}
        autoComplete="off"
        className={cn(
          'w-full px-1.5 py-0.5 text-xs bg-theme-bg',
          'border rounded outline-none',
          error
            ? 'border-red-500 focus:ring-red-500/50'
            : 'border-theme-accent focus:ring-theme-accent/50',
          'focus:ring-1',
          className
        )}
      />
      {displayError && (
        <div className="absolute left-0 top-full z-[110] mt-1 px-2 py-1 text-[10px] text-red-400 bg-theme-bg border border-red-500/30 rounded shadow-lg whitespace-nowrap max-w-[200px] truncate">
          {displayError}
        </div>
      )}
    </div>
  );
}
