import { useTranslation } from 'react-i18next';
import { Trash2, FolderInput } from 'lucide-react';
import { useState } from 'react';
import { Button } from '../ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
  DropdownMenuLabel,
} from '../ui/dropdown-menu';
import { api } from '../../lib/api';
import { useToast } from '../../hooks/useToast';
import { useConfirm } from '../../hooks/useConfirm';
import type { ConnectionInfo } from '../../types';

type BatchActionsMenuProps = {
  selectedIds: Set<string>;
  allConnections: ConnectionInfo[];
  groups: string[];
  onRefresh: () => Promise<void>;
  onClearSelection: () => void;
};

export const BatchActionsMenu = ({
  selectedIds,
  allConnections,
  groups,
  onRefresh,
  onClearSelection,
}: BatchActionsMenuProps) => {
  const { t } = useTranslation();
  const { toast } = useToast();
  const { confirm, ConfirmDialog } = useConfirm();
  const [moveMenuOpen, setMoveMenuOpen] = useState(false);

  const handleBatchDelete = async () => {
    if (!await confirm({
      title: t('sessionManager.actions.confirm_batch_delete', { count: selectedIds.size }),
      variant: 'danger',
    })) {
      return;
    }
    try {
      await Promise.all(Array.from(selectedIds).map(id => api.deleteConnection(id)));
      toast({
        title: t('sessionManager.toast.connections_deleted', { count: selectedIds.size }),
        description: '',
        variant: 'success',
      });
      onClearSelection();
      await onRefresh();
    } catch (err) {
      console.error('Batch delete failed:', err);
    }
  };

  const handleMoveToGroup = async (group: string) => {
    try {
      for (const id of selectedIds) {
        const conn = allConnections.find(c => c.id === id);
        if (conn) {
          await api.saveConnection({
            id: conn.id,
            name: conn.name,
            group: group || null,
            host: conn.host,
            port: conn.port,
            username: conn.username,
            auth_type: conn.auth_type,
          });
        }
      }
      toast({
        title: t('sessionManager.toast.connections_moved', { count: selectedIds.size, group: group || t('sessionManager.folder_tree.ungrouped') }),
        description: '',
        variant: 'success',
      });
      onClearSelection();
      setMoveMenuOpen(false);
      await onRefresh();
    } catch (err) {
      console.error('Move to group failed:', err);
    }
  };

  return (
    <div className="flex items-center gap-1">
      <span className="text-xs text-theme-text-muted px-1">
        {t('sessionManager.table.selected_count', { count: selectedIds.size })}
      </span>

      {/* Move to group */}
      <DropdownMenu open={moveMenuOpen} onOpenChange={setMoveMenuOpen}>
        <DropdownMenuTrigger asChild>
          <Button variant="outline" size="sm" className="gap-1.5">
            <FolderInput className="h-3.5 w-3.5" />
            {t('sessionManager.batch.move_to_group')}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <DropdownMenuLabel>{t('sessionManager.batch.move_to_group')}</DropdownMenuLabel>
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={() => handleMoveToGroup('')}>
            {t('sessionManager.folder_tree.ungrouped')}
          </DropdownMenuItem>
          {groups.map(g => (
            <DropdownMenuItem key={g} onClick={() => handleMoveToGroup(g)}>
              {g}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Batch delete */}
      <Button variant="outline" size="sm" className="gap-1.5 text-red-400 hover:text-red-300" onClick={handleBatchDelete}>
        <Trash2 className="h-3.5 w-3.5" />
        {t('sessionManager.batch.delete')}
      </Button>
      {ConfirmDialog}
    </div>
  );
};
