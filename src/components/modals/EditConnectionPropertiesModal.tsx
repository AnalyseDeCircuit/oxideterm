// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Eye, EyeOff, KeyRound, Loader2, Plus, Save as SaveIcon, Trash2 } from 'lucide-react';
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../ui/select';
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '../ui/tabs';
import { open } from '@tauri-apps/plugin-dialog';
import { api } from '../../lib/api';
import type {
  ConnectionInfo,
  PrivilegeCredentialKind,
  SaveConnectionRequest,
  SavedPrivilegeCredential,
  SavedUpstreamProxyPolicy,
} from '../../types';
import { ManagedSshKeySelector } from './ManagedSshKeySelector';
import {
  UpstreamProxyPolicyFields,
  defaultSavedUpstreamProxyPolicy,
} from './UpstreamProxyPolicyFields';

type EditableAuthType = 'password' | 'key' | 'managed_key' | 'agent' | 'certificate';

export type DuplicateConnectionDraft = {
  connection: ConnectionInfo;
  saveRequest: SaveConnectionRequest;
};

type EditConnectionPropertiesModalProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  connection: ConnectionInfo | null;
  duplicateDraft?: DuplicateConnectionDraft | null;
  onSaved?: () => void | Promise<void>;
};

type PrivilegeCredentialDraft = {
  credentialId: string | null;
  label: string;
  kind: PrivilegeCredentialKind;
  usernameHint: string;
  promptPatterns: string;
  secret: string;
  enabled: boolean;
};

const EMPTY_PRIVILEGE_DRAFT: PrivilegeCredentialDraft = {
  credentialId: null,
  label: '',
  kind: 'sudo_password',
  usernameHint: '',
  promptPatterns: '',
  secret: '',
  enabled: true,
};

