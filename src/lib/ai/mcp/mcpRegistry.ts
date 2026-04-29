// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * MCP Server Registry — Zustand store for managing MCP server lifecycle
 */

import { create } from 'zustand';
import { useSettingsStore } from '../../../store/settingsStore';
import {
  connectMcpServer,
  disconnectMcpServer,
  callMcpTool,
  readMcpResource,
  refreshMcpTools,
} from './mcpClient';
import type {
  McpServerConfig,
  McpServerState,
  McpResource,
  McpResourceContent,
  McpCallToolResult,
  McpToolSchema,
} from './mcpTypes';
import type { AiToolDefinition } from '../providers';
import { createExternalToolSpec, type ToolSpec } from '../tools/toolDefinitions';

type McpRegistryState = {
  servers: Map<string, McpServerState>;
  /** Reverse lookup: prefixed tool name → { serverId, originalToolName } */
  toolIndex: Map<string, { serverId: string; originalName: string }>;
  /** Connect to an MCP server */
  connect: (configId: string) => Promise<void>;
  /** Disconnect from an MCP server */
  disconnect: (configId: string) => Promise<void>;
  /** Connect all enabled servers */
  connectAll: () => Promise<void>;
  /** Disconnect all servers */
  disconnectAll: () => Promise<void>;
  /** Call a tool on the appropriate MCP server */
  callTool: (serverId: string, toolName: string, args: Record<string, unknown>) => Promise<McpCallToolResult>;
  /** Get all tools from all connected servers as AiToolDefinitions */
  getAllMcpToolDefinitions: () => AiToolDefinition[];
  /** Get all tools from all connected servers as v3 ToolSpecs */
  getAllMcpToolSpecs: () => ToolSpec[];
  /** Find which server owns a tool and return the original tool name */
  findServerForTool: (toolName: string) => { server: McpServerState; originalName: string } | undefined;
  /** Refresh tools list for a server */
  refreshTools: (configId: string) => Promise<void>;
  /** Get all resources from all connected servers */
  getAllMcpResources: () => Array<McpResource & { serverId: string; serverName: string }>;
  /** Read a specific resource from a server */
  readResource: (serverId: string, uri: string) => Promise<McpResourceContent>;
};

function getServerConfigs(): McpServerConfig[] {
  return useSettingsStore.getState().settings.ai.mcpServers ?? [];
}

function buildServerNameCounts(servers: Iterable<McpServerState>): Map<string, number> {
  const counts = new Map<string, number>();
  for (const server of servers) {
    counts.set(server.config.name, (counts.get(server.config.name) ?? 0) + 1);
  }
  return counts;
}

function getServerToolNamespace(config: McpServerConfig, counts: Map<string, number>): string {
  if ((counts.get(config.name) ?? 0) <= 1) {
    return config.name;
  }
  return `${config.name}#${config.id}`;
}

function mcpToolToAiTool(tool: McpToolSchema, serverName: string, namespace: string): AiToolDefinition {
  return {
    name: `mcp::${namespace}::${tool.name}`,
    description: `[MCP: ${serverName}] ${tool.description ?? tool.name}`,
    parameters: tool.inputSchema as Record<string, unknown>,
  };
}

function mcpToolToToolSpec(tool: McpToolSchema, serverName: string, namespace: string): ToolSpec {
  return createExternalToolSpec({
    definition: mcpToolToAiTool(tool, serverName, namespace),
    domain: 'mcp',
    intentTags: ['knowledge'],
    requiredTarget: 'mcp_server',
    sideEffect: 'read',
    groupKey: 'mcp',
    readOnly: true,
    write: false,
    contextFree: true,
  });
}

// ═══════════════════════════════════════════════════════════════════════════
// Auto-Retry with Exponential Backoff
// ═══════════════════════════════════════════════════════════════════════════

const MAX_RETRIES = 3;
const RETRY_BASE_MS = 1000;
const retryCounters = new Map<string, number>();
const retryTimers = new Map<string, ReturnType<typeof setTimeout>>();
const connectGenerations = new Map<string, number>();

function clearRetryTimer(configId: string): void {
  const timer = retryTimers.get(configId);
  if (timer) {
    clearTimeout(timer);
    retryTimers.delete(configId);
  }
}

function resetRetryState(configId: string): void {
  clearRetryTimer(configId);
  retryCounters.delete(configId);
}

function nextConnectGeneration(configId: string): number {
  const generation = (connectGenerations.get(configId) ?? 0) + 1;
  connectGenerations.set(configId, generation);
  return generation;
}

function currentGeneration(configId: string): number {
  return connectGenerations.get(configId) ?? 0;
}

