// src/lib/agentService.ts
//
// Agent Service — agent-first + SFTP-fallback facade for IDE operations.
//
// This module provides a unified API that transparently tries the OxideTerm Agent
// first and falls back to SFTP/exec when the agent is unavailable.
// It also manages agent deployment lifecycle and watch event subscriptions.

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  nodeAgentDeploy,
  nodeAgentStatus,
  nodeAgentReadFile,
  nodeAgentWriteFile,
  nodeAgentListTree,
  nodeAgentGrep,
  nodeAgentGitStatus,
  nodeAgentWatchStart,
  nodeAgentWatchStop,
  nodeAgentStartWatchRelay,
  nodeAgentSymbolIndex,
  nodeAgentSymbolComplete,
  nodeAgentSymbolDefinitions,
  nodeSftpListDir,
  nodeSftpPreview,
  nodeSftpWrite,
} from './api';
import type {
  AgentStatus,
  AgentFileEntry,
  AgentListTreeResult,
  AgentGrepMatch,
  AgentGitStatusResult,
  AgentWatchEvent,
  AgentSymbolInfo,
  AgentSymbolIndexResult,
  FileInfo,
} from '../types';

// ═══════════════════════════════════════════════════════════════════════════
// Agent availability tracking (per node)
// ═══════════════════════════════════════════════════════════════════════════

/** Cache of agent readiness per nodeId */
const agentReadyCache = new Map<string, boolean>();

/** Cache of deployment in-flight promises to prevent duplicate deploys */
const deployPromises = new Map<string, Promise<AgentStatus>>();

/**
 * Check if the agent is available for a node (cached).
 * Returns `true` if the agent is deployed and ready.
 */
export async function isAgentReady(nodeId: string): Promise<boolean> {
  const cached = agentReadyCache.get(nodeId);
  if (cached !== undefined) return cached;

  try {
    const status = await nodeAgentStatus(nodeId);
    const ready = status.type === 'ready';
    agentReadyCache.set(nodeId, ready);
    return ready;
  } catch {
    agentReadyCache.set(nodeId, false);
    return false;
  }
}

/**
 * Deploy the agent to a node (idempotent, deduped).
 * Returns the deployment status. Does not throw on failure.
 */
export async function ensureAgent(nodeId: string): Promise<AgentStatus> {
  // Already ready?
  if (agentReadyCache.get(nodeId)) {
    return nodeAgentStatus(nodeId);
  }

  // Dedupe concurrent deploys
  const existing = deployPromises.get(nodeId);
  if (existing) return existing;

  const promise = nodeAgentDeploy(nodeId)
    .then((status) => {
      agentReadyCache.set(nodeId, status.type === 'ready');
      deployPromises.delete(nodeId);
      return status;
    })
    .catch((err) => {
      agentReadyCache.set(nodeId, false);
      deployPromises.delete(nodeId);
      return { type: 'failed', reason: String(err) } as AgentStatus;
    });

  deployPromises.set(nodeId, promise);
  return promise;
}

/**
 * Invalidate agent cache for a node (e.g. on disconnect).
 */
export function invalidateAgentCache(nodeId: string): void {
  agentReadyCache.delete(nodeId);
  deployPromises.delete(nodeId);
}

// ═══════════════════════════════════════════════════════════════════════════
// File Operations — agent-first + SFTP fallback
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Read a file — agent first (with hash), SFTP fallback.
 * Returns content, hash (agent only), and mtime.
 */
export async function readFile(
  nodeId: string,
  path: string,
): Promise<{ content: string; hash?: string; mtime?: number }> {
  if (await isAgentReady(nodeId)) {
    try {
      const result = await nodeAgentReadFile(nodeId, path);
      return {
        content: result.content,
        hash: result.hash,
        mtime: result.mtime,
      };
    } catch {
      // Agent failed — mark as unavailable and fallback
      agentReadyCache.set(nodeId, false);
    }
  }

  // SFTP fallback
  const preview = await nodeSftpPreview(nodeId, path);
  if ('Text' in preview) {
    return { content: preview.Text.data };
  }
  throw new Error('File is not a text file');
}

/**
 * Write a file atomically — agent first (with optimistic lock), SFTP fallback.
 * Returns mtime of written file.
 */
export async function writeFile(
  nodeId: string,
  path: string,
  content: string,
  expectHash?: string,
): Promise<{ mtime?: number; hash?: string }> {
  if (await isAgentReady(nodeId)) {
    try {
      const result = await nodeAgentWriteFile(nodeId, path, content, expectHash);
      return { mtime: result.mtime, hash: result.hash };
    } catch (err) {
      // If it's a hash conflict, propagate it — don't fallback
      if (String(err).includes('CONFLICT') || String(err).includes('hash mismatch') || String(err).includes('File modified externally')) {
        throw err;
      }
      agentReadyCache.set(nodeId, false);
    }
  }

  // SFTP fallback
  const result = await nodeSftpWrite(nodeId, path, content);
  return { mtime: result.mtime ?? undefined };
}

/**
 * List directory — agent (recursive tree) or SFTP (single level).
 * When agent is available, returns a flattened single-level listing
 * (for compatibility with IdeTree's per-node expansion model).
 */
