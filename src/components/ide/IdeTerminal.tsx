// src/components/ide/IdeTerminal.tsx
import { useTranslation } from 'react-i18next';
import { Terminal } from 'lucide-react';

export function IdeTerminal() {
  const { t } = useTranslation();
  
  return (
    <div className="h-full bg-zinc-950 flex items-center justify-center text-zinc-500">
      <Terminal className="w-8 h-8 mr-2 opacity-20" />
      <span>{t('ide.terminal_placeholder')}</span>
    </div>
  );
}
