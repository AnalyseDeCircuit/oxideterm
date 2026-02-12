# Persistence Audit (Frontend + Backend)

> 目的：完整盘点当前所有“持久化存储入口”，为后续重构提供基线。
> 范围：前端浏览器存储（localStorage）与后端 redb 存储（StateStore / ProgressStore）。

---

## 1) Frontend — localStorage

### 1.1 `oxide-settings`（设置项）
- **读**
  - [src/components/modals/SettingsModal.tsx](../src/components/modals/SettingsModal.tsx)（`usePersistedSettings` 初始化）
  - [src/components/settings/SettingsView.tsx](../src/components/settings/SettingsView.tsx)（`usePersistedSettings` 初始化）
  - [src/components/terminal/TerminalView.tsx](../src/components/terminal/TerminalView.tsx)（初始化终端 `settings`）
  - [src/lib/themeManager.ts](../src/lib/themeManager.ts)（`initializeTheme()`）
  - [src/components/modals/EditConnectionModal.tsx](../src/components/modals/EditConnectionModal.tsx)（读取 `bufferMaxLines` / `bufferSaveOnDisconnect`）
- **写**
  - [src/components/modals/SettingsModal.tsx](../src/components/modals/SettingsModal.tsx)（`usePersistedSettings` 的 `useEffect`）
  - [src/components/settings/SettingsView.tsx](../src/components/settings/SettingsView.tsx)（`usePersistedSettings` 的 `useEffect`）
- **数据结构**（前端偏好）
  - 主题、字体、字号、行高、光标、滚动行数、renderer、buffer 配置、UI 外观等
- **备注**
  - SettingsModal 与 SettingsView 并存，两处实现同名 `PersistedSettings`，易漂移。

### 1.2 `oxide-ui-state`（UI 状态）
- **读**
  - [src/store/appStore.ts](../src/store/appStore.ts)（`loadPersistedUIState()`）
- **写**
  - [src/store/appStore.ts](../src/store/appStore.ts)（`saveUIState()`）
  - [src/main.tsx](../src/main.tsx)（`beforeunload` 时触发 `saveUIState()`）
- **数据结构**
  - `sidebarCollapsed`, `sidebarActiveSection`（不保存 tabs/activeTabId）

### 1.3 `oxide-tree-expanded`（会话树展开状态）
- **读**
  - [src/store/sessionTreeStore.ts](../src/store/sessionTreeStore.ts)（`fetchTree()` 中恢复展开状态）
- **写**
  - [src/store/sessionTreeStore.ts](../src/store/sessionTreeStore.ts)（`toggleExpand()` 持久化展开集合）
- **数据结构**
  - `string[]`（节点 ID 列表）

### 1.4 `oxide-focused-node`（会话树聚焦节点）
- **读**
  - [src/store/sessionTreeStore.ts](../src/store/sessionTreeStore.ts)（初始化聚焦状态相关逻辑）
- **写**
  - [src/store/sessionTreeStore.ts](../src/store/sessionTreeStore.ts)
    - `setFocusedNode()`
    - `enterNode()`
    - `goBack()`
- **数据结构**
  - `string`（节点 ID）

---

## 2) Backend — redb (StateStore)

### 2.1 `state.redb`（会话 + 转发规则）
- **初始化位置**
  - [src-tauri/src/lib.rs](../src-tauri/src/lib.rs)（`StateStore::new(...)`）
- **Session 元数据与缓冲区**
  - [src-tauri/src/state/session.rs](../src-tauri/src/state/session.rs)
    - `PersistedSession` / `SessionPersistence`
    - 包含 `terminal_buffer` 与 `BufferConfig`
- **Forward 规则**
  - [src-tauri/src/state/forwarding.rs](../src-tauri/src/state/forwarding.rs)
    - `PersistedForward` / `ForwardPersistence`
- **备注**
  - 当前用途是“连接身份/会话恢复/转发规则”，不含用户偏好设置。

### 2.2 `sftp_progress.redb`（SFTP 断点续传）
- **初始化位置**
  - [src-tauri/src/lib.rs](../src-tauri/src/lib.rs)（`RedbProgressStore::new(...)`）
- **持久化类型**
  - [src-tauri/src/sftp/progress.rs](../src-tauri/src/sftp/progress.rs)
    - `StoredTransferProgress`
- **备注**
  - 仅用于断点续传与进度恢复。

### 2.3 `launcher_icons/`（应用启动器图标缓存 - macOS）
- **存储位置**
  - `~/Library/Application Support/com.oxideterm.app/launcher_icons/`
  - 通过 `app.path().app_data_dir().join("launcher_icons")` 获取
