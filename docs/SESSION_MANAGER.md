# Session Manager Panel — 构建文档

> **版本**: v1.0  
> **状态**: ✅ 已实施  
> **目标**: 将保存连接管理从侧边栏简单列表升级为独立全功能管理面板（SecureCRT/Xshell 风格）

## 1. 设计目标

当前侧边栏的 Saved Connections 面板只有基础的分组筛选 + 列表浏览。升级目标：

- **左侧文件夹树** (~200px): 层级分组导航（支持 `/` 分隔符嵌套文件夹，如 `Production/Asia/Tokyo`）
- **右侧表格视图**: 可排序列（名称、主机、端口、用户名、认证类型、分组、上次使用、标签）
- **工具栏**: 搜索栏（利用已有 `searchConnections` 后端）、批量操作、新建连接
- **行操作**: 连接、编辑、复制、删除、标签管理
- **以 Singleton Tab** 打开（同 Settings、Topology 等全局 Tab 的模式）

### 1.1 与现有组件的关系

| 现有组件 | 功能 | 是否重叠 |
|---------|------|---------|
| Sidebar "Saved" 面板 | 简单连接列表 + 分组筛选 | **互补**。Sidebar 保留快速入口，Session Manager 提供完整管理 |
| `ConnectionsPanel` | 活跃 SSH 连接监控（心跳/状态） | **不重叠**。ConnectionsPanel 是运行时监控 |
| `EditConnectionPropertiesModal` | 单连接编辑表单 | **复用**。Session Manager 的编辑操作调用此 Modal |
| `NewConnectionModal` | 新建连接表单 | **复用**。Session Manager 的"新建"操作调用此 Modal |

---

## 2. 数据模型

### 2.1 ConnectionInfo（只读，来自后端）

```typescript
// src/types/index.ts line 367
interface ConnectionInfo {
  id: string;
  name: string;
  group: string | null;        // 分组，支持 "/" 嵌套
  host: string;
  port: number;
  username: string;
  auth_type: 'password' | 'key' | 'agent';
  key_path: string | null;
  created_at: string;
  last_used_at: string | null;
  color: string | null;         // ⚠️ 有字段但目前无 UI
  tags: string[];               // ⚠️ 有字段但目前无 UI
  proxy_chain?: ProxyHopInfo[];
}
```

### 2.2 SaveConnectionRequest（写入）

```typescript
// src/types/index.ts line 428
interface SaveConnectionRequest {
  id?: string;                  // 有值 = 更新，空 = 新建
  name: string;
  group: string | null;
  host: string;
  port: number;
  username: string;
  auth_type: 'password' | 'key' | 'agent' | 'certificate';
  password?: string;
  key_path?: string;
  cert_path?: string;
  color?: string;
  tags?: string[];
}
```

### 2.3 已有后端 API（全部可直接复用，无需改动后端）

| API 函数 | Tauri 命令 | 用途 |
|---------|-----------|------|
| `api.getConnections()` | `get_connections` | 获取所有保存连接 |
| `api.searchConnections(query)` | `search_connections` | **已存在但前端未暴露搜索 UI** |
| `api.getConnectionsByGroup(group?)` | `get_connections_by_group` | 按分组筛选 |
| `api.getRecentConnections(limit?)` | `get_recent_connections` | 最近使用 |
| `api.saveConnection(req)` | `save_connection` | 创建/更新连接 |
| `api.deleteConnection(id)` | `delete_connection` | 删除连接 |
| `api.markConnectionUsed(id)` | `mark_connection_used` | 更新 last_used_at |
| `api.getGroups()` | `get_groups` | 获取所有分组 |
| `api.createGroup(name)` | `create_group` | 创建分组 |
| `api.deleteGroup(name)` | `delete_group` | 删除分组 |
| `api.getSavedConnectionForConnect(id)` | `get_saved_connection_for_connect` | 获取含密码的完整信息用于连接 |

---

## 3. UI 规格

### 3.1 整体布局

