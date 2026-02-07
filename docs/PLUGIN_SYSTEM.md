# OxideTerm 运行时动态插件系统设计文档

> **状态**: 已实施  
> **版本**: v1.1  
> **日期**: 2026-02-09  
> **前置依赖**: OxideTerm v1.6.2+

---

## 1. 概述

OxideTerm 当前所有功能模块（AI、IDE、SFTP、端口转发等）都是编译时内置的。本方案引入运行时动态插件系统，允许第三方开发者编写插件，用户可在运行时安装/卸载/启用/禁用。

### 1.1 支持的扩展能力

| 扩展类型 | 说明 | 示例 |
|----------|------|------|
| 连接生命周期钩子 | 订阅 connect/disconnect/reconnect/link_down | SSH 审计、连接统计 |
| UI 视图 | 注册新 Tab 类型、侧边栏面板 | 仪表盘、监控面板 |
| 终端增强 | 输入拦截、输出处理、自定义快捷键 | 命令补全、输出高亮 |

### 1.2 架构决策：Membrane-based Direct Injection

插件 ESM bundle 在主线程运行，通过 `Proxy + Object.freeze` 构建的 membrane 层获取受限 API。

**选择理由**：
- iframe 无法访问 xterm.js 实例 → 终端钩子无法实现
- Web Worker 无法操作 DOM → UI 注册无法实现
- Membrane 提供冻结只读状态快照、可撤销事件订阅、每回调 try/catch 错误边界

**不做 feature gate**：插件基础设施始终编译，不像 `local-terminal` 那样可剥离。

---

## 2. 插件包结构

### 2.1 磁盘布局

```
~/.oxideterm/plugins/{plugin-id}/
  plugin.json          # 清单文件（必需）
  index.js             # ESM 入口（必需，单文件 bundle）
  icon.svg             # 图标（可选）
  locales/             # i18n 翻译（可选）
    en.json
    zh-CN.json
```

路径由 Rust `config_dir()` 决定：
- macOS/Linux: `~/.oxideterm/plugins/`
- Windows: `%APPDATA%\OxideTerm\plugins\`

### 2.2 plugin.json 清单

```json
{
  "id": "com.example.ssh-audit",
  "name": "SSH Audit",
  "version": "1.0.0",
  "description": "Security audit for SSH connections",
  "author": "Example Author",
  "main": "./index.js",
  "engines": { "oxideterm": ">=1.6.0" },

  "contributes": {
    "tabs": [{
      "id": "ssh-audit-dashboard",
      "title": "plugin.ssh_audit.tab_title",
      "icon": "Shield"
    }],
    "sidebarPanels": [{
      "id": "ssh-audit-panel",
      "title": "plugin.ssh_audit.panel_title",
      "icon": "Shield",
      "position": "bottom"
    }],
    "settings": [{
      "id": "scanDepth",
      "type": "number",
      "default": 3,
      "title": "plugin.ssh_audit.settings.scan_depth"
    }],
    "terminalHooks": {
      "inputInterceptor": true,
      "outputProcessor": true,
      "shortcuts": [{ "key": "Ctrl+Shift+A", "command": "sshAudit.scan" }]
    },
    "connectionHooks": ["onConnect", "onDisconnect", "onReconnect", "onLinkDown", "onIdle"],
    "apiCommands": []
  },

  "locales": "./locales"
}
```

### 2.3 插件入口约定

```typescript
// index.js (ESM, 所有依赖打包为单文件)
// React/ReactDOM/zustand 从 window.__OXIDE__ 引用，构建时标记 external

export function activate(ctx: PluginContext): void | Promise<void> {
  // 注册钩子、UI 组件、事件处理器
  ctx.events.onConnect((snapshot) => {
    console.log('Connected:', snapshot.host);
  });

  ctx.ui.registerTabView('my-tab', MyTabComponent);
  ctx.terminal.registerInputInterceptor((data, { sessionId }) => {
    return data; // 原样传递，或修改/返回 null 抑制
  });
}