- **文件命名**
  - 使用 app 路径的 64 位哈希值：`{hash}.png`（如 `a1b2c3d4e5f67890.png`）
  - 哈希算法：`std::collections::hash_map::DefaultHasher`
- **缓存策略**
  - **提取来源**：通过 Swift `NSWorkspace.shared.icon(forFile:)` 批量提取（支持 `.icns` 和 Asset Catalog）
  - **尺寸规格**：128×128 PNG
  - **缓存时效**：已存在且修改时间 < 7 天的图标会复用，无需重新提取
  - **懒加载**：仅在首次打开 Launcher 时扫描 `/Applications` 并提取缺失图标
- **Asset Protocol 授权**
  - 在 `launcher_list_apps` 命令中，一次性对整个 `launcher_icons/` 目录调用 `app.asset_protocol_scope().allow_directory(&icon_cache_dir, false)`
  - 前端直接用 `convertFileSrc(iconPath)` 构造 `asset://` URL，无需 per-icon IPC
- **实现位置**
  - [src-tauri/src/launcher/mod.rs](../src-tauri/src/launcher/mod.rs)（commands: `launcher_list_apps`）
  - [src-tauri/src/launcher/macos.rs](../src-tauri/src/launcher/macos.rs)（扫描/提取/缓存逻辑）
  - [src/components/launcher/LauncherView.tsx](../src/components/launcher/LauncherView.tsx)（前端展示）
- **备注**
  - 仅 macOS 平台启用；Windows 使用 WSL distro 列表，无图标缓存
  - 图标提取采用 spawn_blocking 避免阻塞 async runtime

---

## 3) 其他持久化入口（需留意）

- **主题初始化依赖**
  - [src/lib/themeManager.ts](../src/lib/themeManager.ts) 直接从 `oxide-settings` 读取主题。
- **连接时缓冲配置依赖**
  - [src/components/modals/EditConnectionModal.tsx](../src/components/modals/EditConnectionModal.tsx) 使用 `oxide-settings` 中的 buffer 配置。

---

## 4) 结论（供后续重构决策）

1. **前端设置持久化**：存在多处入口（SettingsModal + SettingsView + TerminalView + themeManager），必须收敛。
2. **UI 状态持久化**：仅在 `beforeunload` 保存，异常退出时丢失风险存在。
3. **后端持久化**：已经有明确边界（session/forwarding/sftp/launcher_icons），暂不承载端偏好设置。

---

# Part II: 统一设置架构设计（v2）

> 设计原则：
> - **单一数据来源**：所有设置读写通过唯一 store 入口
> - **不兼容旧格式**：检测到旧格式直接丢弃并重建，使用 `version: 2` 标识
> - **Zustand 驱动**：利用现有 Zustand 生态，支持订阅式响应
> - **即时持久化**：写入时立即同步 localStorage，无需依赖 beforeunload

---

## 5) 新架构：统一 SettingsStore

### 5.1 数据结构定义

```typescript
// src/store/settingsStore.ts

/** 设置数据版本，用于检测旧格式 */
const SETTINGS_VERSION = 2;

/** localStorage key */
const STORAGE_KEY = 'oxide-settings-v2';

/** 渲染器类型 */
type RendererType = 'auto' | 'webgl' | 'canvas';

/** 平台检测 */
const isWindows = navigator.platform.toLowerCase().includes('win');

/** 终端设置 */
interface TerminalSettings {
  theme: string;
  fontFamily: 'jetbrains' | 'meslo' | 'maple' | 'cascadia' | 'consolas' | 'menlo' | 'custom';
  fontSize: number;        // 8-32
  lineHeight: number;      // 0.8-3.0
  cursorStyle: 'block' | 'underline' | 'bar';
  cursorBlink: boolean;
  scrollback: number;      // 前端 xterm 滚动行数
  renderer: RendererType;  // 渲染器选择
}

/** 缓冲区设置（后端使用） */
interface BufferSettings {
  maxLines: number;          // 后端 ScrollBuffer 最大行数
  saveOnDisconnect: boolean; // 断开时是否保存缓冲区
}

/** UI 外观设置 */
interface AppearanceSettings {
  sidebarCollapsedDefault: boolean;
}

/** 连接默认值 */
interface ConnectionDefaults {
  username: string;
  port: number;
}

/** 会话树 UI 状态（运行时，按需持久化） */
interface TreeUIState {
  expandedIds: string[];
  focusedNodeId: string | null;
}

/** 侧边栏 UI 状态 */
interface SidebarUIState {
  collapsed: boolean;
  activeSection: 'sessions' | 'saved' | 'sftp' | 'forwards' | 'connections';
}

/** 完整设置结构 */
interface PersistedSettingsV2 {
  version: 2;
  terminal: TerminalSettings;
  buffer: BufferSettings;
  appearance: AppearanceSettings;
  connectionDefaults: ConnectionDefaults;
  treeUI: TreeUIState;
  sidebarUI: SidebarUIState;
}
```

