// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

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
  nodeAgentRemove,
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

/** Nodes with an active backend watch relay already started. */
const watchRelayReadyNodes = new Set<string>();

function markAgentUnavailable(nodeId: string): void {
  agentReadyCache.set(nodeId, false);
  watchRelayReadyNodes.delete(nodeId);
}

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
    if (ready) {
      agentReadyCache.set(nodeId, true);
    } else {
      markAgentUnavailable(nodeId);
    }
    return ready;
  } catch {
    markAgentUnavailable(nodeId);
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
    const status = await nodeAgentStatus(nodeId);
    switch (status.type) {
      case 'ready':
        return status;
      case 'manualUploadRequired':
      case 'manualUpdateRequired':
      case 'unsupportedArch':
        markAgentUnavailable(nodeId);
        return status;
      default:
        markAgentUnavailable(nodeId);
    }
  }

  // Dedupe concurrent deploys
  const existing = deployPromises.get(nodeId);
  if (existing) return existing;

  const promise = nodeAgentDeploy(nodeId)
    .then((status) => {
      if (status.type === 'ready') {
        agentReadyCache.set(nodeId, true);
      } else {
        markAgentUnavailable(nodeId);
      }
      deployPromises.delete(nodeId);
      return status;
    })
    .catch((err) => {
      markAgentUnavailable(nodeId);
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
  watchRelayReadyNodes.delete(nodeId);
}

/**
 * Remove the agent binary from a remote host.
 * Shuts down the running agent, deletes `~/.oxideterm/oxideterm-agent`,
 * and invalidates local cache.
 */
export async function removeAgent(nodeId: string): Promise<void> {
  await nodeAgentRemove(nodeId);
  invalidateAgentCache(nodeId);
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
      markAgentUnavailable(nodeId);
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
      markAgentUnavailable(nodeId);
    }
  }

  // SFTP fallback
  const result = await nodeSftpWrite(nodeId, path, content);
  return { mtime: result.mtime ?? undefined };
}

/**
 * List directory — agent (flat listing) or SFTP (single level).
 * When agent is available, returns a flattened single-level listing
 * (for compatibility with IdeTree's per-node expansion model).
 *
 * Uses max_depth=0 so the agent only reads the directory's direct children
 * without recursing into subdirectories. This keeps the entry count proportional
 * to the directory's actual size and prevents truncation caused by deep
 * subdirectory expansion inflating the shared count budget.
 */
export async function listDir(
  nodeId: string,
  path: string,
): Promise<FileInfo[]> {
  if (await isAgentReady(nodeId)) {
    try {
      const result = await nodeAgentListTree(nodeId, path, 0, 5000);
      return agentEntriesToFileInfoList(result.entries);
    } catch {
      markAgentUnavailable(nodeId);
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
      markAgentUnavailable(nodeId);
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
      markAgentUnavailable(nodeId);
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
      markAgentUnavailable(nodeId);
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

  let watchStarted = false;
  try {
    // Start the watch on the agent side
    await nodeAgentWatchStart(nodeId, path, ignore);
    watchStarted = true;

    // Start the relay (backend → frontend Tauri events) once per node.
    if (!watchRelayReadyNodes.has(nodeId)) {
      try {
        await nodeAgentStartWatchRelay(nodeId);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        if (!message.includes('Watch relay already started')) {
          throw error;
        }
      }
      watchRelayReadyNodes.add(nodeId);
    }

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
    if (watchStarted) {
      try {
        await nodeAgentWatchStop(nodeId, path);
      } catch {
        // Best effort cleanup only.
      }
    }
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
