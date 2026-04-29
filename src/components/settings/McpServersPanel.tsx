// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * MCP Servers Settings Panel
 * 
 * Allows users to add, remove, configure, and manage MCP server connections.
 */

import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useSettingsStore } from '../../store/settingsStore';
import { useMcpRegistry, setMcpAuthToken, deleteMcpAuthToken } from '../../lib/ai/mcp';
import type { McpServerConfig, McpTransport, McpServerStatus } from '../../lib/ai/mcp';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Checkbox } from '../ui/checkbox';
import { Separator } from '../ui/separator';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../ui/select';
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
  DialogHeader,
  DialogFooter,
} from '../ui/dialog';
import { Plus, Trash2, Radio, CircleStop, RefreshCw, Wrench, Loader2, CheckCircle2, Eye, EyeOff } from 'lucide-react';

function generateId(): string {
  return `mcp-${crypto.randomUUID()}`;
}

function normalizeTransportLabel(transport: McpTransport): string {
  switch (transport) {
    case 'streamable-http':
    case 'sse':
      return 'Streamable HTTP';
    case 'legacy-sse':
      return 'Legacy SSE';
    case 'stdio':
      return 'stdio';
  }
}

function cleanRecord(value?: Record<string, string>): Record<string, string> | undefined {
  if (!value) return undefined;
  const entries = Object.entries(value)
    .map(([key, val]) => [key.trim(), val] as const)
    .filter(([key]) => key.length > 0);
  return entries.length > 0 ? Object.fromEntries(entries) : undefined;
}

function KeyValueEditor({
  value,
  onChange,
  keyPlaceholder,
  valuePlaceholder,
  addLabel,
}: {
  value?: Record<string, string>;
  onChange: (value: Record<string, string> | undefined) => void;
  keyPlaceholder: string;
  valuePlaceholder: string;
  addLabel: string;
}) {
  const entries = Object.entries(value ?? {});
  const commit = (next: Record<string, string>) => onChange(cleanRecord(next));

  return (
    <div className="space-y-2">
      {entries.map(([key, val], index) => (
        <div key={`${key}-${index}`} className="flex gap-2">
          <Input
            value={key}
            onChange={(e) => {
              const nextKey = e.target.value;
              const next = { ...(value ?? {}) };
              delete next[key];
              if (nextKey.trim()) next[nextKey] = val;
              commit(next);
            }}
            placeholder={keyPlaceholder}
            className="flex-1"
          />
          <Input
            value={val}
            onChange={(e) => commit({ ...(value ?? {}), [key]: e.target.value })}
            placeholder={valuePlaceholder}
            className="flex-1"
          />
          <Button
            variant="ghost"
            size="sm"
            type="button"
            onClick={() => {
              const next = { ...(value ?? {}) };
              delete next[key];
              commit(next);
            }}
            className="h-9 w-9 p-0 shrink-0 text-red-400 hover:text-red-300"
          >
            <Trash2 className="w-4 h-4" />
          </Button>
        </div>
      ))}
      <Button
        variant="outline"
        size="sm"
        type="button"
        onClick={() => {
          const next = { ...(value ?? {}) };
          let index = entries.length + 1;
          let key = `KEY_${index}`;
          while (key in next) {
            index += 1;
            key = `KEY_${index}`;
          }
          next[key] = '';
          commit(next);
        }}
        className="gap-1.5"
      >
        <Plus className="w-3.5 h-3.5" />
        {addLabel}
      </Button>
    </div>
  );
}

function StatusBadge({ status }: { status: McpServerStatus }) {
  const { t } = useTranslation();
  const styles: Record<McpServerStatus, string> = {
    disconnected: 'bg-theme-text-muted/20 text-theme-text-muted',
    connecting: 'bg-yellow-500/20 text-yellow-400',
    connected: 'bg-emerald-500/20 text-emerald-400',
    error: 'bg-red-500/20 text-red-400',
  };
  const labels: Record<McpServerStatus, string> = {
    disconnected: t('settings_view.mcp.status_disconnected'),
    connecting: t('settings_view.mcp.status_connecting'),
    connected: t('settings_view.mcp.status_connected'),
    error: t('settings_view.mcp.status_error'),
  };

  return (
    <span className={`inline-flex items-center gap-1 px-2 py-0.5 text-[10px] font-medium rounded ${styles[status]}`}>
      {status === 'connecting' && <Loader2 className="w-3 h-3 animate-spin" />}
      {status === 'connected' && <CheckCircle2 className="w-3 h-3" />}
      {labels[status]}
    </span>
  );
}