### 5.2 默认值定义

```typescript
const defaultTerminalSettings: TerminalSettings = {
  theme: 'default',
  fontFamily: 'jetbrains',
  fontSize: 14,
  lineHeight: 1.2,
  cursorStyle: 'block',
  cursorBlink: true,
  scrollback: 5000,
  renderer: isWindows ? 'canvas' : 'auto',
};

const defaultBufferSettings: BufferSettings = {
  maxLines: 100000,
  saveOnDisconnect: true,
};

const defaultAppearanceSettings: AppearanceSettings = {
  sidebarCollapsedDefault: false,
};

const defaultConnectionDefaults: ConnectionDefaults = {
  username: 'root',
  port: 22,
};

const defaultTreeUIState: TreeUIState = {
  expandedIds: [],
  focusedNodeId: null,
};

const defaultSidebarUIState: SidebarUIState = {
  collapsed: false,
  activeSection: 'sessions',
};

const createDefaultSettings = (): PersistedSettingsV2 => ({
  version: 2,
  terminal: { ...defaultTerminalSettings },
  buffer: { ...defaultBufferSettings },
  appearance: { ...defaultAppearanceSettings },
  connectionDefaults: { ...defaultConnectionDefaults },
  treeUI: { ...defaultTreeUIState },
  sidebarUI: { ...defaultSidebarUIState },
});
```

### 5.3 Store 实现

