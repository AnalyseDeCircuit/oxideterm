// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useState, useEffect, useCallback, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { open } from '@tauri-apps/plugin-dialog';
import { useAppStore } from '../../store/appStore';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Checkbox } from '../ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogFooter
} from '../ui/dialog';
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger
} from '../ui/tabs';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from '../ui/select';
import { ProxyHopConfig } from '../../types';
import { api } from '../../lib/api';
import type { HostKeyStatus } from '../../types';
import type { TestConnectionRequest, TestConnectionResponse } from '../../lib/api';
import {
  cleanupSessionTreeConnectPlan,
  continueSessionTreeConnectPlan,
  type SessionTreeConnectPlan,
} from '../../lib/sessionTreeConnectPlan';
import { buildTestConnectionRequest } from '../../lib/testConnectionRequest';
import { AddJumpServerDialog } from './AddJumpServerDialog';
import { HostKeyConfirmDialog } from './HostKeyConfirmDialog';
import { Plus, Trash2, Key, Lock, ChevronDown, ChevronRight, Info } from 'lucide-react';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { useToast } from '../../hooks/useToast';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '../ui/tooltip';

const SAVE_CONNECTION_KEY = 'oxideterm.saveConnection';

type ConnectFormRequest = {
  displayName?: string;
  host: string;
  port: number;
  username: string;
  authType: 'password' | 'key' | 'default_key' | 'agent' | 'certificate' | 'keyboard_interactive';
  password?: string;
  keyPath?: string;
  certPath?: string;
  passphrase?: string;
  agentForwarding?: boolean;
};

