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
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui/tooltip';
import { Info } from 'lucide-react';

interface JumpServer {
  id: string;
  host: string;
  port: string;
  username: string;
  authType: 'password' | 'key' | 'default_key' | 'agent';
  password?: string;
  keyPath?: string;
  passphrase?: string;
  agentForwarding?: boolean;
}

interface AddJumpServerDialogProps {
  open: boolean;
  onClose: () => void;
  onAdd: (server: JumpServer) => void;
}

export const AddJumpServerDialog: React.FC<AddJumpServerDialogProps> = ({
  open,
  onClose,
  onAdd
}) => {
  const { t } = useTranslation();
  const [host, setHost] = useState('');
  const [port, setPort] = useState('22');
  const [username, setUsername] = useState('');
  const [authType, setAuthType] = useState<'password' | 'key' | 'default_key' | 'agent'>('key');
  const [password, setPassword] = useState('');
  const [keyPath, setKeyPath] = useState('');
  const [passphrase, setPassphrase] = useState<string>('');
  const [agentForwarding, setAgentForwarding] = useState(false);

  // Type-safe auth type handler
  const handleAuthTypeChange = (value: string) => {
    if (value === 'password' || value === 'key' || value === 'default_key' || value === 'agent') {
      setAuthType(value);
    }
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

  const handleAdd = () => {
    if (!host || !username) return;

    onAdd({
      id: crypto.randomUUID(),
      host,
      port: port || '22',
      username,
      authType,
      password: authType === 'password' ? password : undefined,
      keyPath: authType === 'key' ? keyPath : undefined,
      passphrase: authType === 'key' ? passphrase || undefined : undefined,
      agentForwarding,
    });
    onClose();
  };

  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle>{t('modals.jump_server.title')}</DialogTitle>
        </DialogHeader>

        <div className="space-y-4 p-4">
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
              <TabsList className="grid w-full grid-cols-4">
          <TabsTrigger value="default_key">{t('modals.jump_server.auth_default_key')}</TabsTrigger>
          <TabsTrigger value="key">{t('modals.jump_server.auth_ssh_key')}</TabsTrigger>
          <TabsTrigger value="password">{t('modals.jump_server.auth_password')}</TabsTrigger>
          <TabsTrigger value="agent">{t('modals.jump_server.auth_agent')}</TabsTrigger>
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
          <Button onClick={handleAdd} disabled={!host || !username}>
            {t('modals.jump_server.add')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
