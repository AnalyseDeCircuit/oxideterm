// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useState, useEffect, useCallback, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
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
import { buildTestConnectionRequest } from '../../lib/testConnectionRequest';
import { AddJumpServerDialog } from './AddJumpServerDialog';
import { HostKeyConfirmDialog } from './HostKeyConfirmDialog';
import { KbiDialog } from './KbiDialog';
import { Plus, Trash2, Key, Lock, ChevronDown, ChevronRight, Shield, Info } from 'lucide-react';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { useToast } from '../../hooks/useToast';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '../ui/tooltip';

export const NewConnectionModal = () => {
  const { t } = useTranslation();
  const { 
    modals, 
    toggleModal,
    quickConnectData
  } = useAppStore();
  const { addRootNode, connectNode, addKbiSession } = useSessionTreeStore();
  const { success: toastSuccess, error: toastError } = useToast();
  const [loading, setLoading] = useState(false);
  const [testing, setTesting] = useState(false);
  
  // KBI (2FA) specific state
  const [, setKbiFlowActive] = useState(false);
  const [kbiError, setKbiError] = useState<string | null>(null);
  
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
  const [saveConnection, setSaveConnection] = useState(false);
  const [savePassword, setSavePassword] = useState(false);
  const [group, setGroup] = useState('Ungrouped');
  const [groups, setGroups] = useState<string[]>([]);

  const [proxyServers, setProxyServers] = useState<ProxyHopConfig[]>([]);
  const [showAddJumpDialog, setShowAddJumpDialog] = useState(false);
  const [proxyChainExpanded, setProxyChainExpanded] = useState(false);
  const [agentAvailable, setAgentAvailable] = useState<boolean | null>(null);
  const [agentForwarding, setAgentForwarding] = useState(false);
  const [testHostKeyStatus, setTestHostKeyStatus] = useState<HostKeyStatus | null>(null);
  const [pendingTestRequest, setPendingTestRequest] = useState<TestConnectionRequest | null>(null);
  const isComposingRef = useRef(false);

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
        setHost(quickConnectData.host);
        setPort(String(quickConnectData.port));
        setUsername(quickConnectData.username);
      }
    }
  }, [modals.newConnection, quickConnectData]);

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
    authType: 'password' | 'key' | 'default_key' | 'agent';
    password?: string;
    keyPath?: string;
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

  // Handle KBI success - add the session to SessionTree
  const handleKbiSuccess = useCallback(async (sessionId: string, wsPort: number, wsToken: string) => {
    console.log(`KBI auth succeeded, session: ${sessionId}, ws://127.0.0.1:${wsPort}`);
    setKbiFlowActive(false);
    setKbiError(null);
    
    try {
      // Add the KBI session to SessionTree
      // This is a special path since KBI doesn't go through addRootNode+connectNode
      await addKbiSession({
        sessionId,
        wsPort,
        wsToken,
        host,
        port: parseInt(port) || 22,
        username,
        displayName: name || `${username}@${host}`,
      });
      
      toggleModal('newConnection', false);
    } catch (e) {
      console.error('Failed to add KBI session to tree:', e);
      setKbiError(String(e));
    }
  }, [host, port, username, name, addKbiSession, toggleModal]);

  // Handle KBI failure/cancel
  const handleKbiFailure = useCallback((error: string) => {
    console.log(`KBI auth failed: ${error}`);
    setKbiFlowActive(false);
    setKbiError(error);
    setLoading(false);
  }, []);

  // Start KBI connection flow
  const handleKbiConnect = async () => {
    if (!host || !username) return;
    if (proxyServers.length > 0) {
      setKbiError('2FA via proxy chain is not supported. Please use direct connection.');
      return;
    }

    setLoading(true);
    setKbiError(null);
    setKbiFlowActive(true);

    try {
      const { useSettingsStore } = await import('../../store/settingsStore');
      const bufferSettings = useSettingsStore.getState().settings.buffer;
      // Initiate KBI auth flow - this will trigger ssh_kbi_prompt events
      await invoke('ssh_connect_kbi', {
        host,
        port: parseInt(port) || 22,
        username,
        cols: 120,
        rows: 40,
        displayName: name || undefined,
        agentForwarding,
        maxBufferLines: bufferSettings.maxLines,
        saveOnDisconnect: bufferSettings.saveOnDisconnect,
      });
    } catch (e) {
      console.error('Failed to start KBI flow:', e);
      setKbiFlowActive(false);
      setKbiError(String(e));
      setLoading(false);
    }
  };

  const handleConnect = async () => {
    if (proxyServers.length > 0) {
      if (!proxyServers.every(server => server.host && server.username)) return;
    } else {
      if (!host || !username) return;
    }

    // Special handling for KBI - use separate flow
    if (authType === 'keyboard_interactive') {
      await handleKbiConnect();
      return;
    }

    setLoading(true);
    try {
      // 使用 SessionTree 的 addRootNode API 创建节点
      const request = {
        displayName: name || undefined,
        host,
        port: parseInt(port) || 22,
        username,
        authType: authType === 'default_key' ? 'key' : authType,
        password: authType === 'password' ? password : undefined,
        keyPath: (authType === 'key' || authType === 'default_key' || authType === 'certificate') ? keyPath : undefined,
        certPath: authType === 'certificate' ? certPath : undefined,
        passphrase: (authType === 'key' || authType === 'default_key' || authType === 'certificate') && passphrase ? passphrase : undefined,
        proxy_chain: proxyServers.length > 0 ? proxyServers : undefined,
        agentForwarding,
      };

      // 添加根节点到 SessionTree
      const nodeId = await addRootNode(request);
      console.log(`Root node created: ${nodeId}`);
      
      // 自动连接新创建的节点（与 Saved Connection 流程一致）
      await connectNode(nodeId);
      
      toggleModal('newConnection', false);

      // 保存连接配置（独立 try-catch，连接已成功不应影响用户）
      if (saveConnection) {
        try {
          const saveAuthType = authType === 'default_key' ? 'key' : authType;
          await api.saveConnection({
            name: name || `${username}@${host}`,
            group: group === 'Ungrouped' ? null : (group || null),
            host,
            port: parseInt(port) || 22,
            username,
            auth_type: saveAuthType as 'password' | 'key' | 'agent' | 'certificate',
            password: (authType === 'password' && savePassword) ? password : undefined,
            key_path: (authType === 'key' || authType === 'default_key' || authType === 'certificate') ? keyPath : undefined,
            cert_path: authType === 'certificate' ? certPath : undefined,
            tags: [],
            agent_forwarding: agentForwarding,
            proxy_chain: proxyServers.length > 0 ? proxyServers : undefined,
          });
          // Notify session manager to refresh
          window.dispatchEvent(new CustomEvent('saved-connections-changed'));
        } catch (saveErr) {
          console.error('Failed to save connection:', saveErr);
          toastError(
            t('modals.new_connection.save_failed'),
            String(saveErr),
          );
        }
      }

      // Reset sensitive fields if not saved
      setPassword('');
      setPassphrase('');
    } catch (e) {
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
        toastError(
          t('modals.new_connection.test_failed'),
          t('modals.new_connection.test_host_key_changed', {
            expected: preflight.expectedFingerprint,
            actual: preflight.actualFingerprint,
          }),
        );
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

  return (
    <>
      {/* Keep mounted so listeners are ready before the backend emits the first prompt. */}
      <KbiDialog
        onSuccess={handleKbiSuccess}
        onFailure={handleKbiFailure}
      />

      <HostKeyConfirmDialog
        open={!!testHostKeyStatus && testHostKeyStatus.status === 'unknown'}
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
          setKbiError(null);
          setAgentForwarding(false);
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
          
          {/* KBI Error display */}
          {kbiError && (
            <div className="mx-4 mt-2 p-3 bg-red-950/30 border border-red-900/50 rounded text-sm text-red-400 shrink-0">
              <div className="flex items-center gap-2">
                <Shield className="h-4 w-4" />
                <span>{t('modals.new_connection.twofa_error')}: {kbiError}</span>
              </div>
            </div>
          )}
          
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
                          {server.auth_type === 'key' || server.auth_type === 'default_key' ? (
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
                  <TabsTrigger value="keyboard_interactive">{t('modals.new_connection.auth_2fa')}</TabsTrigger>
                </TabsList>
                
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
                onCheckedChange={(c) => setSaveConnection(!!c)}
              />
              <Label htmlFor="save-conn">{t('modals.new_connection.save_connection')}</Label>
            </div>
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
                                   server.auth_type === 'default_key' ? t('modals.new_connection.auth_default_key') :
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