function isCurrentGeneration(configId: string, generation: number): boolean {
  return connectGenerations.get(configId) === generation;
}

function shouldRetry(configId: string): boolean {
  const config = getServerConfigs().find(server => server.id === configId);
  return Boolean(config?.enabled && config.transport !== 'stdio' && config.retryOnDisconnect);
}

async function applyRuntimeError(configId: string, generation: number, message: string): Promise<void> {
  if (!isCurrentGeneration(configId, generation)) {
    return;
  }

  const current = useMcpRegistry.getState().servers.get(configId);
  if (current?.config.transport === 'stdio' && current.runtimeId) {
    await disconnectMcpServer(current).catch(() => current);
  }

  if (!isCurrentGeneration(configId, generation)) {
    return;
  }

  useMcpRegistry.setState((state) => {
    const latest = state.servers.get(configId);
    if (!latest) {
      return state;
    }

    const servers = new Map(state.servers);
    servers.set(configId, {
      ...latest,
      status: 'error',
      error: message,
      runtimeId: undefined,
      tools: [],
      resources: [],
    });

    return { servers, toolIndex: rebuildToolIndex(servers) };
  });

  if (isCurrentGeneration(configId, generation) && shouldRetry(configId)) {
    scheduleRetry(configId, useMcpRegistry.getState().connect);
  }
}

function scheduleRetry(configId: string, connectFn: (id: string) => Promise<void>): void {
  clearRetryTimer(configId);
  const attempt = (retryCounters.get(configId) ?? 0) + 1;
  if (attempt > MAX_RETRIES) {
    console.warn(`[MCP] Giving up retry for ${configId} after ${MAX_RETRIES} attempts`);
    resetRetryState(configId);
    return;
  }
  retryCounters.set(configId, attempt);
  const delay = RETRY_BASE_MS * Math.pow(2, attempt - 1); // 1s, 2s, 4s
  console.info(`[MCP] Scheduling retry #${attempt} for ${configId} in ${delay}ms`);
  const timer = setTimeout(() => {
    retryTimers.delete(configId);
    if (!shouldRetry(configId)) {
      resetRetryState(configId);
      return;
    }
    const current = useMcpRegistry.getState().servers.get(configId);
    if (current?.status === 'connected' || current?.status === 'connecting') {
      return;
    }
    connectFn(configId).then(() => {
      // On success, reset counter
      const server = useMcpRegistry.getState().servers.get(configId);
      if (server?.status === 'connected') {
        resetRetryState(configId);
      }
    }).catch(() => {
      // connectFn already handles errors internally
    });
  }, delay);
  retryTimers.set(configId, timer);
}

/** Rebuild the toolIndex from current server states */
function rebuildToolIndex(servers: Map<string, McpServerState>): Map<string, { serverId: string; originalName: string }> {
  const index = new Map<string, { serverId: string; originalName: string }>();
  const nameCounts = buildServerNameCounts(servers.values());
  for (const server of servers.values()) {
    if (server.status !== 'connected') continue;
    const namespace = getServerToolNamespace(server.config, nameCounts);
    for (const tool of server.tools) {
      const prefixed = `mcp::${namespace}::${tool.name}`;
      index.set(prefixed, { serverId: server.config.id, originalName: tool.name });
    }
  }
  return index;
}

