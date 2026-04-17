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
import { buildSaveConnectionRequestFromSaved } from '../../lib/buildSaveConnectionRequestFromSaved';
import {
  connectToSaved,
  continueConnectToSavedPlan,
  type PendingSavedConnectionPlan,
} from '../../lib/connectToSaved';
import { findUnsupportedProxyHopAuth } from '../../lib/proxyHopSupport';
import { cleanupSessionTreeConnectPlan } from '../../lib/sessionTreeConnectPlan';
import { useAppStore } from '../../store/appStore';
import { useToast } from '../../hooks/useToast';
import { useConfirm } from '../../hooks/useConfirm';
import { useTabBgActive } from '../../hooks/useTabBackground';
import { api } from '../../lib/api';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import {
  buildSavedConnectionTestRequest,
  buildTestConnectionRequest,
  requiresSavedConnectionPasswordPrompt,
} from '../../lib/testConnectionRequest';
import type { ConnectionInfo, HostKeyStatus } from '../../types';
import type { EditConnectionSubmitPayload } from '../modals/EditConnectionModal';

const isValidGroupPath = (name: string) => {
  const trimmedName = name.trim();
  if (!trimmedName) {
    return false;
  }

  return trimmedName.split('/').every(part => part.trim().length > 0);
};

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
    expandPath,
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
  const [connectHostKeyStatus, setConnectHostKeyStatus] = useState<HostKeyStatus | null>(null);
  const [pendingSavedConnectPlan, setPendingSavedConnectPlan] = useState<PendingSavedConnectionPlan | null>(null);
  const [pendingTestConnection, setPendingTestConnection] = useState<{
    label: string;
    request: Parameters<typeof api.testConnection>[0];
  } | null>(null);
  const [hostKeyActionLoading, setHostKeyActionLoading] = useState(false);
  const [connectHostKeyActionLoading, setConnectHostKeyActionLoading] = useState(false);
  const [createGroupDialogOpen, setCreateGroupDialogOpen] = useState(false);
  const [newGroupName, setNewGroupName] = useState('');
  const [creatingGroup, setCreatingGroup] = useState(false);

  const notifySavedConnectionsChanged = useCallback(() => {
    window.dispatchEvent(new CustomEvent('saved-connections-changed', {
      detail: { source: 'session-manager' },
    }));
  }, []);

  const getConnectToSavedOptions = useCallback(() => ({
    createTab,
    toast,
    t,
    onError: (id: string, reason?: 'missing-password' | 'connect-failed') => {
      if (reason === 'missing-password') {
        setConnectPromptAction('connect');
        setConnectPromptConnectionId(id);
        return;
      }
      setEditingConnectionId(id);
    },
    onHostKeyChallenge: ({
      pendingPlan,
      status,
    }: {
      pendingPlan: PendingSavedConnectionPlan;
      status: Extract<HostKeyStatus, { status: 'unknown' } | { status: 'changed' }>;
    }) => {
      setPendingSavedConnectPlan(pendingPlan);
      setConnectHostKeyStatus(status);
    },
  }), [createTab, t, toast]);

  const resetPendingSavedConnectPlan = useCallback(() => {
    const planToCleanup = pendingSavedConnectPlan;
    setPendingSavedConnectPlan(null);
    setConnectHostKeyStatus(null);

    if (planToCleanup) {
      void cleanupSessionTreeConnectPlan(planToCleanup.plan).catch((error) => {
        console.warn('Failed to clean up pending saved connection plan:', error);
      });
    }
  }, [pendingSavedConnectPlan]);

  // Connect action
  const handleConnect = useCallback(async (connectionId: string) => {
    await connectToSaved(connectionId, getConnectToSavedOptions());
  }, [getConnectToSavedOptions]);

  const handleAcceptConnectHostKey = useCallback(async (persist: boolean) => {
    if (!pendingSavedConnectPlan || !connectHostKeyStatus || connectHostKeyStatus.status !== 'unknown') {
      return;
    }

    setConnectHostKeyActionLoading(true);
    try {
      const { currentIndex, steps } = pendingSavedConnectPlan.plan;
      const result = await continueConnectToSavedPlan({
        ...pendingSavedConnectPlan,
        plan: {
          ...pendingSavedConnectPlan.plan,
          steps: steps.map((step, index) => index === currentIndex ? {
            ...step,
            trustHostKey: persist,
            expectedHostKeyFingerprint: connectHostKeyStatus.fingerprint,
          } : step),
        },
      }, getConnectToSavedOptions());

      if (result) {
        setPendingSavedConnectPlan(null);
        setConnectHostKeyStatus(null);
      }
    } finally {
      setConnectHostKeyActionLoading(false);
    }
  }, [connectHostKeyStatus, getConnectToSavedOptions, pendingSavedConnectPlan]);

  const handleRemoveChangedConnectHostKey = useCallback(async () => {
    if (!pendingSavedConnectPlan || !connectHostKeyStatus || connectHostKeyStatus.status !== 'changed') {
      return;
    }

    const currentStep = pendingSavedConnectPlan.plan.steps[pendingSavedConnectPlan.plan.currentIndex];
    setConnectHostKeyActionLoading(true);
    try {
      await api.sshRemoveHostKey({
        host: currentStep.host,
        port: currentStep.port,
        keyType: connectHostKeyStatus.keyType,
        expectedFingerprint: connectHostKeyStatus.expectedFingerprint,
      });

      const result = await continueConnectToSavedPlan(pendingSavedConnectPlan, getConnectToSavedOptions());
      if (result) {
        setPendingSavedConnectPlan(null);
        setConnectHostKeyStatus(null);
      }
    } finally {
      setConnectHostKeyActionLoading(false);
    }
  }, [connectHostKeyStatus, getConnectToSavedOptions, pendingSavedConnectPlan]);

  // Edit action
  const handleEdit = useCallback((connectionId: string) => {
    setEditingConnectionId(connectionId);
  }, []);

  // Duplicate action
  const handleDuplicate = useCallback(async (conn: ConnectionInfo) => {
    try {
      const saved = await api.getSavedConnectionForConnect(conn.id);
      await api.saveConnection(
        buildSaveConnectionRequestFromSaved(conn, saved, {
          id: undefined,
          name: `${conn.name} (Copy)`,
        }),
      );
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
    const confirmed = await confirm({
      title: t('sessionManager.actions.confirm_delete', { name: conn.name }),
      confirmLabel: t('sessionManager.actions.delete'),
      variant: 'danger',
    });
    if (!confirmed) {
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
      setPendingTestConnection({ label, request });
      setTestHostKeyStatus(preflight);
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
    certPath,
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
        certPath,
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

  const handleRemoveChangedHostKey = useCallback(async () => {
    if (!pendingTestConnection || !testHostKeyStatus || testHostKeyStatus.status !== 'changed') {
      return;
    }

    setHostKeyActionLoading(true);
    try {
      await api.sshRemoveHostKey({
        host: pendingTestConnection.request.host,
        port: pendingTestConnection.request.port,
        keyType: testHostKeyStatus.keyType,
        expectedFingerprint: testHostKeyStatus.expectedFingerprint,
      });

      const preflight = await api.sshPreflight({
        host: pendingTestConnection.request.host,
        port: pendingTestConnection.request.port,
      });

      setTestHostKeyStatus(preflight);
    } catch (err) {
      toast({
        title: t('sessionManager.toast.test_failed'),
        description: String(err),
        variant: 'error',
      });
    } finally {
      setHostKeyActionLoading(false);
    }
  }, [pendingTestConnection, testHostKeyStatus, toast, t]);

  // Handle import/export close with refresh
  const handleImportClose = useCallback(async () => {
    setShowImport(false);
    await refresh();
  }, [refresh]);

  const handleOpenCreateGroupDialog = useCallback(() => {
    setNewGroupName('');
    setCreateGroupDialogOpen(true);
  }, []);

  const handleCreateGroupFromTree = useCallback(async () => {
    const trimmedGroupName = newGroupName.trim();
    if (!isValidGroupPath(trimmedGroupName)) {
      return;
    }

    setCreatingGroup(true);
    try {
      await api.createGroup(trimmedGroupName);
      setCreateGroupDialogOpen(false);
      setNewGroupName('');
      await refresh();
      expandPath(trimmedGroupName);
      setSelectedGroup(trimmedGroupName);
      notifySavedConnectionsChanged();
      toast({
        title: t('sessionManager.toast.group_created'),
        description: trimmedGroupName,
        variant: 'success',
      });
    } catch (error) {
      console.error('Failed to create group from Session Manager:', error);
      toast({
        title: t('sessionManager.toast.create_group_failed'),
        description: String(error),
        variant: 'error',
      });
    } finally {
      setCreatingGroup(false);
    }
  }, [expandPath, newGroupName, notifySavedConnectionsChanged, refresh, setSelectedGroup, t, toast]);

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
            onRequestCreateGroup={handleOpenCreateGroupDialog}
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
        open={!!connectHostKeyStatus && connectHostKeyStatus.status !== 'verified'}
        onClose={resetPendingSavedConnectPlan}
        status={connectHostKeyStatus}
        host={pendingSavedConnectPlan?.plan.steps[pendingSavedConnectPlan.plan.currentIndex]?.host ?? ''}
        port={pendingSavedConnectPlan?.plan.steps[pendingSavedConnectPlan.plan.currentIndex]?.port ?? 22}
        onAccept={handleAcceptConnectHostKey}
        onRemoveSavedKey={handleRemoveChangedConnectHostKey}
        onCancel={resetPendingSavedConnectPlan}
        loading={connectHostKeyActionLoading}
      />

      <HostKeyConfirmDialog
        open={!!testHostKeyStatus && testHostKeyStatus.status !== 'verified'}
        onClose={() => {
          setTestHostKeyStatus(null);
          setPendingTestConnection(null);
        }}
        status={testHostKeyStatus}
        host={pendingTestConnection?.request.host ?? ''}
        port={pendingTestConnection?.request.port ?? 22}
        onAccept={handleAcceptTestHostKey}
        onRemoveSavedKey={handleRemoveChangedHostKey}
        onCancel={() => {
          setTestHostKeyStatus(null);
          setPendingTestConnection(null);
        }}
        loading={hostKeyActionLoading}
      />

      <Dialog
        open={createGroupDialogOpen}
        onOpenChange={(open) => {
          setCreateGroupDialogOpen(open);
          if (!open) {
            setNewGroupName('');
          }
        }}
      >
        <DialogContent className="sm:max-w-[420px] bg-theme-bg-elevated border-theme-border text-theme-text">
          <DialogHeader>
            <DialogTitle>{t('sessionManager.folder_tree.new_group')}</DialogTitle>
            <DialogDescription>
              {t('sessionManager.folder_tree.new_group_description')}
            </DialogDescription>
          </DialogHeader>
          <div className="px-4 py-2 space-y-2">
            <Label htmlFor="session-manager-new-group-name" className="text-theme-text">
              {t('sessionManager.folder_tree.new_group')}
            </Label>
            <Input
              id="session-manager-new-group-name"
              autoFocus
              value={newGroupName}
              onChange={(event) => setNewGroupName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && isValidGroupPath(newGroupName) && !creatingGroup) {
                  event.preventDefault();
                  void handleCreateGroupFromTree();
                }
              }}
              placeholder={t('sessionManager.folder_tree.new_group_placeholder')}
            />
          </div>
          <DialogFooter>
            <Button
              variant="ghost"
              onClick={() => {
                setCreateGroupDialogOpen(false);
                setNewGroupName('');
              }}
              disabled={creatingGroup}
            >
              {t('common.cancel')}
            </Button>
            <Button
              onClick={() => void handleCreateGroupFromTree()}
              disabled={!isValidGroupPath(newGroupName) || creatingGroup}
            >
              {creatingGroup ? t('common.loading', { defaultValue: 'Loading...' }) : t('common.create')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

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