```
┌──────────────────────────────────────────────────────────┐
│ Toolbar:  [🔍 Search...]  [New ▼]  [Batch ▼]  [Import] [Export] │
├──────────────┬───────────────────────────────────────────┤
│ Folder Tree  │ Connection Table                         │
│ (180px)      │                                          │
│              │ Name    Host     Port  User  Auth  Tags  │
│ ▼ All        │ ──────────────────────────────────────── │
│   Production │ web-1   1.2.3.4  22   root  key   [web] │
│     Asia     │ web-2   5.6.7.8  22   admin key   [web] │
│     Europe   │ db-1    9.0.1.2  22   dba   pwd   [db]  │
│   Development│                                          │
│   Testing    │                                          │
│              │                                          │
│              │              [< 1 2 3 >] (if paginated)  │
└──────────────┴───────────────────────────────────────────┘
```

### 3.2 文件夹树（左面板）

- 根节点 "All Connections" 显示总数 badge
- 一级节点 = `group` 字段的值（`null` → "Ungrouped"）
- 嵌套节点 = `group` 含 `/` 时自动拆分（如 `Production/Asia` → Production > Asia）
- 选中文件夹 → 右侧表格过滤到该分组及子分组
- 右键菜单：重命名分组、删除分组、新建子分组（⚠️ 推迟至 v1.1，当前未实现）
- 底部 "Recent" 快捷节点：显示最近使用的连接

### 3.3 连接表格（右面板）

**列定义**：

| 列 | 字段 | 可排序 | 默认宽度 |
|---|------|-------|---------|
| ☑ (checkbox) | — | 否 | 40px |
| Name | `name` | ✅ | flex |
| Host | `host` | ✅ | 160px |
| Port | `port` | ✅ | 70px |
| Username | `username` | ✅ | 120px |
| Auth | `auth_type` | ✅ | 80px |
| Group | `group` | ✅ | 120px |
| Tags | `tags` | 否 | 120px | ⚠️ 推迟至 v1.1，当前未显示 |
| Last Used | `last_used_at` | ✅ (默认) | 140px |
| Actions | — | 否 | 120px |

**行操作按钮**（Actions 列）：
- ▶ 连接 — 调用提取后的 `connectToSaved(id)` 
- ✏️ 编辑 — 打开 `EditConnectionPropertiesModal`
- ⋮ 更多 — 下拉菜单：复制连接、删除、管理标签

**交互**：
- 双击行 → 立即连接
- 单击行 → 选中（高亮）
- Ctrl/Cmd+Click → 多选
- Shift+Click → 范围选
- 列头点击 → 排序切换（asc/desc/none）

### 3.4 工具栏

| 元素 | 功能 |
|------|------|
| 搜索框 | 调用 `api.searchConnections(query)`，300ms debounce |
| "New Connection" 按钮 | 打开 `NewConnectionModal`（已有） |
| "Batch" 下拉 | 批量删除、批量移动到分组、批量添加标签 |
| "Import" 按钮 | 打开 `OxideImportModal`（已有） |
| "Export" 按钮 | 打开 `OxideExportModal`（已有） |

---

## 4. 文件结构

```
src/components/sessionManager/
├── index.ts                    // 导出 barrel
├── SessionManagerPanel.tsx     // 主容器组件（Tab 内容）
├── FolderTree.tsx              // 左侧文件夹树
├── ConnectionTable.tsx         // 右侧表格（含排序、选中逻辑）
├── ConnectionTableRow.tsx      // 单行组件
├── ManagerToolbar.tsx          // 顶部工具栏
├── BatchActionsMenu.tsx        // 批量操作下拉
└── useSessionManager.ts        // 本地状态 hook（搜索/排序/过滤/选中）
// 注意: TagEditor.tsx 推迟至 v1.1，当前未实现

src/locales/*/sessionManager.json  // i18n（11 个语言文件）
```

---

## 5. 核心逻辑提取

### 5.1 `connectToSaved` 函数提取

> **关键**：当前连接保存连接的完整逻辑在 `Sidebar.tsx` 的 `handleConnectSaved` 回调中（line 610-730）。  
> Session Manager 需要相同能力，因此必须将此逻辑提取为共享工具函数。

