import { useTranslation } from 'react-i18next';
import { Play, Pencil, MoreHorizontal, Copy, Trash2, KeyRound, Lock, Bot, ShieldQuestion } from 'lucide-react';
import { cn } from '../../lib/utils';
import { Checkbox } from '../ui/checkbox';
import { Button } from '../ui/button';
import { Tooltip, TooltipTrigger, TooltipContent } from '../ui/tooltip';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
} from '../ui/dropdown-menu';
import type { ConnectionInfo } from '../../types';

type ConnectionTableRowProps = {
  connection: ConnectionInfo;
  isSelected: boolean;
  onToggleSelect: (id: string) => void;
  onConnect: (id: string) => void;
  onEdit: (id: string) => void;
  onDuplicate: (conn: ConnectionInfo) => void;
  onDelete: (conn: ConnectionInfo) => void;
};

const AuthBadge = ({ authType }: { authType: string }) => {
  switch (authType) {
    case 'key':
      return (
        <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-emerald-500/20 text-emerald-300">
          <KeyRound className="h-3 w-3" /> Key
        </span>
      );
    case 'password':
      return (
        <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-amber-500/20 text-amber-300">
          <Lock className="h-3 w-3" /> Pwd
        </span>
      );
    case 'agent':
      return (
        <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-blue-500/20 text-blue-300">
          <Bot className="h-3 w-3" /> Agent
        </span>
      );
    default:
      return (
        <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-zinc-500/20 text-zinc-300">
          <ShieldQuestion className="h-3 w-3" /> {authType}
        </span>
      );
  }
};

const formatLastUsed = (lastUsed: string | null, t: (key: string, options?: Record<string, unknown>) => string): string => {
  if (!lastUsed) return t('sessionManager.table.never_used');
  const date = new Date(lastUsed);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMs / 3600000);
  const diffDays = Math.floor(diffMs / 86400000);

  if (diffMins < 1) return t('sessionManager.time.just_now');
  if (diffMins < 60) return t('sessionManager.time.minutes_ago', { count: diffMins });
  if (diffHours < 24) return t('sessionManager.time.hours_ago', { count: diffHours });
  if (diffDays < 7) return t('sessionManager.time.days_ago', { count: diffDays });
  return date.toLocaleDateString();
};

export const ConnectionTableRow = ({
  connection: conn,
  isSelected,
  onToggleSelect,
  onConnect,
  onEdit,
  onDuplicate,
  onDelete,
}: ConnectionTableRowProps) => {
  const { t } = useTranslation();

  return (
    <div
      className={cn(
        'relative flex items-center px-2 py-1.5 text-sm border-b border-theme-border/50 hover:bg-theme-bg-hover transition-colors group cursor-default',
        isSelected && 'bg-blue-500/10'
      )}
      onDoubleClick={() => onConnect(conn.id)}
    >
      {/* Color indicator */}
      {conn.color && (
        <div
          className="absolute left-0 top-0 bottom-0 w-1 rounded-l"
          style={{ backgroundColor: conn.color }}
        />
      )}

      {/* Checkbox */}
      <div className="w-8 flex items-center justify-center shrink-0">
        <Checkbox
          checked={isSelected}
          onCheckedChange={() => onToggleSelect(conn.id)}
        />
      </div>

      {/* Name */}
      <div className="w-[140px] min-w-[100px] flex-1 truncate font-medium pl-1">
        {conn.name}
      </div>

      {/* Host */}
      <div className="w-[130px] shrink-0 truncate text-theme-text-muted font-mono text-xs">
        {conn.host}
      </div>

      {/* Port */}
      <div className="w-[50px] shrink-0 text-theme-text-muted font-mono text-xs">
        {conn.port}
      </div>

      {/* Username */}
      <div className="w-[90px] shrink-0 truncate text-theme-text-muted text-xs">
        {conn.username}
      </div>

      {/* Auth */}
      <div className="w-[72px] shrink-0">
        <AuthBadge authType={conn.auth_type} />
      </div>

      {/* Group */}
      <div className="w-[100px] shrink-0 truncate text-theme-text-muted text-xs">
        {conn.group || '—'}
      </div>

      {/* Last used */}
      <div className="w-[90px] shrink-0 text-theme-text-muted text-xs">
        {formatLastUsed(conn.last_used_at, t)}
      </div>

      {/* Actions */}
      <div className="w-[84px] shrink-0 flex items-center justify-end gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6"
              onClick={() => onConnect(conn.id)}
            >
              <Play className="h-3 w-3 text-green-400" />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="top">{t('sessionManager.actions.connect')}</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6"
              onClick={() => onEdit(conn.id)}
            >
              <Pencil className="h-3 w-3" />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="top">{t('sessionManager.actions.edit')}</TooltipContent>
        </Tooltip>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon" className="h-7 w-7">
              <MoreHorizontal className="h-3.5 w-3.5" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => onDuplicate(conn)}>
              <Copy className="h-4 w-4 mr-2" />
              {t('sessionManager.actions.duplicate')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              className="text-red-400 focus:text-red-400"
              onClick={() => onDelete(conn)}
            >
              <Trash2 className="h-4 w-4 mr-2" />
              {t('sessionManager.actions.delete')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
};