```typescript
import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';

interface SettingsStore {
  // State
  settings: PersistedSettingsV2;
  
  // Actions - 分类更新
  updateTerminal: <K extends keyof TerminalSettings>(key: K, value: TerminalSettings[K]) => void;
  updateBuffer: <K extends keyof BufferSettings>(key: K, value: BufferSettings[K]) => void;
  updateAppearance: <K extends keyof AppearanceSettings>(key: K, value: AppearanceSettings[K]) => void;
  updateConnectionDefaults: <K extends keyof ConnectionDefaults>(key: K, value: ConnectionDefaults[K]) => void;
  
  // Actions - 树 UI 状态
  setTreeExpanded: (ids: string[]) => void;
  toggleTreeNode: (nodeId: string) => void;
  setFocusedNode: (nodeId: string | null) => void;
  
  // Actions - 侧边栏 UI 状态
  setSidebarCollapsed: (collapsed: boolean) => void;
  setSidebarSection: (section: SidebarUIState['activeSection']) => void;
  
  // Actions - 全量操作
  resetToDefaults: () => void;
  
  // Selectors (便捷访问)
  getTerminal: () => TerminalSettings;
  getBuffer: () => BufferSettings;
}

/** 从 localStorage 加载，检测版本 */
function loadSettings(): PersistedSettingsV2 {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed.version === 2) {
        // 合并默认值以处理新增字段
        return mergeWithDefaults(parsed);
      }
    }
    
    // 检测旧格式并清理
    const oldSettings = localStorage.getItem('oxide-settings');
    const oldUIState = localStorage.getItem('oxide-ui-state');
    const oldTreeExpanded = localStorage.getItem('oxide-tree-expanded');
    const oldFocusedNode = localStorage.getItem('oxide-focused-node');
    
    if (oldSettings || oldUIState || oldTreeExpanded || oldFocusedNode) {
      console.warn('[SettingsStore] Detected legacy settings format. Clearing and using defaults.');
      localStorage.removeItem('oxide-settings');
      localStorage.removeItem('oxide-ui-state');
      localStorage.removeItem('oxide-tree-expanded');
      localStorage.removeItem('oxide-focused-node');
    }
  } catch (e) {
    console.error('[SettingsStore] Failed to load settings:', e);
  }
  
  return createDefaultSettings();
}

/** 合并默认值（处理版本升级新增字段） */
function mergeWithDefaults(saved: Partial<PersistedSettingsV2>): PersistedSettingsV2 {
  const defaults = createDefaultSettings();
  return {
    version: 2,
    terminal: { ...defaults.terminal, ...saved.terminal },
    buffer: { ...defaults.buffer, ...saved.buffer },
    appearance: { ...defaults.appearance, ...saved.appearance },
    connectionDefaults: { ...defaults.connectionDefaults, ...saved.connectionDefaults },
    treeUI: { ...defaults.treeUI, ...saved.treeUI },
    sidebarUI: { ...defaults.sidebarUI, ...saved.sidebarUI },
  };
}

/** 持久化到 localStorage */
function persistSettings(settings: PersistedSettingsV2): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch (e) {
    console.error('[SettingsStore] Failed to persist settings:', e);
  }
}

export const useSettingsStore = create<SettingsStore>()(
  subscribeWithSelector((set, get) => ({
    settings: loadSettings(),
    
    updateTerminal: (key, value) => {
      set((state) => {
        const newSettings = {
          ...state.settings,
          terminal: { ...state.settings.terminal, [key]: value },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },
    
    updateBuffer: (key, value) => {
      set((state) => {
        const newSettings = {
          ...state.settings,
          buffer: { ...state.settings.buffer, [key]: value },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },
    
    updateAppearance: (key, value) => {
      set((state) => {
        const newSettings = {
          ...state.settings,
          appearance: { ...state.settings.appearance, [key]: value },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },
    
    updateConnectionDefaults: (key, value) => {
      set((state) => {
        const newSettings = {
          ...state.settings,
          connectionDefaults: { ...state.settings.connectionDefaults, [key]: value },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },
    
    setTreeExpanded: (ids) => {
      set((state) => {
        const newSettings = {
          ...state.settings,
          treeUI: { ...state.settings.treeUI, expandedIds: ids },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },
    
    toggleTreeNode: (nodeId) => {
      set((state) => {
        const current = new Set(state.settings.treeUI.expandedIds);
        if (current.has(nodeId)) {
          current.delete(nodeId);
        } else {
          current.add(nodeId);
        }
        const newSettings = {
          ...state.settings,
          treeUI: { ...state.settings.treeUI, expandedIds: [...current] },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },
    
    setFocusedNode: (nodeId) => {
      set((state) => {
        const newSettings = {
          ...state.settings,
          treeUI: { ...state.settings.treeUI, focusedNodeId: nodeId },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },
    
    setSidebarCollapsed: (collapsed) => {
      set((state) => {
        const newSettings = {
          ...state.settings,
          sidebarUI: { ...state.settings.sidebarUI, collapsed },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },
    
    setSidebarSection: (section) => {
      set((state) => {
        const newSettings = {
          ...state.settings,
          sidebarUI: { ...state.settings.sidebarUI, activeSection: section },
        };
        persistSettings(newSettings);
        return { settings: newSettings };
      });
    },
    
    resetToDefaults: () => {
      const newSettings = createDefaultSettings();
      persistSettings(newSettings);
      set({ settings: newSettings });
    },
    
    getTerminal: () => get().settings.terminal,
    getBuffer: () => get().settings.buffer,
  }))
);
```

---

## 6) 事件传播机制

### 6.1 主题变更（全局 CSS + xterm）

```typescript
// 在 settingsStore 中订阅 terminal.theme 变化
useSettingsStore.subscribe(
  (state) => state.settings.terminal.theme,
  (theme) => {
    // 更新 CSS 变量
    document.documentElement.setAttribute('data-theme', theme);
    // 派发事件供 TerminalView 更新 xterm 实例
    window.dispatchEvent(new CustomEvent('theme-changed', { detail: { theme } }));
  }
);
```

### 6.2 Renderer 变更（Toast 提示）

```typescript
// 在 settingsStore 中订阅 terminal.renderer 变化
let previousRenderer: RendererType | null = null;

useSettingsStore.subscribe(
  (state) => state.settings.terminal.renderer,
  (renderer) => {
    if (previousRenderer !== null && previousRenderer !== renderer) {
      // 显示 Toast 提示
      useToastStore.getState().addToast({
        title: 'Renderer Changed',
        description: `New terminals will use ${renderer === 'auto' ? 'WebGL (auto)' : renderer}. Restart existing terminals to apply.`,
        variant: 'info',
      });
    }
    previousRenderer = renderer;
  }
);
```

### 6.3 终端设置变更（实时应用）