**提取前** (Sidebar.tsx)：
```typescript
const handleConnectSaved = useCallback(async (connectionId: string) => {
  // 130 行复杂逻辑：proxy_chain 处理、节点创建、线性连接器、终端创建...
}, [addRootNode, openConnectionEditor, createTab, toast, t]);
```

**提取后** → `src/lib/connectToSaved.ts`：
```typescript
/**
 * 连接到一个保存的连接配置。
 * 
 * 流程：
 * 1. 通过 getSavedConnectionForConnect 获取含凭据的完整信息
 * 2. 有 proxy_chain → expandManualPreset → connectNodeWithAncestors → createTerminalForNode
 * 3. 无 proxy_chain → 检查已有节点 / addRootNode → connectNodeWithAncestors
 * 4. 打开终端 Tab，标记连接已使用
 * 
 * @param connectionId - SavedConnection 的 UUID
 * @param options.createTab - appStore.createTab
 * @param options.toast - toast 通知函数
 * @param options.t - i18n 翻译函数
 * @param options.onError - 可选错误回调（Sidebar 中是 openConnectionEditor）
 */
export async function connectToSaved(
  connectionId: string,
  options: ConnectToSavedOptions,
): Promise<void>;
```

**修改点**：
1. 创建 `src/lib/connectToSaved.ts`——从 Sidebar.tsx 提取逻辑
2. `Sidebar.tsx` 中 `handleConnectSaved` 改为调用 `connectToSaved()`
3. `SessionManagerPanel.tsx` 中也调用 `connectToSaved()`

### 5.2 useSessionManager Hook

本地状态管理（不需要全局 Store，因为关闭 Tab 后状态不需要保持）：

```typescript
// src/components/sessionManager/useSessionManager.ts
interface SessionManagerState {
  // Data
  connections: ConnectionInfo[];
  groups: string[];
  loading: boolean;
  
  // Folder tree
  selectedGroup: string | null;    // null = "All"
  expandedGroups: Set<string>;
  
  // Table
  searchQuery: string;
  sortField: SortField | null;
  sortDirection: 'asc' | 'desc';
  selectedIds: Set<string>;
  
  // Computed
  filteredConnections: ConnectionInfo[];  // 经过 group + search 过滤 + 排序后
  folderTree: FolderNode[];               // 从 groups 构建的树
}
```

---

## 6. 实施阶段

### Phase 1: Tab 注册 ✅

**目标**：让 `session_manager` 出现在 Tab 系统中。

**修改文件与精确位置**：

#### 1.1 `src/types/index.ts` (line 292)
```diff
- export type TabType = 'terminal' | 'sftp' | 'forwards' | 'settings' | 'connection_monitor' | 'connection_pool' | 'topology' | 'local_terminal' | 'ide' | 'file_manager';
+ export type TabType = 'terminal' | 'sftp' | 'forwards' | 'settings' | 'connection_monitor' | 'connection_pool' | 'topology' | 'local_terminal' | 'ide' | 'file_manager' | 'session_manager';
```

#### 1.2 `src/store/appStore.ts` (line 448)
在 `createTab` 函数的 singleton 分支中添加 `session_manager`：
```diff
- if (type === 'settings' || type === 'connection_monitor' || type === 'connection_pool' || type === 'topology' || type === 'file_manager') {
+ if (type === 'settings' || type === 'connection_monitor' || type === 'connection_pool' || type === 'topology' || type === 'file_manager' || type === 'session_manager') {
```

在 title/icon 分支中添加（约 line 462-470 之后）：
```typescript
} else if (type === 'session_manager') {
  title = i18n.t('tabs.session_manager');
  icon = '📋';
}
```

#### 1.3 `src/components/layout/AppLayout.tsx`

添加 lazy import（约 line 23）：
```typescript
const SessionManagerPanel = lazy(() => import('../sessionManager').then(m => ({ default: m.SessionManagerPanel })));
```

在 tab 渲染区域（约 line 140，`{tab.type === 'file_manager'` 之后）添加：
```tsx
{tab.type === 'session_manager' && (
  <Suspense fallback={<ViewLoader />}>
    <SessionManagerPanel />
  </Suspense>
)}
```

