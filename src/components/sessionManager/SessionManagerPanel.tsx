// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useSessionManager } from './useSessionManager';
import { FolderTree } from './FolderTree';
import { ConnectionTable } from './ConnectionTable';
import { ManagerToolbar } from './ManagerToolbar';
import { OxideExportModal } from '../modals/OxideExportModal';
import { OxideImportModal } from '../modals/OxideImportModal';
import { EditConnectionModal } from '../modals/EditConnectionModal';
import { EditConnectionPropertiesModal } from '../modals/EditConnectionPropertiesModal';
import { HostKeyConfirmDialog } from '../modals/HostKeyConfirmDialog';
import { connectToSaved } from '../../lib/connectToSaved';
import { findUnsupportedProxyHopAuth } from '../../lib/proxyHopSupport';
import { useAppStore } from '../../store/appStore';
import { useToast } from '../../hooks/useToast';
import { useConfirm } from '../../hooks/useConfirm';
import { useTabBgActive } from '../../hooks/useTabBackground';
import { api } from '../../lib/api';
import {
  buildSavedConnectionTestRequest,
  buildTestConnectionRequest,
  requiresSavedConnectionPasswordPrompt,
} from '../../lib/testConnectionRequest';
import type { ConnectionInfo, HostKeyStatus } from '../../types';
import type { EditConnectionSubmitPayload } from '../modals/EditConnectionModal';

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
  const [connectPromptConnectionId, setConnectPromptConnectionId] = useState<string | null>(null);
  const [connectPromptAction, setConnectPromptAction] = useState<'connect' | 'test'>('connect');
  const [testHostKeyStatus, setTestHostKeyStatus] = useState<HostKeyStatus | null>(null);
  const [pendingTestConnection, setPendingTestConnection] = useState<{
    label: string;
    request: Parameters<typeof api.testConnection>[0];
  } | null>(null);

  const notifySavedConnectionsChanged = useCallback(() => {
    window.dispatchEvent(new CustomEvent('saved-connections-changed', {
      detail: { source: 'session-manager' },
    }));
  }, []);

  // Connect action
  const handleConnect = useCallback(async (connectionId: string) => {
    await connectToSaved(connectionId, {
      createTab,
      toast,
      t,
      onError: (id, reason) => {
        if (reason === 'missing-password') {
          setConnectPromptAction('connect');
          setConnectPromptConnectionId(id);
          return;
        }
        setEditingConnectionId(id);
      },
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
      notifySavedConnectionsChanged();
    } catch (err) {
      console.error('Failed to duplicate connection:', err);
    }
  }, [notifySavedConnectionsChanged, refresh, toast, t]);

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
      notifySavedConnectionsChanged();
    } catch (err) {
      console.error('Failed to delete connection:', err);
    }
  }, [notifySavedConnectionsChanged, refresh, toast, t]);

  // Test connection action
  const runTestConnection = useCallback(async (label: string, request: Parameters<typeof api.testConnection>[0]) => {
    toast({
      title: t('sessionManager.toast.test_in_progress'),
      description: label,
    });
    const result = await api.testConnection(request);
    if (!result.success) {
      const description = result.diagnostic.detail && result.diagnostic.detail !== result.diagnostic.summary
        ? `${result.diagnostic.summary}: ${result.diagnostic.detail}`
        : result.diagnostic.summary;
      toast({
        title: t('sessionManager.toast.test_failed'),
        description,
        variant: 'error',
      });
      return;
    }
    toast({
      title: t('sessionManager.toast.test_success'),
      description: t('sessionManager.toast.test_elapsed', { ms: result.elapsedMs }),
      variant: 'success',
    });
  }, [toast, t]);

  const prepareTestConnection = useCallback(async (label: string, request: Parameters<typeof api.testConnection>[0]) => {
    if (request.proxy_chain?.length) {
      await runTestConnection(label, request);
      return;
    }

    const preflight = await api.sshPreflight({ host: request.host, port: request.port });

    if (preflight.status === 'verified') {
      await runTestConnection(label, request);
      return;
    }

    if (preflight.status === 'unknown') {
      setPendingTestConnection({ label, request });
      setTestHostKeyStatus(preflight);
      return;
    }

    if (preflight.status === 'changed') {
      toast({
        title: t('sessionManager.toast.test_failed'),
        description: t('sessionManager.toast.test_host_key_changed', {
          expected: preflight.expectedFingerprint,
          actual: preflight.actualFingerprint,
        }),
        variant: 'error',
      });
      return;
    }

    toast({
      title: t('sessionManager.toast.test_failed'),
      description: preflight.message,
      variant: 'error',
    });
  }, [runTestConnection, t, toast]);

  const handleTestConnection = useCallback(async (conn: ConnectionInfo) => {
    try {
      const savedConn = await api.getSavedConnectionForConnect(conn.id);
      const unsupportedProxyHop = findUnsupportedProxyHopAuth(savedConn.proxy_chain);
      if (unsupportedProxyHop) {
        toast({
          title: t('sessionManager.toast.test_failed'),
          description: unsupportedProxyHop.reason === 'keyboard_interactive'
            ? t('sessionManager.toast.proxy_hop_kbi_unsupported', { hop: unsupportedProxyHop.hopIndex })
            : t('sessionManager.toast.proxy_hop_auth_unsupported', {
              hop: unsupportedProxyHop.hopIndex,
              authType: unsupportedProxyHop.authType,
            }),
          variant: 'error',
        });
        return;
      }

      if (requiresSavedConnectionPasswordPrompt(savedConn)) {
        setConnectPromptAction('test');
        setConnectPromptConnectionId(conn.id);
        return;
      }

      await prepareTestConnection(
        `${conn.username}@${conn.host}:${conn.port}`,
        buildSavedConnectionTestRequest(savedConn),
      );
    } catch (err) {
      toast({
        title: t('sessionManager.toast.test_failed'),
        description: String(err),
        variant: 'error',
      });
    }
  }, [prepareTestConnection, t, toast]);

  const handlePromptTestConnection = useCallback(async ({
    connection,
    authType,
    password,
    keyPath,
    passphrase,
  }: EditConnectionSubmitPayload) => {
    await prepareTestConnection(
      `${connection.username}@${connection.host}:${connection.port}`,
      buildTestConnectionRequest({
        host: connection.host,
        port: connection.port,
        username: connection.username,
        name: connection.name,
        authType,
        password,
        keyPath,
        passphrase,
      }),
    );
  }, [prepareTestConnection]);

  const handleAcceptTestHostKey = useCallback(async (persist: boolean) => {
    if (!pendingTestConnection || !testHostKeyStatus || testHostKeyStatus.status !== 'unknown') {
      return;
    }

    await runTestConnection(pendingTestConnection.label, {
      ...pendingTestConnection.request,
      trust_host_key: persist,
      expected_host_key_fingerprint: testHostKeyStatus.fingerprint,
    });

    setPendingTestConnection(null);
    setTestHostKeyStatus(null);
  }, [pendingTestConnection, runTestConnection, testHostKeyStatus]);

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
              onTestConnection={handleTestConnection}
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

      <EditConnectionModal
        open={!!connectPromptConnectionId}
        onOpenChange={(open) => {
          if (!open) {
            setConnectPromptConnectionId(null);
            setConnectPromptAction('connect');
          }
        }}
        connection={connectPromptConnectionId ? allConnections.find(c => c.id === connectPromptConnectionId) ?? null : null}
        action={connectPromptAction}
        onSubmit={connectPromptAction === 'test' ? handlePromptTestConnection : undefined}
        onConnect={connectPromptAction === 'connect' ? refresh : undefined}
      />

      <HostKeyConfirmDialog
        open={!!testHostKeyStatus && testHostKeyStatus.status === 'unknown'}
        onClose={() => {
          setTestHostKeyStatus(null);
          setPendingTestConnection(null);
        }}
        status={testHostKeyStatus}
        host={pendingTestConnection?.request.host ?? ''}
        port={pendingTestConnection?.request.port ?? 22}
        onAccept={handleAcceptTestHostKey}
        onCancel={() => {
          setTestHostKeyStatus(null);
          setPendingTestConnection(null);
        }}
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