```typescript
// TerminalView.tsx 中订阅
useEffect(() => {
  const unsubscribe = useSettingsStore.subscribe(
    (state) => state.settings.terminal,
    (terminal) => {
      if (!terminalRef.current) return;
      
      terminalRef.current.options.fontFamily = getFontFamily(terminal.fontFamily);
      terminalRef.current.options.fontSize = terminal.fontSize;
      terminalRef.current.options.lineHeight = terminal.lineHeight;
      terminalRef.current.options.cursorStyle = terminal.cursorStyle;
      terminalRef.current.options.cursorBlink = terminal.cursorBlink;
      terminalRef.current.options.theme = themes[terminal.theme] || themes.default;
      terminalRef.current.refresh(0, terminalRef.current.rows - 1);
      fitAddonRef.current?.fit();
    }
  );
  
  return unsubscribe;
}, []);
```

---

## 7) 迁移策略

### 7.1 旧格式检测与清理

```typescript
// loadSettings() 中已实现：
// 1. 优先读取 'oxide-settings-v2'
// 2. 如果不存在，检测旧 key（oxide-settings, oxide-ui-state, oxide-tree-expanded, oxide-focused-node）
// 3. 发现旧格式 → console.warn 并删除旧 key → 使用默认值
// 4. 不做数据迁移，直接重建
```

### 7.2 不兼容策略理由

- **用户基数为零**：产品未发布，无历史数据需要保护
- **结构差异大**：旧格式扁平化，新格式分层嵌套，迁移代码复杂且一次性
- **清理成本低**：旧设置丢失后用户只需重新配置偏好

### 7.3 孤儿 ID 清理机制（Tree UI State Pruning）

**问题**：`treeUI.expandedIds` 和 `treeUI.focusedNodeId` 持久化后，重启时 `sessionTreeStore.rawNodes` 为空。这些 ID 在当前会话中可能根本不存在，成为"孤儿"。

**设计原则**：
1. **惰性清理**：不在 settingsStore 加载时清理（此时无法判断有效性）
2. **同步清理**：在 sessionTreeStore 收到 rawNodes 更新时触发清理
3. **保守策略**：只清理明确无效的 ID，避免误删

**实现位置**：`sessionTreeStore.ts`

```typescript
// sessionTreeStore.ts

import { useSettingsStore } from './settingsStore';

interface SessionTreeStore {
  rawNodes: FlatNode[];
  // expandedIds 和 focusedNodeId 已迁移到 settingsStore
  
  setRawNodes: (nodes: FlatNode[]) => void;
  // ...
}

export const useSessionTreeStore = create<SessionTreeStore>()((set, get) => ({
  rawNodes: [],
  
  setRawNodes: (nodes) => {
    set({ rawNodes: nodes });
    
    // 孤儿 ID 清理：当 rawNodes 更新时，移除不存在的 expandedIds/focusedNodeId
    pruneOrphanedTreeUIState(nodes);
  },
  
  // ...
}));

/**
 * 清理 settingsStore.treeUI 中不再有效的节点 ID
 * 
 * 调用时机：rawNodes 更新后
 * 清理逻辑：
 *   - expandedIds: 移除所有不在 rawNodes 中的 ID
 *   - focusedNodeId: 如果不在 rawNodes 中，置为 null
 */
function pruneOrphanedTreeUIState(currentNodes: FlatNode[]): void {
  const settingsStore = useSettingsStore.getState();
  const { expandedIds, focusedNodeId } = settingsStore.settings.treeUI;
  
  // 构建当前有效 ID 集合
  const validIds = new Set(currentNodes.map(node => node.id));
  
  // 过滤 expandedIds
  const prunedExpandedIds = expandedIds.filter(id => validIds.has(id));
  const expandedChanged = prunedExpandedIds.length !== expandedIds.length;
  
  // 检查 focusedNodeId
  const focusedValid = focusedNodeId === null || validIds.has(focusedNodeId);
  const prunedFocusedNodeId = focusedValid ? focusedNodeId : null;
  
  // 仅在有变化时更新（避免无意义的 localStorage 写入）
  if (expandedChanged || !focusedValid) {
    console.debug(
      '[SessionTree] Pruning orphaned IDs:',
      expandedChanged ? `expandedIds: ${expandedIds.length} -> ${prunedExpandedIds.length}` : '',
      !focusedValid ? `focusedNodeId: ${focusedNodeId} -> null` : ''
    );
    
    // 批量更新 settingsStore
    if (expandedChanged) {
      settingsStore.setTreeExpanded(prunedExpandedIds);
    }
    if (!focusedValid) {
      settingsStore.setFocusedNode(null);
    }
  }
}
```

**边界情况处理**：

| 场景 | 行为 |
|------|------|
| 应用启动，rawNodes 为空 | 不触发清理（空数组会清空所有 ID） |
| 用户断开某会话，节点从 rawNodes 移除 | 清理该节点 ID |
| 后端推送完整 rawNodes 列表 | 一次性清理所有孤儿 ID |