#### 1.4 i18n（11 个 locale 的 `common.json`）

在 `tabs` 对象中添加：
```json
"session_manager": "Session Manager"
```

对应语言翻译：
| Locale | 翻译 |
|--------|------|
| en | Session Manager |
| zh-CN | 会话管理器 |
| zh-TW | 工作階段管理器 |
| ja | セッションマネージャー |
| ko | 세션 관리자 |
| fr-FR | Gestionnaire de sessions |
| de | Sitzungsmanager |
| es-ES | Gestor de sesiones |
| pt-BR | Gerenciador de sessões |
| it | Gestore sessioni |
| vi | Quản lý phiên |

**验证**：`npx tsc --noEmit` 无错误。

---

### Phase 2: 核心 SessionManagerPanel 组件 ✅

**目标**：搭建 Panel 骨架，左右分栏 + Toolbar。

创建 `src/components/sessionManager/SessionManagerPanel.tsx`：

```tsx
export const SessionManagerPanel = () => {
  const { t } = useTranslation();
  // useSessionManager hook 管理所有本地状态
  
  return (
    <div className="h-full w-full flex flex-col bg-theme-bg">
      {/* Toolbar */}
      <ManagerToolbar ... />
      
      {/* Content: left folder tree + right table */}
      <div className="flex-1 flex overflow-hidden">
        {/* Folder Tree */}
        <div className="w-[200px] min-w-[160px] border-r border-theme-border overflow-y-auto">
          <FolderTree ... />
        </div>
        
        {/* Connection Table */}
        <div className="flex-1 overflow-auto">
          <ConnectionTable ... />
        </div>
      </div>
    </div>
  );
};
```

同时创建 `index.ts` barrel：
```typescript
export { SessionManagerPanel } from './SessionManagerPanel';
```

**验证**：Tab 可打开，显示骨架布局。

---

### Phase 3: FolderTree 组件 ✅

**目标**：从 `groups[]` 构建可展开的层级文件夹树。

**核心算法** — 将扁平 group 列表转为树：
```typescript
// 输入: ["Production", "Production/Asia", "Production/Europe", "Development"]
// 输出:
// ├── All (root，特殊节点)
// ├── Production
// │   ├── Asia
// │   └── Europe
// ├── Development
// └── Ungrouped (无分组连接)
```

**FolderTree 组件功能**：
- 渲染树节点，每个节点显示名称 + 连接数量 badge
- 点击节点 → 更新 `selectedGroup` → 过滤右侧表格
- 展开/折叠节点
- 右键菜单（使用已有 `context-menu` UI 组件）
- "All" 根节点始终可见

---

### Phase 4: ConnectionTable 组件 ✅

**目标**：渲染排序、可选中的连接表格。

**关键点**：
- 虚拟化 **不需要**（保存连接一般几十到几百个，不需要虚拟滚动）
- 使用 `<table>` + Tailwind 样式（与 `ConnectionsPanel` 风格一致）
- 列头排序 → 本地排序（`Array.sort`）
- 全选 checkbox、行 checkbox → 管理 `selectedIds` Set
- Auth type 显示为 badge（🔑 key, 🔒 password, 🤖 agent）
- Tags 显示为彩色小 pills
- `color` 字段渲染为行左侧的 4px 竖线指示器
- 空状态提示

**ConnectionTableRow 组件**：
- 双击 → `connectToSaved(row.id, ...)`
- Actions 列：
  - ▶ 连接按钮
  - ✏️ 编辑按钮 → `openConnectionEditor(row.id)` / `toggleModal('editConnection', true, row.id)`
  - ⋮ 更多下拉（DropdownMenu）

---

### Phase 5: 搜索/排序/过滤 Toolbar ✅

**目标**：顶部工具栏。