export const EditConnectionPropertiesModal = ({
  open: isOpen,
  onOpenChange,
  connection,
  duplicateDraft = null,
  onSaved,
}: EditConnectionPropertiesModalProps) => {
  const { t } = useTranslation();
  const activeConnection = duplicateDraft?.connection ?? connection;
  const isDuplicateMode = Boolean(duplicateDraft);

  const [name, setName] = useState('');
  const [host, setHost] = useState('');
  const [port, setPort] = useState('22');
  const [username, setUsername] = useState('');
  const [authType, setAuthType] = useState<EditableAuthType>('password');
  const [keyPath, setKeyPath] = useState('');
  const [certPath, setCertPath] = useState('');
  const [managedKeyId, setManagedKeyId] = useState('');
  const [passphrase, setPassphrase] = useState('');
  const [password, setPassword] = useState('');
  const [passwordLoaded, setPasswordLoaded] = useState(false);
  const [passwordVisible, setPasswordVisible] = useState(false);
  const [passwordLoading, setPasswordLoading] = useState(false);
  const [passwordError, setPasswordError] = useState('');
  const [group, setGroup] = useState('');
  const [color, setColor] = useState('');
  const [postConnectCommand, setPostConnectCommand] = useState('');
  const [upstreamProxyPolicy, setUpstreamProxyPolicy] = useState<SavedUpstreamProxyPolicy>(
    defaultSavedUpstreamProxyPolicy,
  );
  const [groups, setGroups] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const [privilegeCredentials, setPrivilegeCredentials] = useState<SavedPrivilegeCredential[]>([]);
  const [privilegeDraft, setPrivilegeDraft] = useState<PrivilegeCredentialDraft>(EMPTY_PRIVILEGE_DRAFT);
  const [privilegeSaving, setPrivilegeSaving] = useState(false);
  const [privilegeError, setPrivilegeError] = useState('');
  // Capture connection snapshot at open time so handleSave never reads a stale prop
  const connectionRef = useRef<ConnectionInfo | null>(null);
  // Duplicate drafts carry secrets/proxy hops that are not present in ConnectionInfo.
  const duplicateDraftRef = useRef<DuplicateConnectionDraft | null>(null);

  useEffect(() => {
    if (isOpen && activeConnection) {
      connectionRef.current = activeConnection;
      duplicateDraftRef.current = duplicateDraft;
      setError('');
      setName(activeConnection.name || '');
      setHost(activeConnection.host || '');
      setPort(String(activeConnection.port || 22));
      setUsername(activeConnection.username || '');
      setAuthType(activeConnection.auth_type || 'password');
      setKeyPath(activeConnection.key_path || '');
      setCertPath(activeConnection.cert_path || '');
      setManagedKeyId(activeConnection.managed_key_id || '');
      setPassphrase('');
      setPassword('');
      setPasswordLoaded(false);
      setPasswordVisible(false);
      setPasswordLoading(false);
      setPasswordError('');
      setGroup(activeConnection.group || 'Ungrouped');
      setColor(activeConnection.color || '');
      setPostConnectCommand(activeConnection.post_connect_command || '');
      setUpstreamProxyPolicy(
        duplicateDraft?.saveRequest.upstream_proxy
          ?? activeConnection.upstream_proxy
          ?? defaultSavedUpstreamProxyPolicy(),
      );
      setPrivilegeCredentials([]);
      setPrivilegeDraft(EMPTY_PRIVILEGE_DRAFT);
      setPrivilegeError('');
      api.getGroups().then(setGroups).catch(() => setGroups([]));
      if (!duplicateDraft) {
        api.listPrivilegeCredentials(activeConnection.id)
          .then(setPrivilegeCredentials)
          .catch((e) => {
            console.error('Failed to load privilege credentials:', e);
            setPrivilegeCredentials([]);
          });
      }
    }
  }, [activeConnection, duplicateDraft, isOpen]);

  const handlePasswordVisibilityToggle = async () => {
    const conn = connectionRef.current;
    if (!conn) return;

    if (passwordLoaded) {
      setPasswordVisible((prev) => !prev);
      return;
    }

    setPasswordLoading(true);
    setPasswordError('');
    try {
      const draftPassword = duplicateDraftRef.current?.saveRequest.password;
      const storedPassword = duplicateDraftRef.current
        ? draftPassword ?? ''
        : await api.getConnectionPassword(conn.id);
      setPassword(storedPassword);
      setPasswordLoaded(true);
      setPasswordVisible(true);
    } catch (e) {
      console.error('Failed to load saved password:', e);
      setPasswordError(e instanceof Error ? e.message : String(e));
    } finally {
      setPasswordLoading(false);
    }
  };

  const handleBrowseKey = async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: t('modals.new_connection.browse_key'),
        defaultPath: '~/.ssh',
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
        filters: [{ name: 'Certificate', extensions: ['pub'] }],
      });
      if (selected && typeof selected === 'string') {
        setCertPath(selected);
      }
    } catch (e) {
      console.error('Failed to open file dialog:', e);
    }
  };

  const handleAuthTypeChange = (value: string) => {
    if (
      value === 'password'
      || value === 'key'
      || value === 'managed_key'
      || value === 'agent'
      || value === 'certificate'
    ) {
      setAuthType(value);
    }
  };

  const resetPrivilegeDraft = () => {
    setPrivilegeDraft(EMPTY_PRIVILEGE_DRAFT);
    setPrivilegeError('');
  };

  const startEditPrivilegeCredential = (credential: SavedPrivilegeCredential) => {
    setPrivilegeDraft({
      credentialId: credential.id,
      label: credential.label,
      kind: credential.kind,
      usernameHint: credential.username_hint ?? '',
      promptPatterns: credential.prompt_patterns.join('\n'),
      secret: '',
      enabled: credential.enabled,
    });
    setPrivilegeError('');
  };

  const handleSavePrivilegeCredential = async () => {
    const conn = connectionRef.current;
    if (!conn || isDuplicateMode) return;
    const label = privilegeDraft.label.trim();
    if (!label) return;

    setPrivilegeSaving(true);
    setPrivilegeError('');
    try {
      // The secret draft is intentionally sent only through this explicit save
      // action; Rust stores it in the privilege keychain namespace and persists
      // only non-secret metadata on the connection.
      const saved = await api.savePrivilegeCredential({
        connectionId: conn.id,
        credentialId: privilegeDraft.credentialId,
        label,
        kind: privilegeDraft.kind,
        usernameHint: privilegeDraft.usernameHint.trim() || null,
        promptPatterns: privilegeDraft.promptPatterns
          .split(/\r?\n/)
          .map((pattern) => pattern.trim())
          .filter(Boolean),
        secret: privilegeDraft.secret || null,
        enabled: privilegeDraft.enabled,
        requireClickToSend: true,
      });
      setPrivilegeCredentials((current) => {
        const index = current.findIndex((candidate) => candidate.id === saved.id);
        if (index === -1) return [...current, saved];
        const next = [...current];
        next[index] = saved;
        return next;
      });
      resetPrivilegeDraft();
    } catch (e) {
      console.error('Failed to save privilege credential:', e);
      setPrivilegeError(e instanceof Error ? e.message : String(e));
    } finally {
      setPrivilegeSaving(false);
    }
  };

  const handleDeletePrivilegeCredential = async (credential: SavedPrivilegeCredential) => {
    const conn = connectionRef.current;
    if (!conn || isDuplicateMode) return;

    setPrivilegeError('');
    try {
      await api.deletePrivilegeCredential(conn.id, credential.id);
      setPrivilegeCredentials((current) => current.filter((candidate) => candidate.id !== credential.id));
      if (privilegeDraft.credentialId === credential.id) {
        resetPrivilegeDraft();
      }
    } catch (e) {
      console.error('Failed to delete privilege credential:', e);
      setPrivilegeError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleSave = async () => {
    const conn = connectionRef.current;
    if (!conn || !host || !username) return;
    const draftRequest = duplicateDraftRef.current?.saveRequest;
    const usesKeyMaterial = authType === 'key' || authType === 'certificate';
    const usesPassphrase = usesKeyMaterial || authType === 'managed_key';
    setSaving(true);
    setError('');
    try {
      await api.saveConnection({
        ...(draftRequest ?? {}),
        id: duplicateDraftRef.current ? undefined : conn.id,
        name: name || `${username}@${host}`,
        group: group === 'Ungrouped' ? null : group,
        host,
        port: parseInt(port) || 22,
        username,
        auth_type: authType,
        password: authType === 'password'
          ? (passwordLoaded ? password : draftRequest?.password)
          : undefined,
        key_path: usesKeyMaterial ? keyPath : undefined,
        cert_path: authType === 'certificate' ? certPath : undefined,
        managed_key_id: authType === 'managed_key' ? managedKeyId : undefined,
        passphrase: usesPassphrase ? (passphrase || draftRequest?.passphrase) : undefined,
        color: color || undefined,
        tags: conn.tags,
        agent_forwarding: conn.agent_forwarding ?? draftRequest?.agent_forwarding,
        post_connect_command: postConnectCommand.trim(),
        proxy_chain: draftRequest?.proxy_chain,
        upstream_proxy: upstreamProxyPolicy,
      });
      onOpenChange(false);
      await onSaved?.();
    } catch (e) {
      console.error('Failed to save connection:', e);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  if (!activeConnection) return null;

  return (
    <Dialog open={isOpen} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px] max-h-[90vh] overflow-y-auto bg-theme-bg-elevated border-theme-border text-theme-text">
        <DialogHeader>
          <DialogTitle className="text-theme-text">
            {isDuplicateMode
              ? t('sessionManager.edit_properties.duplicate_title')
              : t('sessionManager.edit_properties.title')}
          </DialogTitle>
          <DialogDescription className="text-theme-text-muted">
            {isDuplicateMode
              ? t('sessionManager.edit_properties.duplicate_description')
              : t('sessionManager.edit_properties.description')}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6 p-4">
          {/* Name */}
          <div className="grid gap-2">
            <Label htmlFor="edit-name">{t('sessionManager.edit_properties.name')}</Label>
            <Input
              id="edit-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={`${username}@${host}`}
            />
          </div>

          {/* Host + Port */}
          <div className="grid grid-cols-4 gap-4">
            <div className="col-span-3 grid gap-2">
              <Label htmlFor="edit-host">{t('sessionManager.edit_properties.host')} *</Label>
              <Input
                id="edit-host"
                value={host}
                onChange={(e) => setHost(e.target.value)}
                placeholder="192.168.1.100"
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="edit-port">{t('sessionManager.edit_properties.port')}</Label>
              <Input
                id="edit-port"
                value={port}
                onChange={(e) => setPort(e.target.value)}
              />
            </div>
          </div>

          {/* Username */}
          <div className="grid gap-2">
            <Label htmlFor="edit-username">{t('sessionManager.edit_properties.username')} *</Label>
            <Input
              id="edit-username"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
            />
          </div>

          {/* Auth Type */}
          <div className="grid gap-2">
            <Label>{t('sessionManager.edit_properties.auth_type')}</Label>
            <Tabs value={authType} onValueChange={handleAuthTypeChange} className="w-full">
              <TabsList className="grid w-full grid-cols-5">
                <TabsTrigger value="password">{t('sessionManager.edit_properties.auth_password')}</TabsTrigger>
                <TabsTrigger value="key">{t('sessionManager.edit_properties.auth_key')}</TabsTrigger>
                <TabsTrigger value="managed_key">{t('modals.new_connection.auth_managed_key')}</TabsTrigger>
                <TabsTrigger value="certificate">{t('modals.new_connection.auth_certificate')}</TabsTrigger>
                <TabsTrigger value="agent">{t('sessionManager.edit_properties.auth_agent')}</TabsTrigger>
              </TabsList>

              <TabsContent value="password">
                <div className="space-y-2 pt-2">
                  <Label htmlFor="edit-password">{t('sessionManager.edit_properties.saved_password')}</Label>
                  <div className="relative">
                    <Input
                      id="edit-password"
                      type={passwordVisible ? 'text' : 'password'}
                      value={passwordLoaded ? password : ''}
                      onChange={(e) => {
                        setPassword(e.target.value);
                        setPasswordLoaded(true);
                        setPasswordError('');
                      }}
                      placeholder={t('sessionManager.edit_properties.password_placeholder')}
                      className="pr-11"
                    />
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      radius="sm"
                      onClick={handlePasswordVisibilityToggle}
                      disabled={passwordLoading}
                      className="absolute right-1 top-1 h-7 w-7 text-theme-text-muted hover:text-theme-text"
                      aria-label={passwordVisible
                        ? t('sessionManager.edit_properties.hide_password')
                        : t('sessionManager.edit_properties.show_password')}
                    >
                      {passwordLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : passwordVisible ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                    </Button>
                  </div>
                  <p className="text-xs text-theme-text-muted">
                    {t('sessionManager.edit_properties.password_hint')}
                  </p>
                  {passwordError && (
                    <p className="text-xs text-theme-error">{passwordError}</p>
                  )}
                </div>
              </TabsContent>

              <TabsContent value="key">
                <div className="space-y-2 pt-2">
                  <Label>{t('sessionManager.edit_properties.key_path')}</Label>
                  <div className="flex gap-2">
                    <Input
                      value={keyPath}
                      onChange={(e) => setKeyPath(e.target.value)}
                      placeholder="~/.ssh/id_rsa"
                    />
                    <Button variant="outline" size="sm" onClick={handleBrowseKey}>
                      {t('sessionManager.edit_properties.browse')}
                    </Button>
                  </div>
                  <div className="space-y-2">
                    <Label>{t('modals.edit_connection.passphrase')}</Label>
                    <Input
                      type="password"
                      value={passphrase}
                      onChange={(e) => setPassphrase(e.target.value)}
                      placeholder={t('modals.edit_connection.passphrase_placeholder')}
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

              <TabsContent value="certificate">
                <div className="space-y-2 pt-2">
                  <Label>{t('sessionManager.edit_properties.key_path')}</Label>
                  <div className="flex gap-2">
                    <Input
                      value={keyPath}
                      onChange={(e) => setKeyPath(e.target.value)}
                      placeholder="~/.ssh/id_rsa"
                    />
                    <Button variant="outline" size="sm" onClick={handleBrowseKey}>
                      {t('sessionManager.edit_properties.browse')}
                    </Button>
                  </div>
                  <div className="space-y-2">
                    <Label>{t('modals.new_connection.certificate')}</Label>
                    <div className="flex gap-2">
                      <Input
                        value={certPath}
                        onChange={(e) => setCertPath(e.target.value)}
                        placeholder={t('modals.new_connection.certificate_placeholder')}
                      />
                      <Button variant="outline" size="sm" onClick={handleBrowseCert}>
                        {t('sessionManager.edit_properties.browse')}
                      </Button>
                    </div>
                  </div>
                  <div className="space-y-2">
                    <Label>{t('modals.edit_connection.passphrase')}</Label>
                    <Input
                      type="password"
                      value={passphrase}
                      onChange={(e) => setPassphrase(e.target.value)}
                      placeholder={t('modals.edit_connection.passphrase_placeholder')}
                    />
                  </div>
                </div>
              </TabsContent>

              <TabsContent value="agent">
                <p className="text-xs text-theme-text-muted pt-2">
                  {t('sessionManager.edit_properties.agent_hint')}
                </p>
              </TabsContent>
            </Tabs>
          </div>

          {/* Group */}
          <div className="grid gap-2">
            <Label>{t('sessionManager.edit_properties.group')}</Label>
            <Select value={group} onValueChange={setGroup}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="Ungrouped">{t('sessionManager.edit_properties.ungrouped')}</SelectItem>
                {groups.map(g => (
                  <SelectItem key={g} value={g}>{g}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="grid gap-2">
            <Label htmlFor="edit-post-connect-command">
              {t('modals.new_connection.post_connect_command')}
            </Label>
            <Input
              id="edit-post-connect-command"
              value={postConnectCommand}
              onChange={(e) => setPostConnectCommand(e.target.value)}
              placeholder={t('modals.new_connection.post_connect_command_placeholder')}
            />
            <p className="text-xs text-theme-text-muted">
              {t('modals.new_connection.post_connect_command_hint')}
            </p>
          </div>

          <UpstreamProxyPolicyFields
            value={upstreamProxyPolicy}
            onChange={setUpstreamProxyPolicy}
          />

          <div className="grid gap-3 rounded-lg border border-theme-border/60 bg-theme-bg-panel/45 p-3">
            <div className="flex items-start justify-between gap-3">
              <div>
                <Label className="flex items-center gap-2">
                  <KeyRound className="h-4 w-4 text-theme-text-muted" />
                  {t('sessionManager.privilege_credentials.title')}
                </Label>
                <p className="mt-1 text-xs text-theme-text-muted">
                  {isDuplicateMode
                    ? t('sessionManager.privilege_credentials.duplicate_hint')
                    : t('sessionManager.privilege_credentials.description')}
                </p>
              </div>
              {!isDuplicateMode && (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={resetPrivilegeDraft}
                  className="gap-1"
                >
                  <Plus className="h-3.5 w-3.5" />
                  {t('sessionManager.privilege_credentials.new')}
                </Button>
              )}
            </div>

            {!isDuplicateMode && (
              <>
                <div className="space-y-2">
                  {privilegeCredentials.length === 0 ? (
                    <p className="rounded-md border border-dashed border-theme-border/50 px-3 py-2 text-xs text-theme-text-muted">
                      {t('sessionManager.privilege_credentials.empty')}
                    </p>
                  ) : (
                    privilegeCredentials.map((credential) => (
                      <div key={credential.id} className="flex items-center gap-2 rounded-md border border-theme-border/50 bg-theme-bg/45 px-2 py-1.5">
                        <KeyRound className="h-4 w-4 flex-shrink-0 text-amber-300" />
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-sm text-theme-text">{credential.label}</p>
                          <p className="truncate text-xs text-theme-text-muted">
                            {t(`sessionManager.privilege_credentials.kind.${credential.kind}`)}
                          </p>
                        </div>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() => startEditPrivilegeCredential(credential)}
                        >
                          {t('sessionManager.privilege_credentials.edit')}
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          radius="sm"
                          onClick={() => void handleDeletePrivilegeCredential(credential)}
                          aria-label={t('sessionManager.privilege_credentials.delete')}
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    ))
                  )}
                </div>

                <div className="grid gap-3 rounded-md border border-theme-border/50 bg-theme-bg/50 p-3">
                  <div className="grid gap-2">
                    <Label htmlFor="privilege-label">{t('sessionManager.privilege_credentials.label')}</Label>
                    <Input
                      id="privilege-label"
                      value={privilegeDraft.label}
                      onChange={(event) => setPrivilegeDraft((draft) => ({ ...draft, label: event.target.value }))}
                      placeholder={t('sessionManager.privilege_credentials.label_placeholder')}
                    />
                  </div>

                  <div className="grid grid-cols-2 gap-3">
                    <div className="grid gap-2">
                      <Label>{t('sessionManager.privilege_credentials.kind_label')}</Label>
                      <Select
                        value={privilegeDraft.kind}
                        onValueChange={(value) => setPrivilegeDraft((draft) => ({
                          ...draft,
                          kind: value as PrivilegeCredentialKind,
                        }))}
                      >
                        <SelectTrigger>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="sudo_password">{t('sessionManager.privilege_credentials.kind.sudo_password')}</SelectItem>
                          <SelectItem value="su_password">{t('sessionManager.privilege_credentials.kind.su_password')}</SelectItem>
                          <SelectItem value="custom_prompt">{t('sessionManager.privilege_credentials.kind.custom_prompt')}</SelectItem>
                        </SelectContent>
                      </Select>
                    </div>
                    <div className="grid gap-2">
                      <Label htmlFor="privilege-username">{t('sessionManager.privilege_credentials.username_hint')}</Label>
                      <Input
                        id="privilege-username"
                        value={privilegeDraft.usernameHint}
                        onChange={(event) => setPrivilegeDraft((draft) => ({ ...draft, usernameHint: event.target.value }))}
                        placeholder={username}
                      />
                    </div>
                  </div>

                  <div className="grid gap-2">
                    <Label htmlFor="privilege-secret">{t('sessionManager.privilege_credentials.secret')}</Label>
                    <Input
                      id="privilege-secret"
                      type="password"
                      value={privilegeDraft.secret}
                      onChange={(event) => setPrivilegeDraft((draft) => ({ ...draft, secret: event.target.value }))}
                      placeholder={privilegeDraft.credentialId
                        ? t('sessionManager.privilege_credentials.secret_keep_placeholder')
                        : t('sessionManager.privilege_credentials.secret_placeholder')}
                    />
                  </div>

                  <div className="grid gap-2">
                    <Label htmlFor="privilege-patterns">{t('sessionManager.privilege_credentials.prompt_patterns')}</Label>
                    <textarea
                      id="privilege-patterns"
                      value={privilegeDraft.promptPatterns}
                      onChange={(event) => setPrivilegeDraft((draft) => ({ ...draft, promptPatterns: event.target.value }))}
                      placeholder={t('sessionManager.privilege_credentials.prompt_patterns_placeholder')}
                      className="min-h-20 resize-y rounded-md border border-theme-border bg-theme-bg px-3 py-2 text-sm text-theme-text outline-none placeholder:text-theme-text-muted focus:border-theme-accent/60"
                    />
                    <p className="text-xs text-theme-text-muted">
                      {t('sessionManager.privilege_credentials.prompt_patterns_hint')}
                    </p>
                  </div>

                  <label className="flex items-center gap-2 text-sm text-theme-text">
                    <input
                      type="checkbox"
                      checked={privilegeDraft.enabled}
                      onChange={(event) => setPrivilegeDraft((draft) => ({ ...draft, enabled: event.target.checked }))}
                      className="h-4 w-4 rounded border-theme-border bg-theme-bg"
                    />
                    {t('sessionManager.privilege_credentials.enabled')}
                  </label>

                  {privilegeError && (
                    <p className="text-xs text-theme-error">{privilegeError}</p>
                  )}

                  <div className="flex justify-end gap-2">
                    {privilegeDraft.credentialId && (
                      <Button type="button" variant="ghost" size="sm" onClick={resetPrivilegeDraft}>
                        {t('sessionManager.privilege_credentials.cancel_edit')}
                      </Button>
                    )}
                    <Button
                      type="button"
                      size="sm"
                      onClick={() => void handleSavePrivilegeCredential()}
                      disabled={privilegeSaving || !privilegeDraft.label.trim()}
                      className="gap-1"
                    >
                      <SaveIcon className="h-3.5 w-3.5" />
                      {privilegeSaving
                        ? t('sessionManager.privilege_credentials.saving')
                        : t('sessionManager.privilege_credentials.save')}
                    </Button>
                  </div>
                </div>
              </>
            )}
          </div>

          {/* Color */}
          <div className="grid gap-2">
            <Label>{t('sessionManager.edit_properties.color')}</Label>
            <div className="flex items-center gap-3">
              <input
                type="color"
                value={color || '#22d3ee'}
                onChange={(e) => setColor(e.target.value)}
                className="w-9 h-9 rounded-md border border-theme-border cursor-pointer bg-transparent p-0.5"
              />
              {color && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setColor('')}
                  className="text-xs text-theme-text-muted"
                >
                  {t('sessionManager.edit_properties.clear_color')}
                </Button>
              )}
            </div>
          </div>
        </div>

        <DialogFooter>
          {error && (
            <p className="text-xs text-theme-error mr-auto self-center">{error}</p>
          )}
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t('sessionManager.edit_properties.cancel')}
          </Button>
          <Button onClick={handleSave} disabled={saving || !host || !username || (authType === 'managed_key' && !managedKeyId)}>
            {saving
              ? t(isDuplicateMode ? 'sessionManager.edit_properties.creating' : 'sessionManager.edit_properties.saving')
              : t(isDuplicateMode ? 'sessionManager.edit_properties.create' : 'sessionManager.edit_properties.save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