**优化：跳过空 rawNodes 的清理**

```typescript
function pruneOrphanedTreeUIState(currentNodes: FlatNode[]): void {
  // 空节点列表时不清理，避免启动时误清
  if (currentNodes.length === 0) {
    return;
  }
  
  // ... 原有逻辑
}
```

---

## 8) 文件改造清单

### 8.1 新建文件

| 文件 | 说明 |
|------|------|
| `src/store/settingsStore.ts` | 统一设置 store |

### 8.2 改造文件

| 文件 | 改造内容 |
|------|----------|
| `src/components/modals/SettingsModal.tsx` | 删除 `usePersistedSettings`，改用 `useSettingsStore` |
| `src/components/settings/SettingsView.tsx` | 删除 `usePersistedSettings`，改用 `useSettingsStore` |
| `src/components/terminal/TerminalView.tsx` | 删除内联 settings state，订阅 `useSettingsStore` |
| `src/lib/themeManager.ts` | 改用 `useSettingsStore.getState().settings.terminal.theme` |
| `src/components/modals/EditConnectionModal.tsx` | 改用 `useSettingsStore.getState().settings.buffer` |
| `src/store/appStore.ts` | 删除 `sidebarCollapsed`/`sidebarActiveSection`，委托给 `settingsStore` |
| `src/store/sessionTreeStore.ts` | 删除 `expandedIds`/`focusedNodeId` 持久化逻辑，委托给 `settingsStore` |
| `src/main.tsx` | 删除 `beforeunload` 中的 `saveUIState()`，改为初始化 settingsStore 订阅 |

### 8.3 删除文件/代码

| 位置 | 删除内容 |
|------|----------|
| `src/store/appStore.ts` | `loadPersistedUIState()`, `saveUIState()`, `UI_STATE_STORAGE_KEY` |
| `src/store/sessionTreeStore.ts` | 所有 `localStorage.getItem/setItem` 调用 |
| `src/lib/themeManager.ts` | `initializeTheme()` 中的 localStorage 读取 |

---

## 9) 实施顺序

1. **Phase 1**：创建 `settingsStore.ts`，实现完整 store 与持久化
2. **Phase 2**：改造 `SettingsModal.tsx` 与 `SettingsView.tsx`，验证设置读写
3. **Phase 3**：改造 `TerminalView.tsx`，验证终端实时响应
4. **Phase 4**：改造 `sessionTreeStore.ts`
   - 删除 `expandedIds`/`focusedNodeId` 本地 state 与 localStorage 逻辑
   - 改为调用 `settingsStore.toggleTreeNode()` / `settingsStore.setFocusedNode()`
   - 在 `setRawNodes()` 中添加 `pruneOrphanedTreeUIState()` 清理调用
5. **Phase 5**：改造 `appStore.ts`，迁移侧边栏状态
6. **Phase 6**：清理旧代码，删除废弃函数与 key
7. **Phase 7**：添加 renderer 变更 Toast 提示

---

## 10) 验收标准

- [ ] 所有设置通过 `useSettingsStore` 读写
- [ ] localStorage 中只存在 `oxide-settings-v2` 一个 key
- [ ] 主题切换实时生效（CSS + xterm）
- [ ] 字体/字号/行高切换实时生效
- [ ] Renderer 切换显示 Toast 提示
- [ ] 会话树展开/聚焦状态正确持久化
- [ ] **孤儿 ID 自动清理**：重启后首次收到 rawNodes 时，无效的 expandedIds/focusedNodeId 被移除
- [ ] 侧边栏状态正确持久化
- [ ] 旧格式 key 被自动清理

---

# Part III: 会话与拓扑持久化设计

> 核心问题：**会话（Session）与拓扑关系（Session Tree）是否需要跨启动持久化？**

---

## 11) 当前会话持久化状态分析

### 11.1 后端已有能力

| 组件 | 文件 | 持久化内容 | 存储位置 |
|------|------|-----------|----------|
| `PersistedSession` | `src-tauri/src/state/session.rs` | 会话元数据 + 终端缓冲区 | `state.redb` |
| `PersistedForward` | `src-tauri/src/state/forwarding.rs` | 端口转发规则 | `state.redb` |
| `SessionTree` | `src-tauri/src/session/tree.rs` | 会话树结构（内存） | ❌ 未持久化 |