**ManagerToolbar 组件**：
```tsx
<div className="flex items-center gap-2 px-4 py-2 border-b border-theme-border">
  {/* 搜索框 */}
  <div className="relative flex-1 max-w-sm">
    <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-4 w-4 text-theme-text-muted" />
    <Input 
      value={searchQuery}
      onChange={(e) => setSearchQuery(e.target.value)}
      placeholder={t('sessionManager.toolbar.search_placeholder')}
      className="pl-8"
    />
  </div>
  
  {/* New Connection */}
  <Button onClick={() => toggleModal('newConnection', true)}>
    <Plus className="h-4 w-4 mr-1" />
    {t('sessionManager.toolbar.new_connection')}
  </Button>
  
  {/* Batch Actions (仅当 selectedIds.size > 0) */}
  {selectedIds.size > 0 && <BatchActionsMenu ... />}
  
  {/* Import / Export */}
  <Button variant="ghost" ...>Import</Button>
  <Button variant="ghost" ...>Export</Button>
</div>
```

**搜索实现**：
```typescript
// 使用 useMemo + debounce 搜索
// 优先使用后端 searchConnections（模糊匹配名称/主机/用户名）
// 如果查询为空 → 使用 getConnections 或 getConnectionsByGroup
useEffect(() => {
  const timer = setTimeout(async () => {
    if (searchQuery.trim()) {
      const results = await api.searchConnections(searchQuery);
      setConnections(results);
    } else {
      const all = selectedGroup 
        ? await api.getConnectionsByGroup(selectedGroup)
        : await api.getConnections();
      setConnections(all);
    }
  }, 300);
  return () => clearTimeout(timer);
}, [searchQuery, selectedGroup]);
```

---

### Phase 6: 批量操作与行操作 ✅

**BatchActionsMenu 组件**（使用 `dropdown-menu` UI 组件）：

| 操作 | 实现 |
|------|------|
| 批量删除 | `selectedIds.forEach(id => api.deleteConnection(id))` + confirm dialog |
| 批量移动到分组 | 弹出分组选择器 → `api.saveConnection({ id, group: newGroup })` for each |
| 批量添加标签 | 弹出标签输入 → `api.saveConnection({ id, tags: [...existing, ...new] })` for each |

**行连接操作**：调用提取后的 `connectToSaved(id, options)` 函数。

**行复制操作**：
```typescript
const handleDuplicate = async (conn: ConnectionInfo) => {
  await api.saveConnection({
    name: `${conn.name} (Copy)`,
    group: conn.group,
    host: conn.host,
    port: conn.port,
    username: conn.username,
    auth_type: conn.auth_type,
    key_path: conn.key_path ?? undefined,
    tags: conn.tags,
    color: conn.color ?? undefined,
  });
  await refreshConnections(); // 刷新列表
};
```

**TagEditor 组件**：
- 弹出式小面板（Popover / DropdownMenu）
- 显示当前标签 + 删除按钮
- 输入框添加新标签 + Enter 确认
- 保存时调用 `api.saveConnection({ id, tags: updatedTags })`

---

### Phase 7: Sidebar 入口按钮 ✅

**目标**：在 Sidebar 图标列中添加 Session Manager 入口。

**修改文件**：`src/components/layout/Sidebar.tsx`

在 Saved Connections `<Database>` 按钮之后（约 line 845），添加：
```tsx
{/* Session Manager (Full Tab) */}
<Button
  variant={tabs.find(t => t.id === activeTabId)?.type === 'session_manager' ? 'secondary' : 'ghost'}
  size="icon"
  onClick={() => createTab('session_manager')}
  title={t('sidebar.panels.session_manager')}
  className="rounded-md h-9 w-9"
>
  <LayoutList className="h-5 w-5" />
</Button>
```

需要从 `lucide-react` 导入 `LayoutList` 图标。

同时需要在 `sidebar.json` 的 `panels` 中添加：
```json
"session_manager": "Session Manager"
```

**注意**：Sidebar 有两套图标区域（collapsed/expanded），需要在两处都添加。搜索 `connection_monitor` 找到两处插入点。

---

### Phase 8: i18n 完整翻译 ✅

**目标**：创建 `sessionManager.json` 翻译文件。

需要在 **11 个 locale** 目录下各创建 `sessionManager.json`。

