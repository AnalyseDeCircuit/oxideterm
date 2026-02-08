# OxideTerm Plugin Development Guide

> **版本**: 适用于 OxideTerm v1.6.2+
> **最后更新**: 2026-02-08

---

## 目录

- [1. 插件系统概述](#1-插件系统概述)
  - [1.1 设计哲学](#11-设计哲学)
  - [1.2 架构模型](#12-架构模型)
  - [1.3 安全模型](#13-安全模型)
- [2. 快速入门](#2-快速入门)
  - [2.1 开发环境准备](#21-开发环境准备)
  - [2.2 创建第一个插件](#22-创建第一个插件)
  - [2.3 安装与调试](#23-安装与调试)
- [3. 插件结构](#3-插件结构)
  - [3.1 目录布局](#31-目录布局)
  - [3.2 plugin.json 清单文件](#32-pluginjson-清单文件)
  - [3.3 入口文件 (ESM)](#33-入口文件-esm)
- [4. Manifest 完全参考](#4-manifest-完全参考)
  - [4.1 顶层字段](#41-顶层字段)
  - [4.2 contributes.tabs](#42-contributestabs)
  - [4.3 contributes.sidebarPanels](#43-contributessidebarpanels)
  - [4.4 contributes.settings](#44-contributessettings)
  - [4.5 contributes.terminalHooks](#45-contributesterminalhooks)
  - [4.6 contributes.connectionHooks](#46-contributesconnectionhooks)
  - [4.7 contributes.apiCommands](#47-contributesapicommands)
  - [4.8 locales](#48-locales)
- [5. 插件生命周期](#5-插件生命周期)
  - [5.1 发现 (Discovery)](#51-发现-discovery)
  - [5.2 验证 (Validation)](#52-验证-validation)
  - [5.3 加载 (Loading)](#53-加载-loading)
  - [5.4 激活 (Activation)](#54-激活-activation)
  - [5.5 运行时 (Runtime)](#55-运行时-runtime)
  - [5.6 停用 (Deactivation)](#56-停用-deactivation)
  - [5.7 卸载 (Unloading)](#57-卸载-unloading)
  - [5.8 状态机](#58-状态机)
- [6. PluginContext API 完全参考](#6-plugincontext-api-完全参考)
  - [6.1 ctx.pluginId](#61-ctxpluginid)
  - [6.2 ctx.connections](#62-ctxconnections)
  - [6.3 ctx.events](#63-ctxevents)
  - [6.4 ctx.ui](#64-ctxui)
  - [6.5 ctx.terminal](#65-ctxterminal)
  - [6.6 ctx.settings](#66-ctxsettings)
  - [6.7 ctx.i18n](#67-ctxi18n)
  - [6.8 ctx.storage](#68-ctxstorage)
  - [6.9 ctx.api](#69-ctxapi)
  - [6.10 ctx.assets](#610-ctxassets)
- [7. 共享模块 (window.\_\_OXIDE\_\_)](#7-共享模块-window__oxide__)
  - [7.1 可用模块](#71-可用模块)
  - [7.2 使用 React](#72-使用-react)
  - [7.3 使用 Zustand](#73-使用-zustand)
  - [7.4 使用 Lucide React Icons](#74-使用-lucide-react-icons)
  - [7.5 使用 UI Kit（推荐）](#75-使用-ui-kit推荐)
- [8. UI 组件开发](#8-ui-组件开发)
  - [8.1 Tab View 组件](#81-tab-view-组件)
  - [8.2 Sidebar Panel 组件](#82-sidebar-panel-组件)
  - [8.3 UI Kit 组件详解](#83-ui-kit-组件详解)
  - [8.4 主题 CSS 变量参考（高级）](#84-主题-css-变量参考高级)
  - [8.5 组件间通信](#85-组件间通信)
- [9. Terminal Hooks 开发](#9-terminal-hooks-开发)
  - [9.1 Input Interceptor](#91-input-interceptor)
  - [9.2 Output Processor](#92-output-processor)
  - [9.3 快捷键 (Shortcuts)](#93-快捷键-shortcuts)
  - [9.4 性能预算与断路器](#94-性能预算与断路器)
- [10. 连接事件系统](#10-连接事件系统)
  - [10.1 连接生命周期事件](#101-连接生命周期事件)
  - [10.2 会话事件](#102-会话事件)
  - [10.3 插件间通信](#103-插件间通信)
  - [10.4 ConnectionSnapshot 结构](#104-connectionsnapshot-结构)
- [11. 国际化 (i18n)](#11-国际化-i18n)
  - [11.1 插件 i18n 概述](#111-插件-i18n-概述)
  - [11.2 目录结构](#112-目录结构)
  - [11.3 使用翻译](#113-使用翻译)
  - [11.4 支持的语言列表](#114-支持的语言列表)
- [12. 持久化存储](#12-持久化存储)
  - [12.1 KV 存储 (ctx.storage)](#121-kv-存储-ctxstorage)
  - [12.2 设置存储 (ctx.settings)](#122-设置存储-ctxsettings)
  - [12.3 存储隔离](#123-存储隔离)
- [13. 后端 API 调用](#13-后端-api-调用)
  - [13.1 白名单机制](#131-白名单机制)
  - [13.2 声明与使用](#132-声明与使用)
  - [13.3 安全限制](#133-安全限制)
- [14. 断路器与错误处理](#14-断路器与错误处理)
  - [14.1 断路器机制](#141-断路器机制)
  - [14.2 错误处理最佳实践](#142-错误处理最佳实践)
  - [14.3 自动禁用持久化](#143-自动禁用持久化)
- [15. Disposable 模式](#15-disposable-模式)
  - [15.1 概述](#151-概述)
  - [15.2 手动释放](#152-手动释放)
  - [15.3 自动清理](#153-自动清理)
- [16. 完整示例：Demo Plugin](#16-完整示例demo-plugin)
  - [16.1 目录结构](#161-目录结构)
  - [16.2 plugin.json](#162-pluginjson)
  - [16.3 main.js 解析](#163-mainjs-解析)
- [17. 最佳实践](#17-最佳实践)
- [18. 调试技巧](#18-调试技巧)
- [19. 常见问题 (FAQ)](#19-常见问题-faq)
- [20. 类型参考 (TypeScript)](#20-类型参考-typescript)

---

## 1. 插件系统概述

### 1.1 设计哲学

OxideTerm 插件系统遵循以下设计原则：

- **运行时动态加载**：插件以 ESM 包的形式在运行时通过 `Blob URL + dynamic import()` 加载，不需要重新编译宿主应用
- **膜式隔离 (Membrane Pattern)**：插件通过 `Object.freeze()` 冻结的 `PluginContext` 与宿主通信，所有 API 对象均为不可变的
- **声明式 Manifest**：插件的能力（tabs、sidebar、terminal hooks 等）必须在 `plugin.json` 中预先声明，运行时强制校验
- **失败安全 (Fail-Open)**：Terminal hooks 中的异常不会阻塞终端 I/O，而是回退到原始数据
- **自动清理**：基于 `Disposable` 模式的自动资源回收，插件卸载时所有注册自动清除

### 1.2 架构模型

```
┌──────────────────────────────────────────────────────────────────┐
│                       OxideTerm 宿主应用                         │
│                                                                  │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────────────────┐ │
│  │ Rust Backend │  │  Tauri IPC   │  │     React Frontend      │ │
│  │             │  │  Control      │  │                         │ │
│  │ plugin.rs   │←→│  Plane        │←→│  ┌───────────────────┐  │ │
│  │ - list      │  │              │  │  │   pluginStore      │  │ │
│  │ - read_file │  │              │  │  │   (Zustand)        │  │ │
│  │ - config    │  │              │  │  └───────┬───────────┘  │ │
│  └─────────────┘  └──────────────┘  │          │              │ │
│                                      │  ┌───────▼───────────┐  │ │
│                                      │  │  pluginLoader      │  │ │
│                                      │  │  - discover        │  │ │
│                                      │  │  - validate        │  │ │
│                                      │  │  - load / unload   │  │ │
│                                      │  └───────┬───────────┘  │ │
│                                      │          │              │ │
│                                      │  ┌───────▼───────────┐  │ │
│                                      │  │  Context Factory   │  │ │
│                                      │  │  (buildPluginCtx)  │  │ │
│                                      │  │  → Object.freeze   │  │ │
│                                      │  └───────┬───────────┘  │ │
│                                      │          │              │ │
│                                      └──────────┼──────────────┘ │
│                                                 │                │
│              ┌──────────────────────────────────▼────────────┐   │
│              │                Plugin (ESM)                    │   │
│              │                                                │   │
│              │  activate(ctx) ←── PluginContext (frozen)      │   │
│              │    ctx.connections  ctx.events  ctx.ui         │   │
│              │    ctx.terminal    ctx.settings  ctx.i18n      │   │
│              │    ctx.storage     ctx.api      ctx.assets     │   │
│              │                                                │   │
│              │  window.__OXIDE__                              │   │
│              │    React · ReactDOM · zustand · lucideReact    │   │
│              └────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────┘
```

**关键点**：

1. 插件与宿主运行在**同一个 JS 上下文**中（非 iframe/WebWorker）
2. 通过 `window.__OXIDE__` 共享 React 实例，确保 hooks 兼容
3. Rust 后端负责文件 I/O（带路径遍历保护），前端负责生命周期管理
4. Event Bridge 将 appStore 的连接状态变更桥接为插件事件

### 1.3 安全模型

| 层级 | 机制 | 说明 |
|------|------|------|
| **膜式隔离** | `Object.freeze()` | 所有 API 对象不可修改、不可扩展 |
| **Manifest 声明** | 运行时校验 | 未声明的 tab/panel/hook/command 注册时抛异常 |
| **路径保护** | Rust `validate_plugin_id()` + `validate_relative_path()` + canonicalize | 防止路径遍历攻击 |
| **API 白名单** | `contributes.apiCommands` | 限制插件可调用的 Tauri 命令（**Advisory**） |
| **断路器** | 10 次错误 / 60 秒 → 自动禁用 | 防止故障插件拖垮系统 |
| **时间预算** | Terminal hooks 5ms 预算 | 超时计入断路器 |

> **⚠️ 安全提示**：当前插件运行在同一 JS 上下文中，理论上可以直接 `import '@tauri-apps/api/core'` 绕过 API 白名单。白名单是**纵深防御**措施，防止意外误用；真正的沙箱隔离需要 iframe/WebWorker 架构（未来计划）。**请仅安装来源可信的插件**。

---

## 2. 快速入门

### 2.1 开发环境准备

- 开发 OxideTerm 插件不需要额外的构建工具
- 插件是纯 ESM JavaScript 文件，直接被 OxideTerm 动态导入
- 如需 TypeScript，可自行编译为 ESM（OxideTerm 不提供内置 TS 编译）
- 如需打包（多文件→单文件），可使用 esbuild / rollup（format: `esm`）

### 2.2 创建第一个插件

**步骤 1：创建插件目录**

```bash
mkdir -p ~/.oxideterm/plugins/my-first-plugin
cd ~/.oxideterm/plugins/my-first-plugin
```

> 插件目录名不需要与 `plugin.json` 中的 `id` 一致，但建议保持相同以便管理。

**步骤 2：编写 plugin.json**

```json
{
  "id": "my-first-plugin",
  "name": "My First Plugin",
  "version": "0.1.0",
  "description": "A minimal OxideTerm plugin",
  "author": "Your Name",
  "main": "./main.js",
  "engines": {
    "oxideterm": ">=1.6.0"
  },
  "contributes": {
    "tabs": [
      {
        "id": "hello",
        "title": "Hello World",
        "icon": "Smile"
      }
    ]
  }
}
```

**步骤 3：编写 main.js**

```javascript
// 从宿主获取 React（必须使用宿主的 React 实例！）
const { React } = window.__OXIDE__;
const { createElement: h, useState } = React;

// Tab 组件
function HelloTab({ tabId, pluginId }) {
  const [count, setCount] = useState(0);

  return h('div', { className: 'p-6' },
    h('h1', { className: 'text-xl font-bold text-foreground mb-4' },
      'Hello from Plugin! 🧩'
    ),
    h('p', { className: 'text-muted-foreground mb-4' },
      `Plugin: ${pluginId} | Tab: ${tabId}`
    ),
    h('button', {
      onClick: () => setCount(c => c + 1),
      className: 'px-4 py-2 rounded bg-primary text-primary-foreground hover:bg-primary/90',
    }, `Clicked ${count} times`),
  );
}

// 激活入口
export function activate(ctx) {
  console.log(`[MyPlugin] Activating (id: ${ctx.pluginId})`);
  ctx.ui.registerTabView('hello', HelloTab);
  ctx.ui.showToast({ title: 'My Plugin Activated!', variant: 'success' });
}

// 停用入口（可选）
export function deactivate() {
  console.log('[MyPlugin] Deactivating');
}
```

### 2.3 安装与调试

**方式一：手动安装（开发模式）**

1. 确保插件文件放在 `~/.oxideterm/plugins/my-first-plugin/` 下
2. 在 OxideTerm 中打开 **Plugin Manager**（侧边栏 🧩 图标 → Plugin Manager）
3. 点击 **Refresh** 按钮扫描新插件
4. 插件将自动加载并显示在列表中
5. 在侧边栏中可以看到插件的 Tab 图标，点击打开 Tab

**方式二：从注册表安装（推荐）**

1. 在 Plugin Manager 中切换到 **浏览** 标签页
2. 搜索或浏览可用插件
3. 点击 **安装** 按钮
4. 插件将自动下载、验证并安装
5. 安装完成后插件自动激活

**方式三：更新已安装插件**

1. 在 **浏览** 标签页中，已安装插件如有更新会显示 **更新** 按钮
2. 点击 **更新** 按钮
3. 旧版本将被卸载，新版本自动安装并激活

**卸载插件**

1. 在 **已安装** 标签页中找到要卸载的插件
2. 点击插件行右侧的 🗑️ 按钮
3. 插件将被停用并从磁盘删除

调试提示：

- 打开 DevTools（`Cmd+Shift+I` / `Ctrl+Shift+I`）查看 `console.log` 输出
- 插件加载失败会在 Plugin Manager 中显示红色错误状态
- 修改代码后，在 Plugin Manager 中点击插件的 **Reload** 按钮热重载

---

## 3. 插件结构

### 3.1 目录布局

**v1 单文件 Bundle（默认）**：

```
~/.oxideterm/plugins/
└── your-plugin-id/
    ├── plugin.json          # 必需：插件清单
    ├── main.js              # 必需：ESM 入口（由 manifest.main 指定）
    ├── locales/             # 可选：i18n 翻译文件
    │   ├── en.json
    │   ├── zh-CN.json
    │   ├── ja.json
    │   └── ...
    └── assets/              # 可选：其他资源文件
        └── ...
```

**v2 多文件 Package**（`format: "package"`）：

```
~/.oxideterm/plugins/
└── your-plugin-id/
    ├── plugin.json          # 必需：manifestVersion: 2, format: "package"
    ├── src/
    │   ├── main.js          # ESM 入口（支持模块间相对 import）
    │   ├── components/
    │   │   ├── Dashboard.js
    │   │   └── Charts.js
    │   └── utils/
    │       └── helpers.js
    ├── styles/
    │   ├── main.css         # 声明在 manifest.styles 中自动加载
    │   └── charts.css
    ├── assets/
    │   ├── logo.png         # 通过 ctx.assets.getAssetUrl() 访问
    │   └── config.json
    └── locales/
        ├── en.json
        └── zh-CN.json
```

v2 多文件包通过内置的本地 HTTP 文件服务器（`127.0.0.1`，OS 分配端口）加载，支持文件间的标准 ES Module `import` 语法。

**路径约束**：

- 所有文件路径相对于插件根目录
- **禁止** `..` 路径遍历
- **禁止** 绝对路径
- 插件 ID 中**禁止** `/`、`\`、`..` 和控制字符
- Rust 后端会对解析后的路径做 `canonicalize()` 检查，确保不逃逸出插件目录

### 3.2 plugin.json 清单文件

这是插件的核心描述文件。OxideTerm 通过扫描 `~/.oxideterm/plugins/*/plugin.json` 发现插件。

```json
{
  "id": "your-plugin-id",
  "name": "Human Readable Name",
  "version": "1.0.0",
  "description": "What this plugin does",
  "author": "Your Name",
  "main": "./main.js",
  "engines": {
    "oxideterm": ">=1.6.0"
  },
  "locales": "./locales",
  "contributes": {
    "tabs": [...],
    "sidebarPanels": [...],
    "settings": [...],
    "terminalHooks": {...},
    "connectionHooks": [...],
    "apiCommands": [...]
  }
}
```

### 3.3 入口文件 (ESM)

入口文件必须是有效的 **ES Module**，并 `export` 以下函数：

```javascript
/**
 * 必需。插件激活时被调用。
 * @param {PluginContext} ctx - 冻结的 API 上下文对象
 */
export function activate(ctx) {
  // 注册 UI、hooks、事件监听等
}

/**
 * 可选。插件卸载时被调用。
 * 用于清理全局状态（window 上挂载的东西等）。
 * 注意：Disposable 注册的内容会自动清理，无需在此手动清除。
 */
export function deactivate() {
  // 清理全局引用
}
```

两个函数均支持返回 `Promise`（异步激活/停用），但有 **5 秒超时限制**。

**加载机制（双策略）**：

**v1 单文件 Bundle（默认 / `format: "bundled"`）**：

```
Rust read_plugin_file(id, "main.js")
  → 字节数组传递到前端
    → new Blob([bytes], { type: 'application/javascript' })
      → URL.createObjectURL(blob)
        → import(blobUrl)
          → module.activate(frozenContext)
```

> 使用 Blob URL 加载时，插件内部**不能**使用相对路径 `import`。请使用打包工具（esbuild/rollup）合并为单文件 ESM bundle。

**v2 多文件 Package（`format: "package"`）**：

```
前端调用 api.pluginStartServer()
  → Rust 启动本地 HTTP Server (127.0.0.1:0)
    → 返回 OS 分配的端口号

import(`http://127.0.0.1:{port}/plugins/{id}/src/main.js`)
  → 浏览器标准 ES Module 加载
    → main.js 中的 import './components/Dashboard.js' 自动解析
      → module.activate(frozenContext)
```

> v2 包**支持**文件间的相对路径 `import`，浏览器会自动通过 HTTP Server 解析。服务器首次使用时自动启动，支持优雅停机。

**v2 多文件入口示例**：

```javascript
// src/main.js — import 同包的其他模块
import { Dashboard } from './components/Dashboard.js';
import { formatBytes } from './utils/helpers.js';

export async function activate(ctx) {
  // 动态加载额外 CSS
  const cssDisposable = await ctx.assets.loadCSS('./styles/extra.css');

  // 获取资源文件的 blob URL（用于 <img> src 等）
  const logoUrl = await ctx.assets.getAssetUrl('./assets/logo.png');

  ctx.ui.registerTabView('dashboard', (props) => {
    const { React } = window.__OXIDE__;
    return React.createElement(Dashboard, { ...props, logoUrl });
  });
}

export function deactivate() {
  // Disposable 会自动清理 CSS 和 blob URL
}
```

---

## 4. Manifest 完全参考

### 4.1 顶层字段

| 字段 | 类型 | 必需 | 说明 |
|------|------|------|------|
| `id` | `string` | ✅ | 插件唯一标识符。只能包含字母、数字、连字符、点号。不允许 `/`、`\`、`..`、控制字符。 |
| `name` | `string` | ✅ | 人类可读的插件名称 |
| `version` | `string` | ✅ | 语义化版本号 (如 `"1.0.0"`) |
| `description` | `string` | ⬜ | 插件描述 |
| `author` | `string` | ⬜ | 作者 |
| `main` | `string` | ✅ | ESM 入口文件的相对路径 (如 `"./main.js"` 或 `"./src/main.js"`) |
| `engines` | `object` | ⬜ | 版本兼容性要求 |
| `engines.oxideterm` | `string` | ⬜ | 所需最低 OxideTerm 版本 (如 `">=1.6.0"`)。支持 `>=x.y.z` 格式。 |
| `contributes` | `object` | ⬜ | 插件贡献的能力声明 |
| `locales` | `string` | ⬜ | i18n 翻译文件目录的相对路径 (如 `"./locales"`) |

**v2 Package 扩展字段**：

| 字段 | 类型 | 必需 | 说明 |
|------|------|------|------|
| `manifestVersion` | `1 \| 2` | ⬜ | 清单版本，默认 `1` |
| `format` | `'bundled' \| 'package'` | ⬜ | `bundled`（默认）= 单文件 Blob URL 加载；`package` = 本地 HTTP Server 加载（支持相对 import） |
| `assets` | `string` | ⬜ | 资源目录相对路径（如 `"./assets"`），配合 `ctx.assets` API 使用 |
| `styles` | `string[]` | ⬜ | CSS 文件列表（如 `["./styles/main.css"]`），加载时自动注入 `<style>` 到 `<head>` |
| `sharedDependencies` | `Record<string, string>` | ⬜ | 声明从宿主共享的依赖版本。当前支持：`react`、`react-dom`、`zustand`、`lucide-react` |
| `repository` | `string` | ⬜ | 源码仓库 URL |
| `checksum` | `string` | ⬜ | SHA-256 校验和（用于完整性验证） |

**v2 manifest 示例**：

```json
{
  "id": "com.example.multi-file-plugin",
  "name": "Multi-File Plugin",
  "version": "2.0.0",
  "main": "./src/main.js",
  "engines": { "oxideterm": ">=1.6.2" },
  "manifestVersion": 2,
  "format": "package",
  "styles": ["./styles/main.css"],
  "sharedDependencies": {
    "react": "^18.0.0",
    "lucide-react": "^0.300.0"
  },
  "contributes": {
    "tabs": [{ "id": "dashboard", "title": "Dashboard", "icon": "LayoutDashboard" }]
  },
  "locales": "./locales"
}
```

### 4.2 contributes.tabs

声明插件提供的 Tab 视图。

```json
"tabs": [
  {
    "id": "dashboard",
    "title": "Plugin Dashboard",
    "icon": "LayoutDashboard"
  }
]
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | `string` | Tab 标识符，在插件内唯一 |
| `title` | `string` | Tab 标题（显示在标签栏中） |
| `icon` | `string` | [Lucide React](https://lucide.dev/icons/) 图标名称 |

> 声明后需在 `activate()` 中调用 `ctx.ui.registerTabView(id, Component)` 注册组件。
>
> `icon` 字段直接用于标签栏（Tab Bar）的图标渲染。使用 PascalCase 的 Lucide 图标名，例如 `"LayoutDashboard"`、`"Server"`、`"Activity"`。如果名称无效或缺失，默认显示 `Puzzle` 图标。
>
> 完整图标列表见: https://lucide.dev/icons/

### 4.3 contributes.sidebarPanels

声明插件提供的侧边栏面板。

```json
"sidebarPanels": [
  {
    "id": "quick-info",
    "title": "Quick Info",
    "icon": "Info",
    "position": "bottom"
  }
]
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | `string` | Panel 标识符，在插件内唯一 |
| `title` | `string` | 面板标题 |
| `icon` | `string` | Lucide React 图标名称 |
| `position` | `"top" \| "bottom"` | 在侧边栏中的位置。默认 `"bottom"` |

> `icon` 字段直接用于侧边栏活动栏（Activity Bar）的图标渲染。使用 PascalCase 的 Lucide 图标名，例如 `"Info"`、`"Database"`、`"BarChart"`。如果名称无效或缺失，默认显示 `Puzzle` 图标。
>
> 当插件面板较多时，活动栏中部区域会自动支持滚动，底部的固定按钮（本地终端、文件管理、设置、插件管理）始终可见。

### 4.4 contributes.settings

声明插件的可配置项。用户可在 Plugin Manager 中查看和修改。

```json
"settings": [
  {
    "id": "greeting",
    "type": "string",
    "default": "Hello!",
    "title": "Greeting Message",
    "description": "The greeting shown in the dashboard"
  },
  {
    "id": "enableFeature",
    "type": "boolean",
    "default": false,
    "title": "Enable Feature",
    "description": "Toggle this feature on or off"
  },
  {
    "id": "theme",
    "type": "select",
    "default": "dark",
    "title": "Theme",
    "description": "Choose a color theme",
    "options": [
      { "label": "Dark", "value": "dark" },
      { "label": "Light", "value": "light" },
      { "label": "System", "value": "system" }
    ]
  },
  {
    "id": "maxItems",
    "type": "number",
    "default": 50,
    "title": "Max Items",
    "description": "Maximum number of items to display"
  }
]
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | `string` | 设置标识符 |
| `type` | `"string" \| "number" \| "boolean" \| "select"` | 值类型 |
| `default` | `any` | 默认值 |
| `title` | `string` | 显示标题 |
| `description` | `string?` | 描述说明 |
| `options` | `Array<{ label, value }>?` | 仅 `type: "select"` 时使用 |

### 4.5 contributes.terminalHooks

声明终端 I/O 拦截能力。

```json
"terminalHooks": {
  "inputInterceptor": true,
  "outputProcessor": true,
  "shortcuts": [
    { "key": "ctrl+shift+d", "command": "openDashboard" },
    { "key": "ctrl+shift+s", "command": "saveBuffer" }
  ]
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `inputInterceptor` | `boolean?` | 是否注册输入拦截器 |
| `outputProcessor` | `boolean?` | 是否注册输出处理器 |
| `shortcuts` | `Array<{ key, command }>?` | 终端内快捷键声明 |
| `shortcuts[].key` | `string` | 快捷键组合，如 `"ctrl+shift+d"` |
| `shortcuts[].command` | `string` | 命令名称（用于 `registerShortcut()` 匹配） |

**快捷键格式**：

- 修饰键：`ctrl`（macOS 上 Ctrl/Cmd 都算）、`shift`、`alt`
- 字母键：小写，如 `d`、`s`
- 用 `+` 连接：`ctrl+shift+d`
- 内部会对修饰键排序归一化

### 4.6 contributes.connectionHooks

声明插件关注的连接生命周期事件。

```json
"connectionHooks": ["onConnect", "onDisconnect", "onReconnect", "onLinkDown"]
```

可选值：`"onConnect"` | `"onDisconnect"` | `"onReconnect"` | `"onLinkDown"`

> 注意：这个字段当前仅作为文档声明，实际事件订阅通过 `ctx.events.onConnect()` 等方法完成。

### 4.7 contributes.apiCommands

声明插件需要调用的 Tauri 后端命令白名单。

```json
"apiCommands": ["list_sessions", "get_session_info"]
```

只有声明在此列表中的命令才能通过 `ctx.api.invoke()` 调用。未声明的命令会在调用时抛出异常并在 console 输出警告。

### 4.8 locales

指向 i18n 翻译文件目录的相对路径。

```json
"locales": "./locales"
```

详见 [11. 国际化 (i18n)](#11-国际化-i18n) 章节。

---

## 5. 插件生命周期

### 5.1 发现 (Discovery)

OxideTerm 启动时（或用户在 Plugin Manager 中点击 Refresh 时），Rust 后端扫描 `~/.oxideterm/plugins/` 目录：

```
list_plugins()
  → 遍历 plugins/ 下的每个子目录
    → 查找 plugin.json
      → serde 解析为 PluginManifest
        → 验证必需字段 (id, name, main 非空)
          → 返回 Vec<PluginManifest>
```

不包含 `plugin.json` 或解析失败的目录会被跳过（日志警告）。

### 5.2 验证 (Validation)

前端 `loadPlugin()` 收到 manifest 后进行二次验证：

1. **必需字段检查**：`id`、`name`、`version`、`main` 必须为非空 string
2. **版本兼容检查**：如果声明了 `engines.oxideterm`，与当前 OxideTerm 版本做简单 semver `>=` 比较
3. 验证失败 → 设置 `state: 'error'` 并记录错误信息

### 5.3 加载 (Loading)

```
loadPlugin(manifest)
  1. setPluginState('loading')
  2. api.pluginReadFile(id, mainPath)     // Rust 读取文件字节
  3. new Blob([bytes]) → blobUrl         // 创建 Blob URL
  4. import(blobUrl)                     // 动态 ESM 导入
  5. URL.revokeObjectURL(blobUrl)        // 回收 Blob URL
  6. 验证 module.activate 是 function
  7. setPluginModule(id, module)
  8. loadPluginLocales(id, ...)          // 加载 i18n（如声明）
  9. buildPluginContext(manifest)        // 构建冻结上下文
  10. module.activate(ctx)               // 调用 activate（5s 超时）
  11. setPluginState('active')
```

**失败处理**：加载过程中任何步骤失败会：
- 调用 `store.cleanupPlugin(id)` 清理部分状态
- 调用 `removePluginI18n(id)` 清理 i18n 资源
- 设置 `state: 'error'` 并记录错误消息

### 5.4 激活 (Activation)

`activate(ctx)` 是插件的主入口，应在此完成所有注册：

```javascript
export function activate(ctx) {
  // 1. 注册 UI 组件
  ctx.ui.registerTabView('myTab', MyTabComponent);
  ctx.ui.registerSidebarPanel('myPanel', MyPanelComponent);

  // 2. 注册终端 hooks
  ctx.terminal.registerInputInterceptor(myInterceptor);
  ctx.terminal.registerOutputProcessor(myProcessor);
  ctx.terminal.registerShortcut('myCommand', myHandler);

  // 3. 订阅事件
  ctx.events.onConnect(handleConnect);
  ctx.events.onDisconnect(handleDisconnect);

  // 4. 读取设置
  const value = ctx.settings.get('myKey');

  // 5. 读取存储
  const data = ctx.storage.get('myData');
}
```

**超时**：`activate()` 如返回 Promise，必须在 **5000ms** 内 resolve，否则将被视为加载失败。

### 5.5 运行时 (Runtime)

激活后，插件进入运行状态：

- 注册的 Tab/Sidebar 组件随 React 渲染
- Terminal hooks 在每次终端 I/O 时同步调用
- 事件处理器在连接状态变化时异步触发（`queueMicrotask()`）
- 设置/存储的读写即时生效

### 5.6 停用 (Deactivation)

用户在 Plugin Manager 中禁用或重载插件时触发：

```javascript
export function deactivate() {
  // 清理全局状态
  delete window.__MY_PLUGIN_STATE__;
}
```

**超时**：如返回 Promise，必须在 **5000ms** 内 resolve。

**注意**：通过 `Disposable` 注册的内容（事件监听、UI 组件、terminal hooks 等）无需在 `deactivate()` 中手动清理，系统会自动处理。

### 5.7 卸载 (Unloading)

```
unloadPlugin(pluginId)
  1. 调用 module.deactivate()      // 5s 超时
  2. cleanupPlugin(pluginId)       // 销毁所有 Disposable
  3. removePluginI18n(pluginId)    // 清除 i18n 资源
  4. 关闭该插件的所有 Tab
  5. 清除错误跟踪器
  6. setPluginState('inactive')
```

### 5.8 状态机

```
                  ┌──────────┐
                  │ inactive │ ←── 初始状态 / 卸载后
                  └────┬─────┘
                       │ loadPlugin()
                  ┌────▼─────┐
                  │ loading  │
                  └────┬─────┘
                 成功 / │ \ 失败
             ┌────▼──┐   ┌──▼───┐
             │ active │   │ error│
             └────┬───┘   └──┬───┘
                  │          │ 可重试
         unload / │          ▼
         disable  │    ┌──────────┐
                  │    │ disabled │ ←── 用户手动禁用 / 断路器自动禁用
                  │    └──────────┘
                  ▼
            ┌──────────┐
            │ inactive │
            └──────────┘
```

**PluginState** 枚举值：

| 状态 | 含义 |
|------|------|
| `'inactive'` | 未加载 / 已卸载 |
| `'loading'` | 正在加载中 |
| `'active'` | 已激活，正常运行 |
| `'error'` | 加载或运行时出错 |
| `'disabled'` | 被用户或断路器禁用 |

---

## 6. PluginContext API 完全参考

`PluginContext` 是传递给 `activate(ctx)` 的唯一参数。它是一个深度冻结的对象，包含 10 个命名空间（`pluginId` + 9 个子 API）。

```typescript
type PluginContext = Readonly<{
  pluginId: string;
  connections: PluginConnectionsAPI;
  events: PluginEventsAPI;
  ui: PluginUIAPI;
  terminal: PluginTerminalAPI;
  settings: PluginSettingsAPI;
  i18n: PluginI18nAPI;
  storage: PluginStorageAPI;
  api: PluginBackendAPI;
  assets: PluginAssetsAPI;
}>;
```

### 6.1 ctx.pluginId

```typescript
ctx.pluginId: string
```

当前插件的唯一标识符，与 `plugin.json` 中的 `id` 字段一致。

---

### 6.2 ctx.connections

只读连接状态查询 API。

#### `getAll()`

```typescript
connections.getAll(): ReadonlyArray<ConnectionSnapshot>
```

返回所有 SSH 连接的不可变快照数组。

```javascript
const conns = ctx.connections.getAll();
conns.forEach(c => {
  console.log(`${c.username}@${c.host}:${c.port} [${c.state}]`);
});
```

#### `get(connectionId)`

```typescript
connections.get(connectionId: string): ConnectionSnapshot | null
```

根据连接 ID 获取单个连接快照。不存在时返回 `null`。

#### `getState(connectionId)`

```typescript
connections.getState(connectionId: string): SshConnectionState | null
```

快速获取连接当前状态。不存在时返回 `null`。

可能的状态值：`'idle'` | `'connecting'` | `'active'` | `'disconnecting'` | `'disconnected'` | `'reconnecting'` | `'link_down'` | `{ error: string }`

---

### 6.3 ctx.events

事件订阅与发布 API。所有 `on*` 方法返回 `Disposable`。事件处理器通过 `queueMicrotask()` 异步调用，不会阻塞状态更新。

#### `onConnect(handler)`

```typescript
events.onConnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable
```

当连接变为 `'active'` 状态时触发（新建连接或从非活跃状态恢复）。

#### `onDisconnect(handler)`

```typescript
events.onDisconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable
```

当连接进入 `'disconnected'` 或 `'disconnecting'` 状态时触发，以及连接被移除时触发。

#### `onLinkDown(handler)`

```typescript
events.onLinkDown(handler: (snapshot: ConnectionSnapshot) => void): Disposable
```

当连接进入 `'reconnecting'`、`'link_down'` 或 `error` 状态时触发。

#### `onReconnect(handler)`

```typescript
events.onReconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable
```

当连接从 `'reconnecting'`/`'link_down'`/`error` 状态恢复到 `'active'` 时触发。

#### `onSessionCreated(handler)`

```typescript
events.onSessionCreated(handler: (info: { sessionId: string; connectionId: string }) => void): Disposable
```

当一个新的终端会话（terminal session）在某个连接上创建时触发。

#### `onSessionClosed(handler)`

```typescript
events.onSessionClosed(handler: (info: { sessionId: string }) => void): Disposable
```

当终端会话关闭时触发。

#### `on(name, handler)` — 自定义事件

```typescript
events.on(name: string, handler: (data: unknown) => void): Disposable
```

监听自定义（插件间）事件。事件名会自动加上命名空间前缀 `plugin:{pluginId}:{name}`。

**注意**：你只能监听自己插件命名空间下的事件。如需跨插件通信，接收方需监听发送方的命名空间（例如直接使用 pluginEventBridge）。

#### `emit(name, data)` — 发射自定义事件

```typescript
events.emit(name: string, data: unknown): void
```

发射自定义事件。事件名同样自动加命名空间前缀。

```javascript
// 发射
ctx.events.emit('data-ready', { rows: 100 });

// 同一插件内监听
ctx.events.on('data-ready', (data) => {
  console.log('Received:', data);
});
```

---

### 6.4 ctx.ui

UI 注册与交互 API。

#### `registerTabView(tabId, component)`

```typescript
ui.registerTabView(tabId: string, component: React.ComponentType<PluginTabProps>): Disposable
```

注册 Tab 视图组件。`tabId` 必须在 `contributes.tabs` 中预先声明。

**PluginTabProps**：

```typescript
type PluginTabProps = {
  tabId: string;     // Tab ID
  pluginId: string;  // 插件 ID
};
```

```javascript
function MyTab({ tabId, pluginId }) {
  return h('div', null, `Hello from ${pluginId}!`);
}
ctx.ui.registerTabView('myTab', MyTab);
```

> ⚠️ 未在 manifest 中声明的 tabId 会抛出 `Error: Tab "xxx" not declared in plugin manifest contributes.tabs`

#### `registerSidebarPanel(panelId, component)`

```typescript
ui.registerSidebarPanel(panelId: string, component: React.ComponentType): Disposable
```

注册侧边栏面板组件。`panelId` 必须在 `contributes.sidebarPanels` 中预先声明。

面板组件不接收 props（与 Tab 不同）。

```javascript
function MyPanel() {
  return h('div', { className: 'p-2' }, 'Sidebar content');
}
ctx.ui.registerSidebarPanel('myPanel', MyPanel);
```

#### `openTab(tabId)`

```typescript
ui.openTab(tabId: string): void
```

以编程方式打开一个 Tab。如果已打开则切换到该 Tab，否则创建新 Tab。

```javascript
ctx.ui.openTab('dashboard');
```

#### `showToast(opts)`

```typescript
ui.showToast(opts: {
  title: string;
  description?: string;
  variant?: 'default' | 'success' | 'error' | 'warning';
}): void
```

显示 Toast 通知。

```javascript
ctx.ui.showToast({
  title: 'File Saved',
  description: 'config.json has been updated',
  variant: 'success',
});
```

#### `showConfirm(opts)`

```typescript
ui.showConfirm(opts: {
  title: string;
  description: string;
}): Promise<boolean>
```

显示确认对话框，返回用户选择。当前实现使用 `window.confirm()`。

```javascript
const ok = await ctx.ui.showConfirm({
  title: 'Delete Item?',
  description: 'This action cannot be undone.',
});
if (ok) {
  // 执行删除
}
```

---

### 6.5 ctx.terminal

终端 hooks 和工具 API。

#### `registerInputInterceptor(handler)`

```typescript
terminal.registerInputInterceptor(handler: InputInterceptor): Disposable
```

注册输入拦截器。必须在 manifest 中声明 `contributes.terminalHooks.inputInterceptor: true`。

```typescript
type InputInterceptor = (
  data: string,                    // 用户输入的原始字符串
  context: { sessionId: string },  // 终端会话上下文
) => string | null;                // 返回修改后的字符串，或 null 抑制输入
```

拦截器在终端 I/O 热路径上**同步执行**，有 **5ms 时间预算**。

```javascript
ctx.terminal.registerInputInterceptor((data, { sessionId }) => {
  // 将所有输入转大写（仅示例！）
  return data.toUpperCase();
});
```

```javascript
// 返回 null 可以完全抑制输入
ctx.terminal.registerInputInterceptor((data, ctx) => {
  if (data.includes('dangerous-command')) {
    return null; // 阻止发送
  }
  return data;
});
```

#### `registerOutputProcessor(handler)`

```typescript
terminal.registerOutputProcessor(handler: OutputProcessor): Disposable
```

注册输出处理器。必须在 manifest 中声明 `contributes.terminalHooks.outputProcessor: true`。

```typescript
type OutputProcessor = (
  data: Uint8Array,                // 原始终端输出字节
  context: { sessionId: string },
) => Uint8Array;                   // 返回处理后的字节
```

同样在热路径上同步执行，有 5ms 时间预算。

```javascript
ctx.terminal.registerOutputProcessor((data, { sessionId }) => {
  // 简单的字节统计
  totalBytes += data.length;
  return data; // 透传不修改
});
```

#### `registerShortcut(command, handler)`

```typescript
terminal.registerShortcut(command: string, handler: () => void): Disposable
```

注册终端内快捷键。`command` 必须在 manifest `contributes.terminalHooks.shortcuts` 中有对应声明。

```javascript
// manifest: { "key": "ctrl+shift+d", "command": "openDashboard" }
ctx.terminal.registerShortcut('openDashboard', () => {
  ctx.ui.openTab('dashboard');
});
```

#### `writeToTerminal(sessionId, text)`

```typescript
terminal.writeToTerminal(sessionId: string, text: string): void
```

向指定会话的终端写入文本数据。通过 `terminalRegistry` 查找对应的 writer 回调，直接写入终端的数据通道（SSH WebSocket 或本地 PTY）。

```javascript
// 向终端发送命令
ctx.terminal.writeToTerminal(sessionId, 'ls -la\n');

// 发送特殊控制字符（如 Ctrl+C）
ctx.terminal.writeToTerminal(sessionId, '\x03');
```

> 如果找不到 sessionId 对应的终端或 writer 未注册，会输出 `console.warn` 但不会抛异常。

#### `getBuffer(sessionId)`

```typescript
terminal.getBuffer(sessionId: string): string | null
```

返回指定会话的终端缓冲区文本内容。

```javascript
const buffer = ctx.terminal.getBuffer(sessionId);
if (buffer) {
  const lastLine = buffer.split('\n').pop();
  console.log('Last line:', lastLine);
}
```

#### `getSelection(sessionId)`

```typescript
terminal.getSelection(sessionId: string): string | null
```

返回用户在指定会话终端中选中的文本。

---

### 6.6 ctx.settings

插件作用域的设置 API，持久化到 `localStorage`。

#### `get<T>(key)`

```typescript
settings.get<T>(key: string): T
```

获取设置值。如果没有用户设置过的值，返回 manifest 中声明的 `default`。

```javascript
const greeting = ctx.settings.get('greeting'); // "Hello!"
const max = ctx.settings.get('maxItems');      // 50
```

#### `set<T>(key, value)`

```typescript
settings.set<T>(key: string, value: T): void
```

设置值。会触发通过 `onChange()` 注册的监听器。

#### `onChange(key, handler)`

```typescript
settings.onChange(key: string, handler: (newValue: unknown) => void): Disposable
```

监听设置变更。

```javascript
ctx.settings.onChange('greeting', (newVal) => {
  console.log('Greeting changed to:', newVal);
});
```

**存储键格式**：`oxide-plugin-{pluginId}-setting-{settingId}`

---

### 6.7 ctx.i18n

插件作用域的国际化 API。

#### `t(key, params?)`

```typescript
i18n.t(key: string, params?: Record<string, string | number>): string
```

翻译指定 key。key 会自动加上 `plugin.{pluginId}.` 前缀。

```javascript
const msg = ctx.i18n.t('greeting');
const hello = ctx.i18n.t('hello_user', { name: 'Alice' });
```

对应翻译文件 `locales/en.json`：

```json
{
  "greeting": "Welcome!",
  "hello_user": "Hello, {{name}}!"
}
```

#### `getLanguage()`

```typescript
i18n.getLanguage(): string
```

获取当前语言代码。如 `"en"`、`"zh-CN"`。

#### `onLanguageChange(handler)`

```typescript
i18n.onLanguageChange(handler: (lang: string) => void): Disposable
```

监听语言切换。

---

### 6.8 ctx.storage

插件作用域的持久化 KV 存储，基于 `localStorage`。

#### `get<T>(key)`

```typescript
storage.get<T>(key: string): T | null
```

获取值。不存在或解析失败返回 `null`。值自动 JSON 反序列化。

#### `set<T>(key, value)`

```typescript
storage.set<T>(key: string, value: T): void
```

存储值。自动 JSON 序列化。

#### `remove(key)`

```typescript
storage.remove(key: string): void
```

删除指定 key。

```javascript
// 使用示例：计录启动次数
const count = (ctx.storage.get('launchCount') || 0) + 1;
ctx.storage.set('launchCount', count);
```

**存储键格式**：`oxide-plugin-{pluginId}-{key}`

---

### 6.9 ctx.api

受限的 Tauri 后端命令调用 API。

#### `invoke<T>(command, args?)`

```typescript
api.invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>
```

调用 Tauri 后端命令。命令必须在 `contributes.apiCommands` 中预先声明。

```javascript
// manifest: "apiCommands": ["list_sessions"]
const sessions = await ctx.api.invoke('list_sessions');
```

**未声明的命令**：
- 调用时 console 输出警告
- 抛出 `Error: Command "xxx" not whitelisted in manifest contributes.apiCommands`

---

### 6.10 ctx.assets

插件资源文件访问 API。用于加载 CSS 样式、获取图片/字体/数据文件的 URL。

#### `loadCSS(relativePath)`

```typescript
assets.loadCSS(relativePath: string): Promise<Disposable>
```

读取插件目录中的 CSS 文件，注入 `<style data-plugin="{pluginId}">` 标签到 `<head>`。返回的 `Disposable` 调用 `dispose()` 后会移除该 `<style>` 标签。

```javascript
// 动态加载额外样式
const cssDisposable = await ctx.assets.loadCSS('./styles/extra.css');

// 不再需要时手动移除（也可在卸载时自动清理）
cssDisposable.dispose();
```

> 注意：`manifest.styles` 中声明的 CSS 文件会在插件加载时**自动注入**，无需手动调用 `loadCSS()`。`loadCSS()` 适用于按需加载的额外样式。

#### `getAssetUrl(relativePath)`

```typescript
assets.getAssetUrl(relativePath: string): Promise<string>
```

读取插件目录中的任意文件，返回 blob URL（可用于 `<img src>`、`new Image()` 等）。

```javascript
const logoUrl = await ctx.assets.getAssetUrl('./assets/logo.png');

// 在 React 组件中使用
return h('img', { src: logoUrl, alt: 'Logo' });
```

**MIME 类型自动检测**：

| 扩展名 | MIME |
|--------|------|
| `png` | `image/png` |
| `jpg`/`jpeg` | `image/jpeg` |
| `gif` | `image/gif` |
| `svg` | `image/svg+xml` |
| `webp` | `image/webp` |
| `woff`/`woff2` | `font/woff` / `font/woff2` |
| `ttf`/`otf` | `font/ttf` / `font/otf` |
| `json` | `application/json` |
| `css` | `text/css` |
| `js` | `application/javascript` |
| 其他 | `application/octet-stream` |

#### `revokeAssetUrl(url)`

```typescript
assets.revokeAssetUrl(url: string): void
```

手动释放通过 `getAssetUrl()` 创建的 blob URL，释放内存。

```javascript
const url = await ctx.assets.getAssetUrl('./assets/large-image.png');
// 使用完毕后
ctx.assets.revokeAssetUrl(url);
```

> 卸载插件时，所有未手动释放的 blob URL 和注入的 `<style>` 标签会**自动清理**。

**完整示例**：

```javascript
export async function activate(ctx) {
  // 1. 自动加载 manifest.styles 中的 CSS（无需代码）
  // 2. 按需加载额外 CSS
  const highlightCSS = await ctx.assets.loadCSS('./styles/highlight.css');

  // 3. 获取图片 URL
  const iconUrl = await ctx.assets.getAssetUrl('./assets/icon.svg');

  // 4. 获取 JSON 配置
  const configUrl = await ctx.assets.getAssetUrl('./assets/defaults.json');
  const configResp = await fetch(configUrl);
  const defaults = await configResp.json();
  ctx.assets.revokeAssetUrl(configUrl); // JSON 已读取，释放 blob URL

  ctx.ui.registerTabView('my-tab', (props) => {
    const { React } = window.__OXIDE__;
    return React.createElement('div', null,
      React.createElement('img', { src: iconUrl, width: 32 }),
      React.createElement('pre', null, JSON.stringify(defaults, null, 2)),
    );
  });
}
```

---

## 7. 共享模块 (window.\_\_OXIDE\_\_)

### 7.1 可用模块

插件**必须**使用宿主提供的共享模块，而不是自己打包 React 等库。这确保了 React hooks 的兼容性和避免多实例问题。

```typescript
window.__OXIDE__ = {
  React: typeof import('react');
  ReactDOM: { createRoot: typeof import('react-dom/client').createRoot };
  zustand: { create: typeof import('zustand').create };
  lucideReact: typeof import('lucide-react');
  ui: PluginUIKit;   // 插件 UI 组件库
};
```

### 7.2 使用 React

```javascript
const { React } = window.__OXIDE__;
const { createElement: h, useState, useEffect, useCallback, useRef, useMemo } = React;

// 使用 createElement 代替 JSX
function MyComponent({ name }) {
  const [count, setCount] = useState(0);

  return h('div', null,
    h('h1', null, `Hello ${name}!`),
    h('button', { onClick: () => setCount(c => c + 1) }, `Count: ${count}`),
  );
}
```

> 📝 由于插件是纯 JS（非 JSX），需使用 `React.createElement`（通常缩写为 `h`）代替 JSX 语法。如果使用打包工具，可配置 JSX transform。

**所有 React Hooks 均可使用**，包括但不限于：
- `useState` / `useReducer` — 状态管理
- `useEffect` / `useLayoutEffect` — 副作用
- `useCallback` / `useMemo` — 性能优化
- `useRef` — 引用
- `useContext` — 上下文（需自行创建 Context）

### 7.3 使用 Zustand

插件可以使用宿主的 Zustand 创建自己的状态 store：

```javascript
const { zustand } = window.__OXIDE__;

const useMyStore = zustand.create((set) => ({
  items: [],
  addItem: (item) => set((s) => ({ items: [...s.items, item] })),
  clearItems: () => set({ items: [] }),
}));

// 在组件中使用
function ItemList() {
  const { items, clearItems } = useMyStore();
  return h('div', null,
    h('ul', null, items.map((item, i) => h('li', { key: i }, item))),
    h('button', { onClick: clearItems }, 'Clear'),
  );
}
```

### 7.4 使用 Lucide React Icons

```javascript
const { lucideReact } = window.__OXIDE__;
const { Activity, Terminal, Wifi, Settings, ArrowRight } = lucideReact;

function MyIcon() {
  return h(Activity, { className: 'h-4 w-4 text-primary' });
}
```

完整图标列表见: https://lucide.dev/icons/

> **Manifest 图标解析**：`plugin.json` 中 `contributes.tabs[].icon` 和 `contributes.sidebarPanels[].icon` 字段使用图标名称字符串（如 `"LayoutDashboard"`），系统会通过 `resolvePluginIcon()` 自动将其解析为对应的 Lucide React 组件，用于标签栏和侧边栏活动栏的图标渲染。插件组件内部仍然通过解构 `lucideReact` 直接使用图标组件。

### 7.5 使用 UI Kit（推荐）

OxideTerm 提供了一套轻量级 UI 组件库 `window.__OXIDE__.ui`，封装了 OxideTerm 的主题系统。**强烈建议使用 UI Kit 代替手写 Tailwind CSS 类名**，这样可以：

- 🎨 自动适配所有主题（暗色/亮色/自定义）
- 🔒 避免类名拼写错误
- 📝 大幅减少样板代码
- 🔄 主题系统升级时无需修改插件

```javascript
const { React, lucideReact, ui } = window.__OXIDE__;
const { createElement: h, useState } = React;
const { Activity, Settings, Terminal } = lucideReact;
```

**组件一览**：

| 组件 | 用途 | 示例 |
|------|------|------|
| `ui.ScrollView` | 全高滚动容器（Tab 根容器） | `h(ui.ScrollView, null, children)` |
| `ui.Stack` | 弹性布局（水平/垂直） | `h(ui.Stack, { direction: 'horizontal', gap: 2 }, ...)` |
| `ui.Grid` | 网格布局 | `h(ui.Grid, { cols: 3, gap: 4 }, ...)` |
| `ui.Card` | 带标题/图标的卡片 | `h(ui.Card, { icon: Activity, title: '统计' }, ...)` |
| `ui.Stat` | 数值统计卡 | `h(ui.Stat, { icon: Hash, label: '输入', value: 42 })` |
| `ui.Button` | 按钮 | `h(ui.Button, { variant: 'primary', onClick }, '点击')` |
| `ui.Input` | 文本输入框 | `h(ui.Input, { value, onChange, placeholder: '...' })` |
| `ui.Checkbox` | 复选框 | `h(ui.Checkbox, { checked, onChange, label: '启用' })` |
| `ui.Select` | 下拉选择 | `h(ui.Select, { value, options, onChange })` |
| `ui.Toggle` | 开关控件 | `h(ui.Toggle, { checked, onChange, label: '自动刷新' })` |
| `ui.Text` | 语义化文本 | `h(ui.Text, { variant: 'heading' }, '标题')` |
| `ui.Badge` | 状态徽章 | `h(ui.Badge, { variant: 'success' }, '在线')` |
| `ui.Separator` | 分隔线 | `h(ui.Separator)` |
| `ui.IconText` | 图标+文本行 | `h(ui.IconText, { icon: Terminal }, '终端')` |
| `ui.KV` | 键值对显示行 | `h(ui.KV, { label: '主机' }, '192.168.1.1')` |
| `ui.EmptyState` | 空状态占位 | `h(ui.EmptyState, { icon: Inbox, title: '暂无数据' })` |
| `ui.ListItem` | 可点击列表项 | `h(ui.ListItem, { icon: Server, title: 'prod-01', onClick })` |
| `ui.Progress` | 进度条 | `h(ui.Progress, { value: 75, variant: 'success' })` |
| `ui.Alert` | 提示/警告框 | `h(ui.Alert, { variant: 'warning', title: '注意' }, '...')` |
| `ui.Spinner` | 加载指示器 | `h(ui.Spinner, { label: '加载中...' })` |
| `ui.Table` | 数据表格 | `h(ui.Table, { columns, data, onRowClick })` |
| `ui.CodeBlock` | 代码/终端输出 | `h(ui.CodeBlock, null, 'ssh root@...')` |
| `ui.Tabs` | 选项卡切换 | `h(ui.Tabs, { tabs, activeTab, onTabChange }, content)` |
| `ui.Header` | 页面级标题栏 | `h(ui.Header, { icon: Layout, title: '仪表板' })` |

**快速示例 — Tab 组件**：

```javascript
function MyTab({ tabId, pluginId }) {
  const [count, setCount] = useState(0);

  return h(ui.ScrollView, null,
    h(ui.Header, {
      icon: Activity,
      title: 'My Plugin',
      subtitle: `v1.0.0`,
    }),
    h(ui.Grid, { cols: 3, gap: 3 },
      h(ui.Stat, { icon: Terminal, label: '会话', value: 5 }),
      h(ui.Stat, { icon: Activity, label: '流量', value: '12 KB' }),
      h(ui.Stat, { icon: Clock, label: '运行时间', value: '2h' }),
    ),
    h(ui.Card, { icon: Settings, title: '控制面板' },
      h(ui.Stack, { gap: 2 },
        h(ui.Text, { variant: 'muted' }, '点击按钮增加计数'),
        h(ui.Stack, { direction: 'horizontal', gap: 2 },
          h(ui.Button, { variant: 'primary', onClick: () => setCount(c => c + 1) }, `Count: ${count}`),
          h(ui.Button, { variant: 'ghost', onClick: () => setCount(0) }, 'Reset'),
        ),
      ),
    ),
  );
}
```

**快速示例 — Sidebar 面板**：

```javascript
function MySidebar() {
  return h(ui.Stack, { gap: 2, className: 'p-2' },
    h(ui.Text, { variant: 'label' }, 'My Plugin'),
    h(ui.KV, { label: '状态', mono: true }, 'active'),
    h(ui.KV, { label: '连接数', mono: true }, '3'),
    h(ui.Button, {
      variant: 'outline',
      size: 'sm',
      className: 'w-full',
      onClick: () => ctx.ui.openTab('myTab'),
    }, '打开详情'),
  );
}
```

> 📝 所有 UI Kit 组件都接受 `className` prop，可以追加自定义 Tailwind 类名进行微调。

---

## 8. UI 组件开发

### 8.1 Tab View 组件

Tab 组件接收 `PluginTabProps`：

```javascript
// 推荐：使用 UI Kit
function MyTabView({ tabId, pluginId }) {
  return h(ui.ScrollView, null,
    h(ui.Header, { icon: LayoutDashboard, title: 'My Plugin Tab' }),
    h(ui.Card, { title: '内容区' },
      h(ui.Text, { variant: 'body' }, '这是一个插件 Tab。'),
    ),
  );
}
```

**纯 createElement 写法**（不推荐，但也可以使用）：

```javascript
function MyTabView({ tabId, pluginId }) {
  return h('div', { className: 'h-full overflow-auto p-6' },
    h('div', { className: 'max-w-4xl mx-auto' },
      h('h1', { className: 'text-xl font-bold text-theme-text' }, 'My Plugin Tab'),
    ),
  );
}
```

**注册（在 activate 中）**：

```javascript
ctx.ui.registerTabView('myTab', MyTabView);
```

**打开 Tab**：

```javascript
ctx.ui.openTab('myTab');
```

**建议的 Tab 组件结构**：

```javascript
// 推荐：使用 UI Kit 组件
function MyTab({ tabId, pluginId }) {
  return h(ui.ScrollView, null,                                 // 全高 + 滚动 + 居中
    h(ui.Header, {                                              // 标题栏
      icon: SomeIcon,
      title: 'Title',
      subtitle: 'Description',
    }),
    h(ui.Grid, { cols: 3, gap: 3 },                            // 统计行
      h(ui.Stat, { icon: Icon1, label: 'Metric', value: 42 }),
    ),
    h(ui.Card, { icon: SomeIcon, title: 'Section' },           // 内容卡片
      h(ui.Stack, { gap: 2 }, /* children */),
    ),
  );
}
```

### 8.2 Sidebar Panel 组件

Sidebar 面板组件是无 props 的函数组件：

```javascript
// 推荐：使用 UI Kit
function MyPanel() {
  return h(ui.Stack, { gap: 2, className: 'p-2' },
    h(ui.Text, { variant: 'label', className: 'px-1' }, 'My Panel'),
    h(ui.KV, { label: '状态', mono: true }, 'active'),
    h(ui.KV, { label: '连接数', mono: true }, '3'),
    h(ui.Button, {
      variant: 'outline', size: 'sm', className: 'w-full mt-1',
      onClick: () => ctx.ui.openTab('myTab'),
    }, 'Open in Tab'),
  );
}
```

**纯 createElement 写法**：

```javascript
function MyPanel() {
  return h('div', { className: 'p-2 space-y-2' },
    h('div', { className: 'text-xs font-semibold text-theme-text-muted uppercase tracking-wider px-1 mb-1' },
      'My Panel'
    ),
  );
}
```

Sidebar 面板空间有限，建议：
- 使用小字体 (`text-xs`)
- 保持布局紧凑 (`p-2`, `space-y-1`)
- 提供 "Open in Tab" 按钮链接到详细视图

### 8.3 UI Kit 组件详解

以下是所有 `window.__OXIDE__.ui` 组件的完整 API 参考。

#### 布局组件

**ScrollView** — Tab 的标准根容器

| Prop | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `maxWidth` | `string` | `'4xl'` | 最大宽度 Tailwind 类后缀 |
| `padding` | `string` | `'6'` | 内边距 Tailwind 类后缀 |
| `className` | `string` | — | 追加自定义类名 |

```javascript
h(ui.ScrollView, null, /* 所有 Tab 内容 */);
h(ui.ScrollView, { maxWidth: '6xl', padding: '4' }, children);
```

**Stack** — 弹性布局

| Prop | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `direction` | `'vertical' \| 'horizontal'` | `'vertical'` | 方向 |
| `gap` | `number` | `2` | 间距（Tailwind gap 值） |
| `align` | `'start' \| 'center' \| 'end' \| 'stretch' \| 'baseline'` | — | 交叉轴对齐 |
| `justify` | `'start' \| 'center' \| 'end' \| 'between' \| 'around'` | — | 主轴对齐 |
| `wrap` | `boolean` | `false` | 是否换行 |

```javascript
h(ui.Stack, { direction: 'horizontal', gap: 2, align: 'center' },
  h(ui.Button, null, 'A'),
  h(ui.Button, null, 'B'),
);
```

**Grid** — 网格布局

| Prop | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `cols` | `number` | `2` | 列数 |
| `gap` | `number` | `4` | 间距 |

```javascript
h(ui.Grid, { cols: 3, gap: 3 },
  h(ui.Stat, { label: 'A', value: 1 }),
  h(ui.Stat, { label: 'B', value: 2 }),
  h(ui.Stat, { label: 'C', value: 3 }),
);
```

#### 容器组件

**Card** — 主题化卡片

| Prop | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `title` | `string` | — | 卡片标题 |
| `icon` | `React.ComponentType` | — | 标题前图标（Lucide 组件） |
| `headerRight` | `React.ReactNode` | — | 标题右侧自定义内容 |

```javascript
h(ui.Card, {
  icon: Settings,
  title: '设置',
  headerRight: h(ui.Badge, { variant: 'info' }, 'v2'),
},
  h(ui.Text, { variant: 'muted' }, '卡片内容'),
);
```

**Stat** — 数值统计卡

| Prop | 类型 | 说明 |
|------|------|------|
| `label` | `string` | 描述文本 |
| `value` | `string \| number` | 显示的数值 |
| `icon` | `React.ComponentType` | 可选图标 |

```javascript
h(ui.Stat, { icon: Activity, label: '流量', value: '12.5 KB' })
```

#### 表单组件

**Button** — 按钮

| Prop | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `variant` | `'primary' \| 'secondary' \| 'destructive' \| 'ghost' \| 'outline'` | `'secondary'` | 样式变体 |
| `size` | `'sm' \| 'md' \| 'lg' \| 'icon'` | `'md'` | 尺寸 |
| `disabled` | `boolean` | `false` | 禁用状态 |
| `onClick` | `function` | — | 点击回调 |

```javascript
h(ui.Button, { variant: 'primary', onClick: handler }, '保存');
h(ui.Button, { variant: 'destructive', size: 'sm' }, '删除');
h(ui.Button, { variant: 'ghost', size: 'icon' }, h(Trash2, { className: 'h-4 w-4' }));
```

**Input** — 文本输入

| Prop | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `value` / `defaultValue` | `string` | — | 受控/非受控值 |
| `placeholder` | `string` | — | 占位文本 |
| `type` | `string` | `'text'` | HTML input type |
| `size` | `'sm' \| 'md'` | `'md'` | 尺寸 |
| `onChange` | `function` | — | 变更回调 |
| `onKeyDown` | `function` | — | 键盘事件回调 |

```javascript
h(ui.Input, {
  value: text,
  onChange: (e) => setText(e.target.value),
  placeholder: '输入搜索关键词...',
  size: 'sm',
});
```

**Checkbox** — 复选框

| Prop | 类型 | 说明 |
|------|------|------|
| `checked` | `boolean` | 选中状态 |
| `onChange` | `(checked: boolean) => void` | 变更回调（直接返回 boolean） |
| `label` | `string` | 可选标签 |
| `disabled` | `boolean` | 禁用状态 |

```javascript
h(ui.Checkbox, { checked: enabled, onChange: setEnabled, label: '启用特性' })
```

**Select** — 下拉选择

| Prop | 类型 | 说明 |
|------|------|------|
| `value` | `string \| number` | 当前值 |
| `options` | `{ label: string, value: string \| number }[]` | 选项列表 |
| `onChange` | `(value: string) => void` | 变更回调 |
| `placeholder` | `string` | 占位提示 |
| `size` | `'sm' \| 'md'` | 尺寸 |

```javascript
h(ui.Select, {
  value: theme,
  options: [
    { label: '暗色', value: 'dark' },
    { label: '亮色', value: 'light' },
  ],
  onChange: setTheme,
});
```

#### 排版与展示组件

**Text** — 语义化文本

| variant | 样式 | 典型用途 |
|---------|------|----------|
| `'heading'` | 大号粗体 | 页面标题 |
| `'subheading'` | 小号粗体 | 区域标题 |
| `'body'` | 正常文本 | 段落内容 |
| `'muted'` | 灰色小字 | 描述/提示 |
| `'mono'` | 等宽字体 | IP 地址/代码 |
| `'label'` | 大写灰色 | 区域标签 |
| `'tiny'` | 超小灰字 | 次要信息 |

可通过 `as` prop 改变渲染标签：`h(ui.Text, { variant: 'heading', as: 'h2' }, '...')`

**Badge** — 状态徽章

| variant | 颜色 | 用途 |
|---------|------|------|
| `'default'` | 灰色 | 中性状态 |
| `'success'` | 绿色 | 成功/在线 |
| `'warning'` | 黄色 | 警告 |
| `'error'` | 红色 | 错误/离线 |
| `'info'` | 蓝色 | 信息/版本 |

```javascript
h(ui.Badge, { variant: 'success' }, 'Active')
```

**KV** — 键值对行

```javascript
h(ui.KV, { label: '主机', mono: true }, '192.168.1.1')
```

设置 `mono: true` 使值以等宽字体显示。

**IconText** — 图标 + 文本

```javascript
h(ui.IconText, { icon: Terminal }, '活跃会话')
```

**Separator** — 分隔线

```javascript
h(ui.Separator)
```

**EmptyState** — 空状态占位

```javascript
h(ui.EmptyState, {
  icon: Inbox,
  title: '暂无数据',
  description: '添加一个新项目以开始。',
  action: h(ui.Button, { variant: 'primary' }, '添加'),
})
```

**ListItem** — 列表项

```javascript
h(ui.ListItem, {
  icon: Server,
  title: 'production-01',
  subtitle: 'root@10.0.1.1',
  right: h(ui.Badge, { variant: 'success' }, 'Active'),
  active: isSelected,
  onClick: () => select(item),
})
```

**Header** — 页面标题栏

```javascript
h(ui.Header, {
  icon: LayoutDashboard,
  title: 'Dashboard',
  subtitle: 'v1.0.0',
  action: h(ui.Button, { size: 'sm' }, 'Refresh'),
})
```

**Tabs** — 选项卡切换

```javascript
const [tab, setTab] = useState('overview');
h(ui.Tabs, {
  tabs: [
    { id: 'overview', label: '概览', icon: Activity },
    { id: 'logs', label: '日志', icon: FileText },
  ],
  activeTab: tab,
  onTabChange: setTab,
},
  tab === 'overview' ? h(OverviewPanel) : h(LogsPanel),
)
```

| Prop | 类型 | 说明 |
|------|------|------|
| `tabs` | `{ id: string, label: string, icon?: Component }[]` | Tab 定义数组 |
| `activeTab` | `string` | 当前激活的 tab id |
| `onTabChange` | `(id: string) => void` | Tab 切换回调 |

**Table** — 数据表格

```javascript
h(ui.Table, {
  columns: [
    { key: 'host', header: '主机' },
    { key: 'port', header: '端口', align: 'right', width: '80px' },
    { key: 'status', header: '状态', render: (v) => h(ui.Badge, { variant: v === 'active' ? 'success' : 'error' }, v) },
  ],
  data: connections,
  striped: true,
  onRowClick: (row) => select(row.id),
})
```

| Prop | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `columns` | `{ key, header, width?, align?, render? }[]` | — | 列定义 |
| `data` | `Record<string, unknown>[]` | — | 数据行 |
| `compact` | `boolean` | `false` | 紧凑行高 |
| `striped` | `boolean` | `false` | 斑马条纹 |
| `emptyText` | `string` | `'No data'` | 空数据提示 |
| `onRowClick` | `(row, index) => void` | — | 行点击回调 |

**Progress** — 进度条

```javascript
h(ui.Progress, { value: 75, max: 100, variant: 'success', showLabel: true })
```

| variant | 颜色 |
|---------|------|
| `'default'` | 主题强调色 |
| `'success'` | 绿色 |
| `'warning'` | 黄色 |
| `'error'` | 红色 |

**Toggle** — 开关控件

```javascript
h(ui.Toggle, { checked: autoRefresh, onChange: setAutoRefresh, label: '自动刷新' })
```

与 Checkbox 的区别：Toggle 是滑动开关样式，更适合"开/关"场景。

**Alert** — 提示/警告框

```javascript
h(ui.Alert, { variant: 'warning', icon: AlertTriangle, title: '注意' },
  '此操作无法撤销。',
)
```

| variant | 颜色 | 用途 |
|---------|------|------|
| `'info'` | 蓝色 | 提示信息 |
| `'success'` | 绿色 | 成功提示 |
| `'warning'` | 黄色 | 警告提示 |
| `'error'` | 红色 | 错误提示 |

**Spinner** — 加载指示器

```javascript
h(ui.Spinner, { size: 'sm', label: '加载中...' })
```

size 可选值：`'sm'`（16px）、`'md'`（24px）、`'lg'`（32px）

**CodeBlock** — 代码/终端输出

```javascript
h(ui.CodeBlock, { maxHeight: '200px', wrap: true },
  'ssh root@192.168.1.1\nPassword: ****\nWelcome to Ubuntu 22.04',
)
```

| Prop | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `maxHeight` | `string` | `'300px'` | 最大高度（溢出滚动） |
| `wrap` | `boolean` | `false` | 是否自动换行 |

### 8.4 主题 CSS 变量参考（高级）

如果需要超出 UI Kit 范围的自定义样式，可以直接使用 OxideTerm 的语义化 CSS 类：

**文本颜色**：

| 类名 | 用途 |
|------|------|
| `text-theme-text` | 主要文本 |
| `text-theme-text-muted` | 次要/灰色文本 |
| `text-theme-accent` | 强调色文本 |

**背景颜色**：

| 类名 | 用途 |
|------|------|
| `bg-theme-bg` | 页面背景 |
| `bg-theme-bg-panel` | 卡片/面板背景 |
| `bg-theme-bg-hover` | 悬停高亮背景 |
| `bg-theme-accent` | 强调色背景 |

**边框**：

| 类名 | 用途 |
|------|------|
| `border-theme-border` | 标准边框 |

> ⚠️ **不要使用硬编码颜色**（如 `text-white`、`bg-gray-800`）。始终使用语义化类名以兼容所有主题。

### 8.5 组件间通信

由于 Tab 和 Sidebar 组件分别渲染，它们之间不能直接通过 React props 通信。推荐方案：

**方案 1：Zustand Store（推荐）**

```javascript
const { zustand } = window.__OXIDE__;

// 在模块顶层创建共享 store
const useMyStore = zustand.create((set) => ({
  data: [],
  setData: (data) => set({ data }),
}));

// Tab 组件
function MyTab() {
  const { data } = useMyStore();
  return h('div', null, `Items: ${data.length}`);
}

// Sidebar 组件
function MyPanel() {
  const { data } = useMyStore();
  return h('div', null, `Count: ${data.length}`);
}
```

**方案 2：全局变量 + ctx 引用**

```javascript
// activate 中
window.__MY_PLUGIN_CTX__ = ctx;

// 组件中
function MyTab() {
  const ctx = window.__MY_PLUGIN_CTX__;
  const conns = ctx?.connections.getAll() ?? [];
  // ...
}

// deactivate 中清理
export function deactivate() {
  delete window.__MY_PLUGIN_CTX__;
}
```

---

## 9. Terminal Hooks 开发

### 9.1 Input Interceptor

输入拦截器在用户每次向终端发送数据时同步调用。位于终端 I/O 的热路径上。

**调用链**：

```
用户输入 → term.onData(data)
  → runInputPipeline(data, sessionId)
    → 遍历所有 interceptors
      → interceptor(data, { sessionId })
        → 返回修改后的 data 或 null
  → 如果结果非 null → WebSocket 发送到后端
```

**使用场景**：

- 输入过滤/审计
- 自动补全前缀
- 命令拦截/防误操作
- 输入统计

```javascript
// 示例：根据设置添加输入前缀
ctx.terminal.registerInputInterceptor((data, { sessionId }) => {
  const prefix = ctx.settings.get('inputPrefix');
  if (prefix) return prefix + data;
  return data;
});
```

**重要注意事项**：

1. 拦截器是**同步的**，不支持 async
2. 返回 `null` 会完全抑制输入（数据不会发送到服务器）
3. 多个插件的拦截器按注册顺序串联执行，前一个的输出是后一个的输入
4. 异常被静默捕获，数据透传（fail-open）
5. 有 **5ms 时间预算**，详见 [9.4](#94-性能预算与断路器)

### 9.2 Output Processor

输出处理器在每次从远程服务器接收到终端数据时同步调用。

**调用链**：

```
WebSocket 接收 MSG_TYPE_DATA
  → runOutputPipeline(data, sessionId)
    → 遍历所有 processors
      → processor(data, { sessionId })
        → 返回处理后的 Uint8Array
  → 写入 xterm.js 渲染
```

**使用场景**：

- 输出统计/审计
- 敏感信息遮蔽
- 输出日志记录

```javascript
ctx.terminal.registerOutputProcessor((data, { sessionId }) => {
  // 统计字节数
  totalBytes += data.length;

  // 透传原始数据
  return data;
});
```

**注意**：

1. 输入参数是 `Uint8Array`（原始字节），不是字符串
2. 返回类型也必须是 `Uint8Array`
3. 同 Input Interceptor，有 5ms 时间预算
4. 异常 fail-open：处理器出错时使用上一步的数据

### 9.3 快捷键 (Shortcuts)

注册终端聚焦时的键盘快捷键。

**注册**：

```javascript
// manifest:
// "shortcuts": [{ "key": "ctrl+shift+d", "command": "openDashboard" }]

ctx.terminal.registerShortcut('openDashboard', () => {
  ctx.ui.openTab('dashboard');
});
```

**快捷键匹配流程**：

```
终端 keydown 事件
  → matchPluginShortcut(event)
    → 构建归一化 key: parts.sort().join('+')
      例: Ctrl+Shift+D → "ctrl+d+shift"
    → 在 shortcuts Map 中查找
    → 找到 → 调用 handler 并阻止默认行为
```

**修饰键映射**：

- `event.ctrlKey || event.metaKey` → `"ctrl"` （macOS 上 Cmd 也算 Ctrl）
- `event.shiftKey` → `"shift"`
- `event.altKey` → `"alt"`

### 9.4 性能预算与断路器

Terminal hooks 运行在终端 I/O 热路径上，每次按键或数据接收都会同步调用。因此有严格的性能限制：

**时间预算**：每个 hook 调用 ≤ **5ms** (`HOOK_BUDGET_MS`)

- 超时会输出 console.warn
- 超时计入断路器错误计数

**断路器**：**10 次错误 / 60 秒** → 自动禁用插件

- 计数器会在 60 秒窗口过期后重置
- 触发断路器后，插件被立即卸载
- 禁用状态持久化到 `plugin-config.json`（跨重启生效）

**最佳实践**：

```javascript
// ✅ 好的做法：轻量同步操作
ctx.terminal.registerInputInterceptor((data) => {
  counter++;
  return data;
});

// ❌ 坏的做法：重操作
ctx.terminal.registerInputInterceptor((data) => {
  // 不要在这里做正则匹配大文本、DOM 操作等
  const result = someExpensiveRegex.test(data);
  return data;
});

// ✅ 好的做法：将重操作推迟到微任务
ctx.terminal.registerOutputProcessor((data) => {
  queueMicrotask(() => {
    // 重操作放这里
    processDataAsync(data);
  });
  return data; // 立即返回原始数据
});
```

---

## 10. 连接事件系统

### 10.1 连接生命周期事件

OxideTerm 的 Event Bridge 将 `appStore` 中的连接状态变更桥接为插件可订阅的事件。

**事件触发条件**：

| 事件 | 触发条件 |
|------|----------|
| `connection:connect` | 新连接出现且状态为 `active`；或非活跃状态（非 reconnecting/link_down/error）→ `active` |
| `connection:reconnect` | 从 `reconnecting`/`link_down`/`error` → `active` |
| `connection:link_down` | 进入 `reconnecting`/`link_down`/`error` 状态 |
| `connection:idle` | 从 `active` → `idle`（SSH 连接存活但无终端） |
| `connection:disconnect` | 进入 `disconnected`/`disconnecting`；或连接从列表中被移除 |

**使用示例**：

```javascript
const disposable1 = ctx.events.onConnect((snapshot) => {
  console.log(`Connected: ${snapshot.username}@${snapshot.host}`);
  console.log(`State: ${snapshot.state}, Terminals: ${snapshot.terminalIds.length}`);
});

const disposable2 = ctx.events.onDisconnect((snapshot) => {
  console.log(`Disconnected: ${snapshot.id}`);
});

const disposable3 = ctx.events.onLinkDown((snapshot) => {
  ctx.ui.showToast({
    title: 'Connection Lost',
    description: `${snapshot.host} link down`,
    variant: 'warning',
  });
});

const disposable4 = ctx.events.onReconnect((snapshot) => {
  ctx.ui.showToast({
    title: 'Reconnected',
    description: `${snapshot.host} is back`,
    variant: 'success',
  });
});
```

### 10.2 会话事件

```javascript
ctx.events.onSessionCreated(({ sessionId, connectionId }) => {
  console.log(`New terminal session ${sessionId} on connection ${connectionId}`);
});

ctx.events.onSessionClosed(({ sessionId }) => {
  console.log(`Session ${sessionId} closed`);
});
```

会话事件通过 diff `terminalIds` 数组检测。

### 10.3 插件间通信

```javascript
// 插件 A：发射事件
ctx.events.emit('data-ready', { items: [...] });

// 插件 A：监听自己的事件
ctx.events.on('data-ready', (data) => {
  console.log('Received:', data.items.length);
});
```

**命名空间规则**：

- `ctx.events.emit('foo', data)` 实际发射 `plugin:{pluginId}:foo`
- `ctx.events.on('foo', handler)` 实际监听 `plugin:{pluginId}:foo`
- 同一插件内的 emit/on 自动匹配

> 🔬 **跨插件通信**：当前 API 设计中，每个插件的 `on`/`emit` 都自动加上了自己的命名空间前缀。因此默认情况下只能监听自己的事件，跨插件通信需要通过其他机制（如共享 store 或约定好的事件名直接使用底层 bridge）。

### 10.4 ConnectionSnapshot 结构

所有连接事件的 handler 都收到一个**不可变的** `ConnectionSnapshot` 对象：

```typescript
type ConnectionSnapshot = Readonly<{
  id: string;                         // 连接唯一 ID
  host: string;                       // SSH 主机地址
  port: number;                       // SSH 端口
  username: string;                   // SSH 用户名
  state: SshConnectionState;          // 当前连接状态
  refCount: number;                   // 引用计数
  keepAlive: boolean;                 // 是否保持活跃
  createdAt: string;                  // 创建时间
  lastActive: string;                 // 最后活跃时间
  terminalIds: readonly string[];     // 关联的终端会话 ID 列表
  parentConnectionId?: string;        // 父连接 ID（跳板机场景）
}>;
```

**SshConnectionState** 可能的值：

```typescript
type SshConnectionState =
  | 'idle'
  | 'connecting'
  | 'active'
  | 'disconnecting'
  | 'disconnected'
  | 'reconnecting'
  | 'link_down'
  | { error: string };   // 注意：error 状态是一个对象
```

---

## 11. 国际化 (i18n)

### 11.1 插件 i18n 概述

OxideTerm 使用 **i18next** 作为 i18n 框架。插件的翻译资源通过 `loadPluginI18n()` 加载到主 i18next 实例中，命名空间为 `plugin.{pluginId}.*`。

### 11.2 目录结构

```
your-plugin/
├── plugin.json           ← "locales": "./locales"
└── locales/
    ├── en.json           ← 英语（建议必须提供）
    ├── zh-CN.json        ← 简体中文
    ├── zh-TW.json        ← 繁体中文
    ├── ja.json           ← 日语
    ├── ko.json           ← 韩语
    ├── de.json           ← 德语
    ├── es-ES.json        ← 西班牙语
    ├── fr-FR.json        ← 法语
    ├── it.json           ← 意大利语
    ├── pt-BR.json        ← 葡萄牙语（巴西）
    └── vi.json           ← 越南语
```

**翻译文件格式**（扁平 KV）：

```json
{
  "dashboard_title": "Plugin Dashboard",
  "greeting": "Hello, {{name}}!",
  "item_count": "{{count}} items",
  "settings_saved": "Settings saved successfully"
}
```

### 11.3 使用翻译

```javascript
// 在 activate() 中或组件中
const title = ctx.i18n.t('dashboard_title');         // "Plugin Dashboard"
const greeting = ctx.i18n.t('greeting', { name: 'Alice' }); // "Hello, Alice!"

// 监听语言变化
ctx.i18n.onLanguageChange((lang) => {
  console.log('Language changed to:', lang);
  // 触发 UI 更新
});
```

### 11.4 支持的语言列表

OxideTerm 尝试按以下顺序加载语言文件（文件不存在则跳过）：

| 语言代码 | 语言 |
|----------|------|
| `en` | English |
| `zh-CN` | 简体中文 |
| `zh-TW` | 繁體中文 |
| `ja` | 日本語 |
| `ko` | 한국어 |
| `de` | Deutsch |
| `es-ES` | Español |
| `fr-FR` | Français |
| `it` | Italiano |
| `pt-BR` | Português (Brasil) |
| `vi` | Tiếng Việt |

---

## 12. 持久化存储

### 12.1 KV 存储 (ctx.storage)

基于 `localStorage` 的简单 KV 存储，自动 JSON 序列化/反序列化。

```javascript
// 存
ctx.storage.set('myData', { items: [1, 2, 3], updated: Date.now() });

// 取
const data = ctx.storage.get('myData');
// { items: [1, 2, 3], updated: 1719000000000 }

// 删
ctx.storage.remove('myData');
```

**存储键格式**：`oxide-plugin-{pluginId}-{key}`

**限制**：
- 受 `localStorage` 容量限制（通常 5-10 MB per origin）
- 失败时静默处理（不抛异常）
- 所有值序列化为 JSON（不支持 `undefined`、`function`、`Symbol` 等）

### 12.2 设置存储 (ctx.settings)

与 `ctx.storage` 类似但有额外特性：

- 在 manifest 中声明的设置有 `default` 值
- 支持 `onChange` 监听
- 存储键格式：`oxide-plugin-{pluginId}-setting-{settingId}`

### 12.3 存储隔离

每个插件的存储完全隔离：

```
localStorage key 格式:
  oxide-plugin-{pluginId}-{key}              ← storage
  oxide-plugin-{pluginId}-setting-{settingId} ← settings
```

插件卸载时，存储**不会自动清除**（数据保留以便重新安装）。如需完全清除，可调用内部 `clearPluginStorage(pluginId)`（目前不通过 ctx 暴露）。

---

## 13. 后端 API 调用

### 13.1 白名单机制

插件只能调用在 `contributes.apiCommands` 中声明的 Tauri 命令。

```json
// plugin.json
{
  "contributes": {
    "apiCommands": ["list_sessions", "get_session_info"]
  }
}
```

### 13.2 声明与使用

```javascript
// 在 activate() 中
try {
  const sessions = await ctx.api.invoke('list_sessions');
  console.log('Active sessions:', sessions);
} catch (err) {
  console.error('Failed to list sessions:', err);
}
```

### 13.3 安全限制

> **⚠️ Advisory Whitelist（建议性白名单）**

当前的白名单是**建议性的**，不是硬隔离。原因：

1. 插件运行在与宿主相同的 JS 上下文中
2. 插件理论上可以直接 `import { invoke } from '@tauri-apps/api/core'` 绕过检查
3. 白名单通过代码审查发现意外/恶意的命令调用

**白名单实施机制**：
- 调用未声明命令时：
  - `console.warn()` 输出警告
  - 抛出 `Error: Command "xxx" not whitelisted...`
- 不会尝试实际调用该命令

---

## 14. 断路器与错误处理

### 14.1 断路器机制

OxideTerm 的插件系统内置断路器（Circuit Breaker），防止故障插件拖垮整个应用：

| 参数 | 值 | 说明 |
|------|-----|------|
| `MAX_ERRORS` | 10 | 触发阈值 |
| `ERROR_WINDOW_MS` | 60,000 ms (1 分钟) | 滑动窗口 |
| `HOOK_BUDGET_MS` | 5 ms | Terminal hook 时间预算 |

**计入断路器的错误**：

1. Terminal hook（inputInterceptor / outputProcessor）抛出异常
2. Terminal hook 执行时间超过 5ms
3. 其他运行时错误（通过 `trackPluginError()` 追踪）

**触发流程**：

```
插件错误
  → trackPluginError(pluginId)
    → 在 60s 窗口内累计错误次数
      → 达到 10 次
        → persistAutoDisable(pluginId)
          → plugin-config.json: { enabled: false }
          → store.setPluginState('disabled')
        → unloadPlugin(pluginId)
```

### 14.2 错误处理最佳实践

```javascript
// ✅ 在 Terminal hooks 中做好防御
ctx.terminal.registerInputInterceptor((data, { sessionId }) => {
  try {
    // 你的逻辑
    return processInput(data);
  } catch (err) {
    console.warn('[MyPlugin] Input interceptor error:', err);
    return data; // 出错时透传原始数据
  }
});

// ✅ 事件处理器中包裹 try-catch
ctx.events.onConnect((snapshot) => {
  try {
    handleConnection(snapshot);
  } catch (err) {
    console.error('[MyPlugin] onConnect error:', err);
  }
});

// ✅ API 调用使用 try-catch
try {
  const result = await ctx.api.invoke('some_command');
} catch (err) {
  ctx.ui.showToast({
    title: 'API Error',
    description: String(err),
    variant: 'error',
  });
}
```

### 14.3 自动禁用持久化

当断路器触发时：

1. 读取 `plugin-config.json`
2. 设置 `plugins[pluginId].enabled = false`
3. 写回 `plugin-config.json`
4. 设置 store 状态为 `'disabled'`

这意味着**重启 OxideTerm 后插件仍然是禁用状态**。用户需要在 Plugin Manager 中手动重新启用。

---

## 15. Disposable 模式

### 15.1 概述

所有 `register*` 和 `on*` 方法都返回一个 `Disposable` 对象：

```typescript
type Disposable = {
  dispose(): void;  // 调用一次后变为 no-op
};
```

### 15.2 手动释放

如果需要在运行时动态取消注册（例如根据设置切换 hook）：

```javascript
let interceptorDisposable = null;

function enableInterceptor() {
  interceptorDisposable = ctx.terminal.registerInputInterceptor(myHandler);
}

function disableInterceptor() {
  interceptorDisposable?.dispose();
  interceptorDisposable = null;
}

// 根据设置动态启用/禁用
ctx.settings.onChange('enableFilter', (enabled) => {
  if (enabled) enableInterceptor();
  else disableInterceptor();
});
```

### 15.3 自动清理

**你不需要在 `deactivate()` 中手动清理**通过 `ctx` 注册的内容。系统在卸载时会：

1. 遍历该插件的所有 tracked Disposable
2. 逐个调用 `dispose()`
3. 清除 tabViews、sidebarPanels、inputInterceptors、outputProcessors、shortcuts
4. 清除 disposables 跟踪列表

`deactivate()` 适合清理不在 Disposable 管理范围内的内容，例如 `window` 上的全局引用。

---

## 16. 完整示例：Demo Plugin

OxideTerm 内置了一个完整的 Demo Plugin 作为参考实现。

### 16.1 目录结构

```
~/.oxideterm/plugins/oxide-demo-plugin/
├── plugin.json
└── main.js
```

### 16.2 plugin.json

```json
{
  "id": "oxide-demo-plugin",
  "name": "OxideTerm Demo Plugin",
  "version": "1.0.0",
  "description": "A comprehensive demo plugin that exercises all plugin system APIs",
  "author": "OxideTerm Team",
  "main": "./main.js",
  "engines": {
    "oxideterm": ">=1.6.0"
  },
  "contributes": {
    "tabs": [
      { "id": "dashboard", "title": "Plugin Dashboard", "icon": "LayoutDashboard" }
    ],
    "sidebarPanels": [
      { "id": "quick-info", "title": "Quick Info", "icon": "Info", "position": "bottom" }
    ],
    "settings": [
      {
        "id": "greeting", "type": "string", "default": "Hello from Plugin!",
        "title": "Greeting Message", "description": "The greeting shown in the dashboard"
      },
      {
        "id": "inputPrefix", "type": "string", "default": "",
        "title": "Input Prefix", "description": "If set, prefix all terminal input"
      },
      {
        "id": "logOutput", "type": "boolean", "default": false,
        "title": "Log Output", "description": "Log terminal output byte counts to console"
      }
    ],
    "terminalHooks": {
      "inputInterceptor": true,
      "outputProcessor": true,
      "shortcuts": [
        { "key": "ctrl+shift+d", "command": "openDashboard" }
      ]
    },
    "connectionHooks": ["onConnect", "onDisconnect"]
  }
}
```

### 16.3 main.js 解析

Demo Plugin 的 `main.js` 展示了所有 API 的使用方式：

**1. 获取共享模块（含 UI Kit）**

```javascript
const { React, ReactDOM, zustand, lucideReact, ui } = window.__OXIDE__;
const { createElement: h, useState, useEffect, useCallback, useRef } = React;
const { Activity, Wifi, Terminal, Settings /* ... */ } = lucideReact;
```

**2. 创建共享状态 Store**

```javascript
const useDemoStore = zustand.create((set) => ({
  eventLog: [],
  inputCount: 0,
  outputBytes: 0,
  connectionCount: 0,
  addEvent: (msg) => set((s) => ({
    eventLog: [...s.eventLog.slice(-49), { time: new Date().toLocaleTimeString(), msg }],
  })),
  incInput: () => set((s) => ({ inputCount: s.inputCount + 1 })),
  addOutputBytes: (n) => set((s) => ({ outputBytes: s.outputBytes + n })),
  setConnectionCount: (n) => set({ connectionCount: n }),
}));
```

**3. Tab 组件** — 使用 `ui.*` 组件构建界面，通过 `ctx` 引用（window 全局）读取 connections、settings、storage

**4. activate() 中的完整注册**

```javascript
export function activate(ctx) {
  window.__DEMO_PLUGIN_CTX__ = ctx;   // 暴露给组件

  // UI 注册
  ctx.ui.registerTabView('dashboard', DashboardTab);
  ctx.ui.registerSidebarPanel('quick-info', QuickInfoPanel);

  // Terminal Hooks
  ctx.terminal.registerInputInterceptor((data, { sessionId }) => { /* ... */ });
  ctx.terminal.registerOutputProcessor((data, { sessionId }) => { /* ... */ });
  ctx.terminal.registerShortcut('openDashboard', () => ctx.ui.openTab('dashboard'));

  // Events
  ctx.events.onConnect((snapshot) => { /* ... */ });
  ctx.events.onDisconnect((data) => { /* ... */ });
  ctx.events.on('demo-ping', (data) => { /* ... */ });

  // Settings Watch
  ctx.settings.onChange('greeting', (newVal) => { /* ... */ });

  // Storage
  const count = (ctx.storage.get('launchCount') || 0) + 1;
  ctx.storage.set('launchCount', count);

  // Toast
  ctx.ui.showToast({ title: 'Demo Plugin Activated', variant: 'success' });
}
```

**5. deactivate() 清理**

```javascript
export function deactivate() {
  delete window.__DEMO_PLUGIN_CTX__;
}
```

---

## 17. 最佳实践

### 开发规范

1. **始终使用 `window.__OXIDE__` 的共享模块**
   - ❌ 不要在插件中打包自己的 React
   - ✅ 使用 `const { React } = window.__OXIDE__`

2. **遵守 Manifest 声明**
   - 所有 tab、panel、hook、shortcut、api command 必须先在 `plugin.json` 中声明
   - 运行时注册未声明的内容会抛异常

3. **保持 activate() 轻量**
   - 不要在 activate 中做重计算或长时间网络请求
   - 5 秒超时限制

4. **Terminal Hooks 要极其高效**
   - 每次按键都会触发，必须在 5ms 内完成
   - 重操作推迟到 `queueMicrotask()` 或 `setTimeout()`
   - 做好 try-catch 防御

5. **使用语义化 CSS 类**
   - 使用 Tailwind 的语义化类名：`text-foreground`、`bg-card`、`border-border`
   - 不要硬编码颜色值

6. **清理全局状态**
   - 在 `deactivate()` 中 `delete window.__MY_GLOBAL__`
   - Disposable 管理的注册无需手动清理

### 性能建议

1. **Event Log 限制大小**：保留最近 N 条，避免内存泄漏
   ```javascript
   eventLog: [...s.eventLog.slice(-49), newEntry]  // 最多 50 条
   ```

2. **避免在 output processor 中做字符串解码**
   ```javascript
   // ❌
   const text = new TextDecoder().decode(data);
   const processed = text.replace(/pattern/, 'replacement');
   return new TextEncoder().encode(processed);

   // ✅
   totalBytes += data.length;
   return data;
   ```

3. **延迟初始化**：组件中使用 `useEffect` 延迟加载数据

### 安全建议

1. **只声明需要的 apiCommands**
2. **不要在 window 上暴露敏感信息**
3. **不要直接导入 `@tauri-apps/api/core`**（虽然技术上可行）
4. **不要存储密码/密钥到 ctx.storage**（localStorage 不加密）

---

## 18. 调试技巧

### DevTools Console

插件的所有 `console.log/warn/error` 都会出现在 DevTools 中。系统内部日志使用 `[PluginLoader]`、`[PluginEventBridge]`、`[PluginTerminalHooks]` 前缀。

**有用的调试命令**：

```javascript
// 在 DevTools Console 中

// 查看所有已加载插件
JSON.stringify([...window.__ZUSTAND_PLUGIN_STORE__?.getState?.()?.plugins?.entries?.()] ?? 'store not found');

// 查看插件 store 状态（如果你的 store 是全局的）
useDemoStore.getState()

// 手动触发 toast
window.__DEMO_PLUGIN_CTX__?.ui.showToast({ title: 'Test', variant: 'success' });

// 查看当前连接
window.__DEMO_PLUGIN_CTX__?.connections.getAll();
```

### Plugin Manager

- **Status Badge**：显示 `active`/`error`/`disabled` 状态
- **Error Message**：错误状态时显示详细错误信息
- **Reload**：热重载插件（先 unload 再 load）
- **Refresh**：重新扫描磁盘，发现新插件/移除已删除插件

### 常见错误排查

| 现象 | 可能原因 |
|------|----------|
| 加载失败：`module must export "activate"` | 入口文件没有 `export function activate` |
| 加载失败：`timed out after 5000ms` | `activate()` 中有未 resolve 的 Promise |
| Tab 不显示 | 忘记在 `activate()` 中调用 `ctx.ui.registerTabView()` |
| hooks 不工作 | Manifest 中未声明 `terminalHooks.inputInterceptor: true` |
| Toast 不显示 | 确认 variant 拼写正确（`default`/`success`/`error`/`warning`） |
| 快捷键无效 | 确认终端窗口处于聚焦状态 |
| 读取设置返回 undefined | 确认设置 key 与 manifest 中的 `settings[].id` 一致 |
| 插件被自动禁用 | 断路器触发。检查 console 中的错误/超时警告 |
| 样式不对/和主题不协调 | 使用了硬编码颜色而非语义化类名 |

---

## 19. 常见问题 (FAQ)

### Q: 插件可以使用 TypeScript 吗？

可以。你需要自行编译为 ESM JavaScript：

```bash
# 使用 esbuild
npx esbuild src/main.ts --bundle --format=esm --outfile=main.js --external:react

# 或 tsc
npx tsc --module esnext --target esnext --outDir .
```

注意：不要打包 React，使用 `window.__OXIDE__` 获取。

### Q: 插件可以有多个文件吗？

由于加载机制使用 Blob URL，插件内部的相对 `import` 不生效。解决方案：

1. **推荐**：使用打包工具（esbuild/rollup）合并为单文件
2. **备选**：将所有代码写在 `main.js` 一个文件中

```bash
# esbuild 打包示例
npx esbuild src/index.ts \
  --bundle \
  --format=esm \
  --outfile=main.js \
  --external:react \
  --external:react-dom
```

### Q: 插件可以访问文件系统吗？

不能直接访问。插件只能：
- 通过 `ctx.api.invoke()` 调用已声明的 Tauri 后端命令
- 通过 `ctx.storage` 使用 localStorage

### Q: 插件可以发网络请求吗？

可以使用浏览器原生的 `fetch()` API。但注意 Tauri 的 CSP 策略可能限制某些域名。

### Q: 如何在插件中使用 JSX？

插件默认是纯 JS，需使用 `React.createElement`。如需 JSX：

1. 使用 esbuild：`--jsx=automatic --jsx-import-source=react`
2. 使用 Babel：`@babel/plugin-transform-react-jsx`
3. 在打包时将 React 标记为 external，运行时从 `window.__OXIDE__` 获取

### Q: 插件之间可以通信吗？

当前设计中，`ctx.events.on/emit` 有命名空间隔离。跨插件通信选项：

1. **共享全局变量**：双方约定 `window.__SHARED_DATA__`
2. **底层 Event Bridge**：直接使用 `pluginEventBridge`（需理解内部 API，不推荐）
3. **未来计划**：可能添加跨插件事件通道

### Q: 插件被自动禁用了怎么办？

1. 查看 DevTools console 中的错误/超时警告
2. 修复代码中的性能问题或异常
3. 在 Plugin Manager 中重新启用插件
4. 或手动编辑 `~/.oxideterm/plugin-config.json`：

```json
{
  "plugins": {
    "your-plugin-id": {
      "enabled": true
    }
  }
}
```

### Q: 插件可以修改 OxideTerm 的界面吗？

通过声明式 API 可以：
- 添加 Tab 视图
- 添加 Sidebar 面板
- 显示 Toast/Confirm

不能：
- 修改现有 UI 组件
- 注入自定义 CSS
- 修改菜单/工具栏

### Q: 插件配置文件在哪里？

| 文件/位置 | 说明 |
|-----------|------|
| `~/.oxideterm/plugins/{id}/plugin.json` | 插件清单 |
| `~/.oxideterm/plugins/{id}/main.js` | 插件代码 |
| `~/.oxideterm/plugin-config.json` | 全局插件启用/禁用配置 |
| `localStorage: oxide-plugin-{id}-*` | 插件存储数据 |
| `localStorage: oxide-plugin-{id}-setting-*` | 插件设置 |

### Q: 如何发布插件到官方注册表？

1. **打包插件**：将插件目录打包为 ZIP 文件
   ```bash
   cd ~/.oxideterm/plugins/my-plugin
   zip -r my-plugin-1.0.0.zip .
   ```

2. **计算校验和**：
   ```bash
   shasum -a 256 my-plugin-1.0.0.zip
   # 输出: abc123... my-plugin-1.0.0.zip
   ```

3. **托管 ZIP 文件**：上传到可公开访问的 URL（GitHub Releases、CDN 等）

4. **提交到注册表**：
   - 官方注册表：向 OxideTerm 仓库提交 PR，添加你的插件条目
   - 自建注册表：在你的 `registry.json` 中添加条目

**注册表条目格式**：
```json
{
  "id": "my-plugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "description": "Plugin description",
  "author": "Your Name",
  "downloadUrl": "https://example.com/my-plugin-1.0.0.zip",
  "checksum": "sha256:abc123...",
  "size": 12345,
  "tags": ["utility"],
  "homepage": "https://github.com/you/my-plugin"
}
```

### Q: 如何使用自定义插件注册表？

编辑 `~/.oxideterm/plugin-config.json`：

```json
{
  "registryUrl": "https://your-server.com/registry.json",
  "plugins": {}
}
```

注册表 JSON 格式：
```json
{
  "version": 1,
  "plugins": [
    { "id": "...", "name": "...", ... }
  ]
}
```

---

## 20. 类型参考 (TypeScript)

以下是完整的 TypeScript 类型定义，可用于插件开发时的类型提示。

将此文件保存为 `oxideterm-plugin.d.ts` 放在你的插件项目中：

```typescript
// oxideterm-plugin.d.ts
// OxideTerm Plugin System Type Definitions

// ── Disposable ──────────────────────────────────────────────
export type Disposable = {
  dispose(): void;
};

// ── Plugin States ───────────────────────────────────────────
export type PluginState = 'inactive' | 'loading' | 'active' | 'error' | 'disabled';

export type InstallState = 'downloading' | 'extracting' | 'installing' | 'done' | 'error';

export type SshConnectionState =
  | 'idle'
  | 'connecting'
  | 'active'
  | 'disconnecting'
  | 'disconnected'
  | 'reconnecting'
  | 'link_down'
  | { error: string };

// ── Connection Snapshot ─────────────────────────────────────
export type ConnectionSnapshot = Readonly<{
  id: string;
  host: string;
  port: number;
  username: string;
  state: SshConnectionState;
  refCount: number;
  keepAlive: boolean;
  createdAt: string;
  lastActive: string;
  terminalIds: readonly string[];
  parentConnectionId?: string;
}>;

// ── Terminal Hook Types ─────────────────────────────────────
export type TerminalHookContext = { sessionId: string };

export type InputInterceptor = (
  data: string,
  context: TerminalHookContext,
) => string | null;

export type OutputProcessor = (
  data: Uint8Array,
  context: TerminalHookContext,
) => Uint8Array;

// ── Registry Types (Remote Installation) ────────────────────
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

export type RegistryIndex = {
  version: number;
  plugins: RegistryEntry[];
};

// ── Plugin Tab Props ────────────────────────────────────────
export type PluginTabProps = {
  tabId: string;
  pluginId: string;
};

// ── API Interfaces ──────────────────────────────────────────
export type PluginConnectionsAPI = {
  getAll(): ReadonlyArray<ConnectionSnapshot>;
  get(connectionId: string): ConnectionSnapshot | null;
  getState(connectionId: string): SshConnectionState | null;
};

export type PluginEventsAPI = {
  onConnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onDisconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onLinkDown(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onReconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onSessionCreated(handler: (info: { sessionId: string; connectionId: string }) => void): Disposable;
  onSessionClosed(handler: (info: { sessionId: string }) => void): Disposable;
  on(name: string, handler: (data: unknown) => void): Disposable;
  emit(name: string, data: unknown): void;
};

export type PluginUIAPI = {
  registerTabView(tabId: string, component: React.ComponentType<PluginTabProps>): Disposable;
  registerSidebarPanel(panelId: string, component: React.ComponentType): Disposable;
  openTab(tabId: string): void;
  showToast(opts: {
    title: string;
    description?: string;
    variant?: 'default' | 'success' | 'error' | 'warning';
  }): void;
  showConfirm(opts: { title: string; description: string }): Promise<boolean>;
};

export type PluginTerminalAPI = {
  registerInputInterceptor(handler: InputInterceptor): Disposable;
  registerOutputProcessor(handler: OutputProcessor): Disposable;
  registerShortcut(command: string, handler: () => void): Disposable;
  writeToTerminal(sessionId: string, text: string): void;
  getBuffer(sessionId: string): string | null;
  getSelection(sessionId: string): string | null;
};

export type PluginSettingsAPI = {
  get<T>(key: string): T;
  set<T>(key: string, value: T): void;
  onChange(key: string, handler: (newValue: unknown) => void): Disposable;
};

export type PluginI18nAPI = {
  t(key: string, params?: Record<string, string | number>): string;
  getLanguage(): string;
  onLanguageChange(handler: (lang: string) => void): Disposable;
};

export type PluginStorageAPI = {
  get<T>(key: string): T | null;
  set<T>(key: string, value: T): void;
  remove(key: string): void;
};

export type PluginBackendAPI = {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
};

export type PluginAssetsAPI = {
  loadCSS(relativePath: string): Promise<Disposable>;
  getAssetUrl(relativePath: string): Promise<string>;
  revokeAssetUrl(url: string): void;
};

// ── Plugin Context ──────────────────────────────────────────
export type PluginContext = Readonly<{
  pluginId: string;
  connections: PluginConnectionsAPI;
  events: PluginEventsAPI;
  ui: PluginUIAPI;
  terminal: PluginTerminalAPI;
  settings: PluginSettingsAPI;
  i18n: PluginI18nAPI;
  storage: PluginStorageAPI;
  api: PluginBackendAPI;
  assets: PluginAssetsAPI;
}>;

// ── Plugin Manifest (v2) ────────────────────────────────────
export type PluginManifest = {
  id: string;
  name: string;
  version: string;
  description?: string;
  author?: string;
  main: string;
  engines?: { oxideterm?: string };
  // v2 Package fields
  manifestVersion?: 1 | 2;
  format?: 'bundled' | 'package';
  assets?: string;
  styles?: string[];
  sharedDependencies?: Record<string, string>;
  repository?: string;
  checksum?: string;
  contributes?: { /* ... */ };
  locales?: string;
};

// ── Plugin Module ───────────────────────────────────────────
export type PluginModule = {
  activate: (ctx: PluginContext) => void | Promise<void>;
  deactivate?: () => void | Promise<void>;
};

// ── Shared Modules (window.__OXIDE__) ───────────────────────
declare global {
  interface Window {
    __OXIDE__?: {
      React: typeof import('react');
      ReactDOM: { createRoot: typeof import('react-dom/client').createRoot };
      zustand: { create: typeof import('zustand').create };
      lucideReact: typeof import('lucide-react');
      ui: PluginUIKit;         // 24 个预置 UI 组件
      version: string;         // OxideTerm 版本号
      pluginApiVersion: number; // 插件 API 版本号
    };
  }
}
```

---

## 附录 A：Manifest 完整 JSON Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["id", "name", "version", "main"],
  "properties": {
    "id": {
      "type": "string",
      "pattern": "^[a-zA-Z0-9][a-zA-Z0-9_-]*$",
      "description": "Unique plugin identifier"
    },
    "name": { "type": "string", "description": "Human-readable plugin name" },
    "version": { "type": "string", "pattern": "^\\d+\\.\\d+\\.\\d+", "description": "Semver version" },
    "description": { "type": "string" },
    "author": { "type": "string" },
    "main": { "type": "string", "description": "Relative path to ESM entry file" },
    "manifestVersion": {
      "type": "integer", "enum": [1, 2], "default": 1,
      "description": "Manifest schema version; set to 2 for v2 Package format"
    },
    "format": {
      "type": "string", "enum": ["bundled", "package"], "default": "bundled",
      "description": "bundled = single-file Blob URL; package = multi-file HTTP Server"
    },
    "assets": {
      "type": "string",
      "description": "Relative path to assets directory (v2 Package only)"
    },
    "styles": {
      "type": "array", "items": { "type": "string" },
      "description": "CSS files to auto-load on activation (v2 Package only)"
    },
    "sharedDependencies": {
      "type": "object",
      "additionalProperties": { "type": "string" },
      "description": "Dependencies provided by host via window.__OXIDE__"
    },
    "repository": {
      "type": "string",
      "description": "Repository URL for source code"
    },
    "checksum": {
      "type": "string",
      "description": "SHA-256 hash of the main entry file for integrity verification"
    },
    "engines": {
      "type": "object",
      "properties": {
        "oxideterm": { "type": "string", "pattern": "^>=?\\d+\\.\\d+\\.\\d+" }
      }
    },
    "locales": { "type": "string", "description": "Relative path to locales directory" },
    "contributes": {
      "type": "object",
      "properties": {
        "tabs": {
          "type": "array",
          "items": {
            "type": "object",
            "required": ["id", "title", "icon"],
            "properties": {
              "id": { "type": "string" },
              "title": { "type": "string" },
              "icon": { "type": "string", "description": "Lucide React icon name" }
            }
          }
        },
        "sidebarPanels": {
          "type": "array",
          "items": {
            "type": "object",
            "required": ["id", "title", "icon"],
            "properties": {
              "id": { "type": "string" },
              "title": { "type": "string" },
              "icon": { "type": "string" },
              "position": { "type": "string", "enum": ["top", "bottom"], "default": "bottom" }
            }
          }
        },
        "settings": {
          "type": "array",
          "items": {
            "type": "object",
            "required": ["id", "type", "default", "title"],
            "properties": {
              "id": { "type": "string" },
              "type": { "type": "string", "enum": ["string", "number", "boolean", "select"] },
              "default": {},
              "title": { "type": "string" },
              "description": { "type": "string" },
              "options": {
                "type": "array",
                "items": {
                  "type": "object",
                  "required": ["label", "value"],
                  "properties": {
                    "label": { "type": "string" },
                    "value": {}
                  }
                }
              }
            }
          }
        },
        "terminalHooks": {
          "type": "object",
          "properties": {
            "inputInterceptor": { "type": "boolean" },
            "outputProcessor": { "type": "boolean" },
            "shortcuts": {
              "type": "array",
              "items": {
                "type": "object",
                "required": ["key", "command"],
                "properties": {
                  "key": { "type": "string" },
                  "command": { "type": "string" }
                }
              }
            }
          }
        },
        "connectionHooks": {
          "type": "array",
          "items": { "type": "string", "enum": ["onConnect", "onDisconnect", "onReconnect", "onLinkDown"] }
        },
        "apiCommands": {
          "type": "array",
          "items": { "type": "string" }
        }
      }
    }
  }
}
```

---

## 附录 B：内部架构文件速查

| 文件 | 职责 |
|------|------|
| `src/types/plugin.ts` | 所有插件类型定义 |
| `src/store/pluginStore.ts` | Zustand 插件状态管理 |
| `src/lib/plugin/pluginLoader.ts` | 生命周期管理（发现/加载/卸载/断路器） |
| `src/lib/plugin/pluginContextFactory.ts` | 构建冻结的 PluginContext 膜 |
| `src/lib/plugin/pluginEventBridge.ts` | 事件桥接（appStore → plugin events） |
| `src/lib/plugin/pluginTerminalHooks.ts` | 终端 I/O hook 管线 |
| `src/lib/plugin/pluginStorage.ts` | localStorage KV 存储封装 |
| `src/lib/plugin/pluginSettingsManager.ts` | 设置管理（声明+持久化+change 通知） |
| `src/lib/plugin/pluginI18nManager.ts` | 插件 i18n 封装（i18next 集成） |
| `src/lib/plugin/pluginUtils.ts` | 共享工具函数（路径验证、安全检查） |
| `src/lib/plugin/pluginUIKit.tsx` | 24 个预置 UI 组件（UIKit） |
| `src-tauri/src/commands/plugin.rs` | Rust 后端（文件 I/O + 路径安全） |
| `src-tauri/src/commands/plugin_server.rs` | Plugin File Server（多文件 HTTP 访问） |
| `src-tauri/src/commands/plugin_registry.rs` | 插件仓库注册/搜索 |
| `src/components/plugin/PluginManagerView.tsx` | Plugin Manager UI |
| `src/components/plugin/PluginTabRenderer.tsx` | 插件 Tab 渲染器 |
| `src/components/plugin/PluginSidebarRenderer.tsx` | 插件 Sidebar 渲染器 |
| `src/components/plugin/PluginConfirmDialog.tsx` | 主题化确认对话框（Radix UI） |

---

*本文档基于 OxideTerm v1.6.2 插件系统源码自动生成。如有疑问，请参考上述源码文件或提交 Issue。*