export async function listDir(
  nodeId: string,
  path: string,
): Promise<FileInfo[]> {
  if (await isAgentReady(nodeId)) {
    try {
      const result = await nodeAgentListTree(nodeId, path, 1, 5000);
      return agentEntriesToFileInfoList(result.entries);
    } catch {
      agentReadyCache.set(nodeId, false);
    }
  }

  // SFTP fallback
  return nodeSftpListDir(nodeId, path);
}

/**
 * Recursive directory tree listing (agent only, no SFTP equivalent).
 * Returns entries + truncation flag. Falls back to null if agent unavailable.
 */
export async function listTree(
  nodeId: string,
  path: string,
  maxDepth?: number,
  maxEntries?: number,
): Promise<AgentListTreeResult | null> {
  if (await isAgentReady(nodeId)) {
    try {
      return await nodeAgentListTree(nodeId, path, maxDepth, maxEntries);
    } catch {
      agentReadyCache.set(nodeId, false);
    }
  }
  return null;
}

// ═══════════════════════════════════════════════════════════════════════════
// Search — agent grep or exec grep fallback
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Search files for a pattern. Agent grep is much faster than exec grep.
 */
export async function grep(
  nodeId: string,
  pattern: string,
  path: string,
  opts?: { caseSensitive?: boolean; maxResults?: number },
): Promise<AgentGrepMatch[] | null> {
  if (await isAgentReady(nodeId)) {
    try {
      return await nodeAgentGrep(
        nodeId,
        pattern,
        path,
        opts?.caseSensitive,
        opts?.maxResults,
      );
    } catch {
      agentReadyCache.set(nodeId, false);
    }
  }
  // Return null to signal caller should use exec fallback
  return null;
}

// ═══════════════════════════════════════════════════════════════════════════
// Git Status — agent or exec fallback
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Get git status. Returns null if agent unavailable (caller uses exec fallback).
 */
export async function gitStatus(
  nodeId: string,
  path: string,
): Promise<AgentGitStatusResult | null> {
  if (await isAgentReady(nodeId)) {
    try {
      return await nodeAgentGitStatus(nodeId, path);
    } catch {
      agentReadyCache.set(nodeId, false);
    }
  }
  return null;
}

// ═══════════════════════════════════════════════════════════════════════════
// File Watching — agent only (no SFTP equivalent)
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Start watching a directory and subscribe to change events.
 * Returns an unlisten function, or null if agent unavailable.
 */
export async function watchDirectory(
  nodeId: string,
  path: string,
  onEvent: (event: AgentWatchEvent) => void,
  ignore?: string[],
): Promise<UnlistenFn | null> {
  if (!(await isAgentReady(nodeId))) return null;

  try {
    // Start the watch on the agent side
    await nodeAgentWatchStart(nodeId, path, ignore);

    // Start the relay (backend → frontend Tauri events)
    await nodeAgentStartWatchRelay(nodeId);

    // Subscribe to the Tauri event
    const unlisten = await listen<AgentWatchEvent>(
      `agent:watch-event:${nodeId}`,
      (event) => {
        onEvent(event.payload);
      },
    );

    return async () => {
      unlisten();
      try {
        await nodeAgentWatchStop(nodeId, path);
      } catch {
        // Ignore — agent may already be gone
      }
    };
  } catch {
    return null;
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Symbol Operations — agent only (lightweight code intelligence)
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Index symbols in a project directory (agent only).
 * Returns indexed symbols + file count, or null if agent unavailable.
 */
export async function symbolIndex(
  nodeId: string,
  path: string,
  maxFiles?: number,
): Promise<AgentSymbolIndexResult | null> {
  if (!(await isAgentReady(nodeId))) return null;
  try {
    return await nodeAgentSymbolIndex(nodeId, path, maxFiles);
  } catch {
    return null;
  }
}

/**
 * Autocomplete symbol prefix (agent only).
 * Returns matching symbols sorted by relevance, or empty array.
 */
export async function symbolComplete(
  nodeId: string,
  path: string,
  prefix: string,
  limit?: number,
): Promise<AgentSymbolInfo[]> {
  if (!(await isAgentReady(nodeId))) return [];
  try {
    return await nodeAgentSymbolComplete(nodeId, path, prefix, limit);
  } catch {
    return [];
  }
}

/**
 * Find symbol definitions by exact name (agent only).
 * Returns all definitions matching the name.
 */
export async function symbolDefinitions(
  nodeId: string,
  path: string,
  name: string,
): Promise<AgentSymbolInfo[]> {
  if (!(await isAgentReady(nodeId))) return [];
  try {
    return await nodeAgentSymbolDefinitions(nodeId, path, name);
  } catch {
    return [];
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Convert agent FileEntry[] to the SFTP FileInfo[] format for compatibility.
 */
function agentEntriesToFileInfoList(entries: AgentFileEntry[]): FileInfo[] {
  return entries.map((e) => ({
    name: e.name,
    path: e.path,
    file_type: agentFileTypeToSftp(e.file_type),
    size: e.size,
    modified: e.mtime ?? null,
    permissions: e.permissions ?? null,
  }));
}

function agentFileTypeToSftp(
  ft: string,
): 'File' | 'Directory' | 'Symlink' | 'Unknown' {
  switch (ft) {
    case 'file':
      return 'File';
    case 'directory':
    case 'dir': // legacy alias
      return 'Directory';
    case 'symlink':
      return 'Symlink';
    default:
      return 'Unknown';
  }
}