**后端 `PersistedSession` 结构**：
```rust
pub struct PersistedSession {
    pub id: String,
    pub config: SessionConfig,         // host, port, username, auth
    pub created_at: DateTime<Utc>,
    pub order: usize,                   // Tab 顺序
    pub terminal_buffer: Option<Vec<u8>>, // 终端缓冲区
    pub buffer_config: BufferConfig,
}
```

### 11.2 前端状态（纯内存）

| 组件 | 文件 | 状态 | 持久化 |
|------|------|------|--------|
| `appStore.sessions` | `src/store/appStore.ts` | `Map<string, SessionInfo>` | ❌ |
| `sessionTreeStore.rawNodes` | `src/store/sessionTreeStore.ts` | `FlatNode[]` | ❌ |
| `sessionTreeStore.expandedIds` | `src/store/sessionTreeStore.ts` | `Set<string>` | ✅ localStorage |
| `sessionTreeStore.focusedNodeId` | `src/store/sessionTreeStore.ts` | `string \| null` | ✅ localStorage |

### 11.3 拓扑关系来源

会话树拓扑来自后端 `SessionTree`（内存），通过 `FlatNode.parentId` 表达父子关系：

```rust
pub struct SessionNode {
    pub id: String,
    pub parent_id: Option<String>,  // 父节点 ID，None = 直连
    pub children_ids: Vec<String>,
    pub depth: u32,                  // 0 = 直连，1+ = 跳板
    pub origin: NodeOrigin,          // ManualPreset / AutoRoute / DrillDown / Direct
    pub ssh_connection_id: Option<String>,
    // ...
}
```

---

## 12) 设计决策：会话不跨启动持久化

### 12.1 理由

1. **SSH 连接是有状态的**
   - SSH Channel 绑定到特定 TCP 连接
   - 应用重启后 TCP 连接断开，SSH Session 失效
   - 即使恢复元数据，也需要重新认证握手

2. **安全性考量**
   - 密码/密钥凭据不应持久化到磁盘
   - Agent 认证在重启后需要重新请求
   - 持久化会话意味着持久化敏感凭据

3. **用户体验权衡**
   - 终端应用通常不需要"断点续连"
   - 用户习惯：重启应用 = 重新连接
   - "保存的连接"已提供快速重连入口

4. **复杂度成本**
   - 恢复拓扑需要按顺序重建跳板链
   - 任一跳板失败导致整链失败
   - 异常处理与 UI 反馈复杂

### 12.2 当前行为（保持）

| 场景 | 行为 |
|------|------|
| 应用正常退出 | 所有会话断开，下次启动为空白 |
| 应用崩溃 | 同上 |
| 网络闪断 | `auto_reconnect` 自动重连（会话树保持） |
| 系统休眠唤醒 | `auto_reconnect` 自动重连 |

### 12.3 例外：终端缓冲区持久化（可选）

虽然会话不持久化，但**终端输出内容**可选择性保存：

- **已实现**：`PersistedSession.terminal_buffer` + `BufferConfig.save_on_disconnect`
- **用途**：用户手动"保存会话日志"或"导出终端历史"
- **不用于**：自动恢复会话

---

## 13) 设计决策：拓扑关系不独立持久化

### 13.1 理由

1. **拓扑是运行时派生的**
   - `ManualPreset` 来源：保存的连接 `proxy_chain`（已持久化）
   - `AutoRoute` 来源：网络拓扑计算（运行时）
   - `DrillDown` 来源：用户交互（运行时）

2. **保存的连接已包含跳板链**
   ```json
   // connections.json
   {
     "id": "uuid",
     "name": "Internal DB",
     "proxy_chain": [
       { "host": "jump-01", "port": 22, "username": "admin" },
       { "host": "bastion", "port": 22, "username": "ops" }
     ],
     "host": "db.internal",
     "port": 22,
     "username": "dba"
   }
   ```

3. **重建成本低**
   - 启动时从保存的连接恢复 `proxy_chain` → 点击即可重建树
   - 动态钻入的节点本就是临时的

### 13.2 拓扑恢复流程

```
应用启动
    │
    ▼
SessionTree = 空
    │
    ▼
用户点击"保存的连接"
    │
    ├─ 有 proxy_chain → api.connectManualPreset() → 展开为树节点
    │
    └─ 无 proxy_chain → api.addRootNode() → 单节点
    │
    ▼
用户在已连接节点上"钻入"
    │
    ▼
api.drillDown() → 添加子节点
```

---

## 14) 持久化边界总结

### 14.1 持久化到 localStorage（前端偏好）

| Key | 内容 | 说明 |
|-----|------|------|
| `oxide-settings-v2` | 终端/缓冲区/外观/连接默认/树UI/侧边栏 | Part II 设计 |