export function McpServersPanel() {
  const { t } = useTranslation();
  const ai = useSettingsStore((s) => s.settings.ai);
  const updateAi = useSettingsStore((s) => s.updateAi);
  const servers = useMcpRegistry((s) => s.servers);
  const connect = useMcpRegistry((s) => s.connect);
  const disconnect = useMcpRegistry((s) => s.disconnect);
  const refreshTools = useMcpRegistry((s) => s.refreshTools);

  const [showAddDialog, setShowAddDialog] = useState(false);
  const [showAuthToken, setShowAuthToken] = useState(false);
  const [newServer, setNewServer] = useState<Partial<McpServerConfig>>({
    transport: 'stdio',
    enabled: true,
  });

  const mcpServers: McpServerConfig[] = ai.mcpServers ?? [];

  const isValidName = (name: string) => /^[a-zA-Z0-9-]+$/.test(name);
  const isNameTaken = (name: string) => mcpServers.some(s => s.name === name);

  const addServer = useCallback(async () => {
    if (!newServer.name || !isValidName(newServer.name) || isNameTaken(newServer.name)) return;
    const id = generateId();
    const transport: McpTransport = newServer.transport === 'sse'
      ? 'streamable-http'
      : (newServer.transport ?? 'stdio');
    const env = cleanRecord(newServer.env);
    const headers = cleanRecord(newServer.headers);
    const config: McpServerConfig = {
      id,
      name: newServer.name,
      transport,
      url: newServer.url,
      command: newServer.command,
      args: newServer.args?.length ? newServer.args : undefined,
      env,
      authHeaderName: newServer.authHeaderName?.trim() || undefined,
      authHeaderMode: newServer.authHeaderMode,
      headers,
      enabled: true,
      // authToken intentionally omitted — stored in OS keychain
      retryOnDisconnect: newServer.retryOnDisconnect,
    };
    // Save auth token to OS keychain FIRST (before config, to avoid config-keychain mismatch)
    if (newServer.authToken && newServer.authHeaderMode !== 'none') {
      try {
        await setMcpAuthToken(id, newServer.authToken);
      } catch (e) {
        console.error('[MCP] Failed to save auth token to keychain:', e);
        return; // abort — don't save config without token
      }
    }
    updateAi('mcpServers', [...mcpServers, config]);
    setNewServer({ transport: 'stdio', enabled: true });
    setShowAuthToken(false);
    setShowAddDialog(false);
  }, [newServer, mcpServers, updateAi]);

  const removeServer = useCallback(async (id: string) => {
    // Disconnect first if connected
    const state = servers.get(id);
    if (state && state.status === 'connected') {
      await disconnect(id);
    }
    // Remove auth token from OS keychain
    await deleteMcpAuthToken(id).catch(() => {});
    updateAi('mcpServers', mcpServers.filter(s => s.id !== id));
  }, [mcpServers, servers, disconnect, updateAi]);

  const toggleServer = useCallback(async (id: string) => {
    const state = servers.get(id);
    if (state?.status === 'connected') {
      await disconnect(id);
    } else {
      await connect(id);
    }
  }, [servers, connect, disconnect]);

  return (
    <div className="space-y-6 animate-in fade-in slide-in-from-bottom-2 duration-300 mt-8">
      <Separator className="opacity-30" />
      <div>
        <div className="flex items-center justify-between mb-4">
          <div>
            <h3 className="text-lg font-medium text-theme-text-heading">{t('settings_view.mcp.title')}</h3>
            <p className="text-sm text-theme-text-muted mt-1">{t('settings_view.mcp.description')}</p>
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setShowAddDialog(true)}
            className="gap-1.5"
          >
            <Plus className="w-3.5 h-3.5" />
            {t('settings_view.mcp.add_server')}
          </Button>
        </div>

        {mcpServers.length === 0 ? (
          <div className="text-center py-8 text-theme-text-muted text-sm border border-dashed border-theme-border/40 rounded-lg">
            {t('settings_view.mcp.no_servers')}
          </div>
        ) : (
          <div className="space-y-3">
            {mcpServers.map((config) => {
              const state = servers.get(config.id);
              const status: McpServerStatus = state?.status ?? 'disconnected';
              const tools = state?.tools ?? [];

              return (
                <div
                  key={config.id}
                  className="border border-theme-border/40 rounded-lg p-4 bg-theme-bg-panel/30"
                >
                  <div className="flex items-center justify-between mb-2">
                    <div className="flex items-center gap-3">
                      <span className="font-medium text-sm text-theme-text">{config.name}</span>
                      <StatusBadge status={status} />
                      <span className="text-[10px] uppercase tracking-wider text-theme-text-muted px-1.5 py-0.5 bg-theme-bg-panel rounded">
                        {normalizeTransportLabel(config.transport)}
                      </span>
                      {state?.resolvedTransport === 'legacy-sse' && config.transport !== 'legacy-sse' && (
                        <span className="text-[10px] uppercase tracking-wider text-amber-300 px-1.5 py-0.5 bg-amber-500/10 rounded">
                          {t('settings_view.mcp.fallback_legacy_sse')}
                        </span>
                      )}
                    </div>
                    <div className="flex items-center gap-1.5">
                      {status === 'connected' && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => refreshTools(config.id)}
                          className="h-7 w-7 p-0"
                          title={t('settings_view.mcp.refresh_tools')}
                        >
                          <RefreshCw className="w-3.5 h-3.5" />
                        </Button>
                      )}
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => toggleServer(config.id)}
                        className="h-7 px-2 gap-1"
                      >
                        {status === 'connected' ? (
                          <><CircleStop className="w-3.5 h-3.5" /> {t('settings_view.mcp.disconnect')}</>
                        ) : status === 'connecting' ? (
                          <><Loader2 className="w-3.5 h-3.5 animate-spin" /> {t('settings_view.mcp.connecting')}</>
                        ) : (
                          <><Radio className="w-3.5 h-3.5" /> {t('settings_view.mcp.connect')}</>
                        )}
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => removeServer(config.id)}
                        className="h-7 w-7 p-0 text-red-400 hover:text-red-300"
                        title={t('settings_view.mcp.remove')}
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </Button>
                    </div>
                  </div>

                  <div className="text-xs text-theme-text-muted">
                    {config.transport === 'stdio' && config.command && (
                      <code className="bg-theme-bg-panel/60 px-1.5 py-0.5 rounded">{config.command} {config.args?.join(' ')}</code>
                    )}
                    {config.transport !== 'stdio' && config.url && (
                      <code className="bg-theme-bg-panel/60 px-1.5 py-0.5 rounded">
                        {state?.endpointUrl ?? config.url}
                      </code>
                    )}
                  </div>

                  {state?.error && (
                    <p className="text-xs text-red-400 mt-2">{state.error}</p>
                  )}

                  {tools.length > 0 && (
                    <div className="mt-3 pt-2 border-t border-theme-border/20">
                      <div className="flex items-center gap-1.5 mb-1.5">
                        <Wrench className="w-3 h-3 text-theme-text-muted" />
                        <span className="text-[10px] uppercase tracking-wider text-theme-text-muted">
                          {t('settings_view.mcp.tools_count', { count: tools.length })}
                        </span>
                      </div>
                      <div className="flex flex-wrap gap-1">
                        {tools.map((tool) => (
                          <span
                            key={tool.name}
                            className="px-1.5 py-0.5 text-[10px] bg-theme-bg-panel/60 rounded text-theme-text-muted"
                            title={tool.description}
                          >
                            {tool.name}
                          </span>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Add Server Dialog */}
      <Dialog open={showAddDialog} onOpenChange={setShowAddDialog}>
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t('settings_view.mcp.add_server_title')}</DialogTitle>
            <DialogDescription>{t('settings_view.mcp.add_server_description')}</DialogDescription>
          </DialogHeader>

          <div className="px-4 py-2 space-y-4">
            <div className="space-y-2">
              <Label>{t('settings_view.mcp.server_name')}</Label>
              <Input
                value={newServer.name ?? ''}
                onChange={(e) => setNewServer(prev => ({ ...prev, name: e.target.value }))}
                placeholder="my-mcp-server"
              />
            </div>

            <div className="space-y-2">
              <Label>{t('settings_view.mcp.transport')}</Label>
              <Select
                value={newServer.transport ?? 'stdio'}
                onValueChange={(v) => setNewServer(prev => ({ ...prev, transport: v as McpTransport }))}
              >
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="stdio">stdio</SelectItem>
                  <SelectItem value="streamable-http">Streamable HTTP (auto fallback)</SelectItem>
                  <SelectItem value="legacy-sse">Legacy SSE</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {newServer.transport === 'stdio' && (
              <>
                <div className="space-y-2">
                  <Label>{t('settings_view.mcp.command')}</Label>
                  <Input
                    value={newServer.command ?? ''}
                    onChange={(e) => setNewServer(prev => ({ ...prev, command: e.target.value }))}
                    placeholder="npx -y @modelcontextprotocol/server-example"
                  />
                </div>
                <div className="space-y-2">
                  <Label>{t('settings_view.mcp.args')}</Label>
                  <Input
                    value={newServer.args?.join(' ') ?? ''}
                    onChange={(e) => setNewServer(prev => ({ ...prev, args: e.target.value.split(' ').filter(Boolean) }))}
                    placeholder="--flag value"
                  />
                </div>
                <div className="space-y-2">
                  <Label>{t('settings_view.mcp.env_vars')}</Label>
                  <KeyValueEditor
                    value={newServer.env}
                    onChange={(env) => setNewServer(prev => ({ ...prev, env }))}
                    keyPlaceholder={t('settings_view.mcp.env_key_placeholder')}
                    valuePlaceholder={t('settings_view.mcp.env_value_placeholder')}
                    addLabel={t('settings_view.mcp.add_env_var')}
                  />
                </div>
              </>
            )}

            {newServer.transport !== 'stdio' && (
              <>
                <div className="space-y-2">
                  <Label>{t('settings_view.mcp.url')}</Label>
                  <Input
                    value={newServer.url ?? ''}
                    onChange={(e) => setNewServer(prev => ({ ...prev, url: e.target.value }))}
                    placeholder="http://localhost:3000"
                  />
                </div>
                <div className="grid grid-cols-2 gap-3">
                  <div className="space-y-2">
                    <Label>{t('settings_view.mcp.auth_header_name')}</Label>
                    <Input
                      value={newServer.authHeaderName ?? 'Authorization'}
                      onChange={(e) => setNewServer(prev => ({ ...prev, authHeaderName: e.target.value }))}
                      placeholder="Authorization"
                    />
                  </div>
                  <div className="space-y-2">
                    <Label>{t('settings_view.mcp.auth_header_mode')}</Label>
                    <Select
                      value={newServer.authHeaderMode ?? 'bearer'}
                      onValueChange={(v) => setNewServer(prev => ({ ...prev, authHeaderMode: v as McpServerConfig['authHeaderMode'] }))}
                    >
                      <SelectTrigger><SelectValue /></SelectTrigger>
                      <SelectContent>
                        <SelectItem value="bearer">{t('settings_view.mcp.auth_header_mode_bearer')}</SelectItem>
                        <SelectItem value="raw">{t('settings_view.mcp.auth_header_mode_raw')}</SelectItem>
                        <SelectItem value="none">{t('settings_view.mcp.auth_header_mode_none')}</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                </div>
                <div className="space-y-2">
                  <Label>{t('settings_view.mcp.auth_token')}</Label>
                  <div className="flex gap-2">
                    <Input
                      type={showAuthToken ? 'text' : 'password'}
                      value={newServer.authToken ?? ''}
                      onChange={(e) => setNewServer(prev => ({ ...prev, authToken: e.target.value }))}
                      placeholder={t('settings_view.mcp.auth_token_placeholder')}
                      className="flex-1"
                    />
                    <Button
                      variant="ghost"
                      size="sm"
                      type="button"
                      onClick={() => setShowAuthToken(v => !v)}
                      className="h-9 w-9 p-0 shrink-0"
                    >
                      {showAuthToken ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                    </Button>
                  </div>
                </div>
                <div className="space-y-2">
                  <Label>{t('settings_view.mcp.extra_headers')}</Label>
                  <KeyValueEditor
                    value={newServer.headers}
                    onChange={(headers) => setNewServer(prev => ({ ...prev, headers }))}
                    keyPlaceholder={t('settings_view.mcp.header_key_placeholder')}
                    valuePlaceholder={t('settings_view.mcp.header_value_placeholder')}
                    addLabel={t('settings_view.mcp.add_header')}
                  />
                  <p className="text-[11px] text-theme-text-muted">
                    {t('settings_view.mcp.extra_headers_hint')}
                  </p>
                </div>
                <div className="flex items-center justify-between">
                  <Label htmlFor="retry-toggle">{t('settings_view.mcp.retry_on_disconnect')}</Label>
                  <Checkbox
                    id="retry-toggle"
                    checked={newServer.retryOnDisconnect ?? false}
                    onCheckedChange={(v) => setNewServer(prev => ({ ...prev, retryOnDisconnect: v === true }))}
                  />
                </div>
              </>
            )}
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setShowAddDialog(false)}>
              {t('settings_view.mcp.cancel')}
            </Button>
            <Button onClick={addServer} disabled={!newServer.name || !isValidName(newServer.name) || isNameTaken(newServer.name)}>
              {t('settings_view.mcp.add')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
