import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useSessionManager } from './useSessionManager';
import { FolderTree } from './FolderTree';
import { ConnectionTable } from './ConnectionTable';
import { ManagerToolbar } from './ManagerToolbar';
import { OxideExportModal } from '../modals/OxideExportModal';
import { OxideImportModal } from '../modals/OxideImportModal';
import { EditConnectionPropertiesModal } from '../modals/EditConnectionPropertiesModal';
import { connectToSaved } from '../../lib/connectToSaved';
import { useAppStore } from '../../store/appStore';
import { useToast } from '../../hooks/useToast';
import { useConfirm } from '../../hooks/useConfirm';
import { useTabBgActive } from '../../hooks/useTabBackground';
import { api } from '../../lib/api';
import type { ConnectionInfo } from '../../types';

export const SessionManagerPanel = () => {
  const { t } = useTranslation();
  const bgActive = useTabBgActive('session_manager');
  const { toast } = useToast();
  const { confirm, ConfirmDialog } = useConfirm();
  const createTab = useAppStore(s => s.createTab);

  const {
    connections,
    allConnections,
    groups,
    loading,
    folderTree,
    ungroupedCount,
    selectedGroup,
    setSelectedGroup,
    expandedGroups,
    toggleExpand,
    searchQuery,
    setSearchQuery,
    sortField,
    sortDirection,
    toggleSort,
    selectedIds,
    toggleSelect,
    toggleSelectAll,
    clearSelection,
    refresh,
  } = useSessionManager();

  const [showExport, setShowExport] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [editingConnectionId, setEditingConnectionId] = useState<string | null>(null);

  // Connect action
  const handleConnect = useCallback(async (connectionId: string) => {
    await connectToSaved(connectionId, {
      createTab,
      toast,
      t,
      onError: (id) => setEditingConnectionId(id),
    });
  }, [createTab, toast, t]);

  // Edit action
  const handleEdit = useCallback((connectionId: string) => {
    setEditingConnectionId(connectionId);
  }, []);

  // Duplicate action
  const handleDuplicate = useCallback(async (conn: ConnectionInfo) => {
    try {
      await api.saveConnection({
        name: `${conn.name} (Copy)`,
        group: conn.group,
        host: conn.host,
        port: conn.port,
        username: conn.username,
        auth_type: conn.auth_type,
        key_path: conn.key_path ?? undefined,
        tags: conn.tags,
        color: conn.color ?? undefined,
      });
      toast({
        title: t('sessionManager.toast.connection_duplicated'),
        description: '',
        variant: 'success',
      });
      await refresh();
    } catch (err) {
      console.error('Failed to duplicate connection:', err);
    }
  }, [refresh, toast, t]);

  // Delete action
  const handleDelete = useCallback(async (conn: ConnectionInfo) => {
    if (!await confirm({
      title: t('sessionManager.actions.confirm_delete', { name: conn.name }),
      variant: 'danger',
    })) {
      return;
    }
    try {
      await api.deleteConnection(conn.id);
      toast({
        title: t('sessionManager.toast.connection_deleted'),
        description: '',
        variant: 'success',
      });
      await refresh();
    } catch (err) {
      console.error('Failed to delete connection:', err);
    }
  }, [refresh, toast, t]);

  // Handle import/export close with refresh
  const handleImportClose = useCallback(async () => {
    setShowImport(false);
    await refresh();
  }, [refresh]);

  return (
    <div className={`h-full w-full flex flex-col text-theme-text ${bgActive ? '' : 'bg-theme-bg'}`} data-bg-active={bgActive || undefined}>
      {/* Toolbar */}
      <ManagerToolbar
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        selectedIds={selectedIds}
        allConnections={allConnections}
        groups={groups}
        onRefresh={refresh}
        onClearSelection={clearSelection}
        onShowImport={() => setShowImport(true)}
        onShowExport={() => setShowExport(true)}
      />

      {/* Content area */}
      <div className="flex-1 flex overflow-hidden">
        {/* Left: Folder Tree */}
        <div className="w-[180px] min-w-[140px] border-r border-theme-border shrink-0 overflow-hidden">
          <FolderTree
            folderTree={folderTree}
            selectedGroup={selectedGroup}
            expandedGroups={expandedGroups}
            totalCount={allConnections.length}
            ungroupedCount={ungroupedCount}
            onSelectGroup={setSelectedGroup}
            onToggleExpand={toggleExpand}
          />
        </div>

        {/* Right: Connection Table */}
        <div className="flex-1 min-w-0 overflow-hidden">
          {loading ? (
            <div className="flex items-center justify-center h-full text-theme-text-muted">
              <div className="animate-pulse">{t('common.loading', { defaultValue: 'Loading...' })}</div>
            </div>
          ) : (
            <ConnectionTable
              connections={connections}
              selectedIds={selectedIds}
              sortField={sortField}
              sortDirection={sortDirection}
              onToggleSort={toggleSort}
              onToggleSelect={toggleSelect}
              onToggleSelectAll={toggleSelectAll}
              onConnect={handleConnect}
              onEdit={handleEdit}
              onDuplicate={handleDuplicate}
              onDelete={handleDelete}
            />
          )}
        </div>
      </div>

      {/* Modals */}
      <EditConnectionPropertiesModal
        open={!!editingConnectionId}
        onOpenChange={(open) => {
          if (!open) {
            setEditingConnectionId(null);
          }
        }}
        connection={editingConnectionId ? allConnections.find(c => c.id === editingConnectionId) ?? null : null}
        onSaved={refresh}
      />

      <OxideExportModal
        isOpen={showExport}
        onClose={() => setShowExport(false)}
      />
      <OxideImportModal
        isOpen={showImport}
        onClose={handleImportClose}
      />
      {ConfirmDialog}
    </div>
  );
};