### 14.2 持久化到 redb（后端数据）

| 数据库 | 内容 | 说明 |
|--------|------|------|
| `state.redb` | 会话元数据（可选缓冲区） | 仅用于手动导出/日志 |
| `state.redb` | 端口转发规则 | 用于重连后恢复转发 |
| `sftp_progress.redb` | SFTP 传输进度 | 用于断点续传 |

### 14.3 持久化到 JSON 文件（配置数据）

| 文件 | 内容 | 说明 |
|------|------|------|
| `~/.oxideterm/connections.json` | 保存的连接 + proxy_chain | 已有实现 |
| `~/.oxideterm/groups.json` | 连接分组 | 已有实现 |

### 14.4 不持久化（运行时状态）

| 数据 | 说明 |
|------|------|
| `SessionTree` 节点 | 重启后重建 |
| `appStore.sessions` | 重启后清空 |
| `appStore.tabs` | 重启后清空 |
| SSH 连接 / WebSocket | 运行时绑定 |

---

## 15) 完整持久化架构图

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           OxideTerm 持久化架构                           │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                     Frontend (localStorage)                      │   │
│  │  ┌─────────────────────────────────────────────────────────┐    │   │
│  │  │  oxide-settings-v2                                       │    │   │
│  │  │  ├── terminal: theme, font, cursor, renderer, scrollback │    │   │
│  │  │  ├── buffer: maxLines, saveOnDisconnect                  │    │   │
│  │  │  ├── appearance: sidebarCollapsedDefault                 │    │   │
│  │  │  ├── connectionDefaults: username, port                  │    │   │
│  │  │  ├── treeUI: expandedIds[], focusedNodeId               │    │   │
│  │  │  └── sidebarUI: collapsed, activeSection                 │    │   │
│  │  └─────────────────────────────────────────────────────────┘    │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                      Backend (redb + JSON)                       │   │
│  │                                                                  │   │
│  │  ┌──────────────────────┐  ┌──────────────────────────────┐    │   │
│  │  │  state.redb          │  │  sftp_progress.redb           │    │   │
│  │  │  ├── sessions:*      │  │  └── transfers:*              │    │   │
│  │  │  │   (可选缓冲区)    │  │      (断点续传进度)           │    │   │
│  │  │  └── forwards:*      │  └──────────────────────────────┘    │   │
│  │  │      (转发规则)      │                                       │   │
│  │  └──────────────────────┘                                       │   │
│  │                                                                  │   │
│  │  ┌──────────────────────┐  ┌──────────────────────────────┐    │   │
│  │  │connections.json      │  │  groups.json                  │    │   │
│  │  │  ├── id, name, host  │  │  └── ["Production", "Dev"]    │    │   │
│  │  │  ├── proxy_chain[]   │  └──────────────────────────────┘    │   │
│  │  │  └── auth_type, ...  │                                       │   │
│  │  └──────────────────────┘                                       │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    Runtime Only (不持久化)                       │   │
│  │  ├── SessionTree (会话树节点与拓扑)                             │   │
│  │  ├── appStore.sessions (活跃会话)                               │   │
│  │  ├── appStore.tabs (打开的标签页)                               │   │
│  │  ├── SSH Connections (TCP 连接)                                 │   │
│  │  └── WebSocket Bridges (终端 I/O)                               │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 16) 未来可选扩展（不在 v2 范围内）

### 16.1 会话快照导出

- 用户主动触发"导出会话"
- 保存：会话配置 + 终端缓冲区 + 拓扑结构
- 格式：`.oxide-session` 加密文件
- 导入时重建连接（需重新认证）

### 16.2 工作区恢复

- 类似 VS Code 的"恢复上次会话"
- 仅恢复"保存的连接"引用，不恢复实际连接
- 用户确认后批量重连

### 16.3 远程同步

- 通过云服务同步"保存的连接"与"设置"
- 不同步运行时状态

---

## 17) 实施顺序（更新）

在 Part II 的 Phase 1-7 基础上，**无需额外工作**：

- ✅ 会话不持久化 = 当前行为，保持
- ✅ 拓扑不独立持久化 = 当前行为，保持
- ✅ 树 UI 状态（expandedIds/focusedNodeId）已纳入 `settingsStore`

**唯一调整**：
- `sessionTreeStore` 中的 `expandedIds`/`focusedNodeId` 从 localStorage 迁移到 `settingsStore.treeUI`
- 保持 `sessionTreeStore.rawNodes` 为运行时状态，不持久化