export const useMcpRegistry = create<McpRegistryState>((set, get) => ({
  servers: new Map(),
  toolIndex: new Map(),

  connect: async (configId: string) => {
    const configs = getServerConfigs();
    const config = configs.find(c => c.id === configId);
    if (!config) return;

    clearRetryTimer(configId);

    // Guard against double-connect
    const existing = get().servers.get(configId);
    if (existing?.status === 'connecting' || existing?.status === 'connected') return;

    const generation = nextConnectGeneration(configId);

    // Set connecting state
    set(state => {
      const servers = new Map(state.servers);
      servers.set(configId, { config, status: 'connecting', tools: [], resources: [] });
      return { servers };
    });

    const initial: McpServerState = { config, status: 'connecting', tools: [], resources: [] };
    const result = await connectMcpServer(initial);

    if (!isCurrentGeneration(configId, generation)) {
      if (result.runtimeId) {
        await disconnectMcpServer(result).catch(() => result);
      }
      return;
    }

    set(state => {
      const servers = new Map(state.servers);
      servers.set(configId, result);
      return { servers, toolIndex: rebuildToolIndex(servers) };
    });

    if (result.status === 'connected') {
      resetRetryState(configId);
      return;
    }

    // Schedule auto-retry on SSE connect failure if configured
    if (result.status === 'error' && shouldRetry(configId)) {
      scheduleRetry(configId, get().connect);
    }
  },

  disconnect: async (configId: string) => {
    const generation = nextConnectGeneration(configId);
    resetRetryState(configId);
    const current = get().servers.get(configId);
    if (!current) return;

    set((state) => {
      const servers = new Map(state.servers);
      servers.set(configId, {
        ...current,
        status: 'disconnected',
        runtimeId: undefined,
        endpointUrl: undefined,
        sessionId: undefined,
        resolvedTransport: undefined,
        error: undefined,
        tools: [],
        resources: [],
      });
      return { servers, toolIndex: rebuildToolIndex(servers) };
    });

    const result = await disconnectMcpServer(current);

    if (!isCurrentGeneration(configId, generation)) {
      return;
    }

    set(state => {
      const servers = new Map(state.servers);
      servers.set(configId, result);
      return { servers, toolIndex: rebuildToolIndex(servers) };
    });
  },

  connectAll: async () => {
    const configs = getServerConfigs().filter(c => c.enabled);
    await Promise.allSettled(configs.map(c => get().connect(c.id)));
  },

  disconnectAll: async () => {
    for (const serverId of Array.from(get().servers.keys())) {
      resetRetryState(serverId);
    }
    const serverIds = Array.from(get().servers.keys());
    await Promise.allSettled(serverIds.map(id => get().disconnect(id)));
  },

  callTool: async (serverId: string, toolName: string, args: Record<string, unknown>) => {
    const server = get().servers.get(serverId);
    if (!server || server.status !== 'connected') {
      throw new Error(`MCP server ${serverId} is not connected`);
    }
    const generation = currentGeneration(serverId);
    try {
      return await callMcpTool(server, toolName, args);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      await applyRuntimeError(serverId, generation, message);
      throw error;
    }
  },

  getAllMcpToolDefinitions: () => {
    const definitions: AiToolDefinition[] = [];
    const connectedServers = Array.from(get().servers.values()).filter((server) => server.status === 'connected');
    const nameCounts = buildServerNameCounts(connectedServers);
    for (const server of connectedServers) {
      const namespace = getServerToolNamespace(server.config, nameCounts);
      for (const tool of server.tools) {
        definitions.push(mcpToolToAiTool(tool, server.config.name, namespace));
      }
    }
    return definitions;
  },

  getAllMcpToolSpecs: () => {
    const specs: ToolSpec[] = [];
    const connectedServers = Array.from(get().servers.values()).filter((server) => server.status === 'connected');
    const nameCounts = buildServerNameCounts(connectedServers);
    for (const server of connectedServers) {
      const namespace = getServerToolNamespace(server.config, nameCounts);
      for (const tool of server.tools) {
        specs.push(mcpToolToToolSpec(tool, server.config.name, namespace));
      }
    }
    return specs;
  },

  findServerForTool: (toolName: string) => {
    const entry = get().toolIndex.get(toolName);
    if (!entry) return undefined;
    const server = get().servers.get(entry.serverId);
    if (!server || server.status !== 'connected') return undefined;
    return { server, originalName: entry.originalName };
  },

  refreshTools: async (configId: string) => {
    const current = get().servers.get(configId);
    if (!current || current.status !== 'connected') return;
    const generation = currentGeneration(configId);

    let tools: McpToolSchema[];
    try {
      tools = await refreshMcpTools(current);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      await applyRuntimeError(configId, generation, message);
      throw error;
    }

    if (!isCurrentGeneration(configId, generation)) {
      return;
    }

    set(state => {
      const latest = state.servers.get(configId);
      if (!latest || latest.status !== 'connected') {
        return state;
      }
      const servers = new Map(state.servers);
      servers.set(configId, { ...latest, tools });
      return { servers, toolIndex: rebuildToolIndex(servers) };
    });
  },

  getAllMcpResources: () => {
    const result: Array<McpResource & { serverId: string; serverName: string }> = [];
    for (const server of get().servers.values()) {
      if (server.status !== 'connected') continue;
      for (const res of server.resources) {
        result.push({ ...res, serverId: server.config.id, serverName: server.config.name });
      }
    }
    return result;
  },

  readResource: async (serverId: string, uri: string) => {
    const server = get().servers.get(serverId);
    if (!server || server.status !== 'connected') {
      throw new Error(`MCP server ${serverId} is not connected`);
    }
    const generation = currentGeneration(serverId);
    try {
      return await readMcpResource(server, uri);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      await applyRuntimeError(serverId, generation, message);
      throw error;
    }
  },
}));