export const NewConnectionModal = () => {
  const { t } = useTranslation();
  const { 
    modals, 
    toggleModal,
    quickConnectData,
    createTab,
  } = useAppStore();
  const {
    addRootNode,
    connectNode,
    createTerminalForNode,
    expandManualPreset,
  } = useSessionTreeStore();
  const { success: toastSuccess, error: toastError } = useToast();
  const [loading, setLoading] = useState(false);
  const [testing, setTesting] = useState(false);
  
  // Form State
  const [name, setName] = useState('');
  const [host, setHost] = useState('');
  const [port, setPort] = useState('22');
  const [username, setUsername] = useState('root');
  const [authType, setAuthType] = useState<'password' | 'key' | 'default_key' | 'agent' | 'certificate' | 'keyboard_interactive'>('password');
  const [password, setPassword] = useState('');
  const [keyPath, setKeyPath] = useState('');
  const [certPath, setCertPath] = useState('');  // Certificate path
  const [passphrase, setPassphrase] = useState('');  // Key passphrase for certificate
  const [saveConnection, setSaveConnection] = useState(() => {
    try {
      return localStorage.getItem(SAVE_CONNECTION_KEY) === 'true';
    } catch {
      return false;
    }
  });
  const [savePassword, setSavePassword] = useState(false);
  const [group, setGroup] = useState('Ungrouped');
  const [groups, setGroups] = useState<string[]>([]);

  const [proxyServers, setProxyServers] = useState<ProxyHopConfig[]>([]);
  const [showAddJumpDialog, setShowAddJumpDialog] = useState(false);
  const [proxyChainExpanded, setProxyChainExpanded] = useState(false);
  const [agentAvailable, setAgentAvailable] = useState<boolean | null>(null);
  const [agentForwarding, setAgentForwarding] = useState(false);
  const [connectHostKeyStatus, setConnectHostKeyStatus] = useState<HostKeyStatus | null>(null);
  const [pendingConnectRequest, setPendingConnectRequest] = useState<ConnectFormRequest | null>(null);
  const [pendingProxyConnectPlan, setPendingProxyConnectPlan] = useState<SessionTreeConnectPlan | null>(null);
  const [testHostKeyStatus, setTestHostKeyStatus] = useState<HostKeyStatus | null>(null);
  const [pendingTestRequest, setPendingTestRequest] = useState<TestConnectionRequest | null>(null);
  const isComposingRef = useRef(false);
  const kbiDisabledForProxyChain = proxyServers.length > 0;
  const currentConnectStep = pendingProxyConnectPlan
    ? pendingProxyConnectPlan.steps[pendingProxyConnectPlan.currentIndex]
    : pendingConnectRequest
      ? { host: pendingConnectRequest.host, port: pendingConnectRequest.port }
      : null;

  const formatTestFailure = useCallback((result: TestConnectionResponse) => {
    const { summary, detail } = result.diagnostic;
    return detail && detail !== summary ? `${summary}: ${detail}` : summary;
  }, []);

  const executeTestConnection = useCallback(async (request: TestConnectionRequest) => {
    const result = await api.testConnection(request);
    if (result.success) {
      toastSuccess(
        t('modals.new_connection.test_success'),
        t('modals.new_connection.test_elapsed', { ms: result.elapsedMs }),
      );
      return;
    }

    toastError(
      t('modals.new_connection.test_failed'),
      formatTestFailure(result),
    );
  }, [formatTestFailure, t, toastError, toastSuccess]);

  const finalizeConnectedNode = useCallback(async (request: ConnectFormRequest, nodeId: string) => {
    const terminalId = await createTerminalForNode(nodeId, 120, 40);
    createTab('terminal', terminalId);

    setConnectHostKeyStatus(null);
    setPendingConnectRequest(null);
  setPendingProxyConnectPlan(null);
    toggleModal('newConnection', false);

    if (saveConnection) {
      try {
        await api.saveConnection({
          name: name || `${request.username}@${request.host}`,
          group: group === 'Ungrouped' ? null : (group || null),
          host: request.host,
          port: request.port,
          username: request.username,
          auth_type: request.authType as 'password' | 'key' | 'default_key' | 'agent' | 'certificate',
          password: (authType === 'password' && savePassword) ? password : undefined,
          key_path: (authType === 'key' || authType === 'certificate') ? keyPath : undefined,
          cert_path: authType === 'certificate' ? certPath : undefined,
          passphrase: (authType === 'key' || authType === 'certificate' || authType === 'default_key') && passphrase
            ? passphrase
            : undefined,
          tags: [],
          agent_forwarding: request.agentForwarding,
          proxy_chain: proxyServers.length > 0 ? proxyServers : undefined,
        });
        window.dispatchEvent(new CustomEvent('saved-connections-changed'));
      } catch (saveErr) {
        console.error('Failed to save connection:', saveErr);
        toastError(
          t('modals.new_connection.save_failed'),
          String(saveErr),
        );
      }
    }

    setPassword('');
    setPassphrase('');
  }, [
    authType,
    createTab,
    createTerminalForNode,
    group,
    keyPath,
    name,
    password,
    proxyServers,
    saveConnection,
    savePassword,
    t,
    toastError,
    toggleModal,
    certPath,
  ]);

  const executeConnect = useCallback(async (
    request: ConnectFormRequest,
    options?: { trustHostKey?: boolean; expectedHostKeyFingerprint?: string },
  ) => {
    const nodeId = await addRootNode(request);
    await connectNode(nodeId, options);
    await finalizeConnectedNode(request, nodeId);
  }, [addRootNode, connectNode, finalizeConnectedNode]);

  const continueProxyConnectPlan = useCallback(async (
    request: ConnectFormRequest,
    plan: SessionTreeConnectPlan,
  ) => {
    const challenge = await continueSessionTreeConnectPlan(plan);

    if (challenge) {
      setPendingConnectRequest(request);
      setPendingProxyConnectPlan(challenge.plan);
      setConnectHostKeyStatus(challenge.status);
      return false;
    }

    await finalizeConnectedNode(request, plan.targetNodeId);
    return true;
  }, [finalizeConnectedNode]);

  const resetPendingProxyConnectPlan = useCallback(() => {
    const planToCleanup = pendingProxyConnectPlan;
    setConnectHostKeyStatus(null);
    setPendingConnectRequest(null);
    setPendingProxyConnectPlan(null);

    if (planToCleanup) {
      void cleanupSessionTreeConnectPlan(planToCleanup).catch((error) => {
        console.warn('Failed to clean up pending proxy connect plan:', error);
      });
    }
  }, [pendingProxyConnectPlan]);

  // Enter key submit (with IME guard)
  const handleFormKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (isComposingRef.current || (e.nativeEvent as KeyboardEvent).isComposing || e.key === 'Process') return;
    if (e.key === 'Enter' && !loading && canConnect()) {
      // Don't submit when focus is on a button, select trigger, or checkbox
      const tag = (e.target as HTMLElement).tagName;
      const role = (e.target as HTMLElement).getAttribute('role');
      if (tag === 'BUTTON' || role === 'combobox' || role === 'checkbox') return;
      e.preventDefault();
      handleConnect();
    }
  }, [loading]);

  // Type-safe auth type handler
  const handleAuthTypeChange = (value: string) => {
    if (value === 'password' || value === 'key' || value === 'default_key' || value === 'agent' || value === 'certificate' || value === 'keyboard_interactive') {
      setAuthType(value);
    }
  };

  // Load groups and check agent availability when modal opens
  useEffect(() => {
    if (modals.newConnection) {
      api.getGroups().then(setGroups).catch(() => setGroups([]));
      api.isAgentAvailable().then(setAgentAvailable).catch(() => setAgentAvailable(false));

      // Pre-fill from Quick Connect data (⌘K user@host:port)
      if (quickConnectData) {
        setName(quickConnectData.name ?? quickConnectData.alias ?? '');
        setHost(quickConnectData.host);
        setPort(String(quickConnectData.port));
        setUsername(quickConnectData.username);
        setAuthType(quickConnectData.authType ?? 'password');
        setKeyPath(quickConnectData.keyPath ?? '');
        setCertPath(quickConnectData.certPath ?? '');
        setProxyServers(quickConnectData.proxyChain ?? []);
        setProxyChainExpanded((quickConnectData.proxyChain?.length ?? 0) > 0);
      }
    }
  }, [modals.newConnection, quickConnectData]);

  useEffect(() => {
    if (kbiDisabledForProxyChain && authType === 'keyboard_interactive') {
      setAuthType('password');
    }
  }, [authType, kbiDisabledForProxyChain]);

  // 移除了连接复用检查逻辑，现在由 SessionTree 后端统一处理
  /* 旧逻辑已删除 */

  const handleBrowseKey = async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: t('modals.new_connection.browse_key'),
        defaultPath: '~/.ssh'
      });
      if (selected && typeof selected === 'string') {
        setKeyPath(selected);
      }
    } catch (e) {
      console.error('Failed to open file dialog:', e);
    }
  };

  const handleBrowseCert = async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: t('modals.new_connection.browse_cert'),
        defaultPath: '~/.ssh',
        filters: [{ name: 'Certificate', extensions: ['pub'] }]
      });
      if (selected && typeof selected === 'string') {
        setCertPath(selected);
      }
    } catch (e) {
      console.error('Failed to open file dialog:', e);
    }
  };

  // Convert JumpServer from dialog to ProxyHopConfig for backend
  const handleAddJumpServer = (server: { 
    id: string; 
    host: string; 
    port: string; 
    username: string; 
    authType: 'password' | 'key' | 'default_key' | 'agent' | 'certificate';
    password?: string;
    keyPath?: string;
    certPath?: string;
    passphrase?: string;
    agentForwarding?: boolean;
  }) => {
    const proxyConfig: ProxyHopConfig = {
      id: server.id,
      host: server.host,
      port: parseInt(server.port, 10) || 22,
      username: server.username,
      auth_type: server.authType,
      password: server.password,
      key_path: server.keyPath,
      cert_path: server.certPath,
      passphrase: server.passphrase,
      agent_forwarding: server.agentForwarding,
    };
    setProxyServers([...proxyServers, proxyConfig]);
  };

  const handleRemoveJumpServer = (index: number) => {
    const newServers = proxyServers.filter((_, i) => i !== index);
    setProxyServers(newServers);
  };

  const canConnect = () => {
    if (proxyServers.length > 0) {
      return proxyServers.every(server => server.host && server.username);
    }
    return host && username;
  };

  const handleConnect = async () => {
    if (proxyServers.length > 0) {
      if (!proxyServers.every(server => server.host && server.username)) return;
    } else {
      if (!host || !username) return;
    }

    setLoading(true);
  let createdProxyPlan: SessionTreeConnectPlan | null = null;
    try {
      if (authType === 'keyboard_interactive' && kbiDisabledForProxyChain) {
        throw new Error(t('sessionManager.toast.proxy_hop_kbi_unsupported'));
      }

      // 使用 SessionTree 的 addRootNode API 创建节点
      const request: ConnectFormRequest = {
        displayName: name || undefined,
        host,
        port: parseInt(port) || 22,
        username,
        authType,
        password: authType === 'password' ? password : undefined,
        keyPath: (authType === 'key' || authType === 'certificate') ? keyPath : undefined,
        certPath: authType === 'certificate' ? certPath : undefined,
        passphrase: (authType === 'key' || authType === 'default_key' || authType === 'certificate') && passphrase ? passphrase : undefined,
        agentForwarding,
      };

      if (proxyServers.length > 0) {
        const expandResult = await expandManualPreset({
          savedConnectionId: `manual:${Date.now()}`,
          hops: proxyServers.map((hop) => ({
            host: hop.host,
            port: hop.port,
            username: hop.username,
            authType: hop.auth_type,
            password: hop.password,
            keyPath: hop.key_path,
            certPath: hop.cert_path,
            passphrase: hop.passphrase,
            agentForwarding: hop.agent_forwarding,
          })),
          target: {
            host: request.host,
            port: request.port,
            username: request.username,
            authType: request.authType === 'keyboard_interactive' ? 'password' : request.authType,
            password: request.password,
            keyPath: request.keyPath,
            certPath: request.certPath,
            passphrase: request.passphrase,
            agentForwarding: request.agentForwarding,
          },
        });

        createdProxyPlan = {
          targetNodeId: expandResult.targetNodeId,
          cleanupNodeId: expandResult.targetNodeId,
          currentIndex: 0,
          steps: expandResult.pathNodeIds.map((nodeId, index) => {
            const endpoint = index < proxyServers.length
              ? proxyServers[index]
              : { host: request.host, port: request.port };

            return {
              nodeId,
              host: endpoint.host,
              port: endpoint.port,
            };
          }),
        };

        await continueProxyConnectPlan(request, createdProxyPlan);
        createdProxyPlan = null;
        return;
      }

      const preflight = await api.sshPreflight({ host: request.host, port: request.port });

      if (preflight.status === 'verified') {
        await executeConnect(request);
        return;
      }

      if (preflight.status === 'unknown' || preflight.status === 'changed') {
        setPendingConnectRequest(request);
        setConnectHostKeyStatus(preflight);
        return;
      }

      toastError(t('modals.new_connection.connect_failed'), preflight.message);
    } catch (e) {
      if (createdProxyPlan) {
        await cleanupSessionTreeConnectPlan(createdProxyPlan).catch((cleanupError) => {
          console.warn('Failed to clean up failed proxy connect plan:', cleanupError);
        });
      }
      console.error(e);
      toastError(
        t('modals.new_connection.connect_failed'),
        String(e),
      );
    } finally {
      setLoading(false);
    }
  };

  const handleTestConnection = async () => {
    if (!host || !username) return;
    if (authType === 'keyboard_interactive') {
      toastError(t('modals.new_connection.test_not_supported_kbi'));
      return;
    }
    setTesting(true);
    try {
      const request = buildTestConnectionRequest({
        host,
        port: parseInt(port) || 22,
        username,
        name: name || undefined,
        authType,
        password,
        keyPath,
        certPath,
        passphrase,
        proxyChain: proxyServers.map((hop) => ({
          host: hop.host,
          port: hop.port,
          username: hop.username,
          authType: hop.auth_type,
          password: hop.password,
          keyPath: hop.key_path,
          certPath: hop.cert_path,
          passphrase: hop.passphrase,
        })),
      });

      if (request.proxy_chain?.length) {
        await executeTestConnection(request);
        return;
      }

      const preflight = await api.sshPreflight({ host, port: parseInt(port) || 22 });

      if (preflight.status === 'verified') {
        await executeTestConnection(request);
        return;
      }

      if (preflight.status === 'unknown') {
        setPendingTestRequest(request);
        setTestHostKeyStatus(preflight);
        return;
      }

      if (preflight.status === 'changed') {
        setPendingTestRequest(request);
        setTestHostKeyStatus(preflight);
        return;
      }

      toastError(t('modals.new_connection.test_failed'), preflight.message);
    } catch (e) {
      toastError(
        t('modals.new_connection.test_failed'),
        String(e),
      );
    } finally {
      setTesting(false);
    }
  };

  const handleAcceptTestHostKey = async (persist: boolean) => {
    if (!pendingTestRequest || !testHostKeyStatus || testHostKeyStatus.status !== 'unknown') {
      return;
    }

    setTesting(true);
    try {
      await executeTestConnection({
        ...pendingTestRequest,
        trust_host_key: persist,
        expected_host_key_fingerprint: testHostKeyStatus.fingerprint,
      });
      setPendingTestRequest(null);
      setTestHostKeyStatus(null);
    } finally {
      setTesting(false);
    }
  };

  const handleAcceptConnectHostKey = async (persist: boolean) => {
    if (!connectHostKeyStatus || connectHostKeyStatus.status !== 'unknown') {
      return;
    }

    setLoading(true);
    try {
      if (pendingProxyConnectPlan) {
        if (!pendingConnectRequest) {
          return;
        }

        const steps = pendingProxyConnectPlan.steps.map((step, index) => (
          index === pendingProxyConnectPlan.currentIndex
            ? {
                ...step,
                trustHostKey: persist,
                expectedHostKeyFingerprint: connectHostKeyStatus.fingerprint,
              }
            : step
        ));

        await continueProxyConnectPlan(pendingConnectRequest, {
          ...pendingProxyConnectPlan,
          steps,
        });
        return;
      }

      if (!pendingConnectRequest) {
        return;
      }

      await executeConnect(pendingConnectRequest, {
        trustHostKey: persist,
        expectedHostKeyFingerprint: connectHostKeyStatus.fingerprint,
      });
    } catch (e) {
      console.error(e);
      setConnectHostKeyStatus(null);
      setPendingConnectRequest(null);
      await cleanupSessionTreeConnectPlan(pendingProxyConnectPlan).catch((cleanupError) => {
        console.warn('Failed to clean up pending proxy plan after accept failure:', cleanupError);
      });
      setPendingProxyConnectPlan(null);
      toastError(t('modals.new_connection.connect_failed'), String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleRemoveChangedHostKey = async () => {
    if (!testHostKeyStatus || testHostKeyStatus.status !== 'changed') {
      return;
    }

    setTesting(true);
    try {
      const currentPort = parseInt(port) || 22;
      await api.sshRemoveHostKey({
        host,
        port: currentPort,
        keyType: testHostKeyStatus.keyType,
        expectedFingerprint: testHostKeyStatus.expectedFingerprint,
      });
      const preflight = await api.sshPreflight({ host, port: currentPort });
      setTestHostKeyStatus(preflight);
    } catch (e) {
      toastError(t('modals.new_connection.test_failed'), String(e));
    } finally {
      setTesting(false);
    }
  };

  const handleRemoveChangedConnectKey = async () => {
    if (!currentConnectStep || !connectHostKeyStatus || connectHostKeyStatus.status !== 'changed') {
      return;
    }

    setLoading(true);
    try {
      await api.sshRemoveHostKey({
        host: currentConnectStep.host,
        port: currentConnectStep.port,
        keyType: connectHostKeyStatus.keyType,
        expectedFingerprint: connectHostKeyStatus.expectedFingerprint,
      });

      const preflight = await api.sshPreflight({
        host: currentConnectStep.host,
        port: currentConnectStep.port,
      });

      if (pendingProxyConnectPlan) {
        if (!pendingConnectRequest) {
          return;
        }

        if (preflight.status === 'verified') {
          await continueProxyConnectPlan(pendingConnectRequest, pendingProxyConnectPlan);
        } else {
          setConnectHostKeyStatus(preflight);
        }
        return;
      }

      if (preflight.status === 'verified') {
        const request = pendingConnectRequest;
        if (!request) {
          return;
        }

        await executeConnect(request);
        return;
      }

      setConnectHostKeyStatus(preflight);
    } catch (e) {
      toastError(t('modals.new_connection.connect_failed'), String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <>
      <HostKeyConfirmDialog
        open={!!connectHostKeyStatus && connectHostKeyStatus.status !== 'verified'}
        onClose={() => {
          if (!loading) {
            resetPendingProxyConnectPlan();
          }
        }}
        status={connectHostKeyStatus}
        host={currentConnectStep?.host || host}
        port={currentConnectStep?.port || parseInt(port) || 22}
        onAccept={handleAcceptConnectHostKey}
        onRemoveSavedKey={handleRemoveChangedConnectKey}
        onCancel={() => {
          resetPendingProxyConnectPlan();
        }}
        loading={loading}
      />

      <HostKeyConfirmDialog
        open={!!testHostKeyStatus && testHostKeyStatus.status !== 'verified'}
        onClose={() => {
          if (!testing) {
            setTestHostKeyStatus(null);
            setPendingTestRequest(null);
          }
        }}
        status={testHostKeyStatus}
        host={host}
        port={parseInt(port) || 22}
        onAccept={handleAcceptTestHostKey}
        onRemoveSavedKey={handleRemoveChangedHostKey}
        onCancel={() => {
          setTestHostKeyStatus(null);
          setPendingTestRequest(null);
        }}
        loading={testing}
      />

      <Dialog open={modals.newConnection} onOpenChange={(open) => {
        // 关闭 modal 时清除敏感数据
        if (!open) {
          setPassword('');
          setPassphrase('');
          setAgentForwarding(false);
          resetPendingProxyConnectPlan();
          setTestHostKeyStatus(null);
          setPendingTestRequest(null);
          // 清除代理链中的密码
          setProxyServers(prev => prev.map(p => ({ ...p, password: undefined, passphrase: undefined })));
        }
        toggleModal('newConnection', open);
      }}>
        <DialogContent
          className="max-h-[90vh] flex flex-col overflow-hidden"
          aria-describedby="new-connection-description"
          onKeyDown={handleFormKeyDown}
          onCompositionStart={() => { isComposingRef.current = true; }}
          onCompositionEnd={() => { isComposingRef.current = false; }}
        >
          <DialogHeader className="shrink-0">
            <DialogTitle>{t('modals.new_connection.title')}</DialogTitle>
            <DialogDescription id="new-connection-description">
              {t('modals.new_connection.description')}
            </DialogDescription>
          </DialogHeader>
          
          <div className="flex-1 overflow-y-auto min-h-0">
          <div className="space-y-6 p-4">
            <div className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="name">{t('modals.new_connection.name')}</Label>
                <Input 
                  id="name" 
                  placeholder={t('modals.new_connection.name_placeholder')} 
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                />
            </div>

            {proxyServers.length > 0 && (
              <div className="bg-theme-bg border-l-4 border-theme-border rounded p-3 mb-4">
                <div className="space-y-2">
                  <p className="text-sm">
                    <span className="font-medium text-theme-text-muted dark:text-theme-text">ⓘ {t('modals.new_connection.proxy_chain.configured')}</span>
                  </p>
                  <div className="text-xs text-theme-text-muted dark:text-theme-text-muted">
                    {proxyServers.map((server, idx) => (
                      <div key={server.id} className="flex items-center gap-2">
                        <div className="w-2 h-2 rounded-full bg-theme-text-muted" />
                        <span className="flex-1 truncate">
                          <span className="font-mono">{idx + 1}.</span>
                          <span className="ml-2">{server.username}@{server.host}:{server.port}</span>
                          {server.auth_type === 'key' || server.auth_type === 'default_key' || server.auth_type === 'certificate' ? (
                            <Key className="inline-block h-3.5 w-3.5 text-theme-text-muted ml-1" />
                          ) : (
                            <Lock className="inline-block h-3.5 w-3.5 text-theme-text-muted ml-1" />
                          )}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            )}

            <div className="grid grid-cols-4 gap-4">
              <div className="col-span-3 grid gap-2">
                <Label htmlFor="host">{t('modals.new_connection.target_host')} *</Label>
                <Input
                  id="host"
                  placeholder="192.168.1.100"
                  value={host}
                  onChange={(e) => setHost(e.target.value)}
                  className={proxyServers.length > 0 && !host ? 'border-orange-500' : ''}
                />
              </div>
              <div className="grid gap-2">
                <Label htmlFor="port">{t('modals.new_connection.port')}</Label>
                <Input
                  id="port"
                  value={port}
                  onChange={(e) => setPort(e.target.value)}
                />
              </div>
            </div>

            <div className="grid gap-2">
              <Label htmlFor="username">{t('modals.new_connection.target_username')} *</Label>
              <Input
                id="username"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                className={proxyServers.length > 0 && !username ? 'border-orange-500' : ''}
              />
            </div>

            <div className="grid gap-2">
              <Label>{t('modals.new_connection.authentication')}</Label>
              <Tabs
                value={authType}
                onValueChange={handleAuthTypeChange}
                className="w-full"
              >
                <TabsList className="grid w-full grid-cols-6">
                  <TabsTrigger value="password">{t('modals.new_connection.auth_password')}</TabsTrigger>
                  <TabsTrigger value="default_key">{t('modals.new_connection.auth_default_key')}</TabsTrigger>
                  <TabsTrigger value="key">{t('modals.new_connection.auth_key')}</TabsTrigger>
                  <TabsTrigger value="certificate">{t('modals.new_connection.auth_certificate')}</TabsTrigger>
                  <TabsTrigger value="agent">{t('modals.new_connection.auth_agent')}</TabsTrigger>
                  <TabsTrigger value="keyboard_interactive" disabled={kbiDisabledForProxyChain}>{t('modals.new_connection.auth_2fa')}</TabsTrigger>
                </TabsList>
                {kbiDisabledForProxyChain && (
                  <p className="pt-2 text-xs text-yellow-600">
                    {t('sessionManager.toast.proxy_hop_kbi_unsupported')}
                  </p>
                )}
                
                <TabsContent value="password">
                  <div className="grid gap-2 pt-2">
                    <Label htmlFor="password">{t('modals.new_connection.password')}</Label>
                    <Input 
                      id="password" 
                      type="password" 
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                    />
                    <div className="flex items-center space-x-2 pt-1">
                       <Checkbox id="save-pass" checked={savePassword} onCheckedChange={(c) => setSavePassword(!!c)} />
                       <Label htmlFor="save-pass" className="font-normal">{t('modals.new_connection.save_password')}</Label>
                    </div>
                  </div>
                </TabsContent>
                
                <TabsContent value="key">
                   <div className="grid gap-3 pt-2">
                     <div className="space-y-2">
                       <Label htmlFor="keypath">{t('modals.new_connection.key_file')}</Label>
                       <div className="flex gap-2">
                          <Input 
                            id="keypath" 
                            value={keyPath}
                            onChange={(e) => setKeyPath(e.target.value)}
                            placeholder="~/.ssh/id_rsa"
                          />
                          <Button variant="outline" onClick={handleBrowseKey}>{t('modals.new_connection.browse')}</Button>
                       </div>
                     </div>
                     <div className="space-y-2">
                       <Label htmlFor="key-passphrase">{t('modals.new_connection.passphrase')}</Label>
                       <Input 
                         id="key-passphrase" 
                         type="password"
                         value={passphrase}
                         onChange={(e) => setPassphrase(e.target.value)}
                         placeholder={t('modals.new_connection.passphrase_placeholder')}
                       />
                     </div>
                   </div>
                </TabsContent>
                
                <TabsContent value="default_key">
                  <div className="grid gap-3 pt-2">
                    <div className="text-sm text-theme-text-muted">
                      {t('modals.new_connection.default_key_desc')}
                    </div>
                    <div className="space-y-2">
                      <Label htmlFor="default-key-passphrase">{t('modals.new_connection.passphrase')}</Label>
                      <Input 
                        id="default-key-passphrase" 
                        type="password"
                        value={passphrase}
                        onChange={(e) => setPassphrase(e.target.value)}
                        placeholder={t('modals.new_connection.passphrase_placeholder')}
                      />
                    </div>
                  </div>
                </TabsContent>
                
                <TabsContent value="agent">
                  <div className="text-sm text-theme-text-muted pt-2 space-y-2">
                  <p>{t('modals.new_connection.agent_desc')}</p>
                  <div className="flex items-center gap-2 text-xs">
                    <span className={`inline-block w-2 h-2 rounded-full ${agentAvailable === true ? 'bg-green-500' : agentAvailable === false ? 'bg-red-500' : 'bg-theme-text-muted animate-pulse'}`} />
                    <span className={agentAvailable === true ? 'text-green-400' : agentAvailable === false ? 'text-red-400' : 'text-theme-text-muted'}>
                      {agentAvailable === true ? t('modals.new_connection.agent_detected') : agentAvailable === false ? t('modals.new_connection.agent_not_detected') : '...'}
                    </span>
                  </div>
                  <p className="text-xs text-theme-text-muted">
                    {t('modals.new_connection.agent_hint')}
                  </p>
                  </div>
                </TabsContent>
                
                <TabsContent value="certificate">
                  <div className="grid gap-3 pt-2">
                    <p className="text-xs text-yellow-600">
                      {t('modals.new_connection.certificate_note')}
                    </p>
                    <div className="space-y-2">
                      <Label htmlFor="cert-keypath">{t('modals.new_connection.private_key')}</Label>
                      <div className="flex gap-2">
                        <Input 
                          id="cert-keypath" 
                          value={keyPath}
                          onChange={(e) => setKeyPath(e.target.value)}
                          placeholder="~/.ssh/id_ed25519"
                        />
                        <Button variant="outline" onClick={handleBrowseKey}>{t('modals.new_connection.browse')}</Button>
                      </div>
                    </div>
                    <div className="space-y-2">
                      <Label htmlFor="certpath">{t('modals.new_connection.certificate')}</Label>
                      <div className="flex gap-2">
                        <Input 
                          id="certpath" 
                          value={certPath}
                          onChange={(e) => setCertPath(e.target.value)}
                          placeholder="~/.ssh/id_ed25519-cert.pub"
                        />
                        <Button variant="outline" onClick={handleBrowseCert}>{t('modals.new_connection.browse')}</Button>
                      </div>
                      <p className="text-xs text-theme-text-muted">
                        {t('modals.new_connection.certificate_hint')}
                      </p>
                    </div>
                    <div className="space-y-2">
                      <Label htmlFor="cert-passphrase">{t('modals.new_connection.passphrase')}</Label>
                      <Input 
                        id="cert-passphrase" 
                        type="password"
                        value={passphrase}
                        onChange={(e) => setPassphrase(e.target.value)}
                        placeholder={t('modals.new_connection.passphrase_placeholder')}
                      />
                    </div>
                  </div>
                </TabsContent>

                <TabsContent value="keyboard_interactive">
                  <div className="text-sm text-theme-text-muted pt-2 space-y-2">
                    <p>{t('modals.new_connection.twofa_desc')}</p>
                    <p className="text-xs text-theme-text-muted">
                      {t('modals.new_connection.twofa_hint')}
                    </p>
                    <p className="text-xs text-yellow-600">
                      {t('modals.new_connection.twofa_warning')}
                    </p>
                  </div>
                </TabsContent>
                </Tabs>
                </div>
            <div className="grid gap-2">
              <Label>{t('modals.new_connection.group')}</Label>
              <Select value={group} onValueChange={setGroup}>
                <SelectTrigger>
                  <SelectValue placeholder={t('modals.new_connection.select_group')} />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="Ungrouped">{t('modals.new_connection.ungrouped')}</SelectItem>
                  {groups.map(g => (
                    <SelectItem key={g} value={g}>{g}</SelectItem>
                  ))}
                  {groups.length === 0 && (
                    <SelectItem value="_help" disabled className="text-theme-text-muted">{t('modals.new_connection.create_groups_hint')}</SelectItem>
                  )}
                </SelectContent>
              </Select>
            </div>
   
            <div className="flex items-center space-x-2">
              <Checkbox 
                id="agent-fwd" 
                checked={agentForwarding}
                onCheckedChange={(c) => setAgentForwarding(!!c)}
              />
              <Label htmlFor="agent-fwd" className="font-normal">{t('modals.new_connection.agent_forwarding')}</Label>
              <TooltipProvider>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Info className="h-3.5 w-3.5 text-yellow-500 cursor-help" />
                  </TooltipTrigger>
                  <TooltipContent side="top" className="max-w-[280px]">
                    <p className="text-xs">{t('modals.new_connection.agent_forwarding_hint')}</p>
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
            </div>

            <div className="flex items-center space-x-2">
              <Checkbox 
                id="save-conn" 
                checked={saveConnection}
                onCheckedChange={(c) => {
                  const val = !!c;
                  setSaveConnection(val);
                  try { localStorage.setItem(SAVE_CONNECTION_KEY, String(val)); } catch { /* noop */ }
                }}
              />
              <Label htmlFor="save-conn">{t('modals.new_connection.save_connection')}</Label>
            </div>
            <p className="text-xs text-muted-foreground -mt-1 ml-6">{t('modals.new_connection.save_connection_hint')}</p>
          </div>

          <div className="border-t border-theme-border rounded-lg p-4">
            <div className="flex items-center justify-between mb-4">
              <div className="text-lg font-semibold">{t('modals.new_connection.proxy_chain.title')}</div>
              <div className="flex items-center gap-2">
                {proxyServers.length > 0 && (
                  <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setProxyChainExpanded(!proxyChainExpanded)}
                    >
                    {proxyChainExpanded ? (
                      <ChevronDown className="h-4 w-4" />
                    ) : (
                      <ChevronRight className="h-4 w-4" />
                    )}
                  </Button>
                )}
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setShowAddJumpDialog(true)}
                >
                  <Plus className="h-4 w-4 mr-2" />
                  {t('modals.new_connection.proxy_chain.add_jump')}
                </Button>
              </div>
            </div>
   
            {proxyChainExpanded ? (
              <div className="space-y-2 max-h-[250px] overflow-y-auto">
                {proxyServers.length === 0 ? (
                  <div className="text-center text-theme-text-muted py-6">
                    {t('modals.new_connection.proxy_chain.empty')}
                  </div>
                ) : (
                  <>
                    {proxyServers.map((server, index) => (
                      <div key={server.id} className="relative">
                        <div className="absolute left-1/2 top-0 bottom-0 w-0.5 -translate-x-1/2">
                          {index > 0 && (
                            <div className="absolute top-1/2 -translate-y-1/2 w-8 h-0.5 bg-theme-text-muted" />
                          )}
                          <div className="absolute top-1/2 -translate-y-1/2 w-8 h-8 rounded-full bg-theme-bg border-2 border-theme-border-strong flex items-center justify-center">
                            {server.auth_type === 'key' || server.auth_type === 'default_key' ? (
                              <Key className="h-4 w-4 text-theme-text-muted" />
                            ) : (
                              <Lock className="h-4 w-4 text-theme-text-muted" />
                            )}
                          </div>
                        </div>

                        <div className="flex items-start gap-6 pl-12">
                          <div className="flex-1 border border border-theme-border rounded-lg p-3 space-y-2">
                            <div className="flex items-center justify-between">
                              <div className="text-sm font-medium text-theme-text-muted">
                                {index + 1}. {t('modals.new_connection.proxy_chain.jump_server')}
                              </div>
                              <Button
                                variant="ghost"
                                size="icon"
                                onClick={() => handleRemoveJumpServer(index)}
                                className="h-6 w-6 p-0"
                              >
                                <Trash2 className="h-3.5 w-3.5 text-theme-text-muted hover:text-red-500" />
                              </Button>
                            </div>
                            <div className="space-y-1">
                              <div className="text-sm">
                                <span className="text-theme-text-muted">{t('modals.new_connection.proxy_chain.host')}:</span>
                                <span className="font-medium ml-2">{server.host}</span>
                              </div>
                              <div className="text-sm">
                                <span className="text-theme-text-muted">{t('modals.new_connection.proxy_chain.port')}:</span>
                                <span className="font-medium ml-2">{server.port}</span>
                              </div>
                              <div className="text-sm">
                                <span className="text-theme-text-muted">{t('modals.new_connection.proxy_chain.username')}:</span>
                                <span className="font-medium ml-2">{server.username}</span>
                              </div>
                              <div className="text-sm">
                                <span className="text-theme-text-muted">{t('modals.new_connection.proxy_chain.auth')}:</span>
                                <span className="font-medium ml-2">
                                  {server.auth_type === 'key' ? t('modals.new_connection.auth_key') :
                                   server.auth_type === 'certificate' ? t('modals.new_connection.auth_certificate') :
                                   server.auth_type === 'default_key' ? t('modals.new_connection.auth_default_key') :
                                   server.auth_type === 'agent' ? t('modals.new_connection.auth_agent') :
                                   t('modals.new_connection.auth_password')}
                                </span>
                              </div>
                            </div>
                          </div>
                        </div>
                      </div>
                    ))}
                  </>
                )}
              </div>
            ) : (
              <div className="text-center text-theme-text-muted py-6">
                {proxyServers.length === 0 ? (
                  t('modals.new_connection.proxy_chain.empty')
                ) : (
                  t('modals.new_connection.proxy_chain.count', { count: proxyServers.length })
                )}
              </div>
            )}
          </div>
        </div>
        </div>
   
        <DialogFooter className="shrink-0">
           <Button variant="ghost" onClick={() => toggleModal('newConnection', false)}>{t('modals.new_connection.cancel')}</Button>
           <Button
              variant="outline"
              onClick={handleTestConnection}
              disabled={loading || testing || !canConnect()}
           >
              {testing ? t('modals.new_connection.testing') : t('modals.new_connection.test')}
           </Button>
           <Button onClick={handleConnect} disabled={loading || testing || !canConnect()}>
              {loading ? t('modals.new_connection.connecting') : t('modals.new_connection.connect')}
           </Button>
        </DialogFooter>
      </DialogContent>
   
      <AddJumpServerDialog
        open={showAddJumpDialog}
        onClose={() => setShowAddJumpDialog(false)}
        onAdd={handleAddJumpServer}
      />
    </Dialog>
    </>
  );
 };
