// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

// src/store/ideStore.ts
import { create } from 'zustand';
import { subscribeWithSelector, persist } from 'zustand/middleware';
import {
  nodeSftpInit,
  nodeSftpStat,
  nodeSftpMkdir,
  nodeSftpDelete,
  nodeSftpDeleteRecursive,
  nodeSftpRename,
  nodeIdeOpenProject,
  nodeIdeCheckFile,
} from '../lib/api';
import * as agentService from '../lib/agentService';
import {
  normalizePath,
  joinPath,
  getParentPath,
  getBaseName,
  validateFileName,
} from '../lib/pathUtils';
import { useSessionTreeStore } from './sessionTreeStore';
import { useSettingsStore } from './settingsStore';

// ═══════════════════════════════════════════════════════════════════════════
// State Gating: IO 操作前校验节点连接状态
// ═══════════════════════════════════════════════════════════════════════════

/**
 * 断言节点处于可用状态（connected 或 active），否则抛出错误。
 * 使用 sessionTreeStore 的 getNode() 避免 IPC 开销。
 */
function assertNodeReady(nodeId: string): void {
  const node = useSessionTreeStore.getState().getNode(nodeId);
  if (!node) {
    throw new Error('Node not found in session tree');
  }
  const status = node.runtime?.status;
  if (status !== 'active' && status !== 'connected') {
    throw new Error(`Node is not connected (status: ${status ?? 'unknown'})`);
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// 搜索缓存清除回调（由 IdeSearchPanel 注册）
// ═══════════════════════════════════════════════════════════════════════════
let onSearchCacheClear: (() => void) | null = null;

/** 注册搜索缓存清除回调 */
export function registerSearchCacheClearCallback(callback: () => void) {
  onSearchCacheClear = callback;
}

/** 触发搜索缓存清除 */
function triggerSearchCacheClear() {
  onSearchCacheClear?.();
}

// ═══════════════════════════════════════════════════════════════════════════
// Git 状态刷新回调（由 useGitStatus 注册）
// ═══════════════════════════════════════════════════════════════════════════
let onGitRefresh: (() => void) | null = null;

/** 注册 Git 刷新回调 */
export function registerGitRefreshCallback(callback: () => void) {
  onGitRefresh = callback;
}

/** 触发 Git 状态刷新（保存文件、终端回车等行为后调用） */
export function triggerGitRefresh() {
  onGitRefresh?.();
}

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

export interface IdeTab {
  id: string;
  path: string;           // 远程文件完整路径
  name: string;           // 文件名（显示用）
  language: string;       // CodeMirror 语言标识
  content: string | null; // null = 尚未加载
  originalContent: string | null; // 打开时的原始内容（用于 diff/dirty 检测）
  isDirty: boolean;
  isLoading: boolean;
  isPinned: boolean;      // 是否已 Pin（不参与 LRU 驱逐）
  cursor?: { line: number; col: number };
  serverMtime?: number;   // 服务器端文件修改时间（Unix timestamp 秒）
  agentHash?: string;     // Agent 乐观锁 hash（agent 可用时才有值）
  lastAccessTime: number; // 最后访问时间（用于 LRU 驱逐）
  contentVersion: number; // 内容版本号，用于强制编辑器刷新（冲突 reload 等场景）
}

export interface IdeProject {
  rootPath: string;
  name: string;
  isGitRepo: boolean;
  gitBranch?: string;
}

interface IdeState {
  // ─── 会话关联 ───
  nodeId: string | null;            // Node ID（node-first 唯一标识）
  terminalSessionId: string | null; // 终端会话 ID（可选）
  
  // ─── 项目状态 ───
  project: IdeProject | null;
  
  // ─── 编辑器状态 ───
  tabs: IdeTab[];
  activeTabId: string | null;
  
  // ─── 布局状态 ───
  treeWidth: number;
  terminalHeight: number;
  terminalVisible: boolean;
  
  // ─── 分栏编辑器 ───
  splitDirection: 'horizontal' | 'vertical' | null; // null = 无分栏
  splitActiveTabId: string | null; // 分栏侧激活的标签 ID
  
  // ─── 文件树状态 ───
  expandedPaths: Set<string>;  // 展开的目录路径
  treeRefreshSignal: Record<string, number>; // 树刷新信号 { path: version }
  
  // ─── 冲突状态 ───
  conflictState: {
    tabId: string;
    localMtime: number;
    remoteMtime: number;
  } | null;
  
  // ─── 搜索跳转 ───
  pendingScroll: { tabId: string; line: number; col?: number } | null;

  // ─── 重连恢复缓存 ───
  cachedProjectPath: string | null;
  cachedTabPaths: string[];
  cachedNodeId: string | null;

  // ─── 重连用户意图追踪 ───
  /** Timestamp of the last user-initiated closeProject, used by reconnect orchestrator */
  lastClosedAt: number | null;
}

interface IdeActions {
  // 项目操作
  openProject: (nodeId: string, rootPath: string) => Promise<void>;
  closeProject: (force?: boolean) => void;
  changeRootPath: (newRootPath: string) => Promise<void>;
  
  // 文件操作
  openFile: (path: string) => Promise<void>;
  closeTab: (tabId: string) => Promise<boolean>;
  closeAllTabs: () => Promise<boolean>;
  saveFile: (tabId: string) => Promise<void>;
  saveAllFiles: () => Promise<void>;
  
  // 标签操作
  setActiveTab: (tabId: string) => void;
  updateTabContent: (tabId: string, content: string) => void;
  updateTabCursor: (tabId: string, line: number, col: number) => void;
  togglePinTab: (tabId: string) => void;
  reorderTabs: (orderedIds: string[]) => void;
  
  // AI 精确编辑操作
  replaceStringInTab: (tabId: string, oldStr: string, newStr: string) => { success: boolean; error?: string };
  insertTextInTab: (tabId: string, line: number, text: string) => { success: boolean; insertedAtLine?: number; error?: string };
  
  // 布局操作
  setTreeWidth: (width: number) => void;
  setTerminalHeight: (height: number) => void;
  toggleTerminal: () => void;
  
  // 分栏编辑器
  splitEditor: (direction?: 'horizontal' | 'vertical') => void;
  closeSplit: () => void;
  setSplitActiveTab: (tabId: string) => void;
  
  // 文件树操作
  togglePath: (path: string) => void;
  
  // 终端操作
  setTerminalSession: (sessionId: string | null) => void;
  
  // 冲突处理
  resolveConflict: (resolution: 'overwrite' | 'reload') => Promise<void>;
  clearConflict: () => void;
  
  // 搜索跳转
  setPendingScroll: (tabId: string, line: number, col?: number) => void;
  clearPendingScroll: () => void;
  
  // 文件系统操作（CRUD）
  createFile: (parentPath: string, name: string) => Promise<string>;
  createFolder: (parentPath: string, name: string) => Promise<string>;
  deleteItem: (path: string, isDirectory: boolean) => Promise<void>;
  renameItem: (oldPath: string, newName: string) => Promise<string>;
  refreshTreeNode: (parentPath: string) => void;
  getAffectedTabs: (path: string) => { affected: IdeTab[]; unsaved: IdeTab[] };
  
  // 内部方法
  _findTabByPath: (path: string) => IdeTab | undefined;
}

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

const MAX_OPEN_TABS = 20;

// ═══════════════════════════════════════════════════════════════════════════
// Store
// ═══════════════════════════════════════════════════════════════════════════

// Module-level throttle map for updateTabCursor debouncing
const _cursorThrottleTimers = new Map<string, ReturnType<typeof setTimeout>>();

export const useIdeStore = create<IdeState & IdeActions>()(
  subscribeWithSelector(
    persist(
      (set, get) => ({
        // ─── Initial State ───
        nodeId: null,
        terminalSessionId: null,
        project: null,
        tabs: [],
        activeTabId: null,
        treeWidth: 280,
        terminalHeight: 200,
        terminalVisible: false,
        splitDirection: null,
        splitActiveTabId: null,
        expandedPaths: new Set<string>(),
        treeRefreshSignal: {},
        conflictState: null,
        pendingScroll: null,
        cachedProjectPath: null,
        cachedTabPaths: [],
        cachedNodeId: null,
        lastClosedAt: null,

        // ─── Project Actions ───
        openProject: async (nodeId, rootPath) => {
          const currentState = get();
          
          // 如果已经打开了相同的项目，不要重置状态
          if (currentState.project?.rootPath === rootPath && 
              currentState.nodeId === nodeId) {
            return;
          }
          
          // State Gating: 确认节点处于可用状态
          assertNodeReady(nodeId);
          
          // node-first: nodeSftpInit 是幂等的，总是安全调用
          await nodeSftpInit(nodeId);
          
          // Deploy agent based on agentMode setting
          const agentMode = useSettingsStore.getState().getIde().agentMode;
          if (agentMode === 'enabled') {
            agentService.ensureAgent(nodeId).catch(() => {
              // Agent deployment is optional — IDE works with SFTP alone
            });
          }
          // If agentMode === 'ask', IdeWorkspace will show opt-in dialog
          // If agentMode === 'disabled', skip agent entirely
          
          // 调用后端获取项目信息
          const projectInfo = await nodeIdeOpenProject(nodeId, rootPath);
          
          set({
            nodeId,
            project: {
              rootPath: projectInfo.rootPath,
              name: projectInfo.name,
              isGitRepo: projectInfo.isGitRepo,
              gitBranch: projectInfo.gitBranch ?? undefined,
            },
            tabs: [],
            activeTabId: null,
            expandedPaths: new Set([projectInfo.rootPath]), // 默认展开根目录
            cachedProjectPath: projectInfo.rootPath,
            cachedTabPaths: [],
          });
        },

        closeProject: (force?: boolean) => {
          const { tabs } = get();
          const hasDirty = tabs.some(t => t.isDirty);
          
          if (hasDirty && !force) {
            throw new Error('IDE_DIRTY_TABS');
          }
          
          set({
            nodeId: null,
            terminalSessionId: null,
            project: null,
            tabs: [],
            activeTabId: null,
            expandedPaths: new Set(),
            conflictState: null,
            splitDirection: null,
            splitActiveTabId: null,
            cachedProjectPath: null,
            cachedTabPaths: [],
            cachedNodeId: null,
            lastClosedAt: Date.now(),
          });
        },

        changeRootPath: async (newRootPath: string) => {
          const { nodeId, tabs } = get();
          
          if (!nodeId) {
            throw new Error('No active session');
          }
          
          // State Gating
          assertNodeReady(nodeId);
          
          // 检查是否有未保存的文件
          const hasDirty = tabs.some(t => t.isDirty);
          if (hasDirty) {
            throw new Error('Please save all files before changing root directory');
          }
          
          // 调用后端获取新项目信息
          const projectInfo = await nodeIdeOpenProject(nodeId, newRootPath);
          
          // 更新状态，关闭所有标签
          set({
            project: {
              rootPath: projectInfo.rootPath,
              name: projectInfo.name,
              isGitRepo: projectInfo.isGitRepo,
              gitBranch: projectInfo.gitBranch ?? undefined,
            },
            tabs: [],
            activeTabId: null,
            expandedPaths: new Set([projectInfo.rootPath]),
          });
        },

        // ─── File Actions ───
        openFile: async (path) => {
          const { tabs, nodeId, _findTabByPath } = get();
          
          if (!nodeId) {
            throw new Error('No active node');
          }
          
          // State Gating
          assertNodeReady(nodeId);
          
          // 检查是否已打开
          const existingTab = _findTabByPath(path);
          if (existingTab) {
            set({ activeTabId: existingTab.id });
            return;
          }
          
          // 检查标签数量限制 — LRU 驱逐而非抛出
          if (tabs.length >= MAX_OPEN_TABS) {
            // 找出最久未访问的非 dirty、非 pinned tab
            const evictionCandidates = tabs
              .filter(t => !t.isDirty && !t.isPinned)
              .sort((a, b) => a.lastAccessTime - b.lastAccessTime);
            
            if (evictionCandidates.length === 0) {
              throw new Error('IDE_ALL_TABS_PROTECTED');
            }
            
            const toEvict = evictionCandidates[0];
            set(state => ({
              tabs: state.tabs.filter(t => t.id !== toEvict.id),
              activeTabId: state.activeTabId === toEvict.id ? null : state.activeTabId,
            }));
          }
          
          // 创建新标签（loading 状态）
          const tabId = crypto.randomUUID();
          const fileName = path.split('/').pop() || path;
          
          const newTab: IdeTab = {
            id: tabId,
            path,
            name: fileName,
            language: detectLanguage(fileName),
            content: null,
            originalContent: null,
            isDirty: false,
            isLoading: true,
            isPinned: false,
            lastAccessTime: Date.now(),
            contentVersion: 0,
          };
          
          set(state => ({
            tabs: [...state.tabs, newTab],
            activeTabId: tabId,
          }));
          
          try {
            // 先检查文件是否可编辑
            const checkResult = await nodeIdeCheckFile(nodeId, path);
            
            if (checkResult.type === 'too_large') {
              // 文件太大
              set(state => ({
                tabs: state.tabs.filter(t => t.id !== tabId),
                activeTabId: state.tabs.length > 1 ? state.tabs[0].id : null,
              }));
              throw new Error(`File too large: ${checkResult.size} bytes (limit: ${checkResult.limit})`);
            }
            
            if (checkResult.type === 'binary') {
              // 二进制文件，静默关闭标签
              set(state => ({
                tabs: state.tabs.filter(t => t.id !== tabId),
                activeTabId: state.tabs.length > 1 ? state.tabs[0].id : null,
              }));
              // 不抛出错误，静默处理
              console.info(`[IDE] Skipping binary file: ${path}`);
              return;
            }
            
            if (checkResult.type === 'not_editable') {
              set(state => ({
                tabs: state.tabs.filter(t => t.id !== tabId),
                activeTabId: state.tabs.length > 1 ? state.tabs[0].id : null,
              }));
              throw new Error(`Cannot edit file: ${checkResult.reason}`);
            }
            
            // 文件可编辑，使用 agent-first 加载内容（SFTP 回退）
            const result = await agentService.readFile(nodeId, path);
            
            set(state => ({
              tabs: state.tabs.map(t => 
                t.id === tabId 
                  ? {
                      ...t,
                      content: result.content,
                      originalContent: result.content,
                      language: detectLanguage(fileName),
                      isLoading: false,
                      serverMtime: result.mtime ?? checkResult.mtime,
                      agentHash: result.hash, // 乐观锁 hash（agent only）
                    }
                  : t
              ),
              cachedTabPaths: [...new Set([...state.cachedTabPaths, path])],
            }));
          } catch (error) {
            // 加载失败，移除标签
            set(state => ({
              tabs: state.tabs.filter(t => t.id !== tabId),
              activeTabId: state.tabs.length > 1 ? state.tabs[0].id : null,
            }));
            throw error;
          }
        },

        closeTab: async (tabId) => {
          const { tabs, activeTabId } = get();
          const tab = tabs.find(t => t.id === tabId);
          
          if (!tab) return true;
          
          // 如果有未保存更改，调用方需要先确认
          if (tab.isDirty) {
            return false; // 返回 false 表示需要用户确认
          }
          
          const newTabs = tabs.filter(t => t.id !== tabId);
          const newActiveId = activeTabId === tabId
            ? (newTabs.length > 0 ? newTabs[newTabs.length - 1].id : null)
            : activeTabId;
          
          set({
            tabs: newTabs,
            activeTabId: newActiveId,
          });
          
          return true;
        },

        closeAllTabs: async () => {
          const { tabs } = get();
          const hasDirty = tabs.some(t => t.isDirty);
          
          if (hasDirty) {
            return false; // 需要用户确认
          }
          
          set({ tabs: [], activeTabId: null });
          return true;
        },

        saveFile: async (tabId) => {
          const { tabs, nodeId } = get();
          const tab = tabs.find(t => t.id === tabId);
          
          if (!tab || !nodeId || tab.content === null) {
            throw new Error('Cannot save: invalid state');
          }
          
          // State Gating
          assertNodeReady(nodeId);
          
          // Agent-first: 原子写入 + 乐观锁
          // 如果有 agentHash，使用 agent 写入（自动检测冲突）
          // 否则回退到 SFTP stat + write
          try {
            const agentHash = tab.agentHash;
            const writeResult = await agentService.writeFile(
              nodeId,
              tab.path,
              tab.content,
              agentHash,
            );
            
            // 清除搜索缓存（文件内容已变化）
            triggerSearchCacheClear();
            
            // 触发 Git 状态刷新
            triggerGitRefresh();
            
            set(state => ({
              tabs: state.tabs.map(t =>
                t.id === tabId
                  ? {
                      ...t,
                      isDirty: false,
                      originalContent: t.content,
                      serverMtime: writeResult.mtime ?? undefined,
                      agentHash: writeResult.hash, // 更新 hash
                    }
                  : t
              ),
              conflictState: null,
            }));
          } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            // Agent hash 冲突 → 转为 UI 冲突状态
            if (msg.includes('CONFLICT') || msg.includes('hash mismatch') || msg.includes('File modified externally')) {
              set({
                conflictState: {
                  tabId,
                  localMtime: tab.serverMtime ?? 0,
                  remoteMtime: 0, // SFTP stat 来获取精确时间
                }
              });
              throw new Error('CONFLICT');
            }
            throw err;
          }
        },

        saveAllFiles: async () => {
          const { tabs, saveFile } = get();
          const dirtyTabs = tabs.filter(t => t.isDirty);
          const failedFiles: string[] = [];
          
          for (const tab of dirtyTabs) {
            try {
              await saveFile(tab.id);
            } catch (err) {
              // Collect failures instead of aborting the entire loop
              const msg = err instanceof Error ? err.message : String(err);
              // Skip conflict errors — they are handled by the UI layer
              if (msg !== 'CONFLICT') {
                failedFiles.push(tab.name);
                console.error(`[IDE] saveAllFiles: failed to save ${tab.name}:`, err);
              }
            }
          }
          
          // If there were failures, throw a summary error for the caller to toast
          if (failedFiles.length > 0) {
            throw new Error(`SAVE_ALL_PARTIAL:${failedFiles.join(',')}`);
          }
        },

        // ─── Tab Actions ───
        setActiveTab: (tabId) => {
          set(state => ({
            activeTabId: tabId,
            tabs: state.tabs.map(t =>
              t.id === tabId
                ? { ...t, lastAccessTime: Date.now() }
                : t
            ),
          }));
        },

        updateTabContent: (tabId, content) => {
          set(state => ({
            tabs: state.tabs.map(t =>
              t.id === tabId
                ? {
                    ...t,
                    content,
                    isDirty: content !== t.originalContent,
                  }
                : t
            ),
          }));
        },

        replaceStringInTab: (tabId, oldStr, newStr) => {
          const tab = get().tabs.find(t => t.id === tabId);
          if (!tab) return { success: false, error: `Tab not found: ${tabId}` };
          if (tab.content === null) return { success: false, error: 'Tab content not loaded' };
          if (!oldStr) return { success: false, error: 'old_string must not be empty' };
          const idx = tab.content.indexOf(oldStr);
          if (idx === -1) return { success: false, error: 'String not found in file content' };
          const newContent = tab.content.substring(0, idx) + newStr + tab.content.substring(idx + oldStr.length);
          set(state => ({
            tabs: state.tabs.map(t =>
              t.id === tabId
                ? {
                    ...t,
                    content: newContent,
                    isDirty: newContent !== t.originalContent,
                    contentVersion: t.contentVersion + 1,
                  }
                : t
            ),
          }));
          return { success: true };
        },

        insertTextInTab: (tabId, line, text) => {
          const tab = get().tabs.find(t => t.id === tabId);
          if (!tab) return { success: false, error: `Tab not found: ${tabId}` };
          if (tab.content === null) return { success: false, error: 'Tab content not loaded' };
          const lines = tab.content.split('\n');
          const insertAt = Math.max(0, Math.min(line - 1, lines.length));
          const textLines = text.split('\n');
          lines.splice(insertAt, 0, ...textLines);
          const newContent = lines.join('\n');
          set(state => ({
            tabs: state.tabs.map(t =>
              t.id === tabId
                ? {
                    ...t,
                    content: newContent,
                    isDirty: newContent !== t.originalContent,
                    contentVersion: t.contentVersion + 1,
                  }
                : t
            ),
          }));
          return { success: true, insertedAtLine: insertAt + 1 };
        },

        updateTabCursor: (tabId, line, col) => {
          // Throttled: batch rapid cursor updates to reduce store churn
          const key = tabId;
          if (_cursorThrottleTimers.has(key)) clearTimeout(_cursorThrottleTimers.get(key));
          _cursorThrottleTimers.set(key, setTimeout(() => {
            _cursorThrottleTimers.delete(key);
            set(state => ({
              tabs: state.tabs.map(t =>
                t.id === tabId
                  ? { ...t, cursor: { line, col } }
                  : t
              ),
            }));
          }, 100));
        },

        togglePinTab: (tabId) => {
          set(state => ({
            tabs: state.tabs.map(t =>
              t.id === tabId
                ? { ...t, isPinned: !t.isPinned }
                : t
            ),
          }));
        },

        reorderTabs: (orderedIds) => {
          set(state => {
            const tabMap = new Map(state.tabs.map(t => [t.id, t]));
            const reordered = orderedIds
              .map(id => tabMap.get(id))
              .filter((t): t is IdeTab => t !== undefined);
            return { tabs: reordered };
          });
        },

        // ─── Layout Actions ───
        setTreeWidth: (width) => set({ treeWidth: width }),
        setTerminalHeight: (height) => set({ terminalHeight: height }),
        toggleTerminal: () => set(state => ({ terminalVisible: !state.terminalVisible })),

        // ─── Split Editor ───
        splitEditor: (direction = 'horizontal') => {
          const { activeTabId, tabs } = get();
          if (!activeTabId || tabs.length < 1) return;
          // Pick a different tab for the split side, or same tab if only one
          const otherTab = tabs.find(t => t.id !== activeTabId) || tabs[0];
          set({
            splitDirection: direction,
            splitActiveTabId: otherTab.id,
          });
        },

        closeSplit: () => {
          set({
            splitDirection: null,
            splitActiveTabId: null,
          });
        },

        setSplitActiveTab: (tabId) => {
          set({ splitActiveTabId: tabId });
        },

        // ─── File Tree Actions ───
        togglePath: (path) => {
          set(state => {
            const newSet = new Set(state.expandedPaths);
            if (newSet.has(path)) {
              newSet.delete(path);
            } else {
              newSet.add(path);
            }
            return { expandedPaths: newSet };
          });
        },

        // ─── Terminal Actions ───
        setTerminalSession: (sessionId) => set({ terminalSessionId: sessionId }),

        // ─── Conflict Actions ───
        resolveConflict: async (resolution) => {
          const { conflictState, tabs, nodeId } = get();
          if (!conflictState || !nodeId) return;
          
          const tab = tabs.find(t => t.id === conflictState.tabId);
          if (!tab || tab.content === null) return;
          
          if (resolution === 'overwrite') {
            // 强制保存（忽略冲突，不传 expectHash）
            const writeResult = await agentService.writeFile(nodeId, tab.path, tab.content);
            
            set(state => ({
              tabs: state.tabs.map(t =>
                t.id === conflictState.tabId
                  ? {
                      ...t,
                      isDirty: false,
                      originalContent: t.content,
                      serverMtime: writeResult.mtime ?? undefined,
                      agentHash: writeResult.hash,
                    }
                  : t
              ),
              conflictState: null,
            }));
          } else if (resolution === 'reload') {
            // 重新加载远程内容 (agent-first + SFTP 回退)
            const result = await agentService.readFile(nodeId, tab.path);
            
            set(state => ({
              tabs: state.tabs.map(t =>
                t.id === conflictState.tabId
                  ? {
                      ...t,
                      content: result.content,
                      originalContent: result.content,
                      isDirty: false,
                      serverMtime: result.mtime ?? undefined,
                      agentHash: result.hash,
                      contentVersion: t.contentVersion + 1, // 强制编辑器刷新
                    }
                  : t
              ),
              conflictState: null,
            }));
          }
        },

        clearConflict: () => {
          set({ conflictState: null });
        },

        // ─── Search Jump ───
        setPendingScroll: (tabId, line, col) => {
          set({ pendingScroll: { tabId, line, col } });
        },
        
        clearPendingScroll: () => {
          set({ pendingScroll: null });
        },

        // ─── File System Operations (CRUD) ───
        
        createFile: async (parentPath, name) => {
          const { nodeId, _findTabByPath, refreshTreeNode } = get();
          
          // 1. 基础验证
          if (!nodeId) {
            throw new Error('No active node');
          }

          // State Gating
          assertNodeReady(nodeId);

          const validationError = validateFileName(name);
          if (validationError) {
            throw new Error(validationError);
          }
          
          // 2. 计算完整路径
          const fullPath = joinPath(parentPath, name);
          
          // 3. 竞态检查：是否已有同名标签打开
          const existingTab = _findTabByPath(fullPath);
          if (existingTab) {
            throw new Error('ide.error.fileAlreadyOpen');
          }
          
          // 4. 竞态检查：远程是否已存在同名文件
          try {
            await nodeSftpStat(nodeId, fullPath);
            // 如果能获取到 stat，说明文件已存在
            throw new Error('ide.error.alreadyExists');
          } catch (e) {
            // 预期行为：文件不存在时 sftpStat 会抛出错误
            if (e instanceof Error && e.message.includes('ide.error.')) {
              throw e; // 重新抛出我们自己的错误
            }
            // 其他错误（如 "not found"）是正常的，继续执行
          }
          
          // 5. 创建空文件（agent-first + SFTP 回退）
          await agentService.writeFile(nodeId, fullPath, '');
          
          // 6. 触发树刷新
          refreshTreeNode(parentPath);
          
          // 7. 触发 Git 刷新（新文件是 untracked）
          triggerGitRefresh();
          
          return fullPath;
        },

        createFolder: async (parentPath, name) => {
          const { nodeId, refreshTreeNode } = get();
          
          if (!nodeId) {
            throw new Error('No active node');
          }

          // State Gating
          assertNodeReady(nodeId);

          const validationError = validateFileName(name);
          if (validationError) {
            throw new Error(validationError);
          }
          
          const fullPath = joinPath(parentPath, name);
          
          // 检查是否已存在
          try {
            await nodeSftpStat(nodeId, fullPath);
            throw new Error('ide.error.alreadyExists');
          } catch (e) {
            if (e instanceof Error && e.message.includes('ide.error.')) {
              throw e;
            }
          }
          
          // 创建目录
          await nodeSftpMkdir(nodeId, fullPath);
          
          // 刷新
          refreshTreeNode(parentPath);
          triggerGitRefresh();
          
          return fullPath;
        },

        getAffectedTabs: (path) => {
          const { tabs } = get();
          const normalizedPath = normalizePath(path);
          
          // 找出所有路径匹配或以该路径为前缀的标签
          const affected = tabs.filter(t => {
            const tabPath = normalizePath(t.path);
            return tabPath === normalizedPath || tabPath.startsWith(normalizedPath + '/');
          });
          
          // 筛选出未保存的
          const unsaved = affected.filter(t => t.isDirty);
          
          return { affected, unsaved };
        },

        deleteItem: async (path, isDirectory) => {
          const { nodeId, closeTab, getAffectedTabs, refreshTreeNode } = get();
          
          if (!nodeId) {
            throw new Error('No active node');
          }

          // State Gating
          assertNodeReady(nodeId);
          
          // 1. 检查受影响的标签
          const { affected, unsaved } = getAffectedTabs(path);
          
          // 2. 如果有未保存的文件，拒绝删除
          if (unsaved.length > 0) {
            const names = unsaved.map(t => getBaseName(t.path)).join(', ');
            throw new Error(`ide.error.unsavedChanges:${names}`);
          }
          
          // 3. 关闭所有受影响的标签（已确认没有未保存）
          for (const tab of [...affected].reverse()) {
            await closeTab(tab.id);
          }
          
          // 4. 执行删除操作
          if (isDirectory) {
            await nodeSftpDeleteRecursive(nodeId, path);
          } else {
            await nodeSftpDelete(nodeId, path);
          }
          
          // 5. 刷新父目录
          const parentPath = getParentPath(path);
          refreshTreeNode(parentPath);
          
          // 6. 触发 Git 和搜索缓存刷新
          triggerGitRefresh();
          triggerSearchCacheClear();
        },

        renameItem: async (oldPath, newName) => {
          const { nodeId, refreshTreeNode } = get();
          
          if (!nodeId) {
            throw new Error('No active node');
          }
          
          // State Gating
          assertNodeReady(nodeId);
          
          // 1. 验证新名称
          const validationError = validateFileName(newName);
          if (validationError) {
            throw new Error(validationError);
          }
          
          // 2. 计算新路径
          const parentPath = getParentPath(oldPath);
          const newPath = joinPath(parentPath, newName);
          const normalizedOld = normalizePath(oldPath);
          const normalizedNew = normalizePath(newPath);
          
          // 3. 检查新路径是否与旧路径相同（无操作）
          if (normalizedOld === normalizedNew) {
            return newPath;
          }
          
          // 4. 检查新路径是否已存在
          try {
            await nodeSftpStat(nodeId, newPath);
            throw new Error('ide.error.alreadyExists');
          } catch (e) {
            if (e instanceof Error && e.message.includes('ide.error.')) {
              throw e;
            }
          }
          
          // 5. 执行重命名
          await nodeSftpRename(nodeId, oldPath, newPath);
          
          // 6. 更新所有受影响的标签路径
          set(state => ({
            tabs: state.tabs.map(tab => {
              const tabPath = normalizePath(tab.path);
              
              // Case 1: 精确匹配 - 重命名的就是这个文件
              if (tabPath === normalizedOld) {
                return {
                  ...tab,
                  path: newPath,
                  name: newName,
                  language: detectLanguage(newName),
                };
              }
              
              // Case 2: 前缀匹配 - 重命名的是父目录
              if (tabPath.startsWith(normalizedOld + '/')) {
                const relativePart = tabPath.substring(normalizedOld.length);
                const updatedPath = normalizedNew + relativePart;
                const updatedName = updatedPath.split('/').pop() || tab.name;
                return {
                  ...tab,
                  path: updatedPath,
                  name: updatedName,
                  language: detectLanguage(updatedName),
                };
              }
              
              // Case 3: 不受影响
              return tab;
            }),
          }));
          
          // 7. 更新 expandedPaths
          set(state => {
            const newExpandedPaths = new Set<string>();
            for (const expandedPath of state.expandedPaths) {
              const normalized = normalizePath(expandedPath);
              if (normalized === normalizedOld) {
                newExpandedPaths.add(normalizedNew);
              } else if (normalized.startsWith(normalizedOld + '/')) {
                newExpandedPaths.add(normalizedNew + normalized.substring(normalizedOld.length));
              } else {
                newExpandedPaths.add(expandedPath);
              }
            }
            return { expandedPaths: newExpandedPaths };
          });
          
          // 8. 刷新父目录
          refreshTreeNode(parentPath);
          
          // 9. 触发 Git 和搜索缓存刷新
          triggerGitRefresh();
          triggerSearchCacheClear();
          
          return newPath;
        },

        refreshTreeNode: (parentPath) => {
          const normalized = normalizePath(parentPath);
          set(state => ({
            treeRefreshSignal: {
              ...state.treeRefreshSignal,
              [normalized]: (state.treeRefreshSignal[normalized] || 0) + 1,
            },
          }));
        },

        // ─── Internal ───
        _findTabByPath: (path) => {
          const normalizedPath = normalizePath(path);
          return get().tabs.find(t => normalizePath(t.path) === normalizedPath);
        },
      }),
      {
        name: 'oxideterm-ide',
        // 只持久化布局设置，不持久化项目/标签状态
        partialize: (state) => ({
          treeWidth: state.treeWidth,
          terminalHeight: state.terminalHeight,
          cachedProjectPath: state.cachedProjectPath,
          cachedTabPaths: state.cachedTabPaths,
          cachedNodeId: state.cachedNodeId,
        }),
      }
    )
  )
);

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

export function extensionToLanguage(ext: string): string {
  const map: Record<string, string> = {
    ts: 'typescript',
    tsx: 'typescript',
    mts: 'typescript',
    cts: 'typescript',
    js: 'javascript',
    jsx: 'javascript',
    mjs: 'javascript',
    cjs: 'javascript',
    rs: 'rust',
    py: 'python',
    pyw: 'python',
    pyi: 'python',
    go: 'go',
    java: 'java',
    c: 'c',
    cpp: 'cpp',
    cc: 'cpp',
    cxx: 'cpp',
    h: 'c',
    hpp: 'cpp',
    hxx: 'cpp',
    cs: 'csharp',
    rb: 'ruby',
    php: 'php',
    swift: 'swift',
    kt: 'kotlin',
    kts: 'kotlin',
    scala: 'scala',
    json: 'json',
    jsonc: 'json',
    json5: 'json',
    yaml: 'yaml',
    yml: 'yaml',
    toml: 'toml',
    xml: 'xml',
    svg: 'xml',
    xsl: 'xml',
    plist: 'xml',
    html: 'html',
    htm: 'html',
    vue: 'html',
    svelte: 'html',
    css: 'css',
    scss: 'css',
    less: 'css',
    md: 'markdown',
    mdx: 'markdown',
    sql: 'sql',
    sh: 'shell',
    bash: 'shell',
    zsh: 'shell',
    fish: 'shell',
    ksh: 'shell',
    csh: 'shell',
    dockerfile: 'dockerfile',
    lua: 'lua',
    pl: 'perl',
    pm: 'perl',
    r: 'r',
    R: 'r',
    diff: 'diff',
    patch: 'diff',
    conf: 'shell',
    cfg: 'shell',
    ini: 'toml',
    env: 'shell',
    properties: 'toml',
    tf: 'shell',
    hcl: 'shell',
    mk: 'shell',
    cmake: 'shell',
    gradle: 'shell',
    zig: 'cpp',
    nim: 'python',
    ex: 'ruby',
    exs: 'ruby',
    erl: 'ruby',
    hs: 'python',
    ml: 'python',
    el: 'lisp',
    clj: 'lisp',
    lisp: 'lisp',
  };
  return map[ext.toLowerCase()] || 'plaintext';
}

/**
 * Detect language from a full file name.
 * Checks exact filename match first, then dotfile patterns, then extension.
 */
export function detectLanguage(fileName: string): string {
  const lower = fileName.toLowerCase();

  // 1. Exact filename → language (case-insensitive)
  const filenameMap: Record<string, string> = {
    'makefile': 'shell',
    'gnumakefile': 'shell',
    'dockerfile': 'dockerfile',
    'containerfile': 'dockerfile',
    'vagrantfile': 'ruby',
    'gemfile': 'ruby',
    'rakefile': 'ruby',
    'guardfile': 'ruby',
    'jenkinsfile': 'shell',
    'procfile': 'shell',
    'justfile': 'shell',
    'cmakelists.txt': 'shell',
    'go.mod': 'go',
    'go.sum': 'go',
    'cargo.lock': 'toml',
    'flake.lock': 'json',
    'composer.lock': 'json',
    'package-lock.json': 'json',
    'pnpm-lock.yaml': 'yaml',
    '.gitignore': 'shell',
    '.gitattributes': 'shell',
    '.gitmodules': 'shell',
    '.dockerignore': 'shell',
    '.editorconfig': 'toml',
    '.prettierrc': 'json',
    '.eslintrc': 'json',
    '.babelrc': 'json',
    '.npmrc': 'shell',
    '.nvmrc': 'shell',
    '.env': 'shell',
    '.env.local': 'shell',
    '.env.production': 'shell',
    '.env.development': 'shell',
    '.flake8': 'toml',
    '.pylintrc': 'toml',
    '.rubocop.yml': 'yaml',
  };

  if (filenameMap[lower]) {
    return filenameMap[lower];
  }

  // 2. Dotfile patterns: .xxxrc → shell, .xxx.yml → yaml, etc.
  if (lower.startsWith('.')) {
    // .foo.json, .foo.yaml, .foo.yml, .foo.toml → use the real extension
    const dotParts = lower.split('.');
    if (dotParts.length >= 3) {
      const realExt = dotParts[dotParts.length - 1];
      const byExt = extensionToLanguage(realExt);
      if (byExt !== 'plaintext') return byExt;
    }
    // .bashrc, .zshrc, .profile, .bash_profile, .zshenv, .zprofile → shell
    if (/rc$|profile$|_profile$|logout$|login$|env$/.test(lower)) {
      return 'shell';
    }
    // .vimrc, .inputrc → shell-like config
    if (lower.endsWith('rc')) {
      return 'shell';
    }
    // .tmux.conf → shell
    if (lower.endsWith('.conf')) {
      return 'shell';
    }
    // .gitconfig → toml-like
    if (lower === '.gitconfig') {
      return 'toml';
    }
  }

  // 3. Fall back to extension-based detection
  const ext = fileName.includes('.') ? fileName.split('.').pop() || '' : '';
  return extensionToLanguage(ext);
}

// ═══════════════════════════════════════════════════════════════════════════
// Selector Hooks (for performance)
// ═══════════════════════════════════════════════════════════════════════════

export const useIdeProject = () => useIdeStore(state => state.project);
export const useIdeTabs = () => useIdeStore(state => state.tabs);
export const useIdeActiveTab = () => useIdeStore(state => 
  state.tabs.find(t => t.id === state.activeTabId)
);
export const useIdeDirtyCount = () => useIdeStore(state => 
  state.tabs.filter(t => t.isDirty).length
);
export const useIdeConflict = () => useIdeStore(state => state.conflictState);

// ═══════════════════════════════════════════════════════════════════════════
// Auto-Save Subscription
// ═══════════════════════════════════════════════════════════════════════════

// When activeTabId changes, auto-save the previous dirty tab (if ide.autoSave is on)
useIdeStore.subscribe(
  (state) => state.activeTabId,
  (newTabId, prevTabId) => {
    if (!prevTabId || prevTabId === newTabId) return;
    // Dynamic import to avoid circular dependency with settingsStore
    import('./settingsStore').then(({ useSettingsStore }) => {
      const ideSettings = useSettingsStore.getState().getIde();
      if (!ideSettings.autoSave) return;
      
      const store = useIdeStore.getState();
      const prevTab = store.tabs.find(t => t.id === prevTabId);
      if (prevTab?.isDirty && !prevTab.isLoading) {
        store.saveFile(prevTabId).catch((err) => {
          console.warn(`[IDE AutoSave] Failed to save ${prevTab.name}:`, err);
        });
      }
    });
  }
);

// Window blur → auto-save all dirty tabs (if ide.autoSave is on)
function _ideBlurHandler() {
  import('./settingsStore').then(({ useSettingsStore }) => {
    const ideSettings = useSettingsStore.getState().getIde();
    if (!ideSettings.autoSave) return;

    const store = useIdeStore.getState();
    store.saveAllFiles().catch((err) => {
      console.warn('[IDE AutoSave] saveAllFiles on blur failed:', err);
    });
  });
}

if (typeof window !== 'undefined') {
  window.addEventListener('blur', _ideBlurHandler);
}

// Clean up on HMR to prevent listener accumulation
if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    window.removeEventListener('blur', _ideBlurHandler);
  });
}