**英文模板** (`src/locales/en/sessionManager.json`)：
```json
{
  "sessionManager": {
    "title": "Session Manager",
    "toolbar": {
      "search_placeholder": "Search connections...",
      "new_connection": "New Connection",
      "import": "Import",
      "export": "Export"
    },
    "folder_tree": {
      "all_connections": "All Connections",
      "ungrouped": "Ungrouped",
      "recent": "Recent",
      "rename_group": "Rename Group",
      "delete_group": "Delete Group",
      "new_subgroup": "New Subgroup",
      "confirm_delete_group": "Delete group \"{{name}}\"? Connections will be moved to Ungrouped."
    },
    "table": {
      "name": "Name",
      "host": "Host",
      "port": "Port",
      "username": "Username",
      "auth_type": "Auth",
      "group": "Group",
      "tags": "Tags",
      "last_used": "Last Used",
      "actions": "Actions",
      "no_connections": "No connections found",
      "no_connections_hint": "Create a new connection to get started",
      "no_search_results": "No connections match your search",
      "select_all": "Select all",
      "selected_count": "{{count}} selected",
      "never_used": "Never"
    },
    "actions": {
      "connect": "Connect",
      "edit": "Edit",
      "duplicate": "Duplicate",
      "delete": "Delete",
      "manage_tags": "Manage Tags",
      "move_to_group": "Move to Group",
      "confirm_delete": "Delete connection \"{{name}}\"?",
      "confirm_batch_delete": "Delete {{count}} selected connections?"
    },
    "batch": {
      "title": "Batch Actions",
      "delete": "Delete Selected",
      "move_to_group": "Move to Group",
      "add_tags": "Add Tags"
    },
    "tags": {
      "add_tag": "Add tag...",
      "remove_tag": "Remove tag \"{{tag}}\""
    },
    "toast": {
      "connection_deleted": "Connection deleted",
      "connections_deleted": "{{count}} connections deleted",
      "connection_duplicated": "Connection duplicated",
      "connections_moved": "{{count}} connections moved to \"{{group}}\"",
      "tags_updated": "Tags updated"
    }
  }
}
```

需要在 `src/i18n.ts` 中注册新的命名空间（如果使用命名空间），或确认现有的 `translation` 单命名空间模式。

---

## 7. `connectToSaved` 提取规格

### 7.1 当前代码位置

`src/components/layout/Sidebar.tsx` lines 610-730，`handleConnectSaved` 回调函数。

### 7.2 依赖分析

此函数依赖：
- `api.getSavedConnectionForConnect(id)` — 获取含密码的完整信息
- `api.markConnectionUsed(id)` — 更新最后使用时间
- `useSessionTreeStore` — `expandManualPreset`, `connectNodeWithAncestors`, `createTerminalForNode`, `nodes`
- `useAppStore` — `createTab('terminal', sessionId)`
- `addRootNode` — 来自 `useSessionTreeStore`
- `toast` / `t` — UI 通知和翻译
- 错误时 `openConnectionEditor(id)` — 可选回调

### 7.3 提取后的函数签名

```typescript
// src/lib/connectToSaved.ts

import { api } from './api';
import { useSessionTreeStore } from '../store/sessionTreeStore';
import { useAppStore } from '../store/appStore';
import { UnifiedFlatNode } from '../types';

export interface ConnectToSavedOptions {
  createTab: (type: 'terminal', sessionId: string) => void;
  toast: (props: { title: string; description: string; variant: string }) => void;
  t: (key: string, options?: Record<string, unknown>) => string;
  onError?: (connectionId: string) => void;
}

export async function connectToSaved(
  connectionId: string,
  options: ConnectToSavedOptions,
): Promise<void> {
  const { createTab, toast, t, onError } = options;
  
  try {
    const savedConn = await api.getSavedConnectionForConnect(connectionId);
    
    // ... 提取自 Sidebar.tsx handleConnectSaved 的全部逻辑 ...
    // auth type 映射、proxy chain 处理、直连处理、终端创建
    
    await api.markConnectionUsed(connectionId);
  } catch (error) {
    console.error('Failed to connect to saved connection:', error);
    const errorMsg = String(error);
    if (!errorMsg.includes('already connecting') && 
        !errorMsg.includes('already connected') &&
        !errorMsg.includes('CHAIN_LOCK_BUSY') &&
        !errorMsg.includes('NODE_LOCK_BUSY')) {
      onError?.(connectionId);
    }
  }
}
```

