// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '../ui/dialog';
import { Label } from '../ui/label';
import { Input } from '../ui/input';
import { Button } from '../ui/button';
import { Checkbox } from '../ui/checkbox';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui/tooltip';
import { Info } from 'lucide-react';
import { ManagedSshKeySelector } from './ManagedSshKeySelector';
import type { ConnectionInfo } from '../../types';

interface JumpServer {
  id: string;
  host: string;
  port: string;
  username: string;
  authType: 'password' | 'key' | 'default_key' | 'managed_key' | 'agent' | 'certificate';
  password?: string;
  keyPath?: string;
  certPath?: string;
  managedKeyId?: string;
  passphrase?: string;
  agentForwarding?: boolean;
}

interface AddJumpServerDialogProps {
  open: boolean;
  onClose: () => void;
  onAdd: (server: JumpServer) => void;
  savedConnections?: ConnectionInfo[];
}

export const AddJumpServerDialog: React.FC<AddJumpServerDialogProps> = ({
  open,
  onClose,
  onAdd,
  savedConnections = []
}) => {
  const { t } = useTranslation();
  const [host, setHost] = useState('');
  const [port, setPort] = useState('22');
  const [username, setUsername] = useState('');
  const [authType, setAuthType] = useState<'password' | 'key' | 'default_key' | 'managed_key' | 'agent' | 'certificate'>('key');
  const [password, setPassword] = useState('');
  const [keyPath, setKeyPath] = useState('');
  const [certPath, setCertPath] = useState('');
  const [managedKeyId, setManagedKeyId] = useState('');
  const [passphrase, setPassphrase] = useState<string>('');
  const [agentForwarding, setAgentForwarding] = useState(false);
  const [savedConnectionId, setSavedConnectionId] = useState('__custom__');

  // Type-safe auth type handler
  const handleAuthTypeChange = (value: string) => {
    if (value === 'password' || value === 'key' || value === 'default_key' || value === 'managed_key' || value === 'agent' || value === 'certificate') {
      setAuthType(value);
    }
  };

  const formatSavedConnectionLabel = (connection: ConnectionInfo) =>
    `${connection.name || `${connection.username}@${connection.host}`} · ${connection.username}@${connection.host}:${connection.port}`;

  const handleSavedConnectionChange = (connectionId: string) => {
    setSavedConnectionId(connectionId);
    if (connectionId === '__custom__') return;

    const connection = savedConnections.find((candidate) => candidate.id === connectionId);
    if (!connection) return;

    setHost(connection.host);
    setPort(String(connection.port || 22));
    setUsername(connection.username);
    setAuthType(connection.auth_type);
    // Saved connection lists intentionally expose metadata only. Do not hydrate
    // keychain-backed passwords or passphrases into this jump-server form.
    setPassword('');
    setPassphrase('');
    setKeyPath(connection.key_path ?? '');
    setCertPath(connection.cert_path ?? '');
    setManagedKeyId(connection.managed_key_id ?? '');
    setAgentForwarding(Boolean(connection.agent_forwarding));
  };

  const handleBrowseKey = async () => {
    try {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        title: t('modals.jump_server.auth_ssh_key'),
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
      const selected = await openDialog({
        multiple: false,
        directory: false,
        title: t('modals.new_connection.browse_cert'),
        defaultPath: '~/.ssh',
        filters: [{ name: 'Certificate', extensions: ['pub'] }],
      });
      if (selected && typeof selected === 'string') {
        setCertPath(selected);
      }
    } catch (e) {
      console.error('Failed to open file dialog:', e);
    }
  };

  const handleAdd = () => {
    if (!host || !username) return;

    onAdd({
      id: crypto.randomUUID(),
      host,
      port: port || '22',
      username,
      authType,
      password: authType === 'password' ? password : undefined,
      keyPath: authType === 'key' || authType === 'certificate' ? keyPath : undefined,
      certPath: authType === 'certificate' ? certPath : undefined,
      managedKeyId: authType === 'managed_key' ? managedKeyId : undefined,
      passphrase: authType === 'key' || authType === 'certificate' || authType === 'managed_key' ? passphrase || undefined : undefined,
      agentForwarding,
    });
    onClose();
  };

  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="grid max-h-[85vh] grid-rows-[auto_minmax(0,1fr)_auto] sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle>{t('modals.jump_server.title')}</DialogTitle>
        </DialogHeader>

        <div className="min-h-0 space-y-4 overflow-y-auto p-4">
          <div className="space-y-2">
            <Label>{t('modals.jump_server.saved_connection')}</Label>
            <Select
              value={savedConnectionId}
              onValueChange={handleSavedConnectionChange}
              disabled={savedConnections.length === 0}
            >
              <SelectTrigger>
                <SelectValue placeholder={t('modals.jump_server.saved_connection_custom')} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__custom__">
                  {t('modals.jump_server.saved_connection_custom')}
                </SelectItem>
                {savedConnections.map((connection) => (
                  <SelectItem key={connection.id} value={connection.id}>
                    {formatSavedConnectionLabel(connection)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-xs text-theme-text-muted">
              {savedConnections.length === 0
                ? t('modals.jump_server.saved_connection_empty')
                : t('modals.jump_server.saved_connection_hint')}
            </p>
          </div>

          <div className="grid grid-cols-4 gap-4">
            <div className="col-span-3 space-y-2">
              <Label htmlFor="jump-host">{t('modals.jump_server.host')} *</Label>
              <Input
          id="jump-host"
          placeholder={t('modals.jump_server.host_placeholder')}
          value={host}
          onChange={(e) => setHost(e.target.value)}
              />
            </div>
            <div className="col-span-1 space-y-2">
              <Label htmlFor="jump-port">{t('modals.jump_server.port')}</Label>
              <Input
          id="jump-port"
          type="number"
          value={port}
          onChange={(e) => setPort(e.target.value)}
              />
            </div>
          </div>

          <div className="space-y-2">
            <Label htmlFor="jump-username">{t('modals.jump_server.username')} *</Label>
            <Input
              id="jump-username"
              placeholder={t('modals.jump_server.username_placeholder')}
              value={username}
              onChange={(e) => setUsername(e.target.value)}
            />
          </div>

          <div className="space-y-2">
            <Label>{t('modals.jump_server.authentication')}</Label>
            <p className="text-xs text-theme-text-muted">
              {t('modals.jump_server.kbi_not_supported_hint')}
            </p>
            <Tabs
              value={authType}
              onValueChange={handleAuthTypeChange}
              className="w-full"
            >
              {/* Split jump auth tabs into two rows so translated labels do not overlap. */}
              <TabsList className="grid h-auto w-full grid-cols-3 gap-1">
                <TabsTrigger value="default_key" className="min-h-9 whitespace-normal px-2 text-center leading-tight">
                  {t('modals.jump_server.auth_default_key')}
                </TabsTrigger>
                <TabsTrigger value="key" className="min-h-9 whitespace-normal px-2 text-center leading-tight">
                  {t('modals.jump_server.auth_ssh_key')}
                </TabsTrigger>
                <TabsTrigger value="managed_key" className="min-h-9 whitespace-normal px-2 text-center leading-tight">
                  {t('modals.new_connection.auth_managed_key')}
                </TabsTrigger>
                <TabsTrigger value="certificate" className="min-h-9 whitespace-normal px-2 text-center leading-tight">
                  {t('modals.new_connection.auth_certificate')}
                </TabsTrigger>
                <TabsTrigger value="password" className="min-h-9 whitespace-normal px-2 text-center leading-tight">
                  {t('modals.jump_server.auth_password')}
                </TabsTrigger>
                <TabsTrigger value="agent" className="min-h-9 whitespace-normal px-2 text-center leading-tight">
                  {t('modals.jump_server.auth_agent')}
                </TabsTrigger>
              </TabsList>

              <TabsContent value="default_key">
          <div className="text-sm text-theme-text-muted pt-2">
            {t('modals.jump_server.default_key_desc')}
          </div>
              </TabsContent>

              <TabsContent value="key">
          <div className="space-y-2 pt-2">
            <Label htmlFor="jump-keypath">{t('modals.jump_server.key_path')}</Label>
            <div className="flex gap-2">
              <Input
                id="jump-keypath"
                value={keyPath}
                onChange={(e) => setKeyPath(e.target.value)}
                placeholder={t('modals.jump_server.key_path_placeholder')}
              />
              <Button variant="outline" onClick={handleBrowseKey} type="button">{t('modals.jump_server.browse')}</Button>
            </div>
            <div className="space-y-1 pt-1">
              <Label htmlFor="jump-passphrase" className="text-sm font-normal">{t('modals.jump_server.passphrase')}</Label>
              <Input
                id="jump-passphrase"
                type="password"
                value={passphrase}
                onChange={(e) => setPassphrase(e.target.value)}
              />
            </div>
          </div>
              </TabsContent>

              <TabsContent value="managed_key">
                <ManagedSshKeySelector
                  selectedId={managedKeyId}
                  onSelectedIdChange={setManagedKeyId}
                  passphrase={passphrase}
                  onPassphraseChange={setPassphrase}
                />
              </TabsContent>

              <TabsContent value="certificate" forceMount>
          <div className="space-y-2 pt-2">
            <Label htmlFor="jump-cert-keypath">{t('modals.new_connection.private_key')}</Label>
            <div className="flex gap-2">
              <Input
                id="jump-cert-keypath"
                value={keyPath}
                onChange={(e) => setKeyPath(e.target.value)}
                placeholder={t('modals.jump_server.key_path_placeholder')}
              />
              <Button variant="outline" onClick={handleBrowseKey} type="button">{t('modals.jump_server.browse')}</Button>
            </div>
            <Label htmlFor="jump-certpath">{t('modals.new_connection.certificate')}</Label>
            <div className="flex gap-2">
              <Input
                id="jump-certpath"
                value={certPath}
                onChange={(e) => setCertPath(e.target.value)}
                placeholder="~/.ssh/id_ed25519-cert.pub"
              />
              <Button variant="outline" onClick={handleBrowseCert} type="button">{t('modals.jump_server.browse')}</Button>
            </div>
            <div className="space-y-1 pt-1">
              <Label htmlFor="jump-cert-passphrase" className="text-sm font-normal">{t('modals.jump_server.passphrase')}</Label>
              <Input
                id="jump-cert-passphrase"
                type="password"
                value={passphrase}
                onChange={(e) => setPassphrase(e.target.value)}
              />
            </div>
          </div>
              </TabsContent>

              <TabsContent value="password">
          <div className="space-y-2 pt-2">
            <Label htmlFor="jump-password">{t('modals.jump_server.password')}</Label>
            <Input
              id="jump-password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
          </div>
              </TabsContent>

              <TabsContent value="agent">
          <div className="text-sm text-theme-text-muted pt-2 space-y-2">
            <p>{t('modals.jump_server.agent_desc')}</p>
            <p className="text-xs text-theme-text-muted">
              {t('modals.jump_server.agent_hint')}
            </p>
          </div>
              </TabsContent>
            </Tabs>
          </div>

          <div className="flex items-center space-x-2">
            <Checkbox
              id="jump-agent-fwd"
              checked={agentForwarding}
              onCheckedChange={(checked) => setAgentForwarding(!!checked)}
            />
            <Label htmlFor="jump-agent-fwd" className="font-normal">
              {t('modals.new_connection.agent_forwarding')}
            </Label>
            <TooltipProvider>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Info className="h-3.5 w-3.5 cursor-help text-yellow-500" />
                </TooltipTrigger>
                <TooltipContent side="top" className="max-w-[280px]">
                  <p className="text-xs">{t('modals.new_connection.agent_forwarding_hint')}</p>
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
          </div>
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>{t('modals.jump_server.cancel')}</Button>
          <Button onClick={handleAdd} disabled={!host || !username || (authType === 'managed_key' && !managedKeyId)}>
            {t('modals.jump_server.add')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
