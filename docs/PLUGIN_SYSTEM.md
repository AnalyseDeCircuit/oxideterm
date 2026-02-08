# OxideTerm 运行时动态插件系统设计文档

> **状态**: 已实施
> **版本**: v2.0
> **日期**: 2026-02-08
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

**v1 单文件 Bundle（默认）**：

```
~/.oxideterm/plugins/{plugin-id}/
  plugin.json          # 清单文件（必需）
  index.js             # ESM 入口（必需，单文件 bundle）
  icon.svg             # 图标（可选）
  locales/             # i18n 翻译（可选）
    en.json
    zh-CN.json
```

**v2 多文件 Package**：

```
~/.oxideterm/plugins/{plugin-id}/
  plugin.json          # 清单文件（必需，format: "package"）
  src/
    main.js            # ESM 入口（可导入同包其他模块）
    components/
      Dashboard.js     # 子模块（使用相对路径 import）
      Charts.js
    utils/
      helpers.js
  styles/
    main.css           # 声明在 manifest.styles 中自动加载
    charts.css
  assets/
    logo.png           # 通过 ctx.assets.getAssetUrl() 访问
    config.json
  locales/
    en.json
    zh-CN.json
```

**v2 多文件包** 通过内置的本地 HTTP 文件服务器加载（`http://127.0.0.1:{port}/plugins/{id}/...`），支持文件间的相对路径 `import`。详见 [8.5 插件文件服务器](#85-插件文件服务器)。

路径由 Rust `config_dir()` 决定：
- macOS/Linux: `~/.oxideterm/plugins/`
- Windows: `%APPDATA%\OxideTerm\plugins\`

### 2.2 plugin.json 清单

**v1 单文件 Bundle（默认）**：

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

**v2 多文件 Package**：

```json
{
  "id": "com.example.advanced-dashboard",
  "name": "Advanced Dashboard",
  "version": "2.0.0",
  "description": "Multi-file plugin with CSS and assets",
  "author": "Example Author",
  "main": "./src/main.js",
  "engines": { "oxideterm": ">=1.6.2" },

  "manifestVersion": 2,
  "format": "package",
  "assets": "./assets",
  "styles": ["./styles/main.css", "./styles/charts.css"],
  "sharedDependencies": {
    "react": "^18.0.0",
    "react-dom": "^18.0.0",
    "zustand": "^4.0.0",
    "lucide-react": "^0.300.0"
  },
  "repository": "https://github.com/example/advanced-dashboard",

  "contributes": {
    "tabs": [{ "id": "dashboard", "title": "Dashboard", "icon": "LayoutDashboard" }],
    "terminalHooks": { "outputProcessor": true }
  },

  "locales": "./locales"
}
```

**v2 新增字段**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `manifestVersion` | `1 \| 2` | 清单版本，默认 `1` |
| `format` | `'bundled' \| 'package'` | `bundled`（默认）= 单文件 Blob URL 加载；`package` = HTTP 服务器加载 |
| `assets` | `string` | 资源目录相对路径，配合 `ctx.assets.getAssetUrl()` 使用 |
| `styles` | `string[]` | CSS 文件列表，加载时自动注入 `<style>` 到 `<head>` |
| `sharedDependencies` | `Record<string, string>` | 声明共享依赖版本（当前支持 react, react-dom, zustand, lucide-react）|
| `repository` | `string` | 源码仓库 URL |
| `checksum` | `string` | SHA-256 校验和用于完整性验证 |

### 2.3 插件入口约定

```typescript
// index.js（v1 单文件）或 src/main.js（v2 多文件）
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

**v2 多文件示例**：

```typescript
// src/main.js — 可以使用相对路径导入
import { Dashboard } from './components/Dashboard.js';
import { formatBytes } from './utils/helpers.js';

export async function activate(ctx) {
  // 加载额外 CSS（除了 manifest.styles 自动加载的）
  const cssDisposable = await ctx.assets.loadCSS('./styles/extra.css');

  // 获取资源文件的 blob URL
  const logoUrl = await ctx.assets.getAssetUrl('./assets/logo.png');

  ctx.ui.registerTabView('dashboard', (props) => {
    const { React } = window.__OXIDE__;
    return React.createElement(Dashboard, { ...props, logoUrl });
  });
}
```

---

## 3. PluginContext API（10 个命名空间）

插件通过 `activate(ctx)` 接收的唯一 API 入口。整个对象通过 `Object.freeze()` 递归冻结。

包含：`pluginId` + 9 个子 API（`connections`、`events`、`ui`、`terminal`、`settings`、`i18n`、`storage`、`api`、`assets`）

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
  writeToTerminal(sessionId: string, text: string): void;  // ✅ 已实现：通过 terminalRegistry 写入通道直接发送数据
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

### 3.9 `ctx.assets`（资源文件访问）

```typescript
interface PluginAssetsAPI {
  loadCSS(relativePath: string): Promise<Disposable>;
  getAssetUrl(relativePath: string): Promise<string>;
  revokeAssetUrl(url: string): void;
}
```

- `loadCSS(path)` — 读取插件目录中的 CSS 文件，注入 `<style data-plugin="{id}">` 到 `<head>`
- `getAssetUrl(path)` — 读取任意资源文件（图片、字体、JSON 等），返回 blob URL
- `revokeAssetUrl(url)` — 手动释放不再需要的 blob URL
- 卸载时自动清理所有注入的 `<style>` 和未释放的 blob URL
- MIME 类型自动检测：支持 png/jpg/gif/svg/webp/woff/woff2/ttf/otf/ico/json/css/js 等
- 配合 manifest `styles` 字段，加载时自动注入声明的 CSS 文件

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

### 7.1 双策略加载

插件支持两种加载策略，由 manifest `format` 字段决定：

| 策略 | format 值 | 适用场景 | 加载方式 |
|------|-----------|----------|----------|
| **Blob URL** | `bundled`（默认） | 单文件 ESM bundle | 读取字节 → Blob → `URL.createObjectURL` → `import()` |
| **HTTP Server** | `package` | 多文件包（支持相对 import） | 启动本地 HTTP 服务 → `import(http://127.0.0.1:{port}/plugins/{id}/...)` |

### 7.2 加载流程

```
1. discoverPlugins()
   └─ api.pluginList() → Rust 扫描 plugins/ 目录 → Vec<PluginManifest>

2. validateManifest(manifest)
   ├─ 检查 id, name, version, main 必填字段
   ├─ 检查 engines.oxideterm 版本兼容
   └─ 检查 sharedDependencies 可用性（advisory warning）

3. loadPlugin(manifest)
   ├─ 判断 format:
   │   ├─ format === 'package' → loadPluginViaServer()
   │   │   ├─ 启动/复用本地 HTTP Server（首次自动启动）
   │   │   └─ import(`http://127.0.0.1:{port}/plugins/{id}/{main}`)
   │   └─ 默认 → loadPluginViaBlobUrl()
   │       ├─ api.pluginReadFile(id, main) → Uint8Array
   │       ├─ Blob(content, 'application/javascript') → URL.createObjectURL
   │       └─ import(blobUrl) → URL.revokeObjectURL(blobUrl)
   ├─ loadPluginLocales(id, localesDir) → 加载翻译资源
   ├─ 自动注入 manifest.styles 声明的 CSS 文件
   ├─ buildPluginContext(manifest) → 构建 membrane 层
   ├─ await activate(ctx) → 5 秒超时
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
   ├─ cleanupPluginAssets(pluginId)
   │   ├─ 移除所有注入的 <style data-plugin="{id}"> 标签
   │   └─ 释放所有未释放的 blob URL
   ├─ clearPluginStorage(pluginId)（卸载时清理 localStorage）
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

### 8.1 基础命令 `src-tauri/src/commands/plugin.rs`

```rust
#[tauri::command]
pub async fn list_plugins() -> Result<Vec<PluginManifest>, String>
// 扫描 config_dir()/plugins/ 目录，读取每个子目录的 plugin.json

#[tauri::command]
pub async fn read_plugin_file(plugin_id: String, relative_path: String) -> Result<Vec<u8>, String>
// 读取指定插件的文件内容
// 安全检查：per-component ".." 检测 + canonicalize 校验防止路径遍历

#[tauri::command]
pub async fn save_plugin_config(config: String) -> Result<(), String>
// 将插件启用/禁用配置写入 config_dir()/plugin-config.json

#[tauri::command]
pub async fn load_plugin_config() -> Result<String, String>
// 读取 config_dir()/plugin-config.json
```

**安全辅助函数**：

- `validate_plugin_id(id)` — 拒绝空值、`..`、路径分隔符、控制字符
- `validate_relative_path(path)` — 逐路径组件检查 `..`，拒绝绝对路径

### 8.2 插件文件服务器 `src-tauri/src/commands/plugin_server.rs`

为 v2 多文件包提供本地 HTTP 文件服务，使浏览器可以通过标准 `import()` 加载相互引用的 JS 模块。

```rust
/// 启动插件文件服务器。返回端口号。
/// 如已运行，返回现有端口。
#[tauri::command]
pub async fn start_plugin_server(server: State<Arc<PluginFileServer>>) -> Result<u16, String>

/// 获取插件服务器端口（如正在运行）
#[tauri::command]
pub async fn get_plugin_server_port(server: State<Arc<PluginFileServer>>) -> Result<Option<u16>, String>

/// 优雅停止插件文件服务器
#[tauri::command]
pub async fn stop_plugin_server(server: State<Arc<PluginFileServer>>) -> Result<bool, String>
```

**服务器特性**：

| 特性 | 实现 |
|------|------|
| 绑定地址 | `127.0.0.1:0`（仅回环，OS 分配端口） |
| URL 格式 | `http://127.0.0.1:{port}/plugins/{plugin-id}/{path}` |
| CORS | `Access-Control-Allow-Origin: *`（支持 OPTIONS 预检） |
| MIME 检测 | 自动检测 js/json/css/html/svg/png/jpg/woff2/wasm 等 20+ 类型 |
| 安全 | 复用 `validate_plugin_id()` + `validate_relative_path()` + canonicalize |
| 禁止目录列表 | 目录请求返回 403 |
| 缓存 | `Cache-Control: no-cache`（开发友好） |
| 生命周期 | 首次加载 v2 插件时自动启动，支持 `tokio::sync::watch` 优雅停机 |

### 8.3 远程安装命令 `src-tauri/src/commands/plugin_registry.rs`

支持从远程仓库发现、下载、安装、更新、卸载插件。

```rust
/// 从远程 URL 获取插件注册表索引
#[tauri::command]
pub async fn fetch_plugin_registry(url: String) -> Result<RegistryIndex, String>

/// 下载、验证并安装插件
/// - SHA-256 校验和验证
/// - zip-slip 防护（使用 enclosed_name()）
/// - 插件 ID 匹配验证
/// - 最大包大小限制 50MB
#[tauri::command]
pub async fn install_plugin(
    download_url: String,
    expected_id: String,
    checksum: Option<String>,
) -> Result<PluginManifest, String>

/// 卸载插件（删除插件目录）
#[tauri::command]
pub async fn uninstall_plugin(plugin_id: String) -> Result<(), String>

/// 检查已安装插件的可用更新
#[tauri::command]
pub async fn check_plugin_updates(
    registry_url: String,
    installed: Vec<InstalledPluginInfo>,
) -> Result<Vec<RegistryEntry>, String>
```

**注册表索引格式**：

```json
{
  "version": 1,
  "plugins": [
    {
      "id": "com.example.my-plugin",
      "name": "My Plugin",
      "version": "1.2.0",
      "description": "Plugin description",
      "author": "Author Name",
      "downloadUrl": "https://example.com/plugins/my-plugin-1.2.0.zip",
      "checksum": "sha256:abc123...",
      "size": 12345,
      "tags": ["utility", "terminal"],
      "homepage": "https://github.com/example/my-plugin",
      "updatedAt": "2026-02-08T12:00:00Z"
    }
  ]
}
```

**安全措施**：

| 措施 | 说明 |
|------|------|
| SHA-256 校验 | 下载后验证校验和，支持 `sha256:` 前缀格式 |
| zip-slip 防护 | 使用 `enclosed_name()` 拒绝包含 `..` 的路径 |
| ID 匹配验证 | 解压后验证 `plugin.json` 中的 ID 与预期一致 |
| 大小限制 | 最大包大小 50MB |
| 原子安装 | 先解压到临时目录，验证后原子重命名 |

---

## 9. 远程安装系统

### 9.1 架构概述

```
┌─────────────────────────────────────────────────────────────────┐
│                    Plugin Manager UI                             │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │  [已安装]  [浏览]                                            ││
│  │                                                              ││
│  │  ┌─────────────────────────────────────────────────────────┐││
│  │  │  搜索插件...                              [刷新]         │││
│  │  └─────────────────────────────────────────────────────────┘││
│  │                                                              ││
│  │  ┌─────────────────────────────────────────────────────────┐││
│  │  │  Plugin Name          v1.2.0    [已安装] / [安装] / [更新]│││
│  │  │  Plugin description...                                   │││
│  │  │  by Author  |  utility, terminal                         │││
│  │  └─────────────────────────────────────────────────────────┘││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      pluginStore (Zustand)                       │
│  registryEntries: RegistryEntry[]                                │
│  installProgress: Map<string, InstallProgress>                   │
│  availableUpdates: RegistryEntry[]                               │
└───────────────��─────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Frontend API (api.ts)                       │
│  pluginFetchRegistry(url) → RegistryIndex                        │
│  pluginInstall(downloadUrl, expectedId, checksum?) → Manifest    │
│  pluginUninstall(pluginId) → void                                │
│  pluginCheckUpdates(registryUrl, installed[]) → RegistryEntry[]  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                   Rust Backend (plugin_registry.rs)              │
│  fetch_plugin_registry  →  reqwest GET → parse JSON              │
│  install_plugin         →  download → verify → extract → install │
│  uninstall_plugin       →  validate → remove directory           │
│  check_plugin_updates   →  fetch registry → compare versions     │
└─────────────────────────────────────────────────────────────────┘
```

### 9.2 安装流程

```
用户点击 [安装]
    │
    ▼
setInstallProgress(id, 'downloading')
    │
    ▼
api.pluginInstall(downloadUrl, expectedId, checksum)
    │
    ├─ Rust: reqwest GET downloadUrl
    │
    ├─ 验证大小 ≤ 50MB
    │
    ├─ 验证 SHA-256 校验和（如提供）
    │
    ├─ 解压到临时目录 .{id}-installing/
    │
    ├─ 读取并验证 plugin.json
    │   └─ manifest.id === expectedId
    │
    ├─ 原子重命名: .{id}-installing/ → {id}/
    │
    └─ 返回 PluginManifest
    │
    ▼
setInstallProgress(id, 'installing')
    │
    ▼
pluginStore.registerPlugin(manifest)
    │
    ▼
loadPlugin(manifest)  // 激活插件
    │
    ▼
setInstallProgress(id, 'done')
    │
    ▼
clearInstallProgress(id)  // 2秒后清除
```

### 9.3 更新流程

```
用户点击 [更新]
    │
    ▼
unloadPlugin(pluginId)  // 先卸载旧版本
    │
    ▼
install_plugin(...)     // 安装新版本（覆盖旧目录）
    │
    ▼
loadPlugin(manifest)    // 激活新版本
    │
    ▼
从 availableUpdates 中移除该插件
```

### 9.4 Store 状态

```typescript
// pluginStore.ts 新增状态
interface PluginStore {
  // ... 现有状态 ...

  // 远程注册表
  registryEntries: RegistryEntry[];
  installProgress: Map<string, InstallProgress>;
  availableUpdates: RegistryEntry[];

  // 操作
  setRegistryEntries(entries: RegistryEntry[]): void;
  setInstallProgress(pluginId: string, state: InstallState, error?: string): void;
  clearInstallProgress(pluginId: string): void;
  setAvailableUpdates(updates: RegistryEntry[]): void;
  hasUpdate(pluginId: string): boolean;
}

type InstallState = 'downloading' | 'extracting' | 'installing' | 'done' | 'error';

type InstallProgress = {
  state: InstallState;
  error?: string;
};
```

### 9.5 类型定义

```typescript
// types/plugin.ts 新增类型

/** 远程注册表中的插件条目 */
export type RegistryEntry = {
  id: string;
  name: string;
  description?: string;
  author?: string;
  version: string;
  minOxidetermVersion?: string;
  downloadUrl: string;
  checksum?: string;
  size?: number;
  tags?: string[];
  homepage?: string;
  updatedAt?: string;
};

/** 注册表索引 */
export type RegistryIndex = {
  version: number;
  plugins: RegistryEntry[];
};

/** 安装状态 */
export type InstallState = 'downloading' | 'extracting' | 'installing' | 'done' | 'error';
```

### 9.6 配置

插件全局配置 (`plugin-config.json`) 扩展：

```json
{
  "plugins": {
    "com.example.my-plugin": { "enabled": true }
  },
  "registryUrl": "",
  "autoCheckUpdates": true,
  "lastUpdateCheck": "2026-02-08T12:00:00Z"
}
```

> **注意**：`registryUrl` 默认为空。当未配置时，浏览标签页将显示"即将推出"占位符。
> 如需使用自定义插件仓库，请将 `registryUrl` 设置为有效的 registry JSON 地址。

---

## 10. 前置重构（Phase 0）

在引入插件系统之前，需要两个独立的重构消除技术债：

### 10.1 TerminalView `handleWsMessage` 提取

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

### 10.2 Sidebar 按钮 data-driven 重构

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

## 11. 文件清单

### 11.1 需要创建的文件（19 个）

| 文件 | 用途 | 实际行数 |
|------|------|---------|
| `src/types/plugin.ts` | 全部插件 TypeScript 类型（v1 + v2 格式 + PluginAssetsAPI） | ~280 |
| `src/store/pluginStore.ts` | 插件状态 + UI 组件注册表 + 远程注册表 | ~350 |
| `src/lib/plugin/pluginLoader.ts` | 发现、校验、双策略加载、卸载生命周期 | ~417 |
| `src/lib/plugin/pluginContextFactory.ts` | 构建 Membrane 隔离的 PluginContext（10 个子 API） | ~431 |
| `src/lib/plugin/pluginEventBridge.ts` | appStore → 插件事件派发 | ~120 |
| `src/lib/plugin/pluginTerminalHooks.ts` | 输入/输出管道 + 快捷键查找 | ~100 |
| `src/lib/plugin/pluginSettingsManager.ts` | 插件设置读写与持久化 | ~80 |
| `src/lib/plugin/pluginI18nManager.ts` | 插件 i18n 命名空间注册 | ~60 |
| `src/lib/plugin/pluginStorage.ts` | 插件作用域 localStorage 封装 | ~40 |
| `src/lib/plugin/pluginUIKit.tsx` | 插件专用 React UI 组件库（24 个组件） | ~1072 |
| `src/lib/plugin/pluginIconResolver.ts` | Lucide 图标名 → React 组件动态解析 | ~35 |
| `src/lib/plugin/pluginUtils.ts` | 共享工具函数（toSnapshot 等） | ~30 |
| `src/components/plugin/PluginTabRenderer.tsx` | 插件 Tab 视图渲染器 | ~50 |
| `src/components/plugin/PluginSidebarRenderer.tsx` | 插件侧边栏面板渲染器 | ~50 |
| `src/components/plugin/PluginManagerView.tsx` | 插件管理 UI（已安装 + 浏览双标签页） | ~740 |
| `src/components/plugin/PluginConfirmDialog.tsx` | 主题化确认对话框（Radix Dialog）| ~60 |
| `src-tauri/src/commands/plugin.rs` | 后端：扫描目录、读文件、配置读写、路径安全 | ~290 |
| `src-tauri/src/commands/plugin_server.rs` | 后端：多文件包本地 HTTP 服务器 + 优雅停机 | ~330 |
| `src-tauri/src/commands/plugin_registry.rs` | 后端：远程安装、卸载、更新检查 | ~366 |

### 11.2 需要修改的文件（12 个）

| 文件 | 修改内容 |
|------|----------|
| `src/components/terminal/TerminalView.tsx` | 提取 `handleWsMessage`；注入输入/输出管道；注册 writer 回调 |
| `src/components/terminal/LocalTerminalView.tsx` | 注册 writer 回调（本地终端写入通道） |
| `src/components/layout/Sidebar.tsx` | 三区布局重构（topButtons/bottomButtons + 分隔线）；插件面板注入 |
| `src/components/layout/TabBar.tsx` | 插件 Tab 图标渲染（`PluginTabIcon` + `resolvePluginIcon`） |
| `src/types/index.ts` | `TabType` 添加 `'plugin'`，`Tab` 添加 `pluginTabId?` |
| `src/store/appStore.ts` | `createTab` 添加 plugin 分支 |
| `src/store/settingsStore.ts` | `SidebarSection` 扩展支持 plugin 格式 |
| `src/components/layout/AppLayout.tsx` | Tab 渲染添加 plugin 分支 |
| `src/hooks/useTerminalKeyboard.ts` | 添加插件快捷键查找 |
| `src/main.tsx` | 暴露 `window.__OXIDE__`（含 `ui: pluginUIKit`） |
| `src/App.tsx` | 启动时初始化插件系统；挂载 PluginConfirmDialog |
| `src/lib/terminalRegistry.ts` | 添加 TerminalWriter 类型 + writeToTerminal 导出 |

---

## 12. 实施顺序

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

Phase 8 — 远程安装系统 ✅
  ├─ 8.1 创建 src-tauri/src/commands/plugin_registry.rs
  ├─ 8.2 扩展 src/lib/api.ts（远程安装 API）
  ├─ 8.3 扩展 src/store/pluginStore.ts（注册表状态）
  ├─ 8.4 扩展 src/types/plugin.ts（RegistryEntry 等类型）
  └─ 8.5 更新 PluginManagerView.tsx（双标签页 UI）
```

---

## 13. 验证方式

1. `npx tsc --noEmit` — 0 类型错误
2. `npx vite build` — 前端构建成功
3. `cd src-tauri && cargo check` — Rust 编译通过
4. 创建示例插件 `com.oxideterm.hello-world` 验证完整生命周期
5. 测试连接事件钩子：连接/断开时插件收到正确事件
6. 测试终端输入拦截：插件修改输入后 WebSocket 发送修改后的数据
7. 测试插件崩溃隔离：故意抛异常的插件不影响其他功能
8. 测试插件卸载：所有 Disposable 被撤销，UI 注册被移除
9. 测试远程安装：从注册表下载、校验、安装插件
10. 测试更新检查：检测已安装插件的可用更新

---

## 14. SYSTEM_INVARIANTS 兼容性声明

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

*文档版本: v2.0 | 最后更新: 2026-02-08*