### 7.4 Sidebar.tsx 修改

```typescript
// 修改后
import { connectToSaved } from '../../lib/connectToSaved';

const handleConnectSaved = useCallback(async (connectionId: string) => {
  await connectToSaved(connectionId, {
    createTab,
    toast,
    t,
    onError: openConnectionEditor,
  });
}, [createTab, toast, t, openConnectionEditor]);
```

---

## 8. 验证清单

### 功能验证
- [x] Session Manager tab 可通过侧边栏按钮打开
- [x] 单例模式：多次点击不会创建多个 tab
- [x] 文件夹树正确展示所有分组（含嵌套）
- [x] 选择文件夹 → 表格正确过滤
- [x] 搜索框输入 → 表格实时过滤（300ms debounce）
- [x] 列头排序工作正常（asc/desc 切换）
- [x] 双击行 → 成功连接（直连 + proxy_chain）
- [x] 行操作：编辑打开 Modal、复制创建副本、删除需确认
- [x] 批量选择：checkbox、Ctrl+Click、Shift+Click
- [x] 批量删除：确认后执行
- [x] 批量移动分组：弹出选择器
- [ ] 标签编辑：添加/删除标签（TagEditor 推迟至 v1.1）
- [x] Import/Export 按钮打开已有 Modal
- [x] `color` 字段渲染为行左侧彩色指示器

### 技术验证
- [x] `npx tsc --noEmit` — 0 错误
- [x] `npm run i18n:check` — sessionManager 命名空间全部通过（其他历史缺失与本功能无关）
- [x] Sidebar.tsx 中 `handleConnectSaved` 已替换为 `connectToSaved()` 调用
- [x] Tab 关闭后重新打开，状态重置（本地状态，非全局 Store）
- [x] 不同主题下 UI 正常（使用 `theme-*` class）

### 安全验证
- [x] 密码永不在前端表格中显示
- [x] 连接前通过 `getSavedConnectionForConnect` 从 Keychain 获取密码
- [x] 删除操作需要用户确认

---

## 9. 技术注意事项

### 9.1 主题适配
所有样式使用 `bg-theme-bg`, `text-theme-text`, `border-theme-border` 等 CSS 变量类，不使用硬编码颜色。参考已有组件如 `ConnectionsPanel`、`SettingsView` 的用法。

### 9.2 响应式
Panel 作为 Tab 内容占满整个工作区，不需要移动端适配。但左侧文件夹树应支持拖拽调整宽度（可选 Phase 2 优化）。

### 9.3 性能
- 连接列表通常 < 500 条，无需虚拟滚动
- `searchConnections` 在后端执行，前端无需索引
- `useCallback` / `useMemo` 避免不必要的重渲染

### 9.4 数据刷新策略
- 打开 Tab 时加载数据
- 新建/编辑/删除连接后刷新列表（调用 `loadSavedConnections()` + 本地重新 fetch）
- 可监听 `appStore.savedConnections` 变化自动同步（但需注意避免循环更新）

### 9.5 不修改后端
所有需要的 API 已存在于 Rust 后端。本 feature 纯前端实现。

---

## 10. 估算时间

| 阶段 | 状态 |
|------|------|
| Phase 1: Tab 注册 | ✅ 完成 |
| Phase 2: 核心 Panel 骨架 | ✅ 完成 |
| Phase 3: FolderTree | ✅ 完成 |
| Phase 4: ConnectionTable | ✅ 完成 |
| Phase 5: Toolbar + 搜索 | ✅ 完成 |
| Phase 6: 批量操作 + 行操作 | ✅ 完成（TagEditor 推迟） |
| Phase 7: Sidebar 入口 | ✅ 完成 |
| Phase 8: i18n 完整翻译 | ✅ 完成 |
| connectToSaved 提取 | ✅ 完成 |
