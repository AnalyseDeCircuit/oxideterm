// src/store/ideStore.ts
import { create } from 'zustand';
import { subscribeWithSelector, persist } from 'zustand/middleware';
import { api } from '../lib/api';

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
  cursor?: { line: number; col: number };
  serverMtime?: number;   // 服务器端文件修改时间（Unix timestamp 秒）
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
  connectionId: string | null;    // SSH 连接 ID（复用连接池）
  sftpSessionId: string | null;   // SFTP 会话 ID
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
  
  // ─── 文件树状态 ───
  expandedPaths: Set<string>;  // 展开的目录路径
  
  // ─── 冲突状态 ───
  conflictState: {
    tabId: string;
    localMtime: number;
    remoteMtime: number;
  } | null;
}

interface IdeActions {
  // 项目操作
  openProject: (connectionId: string, sftpSessionId: string, rootPath: string) => Promise<void>;
  closeProject: () => void;
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
  
  // 布局操作
  setTreeWidth: (width: number) => void;
  setTerminalHeight: (height: number) => void;
  toggleTerminal: () => void;
  
  // 文件树操作
  togglePath: (path: string) => void;
  
  // 终端操作
  setTerminalSession: (sessionId: string | null) => void;
  
  // 冲突处理
  resolveConflict: (resolution: 'overwrite' | 'reload') => Promise<void>;
  clearConflict: () => void;
  
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

export const useIdeStore = create<IdeState & IdeActions>()(
  subscribeWithSelector(
    persist(
      (set, get) => ({
        // ─── Initial State ───
        connectionId: null,
        sftpSessionId: null,
        terminalSessionId: null,
        project: null,
        tabs: [],
        activeTabId: null,
        treeWidth: 280,
        terminalHeight: 200,
        terminalVisible: false,
        expandedPaths: new Set<string>(),
        conflictState: null,

        // ─── Project Actions ───
        openProject: async (connectionId, sftpSessionId, rootPath) => {
          const currentState = get();
          
          // 如果已经打开了相同的项目，不要重置状态
          if (currentState.project?.rootPath === rootPath && 
              currentState.sftpSessionId === sftpSessionId) {
            return;
          }
          
          // 先初始化 SFTP 会话（如果尚未初始化）
          // sftpInit 会在 SFTP 已初始化时返回当前工作目录，不会重复初始化
          const isInitialized = await api.sftpIsInitialized(sftpSessionId);
          if (!isInitialized) {
            await api.sftpInit(sftpSessionId);
          }
          
          // 调用后端获取项目信息
          const projectInfo = await api.ideOpenProject(sftpSessionId, rootPath);
          
          set({
            connectionId,
            sftpSessionId,
            project: {
              rootPath: projectInfo.rootPath,
              name: projectInfo.name,
              isGitRepo: projectInfo.isGitRepo,
              gitBranch: projectInfo.gitBranch ?? undefined,
            },
            tabs: [],
            activeTabId: null,
            expandedPaths: new Set([projectInfo.rootPath]), // 默认展开根目录
          });
        },

        closeProject: () => {
          const { tabs } = get();
          const hasDirty = tabs.some(t => t.isDirty);
          
          if (hasDirty) {
            // 调用方需要先处理未保存文件
            console.warn('closeProject called with dirty tabs');
          }
          
          set({
            connectionId: null,
            sftpSessionId: null,
            terminalSessionId: null,
            project: null,
            tabs: [],
            activeTabId: null,
            expandedPaths: new Set(),
            conflictState: null,
          });
        },

        changeRootPath: async (newRootPath: string) => {
          const { connectionId, sftpSessionId, tabs } = get();
          
          if (!connectionId || !sftpSessionId) {
            throw new Error('No active session');
          }
          
          // 检查是否有未保存的文件
          const hasDirty = tabs.some(t => t.isDirty);
          if (hasDirty) {
            throw new Error('Please save all files before changing root directory');
          }
          
          // 调用后端获取新项目信息
          const projectInfo = await api.ideOpenProject(sftpSessionId, newRootPath);
          
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
          const { tabs, sftpSessionId, _findTabByPath } = get();
          
          if (!sftpSessionId) {
            throw new Error('No SFTP session');
          }
          
          // 检查是否已打开
          const existingTab = _findTabByPath(path);
          if (existingTab) {
            set({ activeTabId: existingTab.id });
            return;
          }
          
          // 检查标签数量限制
          if (tabs.length >= MAX_OPEN_TABS) {
            throw new Error(`Maximum ${MAX_OPEN_TABS} tabs allowed`);
          }
          
          // 创建新标签（loading 状态）
          const tabId = crypto.randomUUID();
          const fileName = path.split('/').pop() || path;
          const extension = fileName.includes('.') ? fileName.split('.').pop() || '' : '';
          
          const newTab: IdeTab = {
            id: tabId,
            path,
            name: fileName,
            language: extensionToLanguage(extension),
            content: null,
            originalContent: null,
            isDirty: false,
            isLoading: true,
            lastAccessTime: Date.now(),
            contentVersion: 0,
          };
          
          set(state => ({
            tabs: [...state.tabs, newTab],
            activeTabId: tabId,
          }));
          
          try {
            // 先检查文件是否可编辑
            const checkResult = await api.ideCheckFile(sftpSessionId, path);
            
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
            
            // 文件可编辑，使用 preview API 加载内容
            const preview = await api.sftpPreview(sftpSessionId, path);
            
            if ('Text' in preview) {
              set(state => ({
                tabs: state.tabs.map(t => 
                  t.id === tabId 
                    ? {
                        ...t,
                        content: preview.Text.data,
                        originalContent: preview.Text.data,
                        language: preview.Text.language || extensionToLanguage(extension),
                        isLoading: false,
                        serverMtime: checkResult.mtime,
                      }
                    : t
                ),
              }));
            } else {
              // 不应该发生，但以防万一
              set(state => ({
                tabs: state.tabs.filter(t => t.id !== tabId),
                activeTabId: state.tabs.length > 1 ? state.tabs[0].id : null,
              }));
              console.warn(`[IDE] Unexpected preview result for: ${path}`);
              return;
            }
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
          const { tabs, sftpSessionId } = get();
          const tab = tabs.find(t => t.id === tabId);
          
          if (!tab || !sftpSessionId || tab.content === null) {
            throw new Error('Cannot save: invalid state');
          }
          
          // 检查冲突
          const stat = await api.sftpStat(sftpSessionId, tab.path);
          if (tab.serverMtime && stat.modified && stat.modified !== tab.serverMtime) {
            // 设置冲突状态，由 UI 层处理
            set({
              conflictState: {
                tabId,
                localMtime: tab.serverMtime,
                remoteMtime: stat.modified,
              }
            });
            throw new Error('CONFLICT');
          }
          
          // 保存文件
          const result = await api.sftpWriteContent(sftpSessionId, tab.path, tab.content);
          
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
                    serverMtime: result.mtime ?? undefined,
                  }
                : t
            ),
            conflictState: null,
          }));
        },

        saveAllFiles: async () => {
          const { tabs, saveFile } = get();
          const dirtyTabs = tabs.filter(t => t.isDirty);
          
          for (const tab of dirtyTabs) {
            await saveFile(tab.id);
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

        updateTabCursor: (tabId, line, col) => {
          set(state => ({
            tabs: state.tabs.map(t =>
              t.id === tabId
                ? { ...t, cursor: { line, col } }
                : t
            ),
          }));
        },

        // ─── Layout Actions ───
        setTreeWidth: (width) => set({ treeWidth: width }),
        setTerminalHeight: (height) => set({ terminalHeight: height }),
        toggleTerminal: () => set(state => ({ terminalVisible: !state.terminalVisible })),

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
          const { conflictState, tabs, sftpSessionId } = get();
          if (!conflictState || !sftpSessionId) return;
          
          const tab = tabs.find(t => t.id === conflictState.tabId);
          if (!tab || tab.content === null) return;
          
          if (resolution === 'overwrite') {
            // 强制保存（忽略冲突）
            const result = await api.sftpWriteContent(sftpSessionId, tab.path, tab.content);
            
            set(state => ({
              tabs: state.tabs.map(t =>
                t.id === conflictState.tabId
                  ? {
                      ...t,
                      isDirty: false,
                      originalContent: t.content,
                      serverMtime: result.mtime ?? undefined,
                    }
                  : t
              ),
              conflictState: null,
            }));
          } else if (resolution === 'reload') {
            // 重新加载远程内容
            const preview = await api.sftpPreview(sftpSessionId, tab.path);
            
            if ('Text' in preview) {
              const stat = await api.sftpStat(sftpSessionId, tab.path);
              
              set(state => ({
                tabs: state.tabs.map(t =>
                  t.id === conflictState.tabId
                    ? {
                        ...t,
                        content: preview.Text.data,
                        originalContent: preview.Text.data,
                        isDirty: false,
                        serverMtime: stat.modified ?? undefined,
                        contentVersion: t.contentVersion + 1, // 强制编辑器刷新
                      }
                    : t
                ),
                conflictState: null,
              }));
            }
          }
        },

        clearConflict: () => {
          set({ conflictState: null });
        },

        // ─── Internal ───
        _findTabByPath: (path) => {
          return get().tabs.find(t => t.path === path);
        },
      }),
      {
        name: 'oxideterm-ide',
        // 只持久化布局设置，不持久化项目/标签状态
        partialize: (state) => ({
          treeWidth: state.treeWidth,
          terminalHeight: state.terminalHeight,
        }),
      }
    )
  )
);

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

function extensionToLanguage(ext: string): string {
  const map: Record<string, string> = {
    ts: 'typescript',
    tsx: 'typescript',
    js: 'javascript',
    jsx: 'javascript',
    rs: 'rust',
    py: 'python',
    go: 'go',
    java: 'java',
    c: 'c',
    cpp: 'cpp',
    h: 'c',
    hpp: 'cpp',
    cs: 'csharp',
    rb: 'ruby',
    php: 'php',
    swift: 'swift',
    kt: 'kotlin',
    scala: 'scala',
    json: 'json',
    yaml: 'yaml',
    yml: 'yaml',
    toml: 'toml',
    xml: 'xml',
    html: 'html',
    css: 'css',
    scss: 'css',
    less: 'css',
    md: 'markdown',
    sql: 'sql',
    sh: 'shell',
    bash: 'shell',
    zsh: 'shell',
    dockerfile: 'dockerfile',
  };
  return map[ext.toLowerCase()] || 'plaintext';
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