export function deactivate(): void | Promise<void> {
  // 可选清理（所有 Disposable 会自动撤销）
}
```

---

## 3. PluginContext API（8 个命名空间）

插件通过 `activate(ctx)` 接收的唯一 API 入口。整个对象通过 `Object.freeze()` 递归冻结。

### 3.1 `ctx.connections`（只读连接状态）

```typescript
interface PluginConnectionsAPI {
  getAll(): ReadonlyArray<ConnectionSnapshot>;  // 冻结快照
  get(connectionId: string): ConnectionSnapshot | null;
  getState(connectionId: string): SshConnectionState | null;
}
```

- 数据来源：`appStore.connections`，每次调用返回新的 `Object.freeze()` 快照
- 插件**不能**直接访问 Zustand store

### 3.2 `ctx.events`（生命周期 + 插件间通信）

```typescript
interface PluginEventsAPI {
  onConnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onDisconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onLinkDown(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onReconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onIdle(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onSessionCreated(handler: (info: { sessionId: string; connectionId: string }) => void): Disposable;
  onSessionClosed(handler: (info: { sessionId: string }) => void): Disposable;
  // 插件间通信（命名空间自动隔离为 plugin:{pluginId}:{name}）
  on(name: string, handler: (data: unknown) => void): Disposable;
  emit(name: string, data: unknown): void;
}
```

- 回调通过 `queueMicrotask()` 异步调用，不阻塞 `appStore` 状态更新
- 保护 Strong Consistency Sync 不变量

### 3.3 `ctx.ui`（视图注册）

```typescript
interface PluginUIAPI {
  registerTabView(tabId: string, component: React.ComponentType<PluginTabProps>): Disposable;
  registerSidebarPanel(panelId: string, component: React.ComponentType): Disposable;
  openTab(tabId: string): void;
  showToast(opts: { title: string; description?: string; variant?: 'default' | 'success' | 'error' | 'warning' }): void;
  showConfirm(opts: { title: string; description: string }): Promise<boolean>;
}
```

- `tabId` / `panelId` 必须在 manifest `contributes.tabs` / `contributes.sidebarPanels` 中声明
- 未声明的 ID 调用 `registerTabView` 会抛出错误

### 3.4 `ctx.terminal`（终端钩子）

```typescript
type InputInterceptor = (data: string, context: { sessionId: string }) => string | null;
type OutputProcessor = (data: Uint8Array, context: { sessionId: string }) => Uint8Array;

interface PluginTerminalAPI {
  registerInputInterceptor(handler: InputInterceptor): Disposable;
  registerOutputProcessor(handler: OutputProcessor): Disposable;
  registerShortcut(command: string, handler: () => void): Disposable;
  writeToTerminal(sessionId: string, text: string): void;  // ⚠️ 尚未实现 (no-op stub)，插件可使用 output processor 替代
  getBuffer(sessionId: string): string | null;              // 只读
  getSelection(sessionId: string): string | null;           // 只读
}
```

- `command` 必须在 manifest `contributes.terminalHooks.shortcuts` 中声明
- 管道是**同步**的，fail-open（异常时传递原始数据）
- 必须尊重 `inputLockedRef` 检查 — 插件不能绕过 State Gating

### 3.5 `ctx.settings`（插件设置）

```typescript
interface PluginSettingsAPI {
  get<T>(key: string): T;
  set<T>(key: string, value: T): void;
  onChange(key: string, handler: (newValue: unknown) => void): Disposable;
}
```

- key 必须在 manifest `contributes.settings` 中声明
- 底层使用 `localStorage` + 前缀 `oxide-plugin-{pluginId}-setting-`

### 3.6 `ctx.i18n`

```typescript
interface PluginI18nAPI {
  t(key: string, params?: Record<string, string | number>): string;
  getLanguage(): string;
  onLanguageChange(handler: (lang: string) => void): Disposable;
}
```

- `t(key)` 自动拼接前缀 `plugin.{pluginId}.{key}` 后调用 `i18n.t()`
- 插件 locales 通过 `i18n.addResourceBundle()` 注入

### 3.7 `ctx.storage`（插件作用域持久化）

```typescript
interface PluginStorageAPI {
  get<T>(key: string): T | null;
  set<T>(key: string, value: T): void;
  remove(key: string): void;
}
```

- 底层：`localStorage` + 前缀 `oxide-plugin-{pluginId}-`
- 卸载插件时可选清理

### 3.8 `ctx.api`（受限后端调用）

```typescript
interface PluginBackendAPI {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
}
```

- **白名单机制**：只代理 manifest `contributes.apiCommands` 中声明的 Tauri 命令
- 默认白名单为空 — 插件必须显式声明需要哪些后端命令

---

## 4. 安全模型

### 4.1 隔离层级

| 层面 | 机制 | 说明 |
|------|------|------|
| API 隔离 | `Object.freeze()` + `Proxy` | 插件只能通过冻结的 PluginContext 交互 |
| 状态只读 | `ConnectionSnapshot` 深冻结 | 不可变，每次调用返回新快照 |
| Disposable 自动撤销 | `pluginStore.cleanupPlugin()` | 卸载时一键清理所有注册 |
| UI 错误边界 | React `ErrorBoundary` | 插件渲染崩溃不影响宿主 |
| 回调错误边界 | try/catch + 计数 | 60s 内 10 次报错自动禁用 |
| 终端 fail-open | try/catch 传递原始数据 | 插件异常不阻塞终端 I/O |
| IPC 白名单 | manifest `apiCommands` | 只代理声明的命令 |
| 事件隔离 | `plugin:{id}:` 前缀 | 插件间事件不会互相干扰 |
| 路径安全 | `..` 检测 | `read_plugin_file` 拒绝向上遍历 |

### 4.2 错误熔断

```
单个回调异常 → catch + errorCount++
errorCount >= 10 (within 60s) → 自动调用 unloadPlugin()
→ Toast 通知用户 "插件 X 已因频繁错误被禁用"
→ 插件 state 设为 'disabled'
→ 需要用户手动在插件管理器中重新启用
```

### 4.3 与系统不变量的关系

- **Strong Consistency Sync**：插件不直接监听 Tauri 事件，而是订阅 `appStore.connections` 状态变化的后处理事件，保护 refreshConnections 单一真相源
- **Key-Driven Reset**：插件不参与终端重建，Terminal key 机制不变
- **State Gating**：`inputLockedRef` 检查在插件管道之前执行，插件看不到被锁定的输入

---

## 5. 终端钩子实现细节

### 5.1 输入拦截插入点

在 `TerminalView.tsx` 的 `term.onData` 回调中：

```
term.onData(data)
  → inputLockedRef 检查（State Gating 不变量保护）
  → runInputPipeline(data, { sessionId })   ← 新增
     ├─ interceptor1(data) → modified1
     ├─ interceptor2(modified1) → modified2
     └─ 任意返回 null → 整体返回 null（抑制输入）
  → if (result === null) return             ← 新增（插件抑制）
  → encodeDataFrame(result)
  → ws.send(frame)
```

### 5.2 输出处理插入点

在统一的 `handleWsMessage` 函数（Phase 0 重构）中：

```
ws.onmessage → parse frame → MSG_TYPE_DATA
  → payloadCopy = payload.slice()
  → runOutputPipeline(payloadCopy, { sessionId })  ← 新增
     ├─ processor1(data) → modified1
     └─ processor2(modified1) → modified2
  → pendingDataRef.push(result)
  → RAF → term.write(combined)
```

- 只处理 `MSG_TYPE_DATA` (0x00) 帧
- HEARTBEAT (0x02) / ERROR (0x03) 帧不经过插件（保护 Wire Protocol v1 不变量）

### 5.3 快捷键

在 `useTerminalKeyboard.ts` 的 `useAppShortcuts` 中，内置快捷键匹配之后添加插件查找：

```typescript
// 内置快捷键优先
for (const shortcut of shortcuts) {
  if (matchesShortcut(event, shortcut)) { ... }
}
// 插件快捷键次之
const pluginHandler = matchPluginShortcut(event);
if (pluginHandler) { event.preventDefault(); pluginHandler(); return; }
```

---

## 6. UI 集成细节

### 6.1 React 共享

插件需要与宿主共享同一个 React 实例（否则 hooks 崩溃）。在 `src/main.tsx` 中暴露：

```typescript
import React from 'react';
import ReactDOM from 'react-dom/client';
import { create } from 'zustand';
import * as lucideReact from 'lucide-react';

window.__OXIDE__ = { React, ReactDOM: { createRoot: ReactDOM.createRoot }, zustand: { create }, lucideReact, ui: pluginUIKit };
```

插件构建时将 `react`, `react-dom`, `zustand`, `lucide-react` 标记为 external，运行时从 `window.__OXIDE__` 解析。

### 6.2 Tab 扩展

**类型扩展**（`types/index.ts`）：
```typescript
export type TabType = '..existing..' | 'plugin';

export interface Tab {
  // ...existing fields...
  pluginTabId?: string;  // 新增：插件 Tab 标识
}
```

**渲染分支**（`AppLayout.tsx`）：

在现有 tab 条件分支的最后添加：
```tsx
{tab.type === 'plugin' && tab.pluginTabId && (
  <Suspense fallback={<ViewLoader />}>
    <PluginTabRenderer pluginTabId={tab.pluginTabId} tab={tab} />
  </Suspense>
)}
```

`PluginTabRenderer` 从 `pluginStore.tabViews` Map 查找组件，用 `ErrorBoundary` 包裹。

**createTab 分支**（`appStore.ts`）：

```typescript
if (type === 'plugin') {
  const existing = tabs.find(t => t.pluginTabId === pluginTabId);
  if (existing) { set({ activeTabId: existing.id }); return; }
  // 从 pluginStore 获取 manifest 信息
  const newTab = { id: uuid(), type: 'plugin', title, icon, pluginTabId };
  set({ tabs: [...tabs, newTab], activeTabId: newTab.id });
  return;
}
```

### 6.3 侧边栏面板

Phase 0 将 Sidebar 按钮重构为 data-driven 数组后，插件面板通过 `pluginStore.sidebarPanels` 动态注入按钮条目。

`SidebarSection` 类型扩展为接受 `plugin:{pluginId}:{panelId}` 格式的字符串。

---

## 7. 插件加载机制

### 7.1 加载流程

```
1. discoverPlugins()
   └─ api.pluginList() → Rust 扫描 plugins/ 目录 → Vec<PluginManifest>

2. validateManifest(manifest)
   ├─ 检查 id, name, version, main 必填字段
   ├─ 检查 engines.oxideterm 版本兼容
   └─ 检查 contributes 中引用的 id 唯一性

3. loadPlugin(manifest)
   ├─ api.pluginReadFile(id, 'index.js') → Uint8Array
   ├─ Blob(content, 'application/javascript') → URL.createObjectURL
   ├─ await import(blobUrl) → 获取 { activate, deactivate }
   ├─ buildPluginContext(manifest) → 构建 membrane 层
   ├─ loadPluginI18n(id, localesDir) → 加载翻译资源
   ├─ await activate(ctx) → 5 秒超时
   ├─ URL.revokeObjectURL(blobUrl)
   └─ 状态 → 'active'

4. 失败处理
   ├─ activate 超时 → state='error', Toast "插件激活超时"
   ├─ activate 异常 → state='error', Toast 显示错误
   └─ import 失败 → state='error', Toast "插件加载失败"
```

### 7.2 卸载流程

```
1. unloadPlugin(pluginId)
   ├─ await module.deactivate() → 5 秒超时（可选）
   ├─ pluginStore.cleanupPlugin(pluginId)
   │   ├─ 撤销所有 Disposable
   │   ├─ 移除 tabViews、sidebarPanels 注册
   │   ├─ 移除 inputInterceptors、outputProcessors
   │   ├─ 移除 shortcuts
   │   └─ 关闭该插件的所有打开 Tab
   ├─ removePluginI18n(pluginId)
   └─ 状态 → 'inactive'
```

### 7.3 启动初始化

在 `App.tsx` 中，应用启动后：
```
discoverPlugins()
  → loadPluginConfig() → 获取启用/禁用列表
  → 对每个 enabled 的插件: await loadPlugin(manifest)
```

---

## 8. 后端命令（Rust）

### 8.1 新增 `src-tauri/src/commands/plugin.rs`

```rust
#[tauri::command]
pub async fn list_plugins() -> Result<Vec<PluginManifest>, String>
// 扫描 config_dir()/plugins/ 目录，读取每个子目录的 plugin.json

#[tauri::command]
pub async fn read_plugin_file(plugin_id: String, relative_path: String) -> Result<Vec<u8>, String>
// 读取指定插件的文件内容
// 安全检查：relative_path 不能包含 ".."

#[tauri::command]
pub async fn save_plugin_config(config: String) -> Result<(), String>
// 将插件启用/禁用配置写入 config_dir()/plugin-config.json

#[tauri::command]
pub async fn load_plugin_config() -> Result<String, String>
// 读取 config_dir()/plugin-config.json
```

### 8.2 扩展 `config/storage.rs`

```rust
pub fn plugins_dir() -> Result<PathBuf, StorageError> {
    Ok(config_dir()?.join("plugins"))
}
```

### 8.3 注册命令

在 `commands/mod.rs` 添加 `pub mod plugin;`。
在 `lib.rs` 两处 `invoke_handler!` 宏中注册 4 个命令。

---

## 9. 前置重构（Phase 0）

在引入插件系统之前，需要两个独立的重构消除技术债：

### 9.1 TerminalView `handleWsMessage` 提取

**问题**：`TerminalView.tsx` 有两处几乎相同的 `ws.onmessage` 处理器：
- **L505**（重连路径）：完整实现，含 Windows IME `isComposingRef` 分支
- **L1006**（初始连接路径）：只有 RAF 批处理，**缺少** IME 分支（bug）

**重构**：

1. 在组件 `useEffect` 内（所有 ref 可见的作用域）定义：
```typescript
const handleWsMessage = (event: MessageEvent, ws: WebSocket) => {
  if (!isMountedRef.current || wsRef.current !== ws) return;
  // 以 L505 版本为基准（含 Windows IME 分支）
  // 统一 ArrayBuffer 解析为 Uint8Array → DataView
  const data = event.data instanceof ArrayBuffer 
    ? new Uint8Array(event.data) 
    : new Uint8Array(event.data);
  // ...完整的帧解析 + switch/case + Windows IME 分支...
};
```

2. 两处 `ws.onmessage = ...` 都改为：
```typescript
ws.onmessage = (e) => handleWsMessage(e, ws);
```

3. **验证**：现有终端行为不变；Windows IME 在重连后也能正确工作。

### 9.2 Sidebar 按钮 data-driven 重构

**问题**：折叠态（L612-L760）和展开态（L786-L920）各有一套硬编码按钮列表，~200 行几乎完全重复。

**实际实现（v1.6.2）**：

Sidebar 按钮已重构为三区结构（`topButtons` + 分隔线 + `bottomButtons`）：

```
┌─────────────────────┐
│  展开/折叠按钮       │  ← 顶部固定
├─────────────────────┤
│  topButtons          │  ← 可滚动区域 (overflow-y-auto scrollbar-none)
│  ├─ Sessions         │
│  ├─ Saved            │
│  ├─ Session Manager  │
│  ├─ Terminal (local) │
│  ├─ Forwards         │
│  ├─ Plugin Manager   │
│  └─ 插件侧边栏面板   │  ← pluginStore.sidebarPanels 动态注入
├─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┤  ← 分隔线 (w-6 h-px bg-theme-border)
│  bottomButtons       │  ← 固定底部 (shrink-0)
│  ├─ Network          │
│  ├─ AI Chat          │
│  ├─ Settings         │
│  └─ Theme Toggle     │
└─────────────────────┘
```

- 插件图标通过 `resolvePluginIcon(panel.icon)` 解析 Lucide 组件名
- 当插件数量超出可视区域时，中间区域自动出现滚动
- 折叠/展开两种模式共享同一结构

---

## 10. 文件清单

### 10.1 需要创建的文件（15 个）

| 文件 | Phase | 用途 | 预估行数 |
|------|-------|------|---------|
| `src/types/plugin.ts` | 1 | 全部插件 TypeScript 类型 | ~150 |
| `src/store/pluginStore.ts` | 1 | 插件状态 + UI 组件注册表 | ~200 |
| `src/lib/plugin/pluginLoader.ts` | 3 | 发现、校验、加载、卸载生命周期 | ~250 |
| `src/lib/plugin/pluginContextFactory.ts` | 3 | 构建 Membrane 隔离的 PluginContext | ~300 |
| `src/lib/plugin/pluginEventBridge.ts` | 4 | appStore → 插件事件派发 | ~120 |
| `src/lib/plugin/pluginTerminalHooks.ts` | 5 | 输入/输出管道 + 快捷键查找 | ~100 |
| `src/lib/plugin/pluginSettingsManager.ts` | 4 | 插件设置读写与持久化 | ~80 |
| `src/lib/plugin/pluginI18nManager.ts` | 4 | 插件 i18n 命名空间注册 | ~60 |
| `src/lib/plugin/pluginStorage.ts` | 3 | 插件作用域 localStorage 封装 | ~40 |
| `src/lib/plugin/pluginUIKit.tsx` | 6 | 插件专用 React UI 组件库（24 个组件） | ~1072 |
| `src/lib/plugin/pluginIconResolver.ts` | 6 | Lucide 图标名 → React 组件动态解析 | ~35 |
| `src/components/plugin/PluginTabRenderer.tsx` | 6 | 插件 Tab 视图渲染器 | ~50 |
| `src/components/plugin/PluginSidebarRenderer.tsx` | 6 | 插件侧边栏面板渲染器 | ~50 |
| `src/components/plugin/PluginManagerView.tsx` | 7 | 插件管理 UI | ~300 |
| `src-tauri/src/commands/plugin.rs` | 2 | 后端：扫描目录、读文件、配置读写 | ~120 |

### 10.2 需要修改的文件（11 个）

| 文件 | Phase | 修改内容 |
|------|-------|----------|
| `src/components/terminal/TerminalView.tsx` | 0, 5 | 提取 `handleWsMessage`；注入输入/输出管道 |
| `src/components/layout/Sidebar.tsx` | 0, 6 | 三区布局重构（topButtons/bottomButtons + 分隔线）；插件面板注入 |
| `src/components/layout/TabBar.tsx` | 6 | 插件 Tab 图标渲染（`PluginTabIcon` + `resolvePluginIcon`） |
| `src/types/index.ts` | 1 | `TabType` 添加 `'plugin'`，`Tab` 添加 `pluginTabId?` |
| `src/store/appStore.ts` | 6 | `createTab` 添加 plugin 分支 |
| `src/store/settingsStore.ts` | 6 | `SidebarSection` 扩展支持 plugin 格式 |
| `src/components/layout/AppLayout.tsx` | 6 | Tab 渲染添加 plugin 分支 |
| `src/hooks/useTerminalKeyboard.ts` | 5 | 添加插件快捷键查找 |
| `src/main.tsx` | 3 | 暴露 `window.__OXIDE__`（含 `ui: pluginUIKit`） |
| `src/App.tsx` | 7 | 启动时初始化插件系统 |
| `src-tauri/src/commands/mod.rs` + `lib.rs` | 2 | 注册 plugin 命令模块 |

---

## 11. 实施顺序

```
Phase 0 — 前置重构（无功能变更）
  ├─ 0.1 提取 handleWsMessage（TerminalView.tsx）
  └─ 0.2 Sidebar data-driven 重构（Sidebar.tsx）

Phase 1 — 类型与 Store
  ├─ 1.1 创建 src/types/plugin.ts
  ├─ 1.2 创建 src/store/pluginStore.ts
  └─ 1.3 扩展 src/types/index.ts（TabType + Tab）

Phase 2 — 后端支持
  ├─ 2.1 创建 src-tauri/src/commands/plugin.rs
  ├─ 2.2 扩展 config/storage.rs
  ├─ 2.3 注册命令（mod.rs + lib.rs）
  └─ 2.4 扩展 src/lib/api.ts

Phase 3 — 核心加载器
  ├─ 3.1 暴露 window.__OXIDE__（main.tsx）
  ├─ 3.2 创建 pluginLoader.ts
  ├─ 3.3 创建 pluginContextFactory.ts
  └─ 3.4 创建 pluginStorage.ts

Phase 4 — 事件与设置
  ├─ 4.1 创建 pluginEventBridge.ts
  ├─ 4.2 创建 pluginI18nManager.ts
  └─ 4.3 创建 pluginSettingsManager.ts

Phase 5 — 终端钩子
  ├─ 5.1 创建 pluginTerminalHooks.ts
  ├─ 5.2 修改 TerminalView.tsx（注入管道）
  └─ 5.3 修改 useTerminalKeyboard.ts（插件快捷键）

Phase 6 — UI 集成
  ├─ 6.1 创建 PluginTabRenderer.tsx
  ├─ 6.2 创建 PluginSidebarRenderer.tsx
  ├─ 6.3 修改 AppLayout.tsx（plugin 渲染分支）
  ├─ 6.4 修改 appStore.ts（createTab plugin 分支）
  ├─ 6.5 修改 settingsStore.ts（SidebarSection 扩展）
  └─ 6.6 修改 Sidebar.tsx（插件按钮注入）

Phase 7 — 管理界面与初始化
  ├─ 7.1 创建 PluginManagerView.tsx
  └─ 7.2 修改 App.tsx（启动初始化）
```

---

## 12. 验证方式

1. `npx tsc --noEmit` — 0 类型错误
2. `npx vite build` — 前端构建成功
3. `cd src-tauri && cargo check` — Rust 编译通过
4. 创建示例插件 `com.oxideterm.hello-world` 验证完整生命周期
5. 测试连接事件钩子：连接/断开时插件收到正确事件
6. 测试终端输入拦截：插件修改输入后 WebSocket 发送修改后的数据
7. 测试插件崩溃隔离：故意抛异常的插件不影响其他功能
8. 测试插件卸载：所有 Disposable 被撤销，UI 注册被移除

---

## 13. SYSTEM_INVARIANTS 兼容性声明

本插件系统设计**完全兼容** `docs/SYSTEM_INVARIANTS.md` 中定义的所有不变量：

| 不变量 | 兼容方式 |
|--------|----------|
| Strong Consistency Sync | 插件订阅 appStore 状态变化的后处理事件，不直接监听 Tauri 事件 |
| Key-Driven Reset | 插件不参与终端重建，key 机制不变 |
| State Gating | `inputLockedRef` 检查在插件管道之前执行 |
| 双 Store 同步 | 插件只读 appStore.connections 快照，不写入 |
| Wire Protocol v1 | 只对 DATA 帧执行插件管道，HEARTBEAT/ERROR 不经过插件 |
| Session 生命周期 | 插件不持有 Session 引用，通过冻结快照交互 |
| 并发锁序 | 插件在 JS 主线程运行，不涉及 Rust 锁 |

---

*文档版本: v1.1 | 最后更新: 2026-02-09*
