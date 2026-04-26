# OxideTerm 使用指南

> 从入门到日常高效使用：**操作步骤与快捷键**以本文为准；连接池、拓扑、重连、SFTP/转发架构与不变量见合订参考 [OXIDETERM_CORE_REFERENCE.md](./OXIDETERM_CORE_REFERENCE.md)。

> **文档版本**: v1.9.1 | **应用参考版本**: 0.20.1 | **最后更新**: 2026-03-24

---

## 目录

1. [快速开始](#快速开始)
2. [设置与页签](#设置与页签)
3. [基础操作](#基础操作)
4. [连接管理](#连接管理)
5. [终端功能](#终端功能)
6. [SFTP 文件管理](#sftp-文件管理)
7. [端口转发](#端口转发)
8. [扩展能力（插件 / 图形 / 主题）](#user-guide-extensions)
9. [高级功能](#高级功能)
10. [快捷键速查](#快捷键速查)
11. [最佳实践](#最佳实践)
12. [故障排查](#故障排查)
13. [相关文档](#相关文档)

---

## 快速开始

### 首次运行

1. **启动 OxideTerm**  
   - macOS：应用程序文件夹  
   - Windows：开始菜单或桌面快捷方式  
   - Linux：发行版提供的启动方式或包内命令（如 `oxideterm`）

2. **创建第一个连接**  
   - 在侧边栏使用 **新建连接**（或 `Ctrl+N` / `⌘N`）  
   - 填写服务器信息，例如：  
     - Name: `My Server`  
     - Host: `example.com`  
     - Port: `22`  
     - Username: `admin`  
     - Authentication: 密码、密钥、SSH Agent 或证书（按界面选项）

3. **连接**  
   - 在会话树中连接目标节点并打开终端；按提示完成认证后，终端标签页会打开。

---

## 设置与页签

- **打开设置**：`Ctrl+,`（Windows/Linux）或 `⌘,`（macOS）；也可从命令面板或侧边栏进入。  
- **界面形态**：设置以**页签**组织（例如终端、外观、AI、插件等），便于在同一窗口内切换，而非零散弹窗。  
- **快捷键总表**：设置内 **帮助 / 关于** 中的快捷键列表与 [src/lib/shortcuts.ts](../../src/lib/shortcuts.ts) 一致；下文 [快捷键速查](#快捷键速查) 为摘要版。  
- **应用与编辑器主题**：终端配色与字体在 **Terminal** 相关页签；应用级主题与自定义变量见 [CUSTOM_THEMES.md](./CUSTOM_THEMES.md)。

---

## 基础操作

### 创建终端

#### 本地终端

- **快捷键**：`Ctrl+T`（Windows/Linux）或 `⌘T`（macOS）— 默认 Shell  
- **Shell 选择器**：`Ctrl+Shift+T` / `⌘⇧T`  
- **侧边栏**：在会话树中选择本地终端相关入口  

#### SSH 终端

1. 在会话树中连接目标节点后新建或选择终端子项  
2. 或使用 **新建连接** 流程后再打开终端  

### 标签页

- **切换标签**：`Ctrl+Tab` / `Ctrl+Shift+Tab`（Windows/Linux），macOS 上为 `⌘}` / `⌘{`（与设置内说明一致）  
- **跳到第 N 个标签**：`Ctrl+1`–`Ctrl+9` / `⌘1`–`⌘9`  
- **关闭当前标签**：`Ctrl+W` / `⌘W`  

### 会话树导航

OxideTerm 用树形结构组织连接、终端、SFTP、钻入节点等：

```
├── 分组 / 预设
│   ├── 连接节点
│   │   ├── 终端
│   │   ├── SFTP
│   │   └── …
│   └── …
└── 本地终端
```

- 点击行可展开/折叠；**右键**打开上下文菜单（连接、断开、SFTP、转发、IDE 等）。  
- 会话树内**拖拽排序**若未在您的版本中提供，请以实际客户端为准；整理结构可使用分组与编辑连接。

---

## 连接管理

### 保存连接

基本字段示例：

```
Name: Production Server
Host: prod.example.com
Port: 22
Username: admin
Group: Production
```

### 认证方式

- **密码**：由系统钥匙串保存（勿写入明文配置文件）。  
- **SSH 密钥**：选择私钥路径；若有 Passphrase 会安全存储。  
- **SSH Agent**：依赖系统 SSH Agent 与已加载密钥。  
- **证书**：按界面选择私钥与证书文件。  

### ProxyJump（跳板）

支持在连接或链路中配置 **Proxy Chain**（单跳或多跳），等价于 `ssh -J` 链式跳板。拓扑与自动路由、级联故障行为见合订文档：

详见：[OXIDETERM_CORE_REFERENCE.md — 2. 网络拓扑与 ProxyJump](./OXIDETERM_CORE_REFERENCE.md#2-网络拓扑与-proxyjump)

### 连接分组

- 使用右键菜单创建 **分组**，将连接归类到 Production / Staging 等。  
- 折叠不常用分组以保持侧边栏清晰。  

---

## 终端功能

### 搜索

1. **打开查找栏**（终端聚焦时）：  
   - **Windows**：`Ctrl+Shift+F`（避免与终端内 `Ctrl+F` 冲突）  
   - **macOS**：`⌘F`  
   - **Linux**：通常为 **`Super`（徽标键）+ F**（与 `useTerminalKeyboard` 一致；勿与 shell 的 Ctrl 组合冲突）  
2. **Visible Buffer**：当前屏内容实时高亮。  
3. **Deep History**：在完整会话历史中异步搜索（默认可达数万行，上限见设置与 [TERMINAL_SEARCH.md](./TERMINAL_SEARCH.md)）。  

详见：[TERMINAL_SEARCH.md](./TERMINAL_SEARCH.md)

### 复制与粘贴

| 操作 | Windows/Linux | macOS |
|------|---------------|-------|
| **复制** | `Ctrl+Shift+C` 或选中自动复制 | `⌘C` 或选中自动复制 |
| **粘贴** | `Ctrl+Shift+V` 或右键 | `⌘V` 或右键 |

### 清屏与滚动

| 方法 | 说明 |
|------|------|
| 完全清屏 | 终端内常用 `Ctrl+L`；应用层清屏以界面说明为准 |
| Shell `clear` | 由 shell 处理 |
| 重置终端 | 右键上下文菜单中的重置选项（若有） |

### 字体与主题

设置 → **Terminal** 页签：字体、字号、内置主题等。自定义主题引擎见 [CUSTOM_THEMES.md](./CUSTOM_THEMES.md)。

---

## SFTP 文件管理

### 打开方式

- 终端标签或连接节点 **右键** → 打开 SFTP  
- 在会话树中对应节点下选择 SFTP 子项  

### 用户可见行为

- **State Gating**：连接未就绪时界面会提示等待，避免误操作；恢复后自动可用。  
- **Path Memory**：重连后尽量恢复上次工作目录（与编排器及连接状态相关）。  

传输队列、双栏操作与快捷键见设置内 **SFTP** 快捷键分类。

详见：[OXIDETERM_CORE_REFERENCE.md — 5. SFTP 文件管理](./OXIDETERM_CORE_REFERENCE.md#5-sftp-文件管理)

---

## 端口转发

支持本地转发（`-L`）、远程转发（`-R`）、动态 SOCKS（`-D`）等，按节点在转发管理界面配置。

**Link Resilience**：规则会随会话持久化，网络恢复并由重连管道重建后，转发由编排逻辑**尽量自动恢复**，一般无需手工重加。

详见：[OXIDETERM_CORE_REFERENCE.md — 4. 端口转发](./OXIDETERM_CORE_REFERENCE.md#4-端口转发)

---

<a id="user-guide-extensions"></a>

## 扩展能力（插件 / 图形 / 主题）

### 插件

- 插件从用户目录加载，通过 **设置 → 插件** 管理；架构与安全边界见 [PLUGIN_SYSTEM.md](./PLUGIN_SYSTEM.md)，开发见 [插件开发资料包](../../plugin-development/README.zh-CN.md)。

### WSL 图形转发

- 在支持的构建与平台上，可通过图形转发在应用内查看远程图形会话；说明与限制见 [GRAPHICS_FORWARDING.zh-CN.md](./GRAPHICS_FORWARDING.zh-CN.md)（需启用相应 **Cargo feature**，见 [src-tauri/Cargo.toml](../../src-tauri/Cargo.toml)）。

### 主题

- 终端主题与应用外观见设置；编辑器与 CSS 变量级定制见 [CUSTOM_THEMES.md](./CUSTOM_THEMES.md)。

---

## 高级功能

### 自动重连（Reconnect Orchestrator）

网络抖动或链路断开时，由前端 `reconnectOrchestratorStore` 等协同编排重试、Grace Period 与恢复（含 SFTP 路径、转发、IDE 等子系统的恢复策略）。用户侧常见现象：终端输入短暂锁定、遮罩提示、恢复后继续操作。

详见：[OXIDETERM_CORE_REFERENCE.md — 3. 重连编排器](./OXIDETERM_CORE_REFERENCE.md#3-重连编排器)

### 连接池

同一底层 SSH 连接可承载多个终端、SFTP、转发等，减少重复认证与服务器负担。

详见：[OXIDETERM_CORE_REFERENCE.md — 1. SSH 连接池与状态管理](./OXIDETERM_CORE_REFERENCE.md#1-ssh-连接池与状态管理)

### 网络拓扑

自动拓扑与最优路径、ProxyJump 链、级联故障表现见：

详见：[OXIDETERM_CORE_REFERENCE.md — 2. 网络拓扑与 ProxyJump](./OXIDETERM_CORE_REFERENCE.md#2-网络拓扑与-proxyjump)

### 远程 IDE / Agent

远程文件编辑、Remote Agent 部署与协议见合订文档中 **IDE** 与 **远程代理** 章节：

- [6. 远程代理（Remote Agent）](./OXIDETERM_CORE_REFERENCE.md#6-远程代理remote-agent)  
- [10. IDE 模式（轻量级远程开发）](./OXIDETERM_CORE_REFERENCE.md#10-ide-模式轻量级远程开发)

### OxideSens（AI）

- **内联助手**（终端内轻量对话、无工具调用）与 **侧边栏**（持久会话、工具、MCP、RAG 等）的配置与隐私说明见 [AICHAT.md](./AICHAT.md)。  
- 打开内联面板：Windows `Ctrl+Shift+I`；macOS/Linux 终端聚焦时多为 **`Super`+`I` 或 `⌘I`**（与 [useTerminalKeyboard.ts](../../src/hooks/useTerminalKeyboard.ts) 一致）。  
- 切换 **AI 侧边栏**：命令面板中对应命令（常见为 `Ctrl+Shift+A` / `⌘⇧A`，见命令面板快捷键列）。

### .oxide 加密导出/导入

用于在设备间迁移连接配置（ChaCha20-Poly1305、Argon2id 等）；导出时可选择是否内嵌私钥（**强密码**与文件保管责任在用户）。

**操作建议**：使用足够长的密码；含内嵌私钥的文件视为高敏感；导入后密钥落盘权限由应用处理。

详见：[SERIALIZATION.md](./SERIALIZATION.md)

---

## 快捷键速查

下列与 **设置 → 帮助** 中的应用级列表一致（`getShortcutCategories`）。终端聚焦时，**查找 / 内联 AI** 在 Windows 与 macOS/Linux 的修饰键组合可能不同，已在上文 [终端功能 — 搜索](#终端功能) 与 [OxideSens](#oxidesensai) 说明。

### 应用与窗口

| 操作 | Windows/Linux | macOS |
|------|---------------|-------|
| 新建本地终端 | `Ctrl+T` | `⌘T` |
| Shell 选择器 | `Ctrl+Shift+T` | `⌘⇧T` |
| 关闭当前标签 | `Ctrl+W` | `⌘W` |
| 下一标签 / 上一标签 | `Ctrl+Tab` / `Ctrl+Shift+Tab` | `⌘}` / `⌘{` |
| 跳到第 N 个标签 | `Ctrl+1`–`9` | `⌘1`–`⌘9` |
| 新建 SSH 连接 | `Ctrl+N` | `⌘N` |
| 命令面板 | `Ctrl+K` | `⌘K` |
| 切换侧边栏 | `Ctrl+\\` | `⌘\\` |
| 打开设置 | `Ctrl+,` | `⌘,` |
| Zen 模式 | `Ctrl+Shift+Z` | `⌘⇧Z` |
| 快捷键帮助 | `Ctrl+/` | `⌘/` |

**命令面板中常见补充**（以客户端显示为准）：底部面板 `Ctrl+J` / `⌘J`；广播输入 `Ctrl+B` / `⌘B`；AI 侧边栏 `Ctrl+Shift+A` / `⌘⇧A`。

### 终端内（设置「终端」类）

| 操作 | Windows/Linux（设置表） | macOS |
|------|-------------------------|-------|
| 查找 | `Ctrl+Shift+F` | `⌘F` |
| 内联 AI 面板 | `Ctrl+Shift+I` | `⌘I` |
| 关闭面板 | `Esc` | `Esc` |

> **Linux 终端聚焦**：实际拦截多为 **Super+F / Super+I**，与上表「Ctrl+Shift」可能不同，以避免占用终端 Ctrl 组合键。

### 分屏

| 操作 | Windows/Linux | macOS |
|------|---------------|-------|
| 横向分屏 | `Ctrl+Shift+E` | `⌘⇧E` |
| 纵向分屏 | `Ctrl+Shift+D` | `⌘⇧D` |
| 关闭当前窗格 | `Ctrl+Shift+W` | `⌘⇧W` |
| 在窗格间移动焦点 | `Ctrl+Alt+方向键` | `⌘⌥+方向键` |

### SFTP（摘录）

| 操作 | Windows/Linux | macOS |
|------|---------------|-------|
| 上传 / 下载 | `→` / `←` | `→` / `←` |
| 全选 | `Ctrl+A` | `⌘A` |
| 重命名 | `F2` | `F2` |
| 删除 | `Delete` | `Delete` |

---

## 最佳实践

### 组织连接

- 按环境使用分组（Production / Staging / Development）。  
- 使用可辨认的命名，例如 `Prod-Web-01`，避免泛用 `server1`。  

### 安全

- 优先 **密钥** 认证并妥善保管 Passphrase。  
- **.oxide** 与内嵌私钥导出：强密码、加密存储介质、最小扩散。  
- API Key（AI 等）仅存钥匙串，勿写入仓库或明文配置。  

### 性能

- 复用连接池：同主机多标签、SFTP、转发共享一条 SSH（行为见核心参考）。  
- 本地工作优先使用 **本地终端**。  
- 终端渲染可在设置中在 WebGL / Canvas 间切换以排查兼容问题。  

### 工作流

- 善用 **命令面板** 搜索不常用能力。  
- 大日志用 **Deep History** 搜索；日常用 Visible Buffer。  
- OxideSens 侧边栏适合工具与知识库；内联适合快速问句（见 [AICHAT.md](./AICHAT.md)）。  

---

## 故障排查

### 连接

- **无法连接**：检查网络、`ping`/`mtr`、端口可达、防火墙、SSH 服务与认证方式。  
- **频繁断开**：检查 NAT/中间盒超时；应用侧 keep-alive 与服务器 `ClientAlive` 类配置可协同调整。  

### SFTP

- **长时间 “Waiting for connection”**：多为 State Gating；等待重连完成或查看会话树状态，必要时手动重连。  
- **重连后路径未恢复**：确认版本与 [OXIDETERM_CORE_REFERENCE.md — 5. SFTP](./OXIDETERM_CORE_REFERENCE.md#5-sftp-文件管理) 所述行为；可尝试手动导航并反馈问题。  

### 性能与显示

- **输入延迟**：排查网络延迟、服务端负载；尝试 Canvas 渲染、略减小字号。  
- **颜色异常**：切换终端主题；远端 `TERM` 建议 `xterm-256color`（视环境而定）。  
- **Linux 下 WebGL 很卡**：这通常不是“WebGL 不可用”，而是底层落到了 `llvmpipe`、`SwiftShader`、`softpipe`、`Mesa OffScreen` 等软件渲染。现在自动模式会检测真实 renderer 并优先回退到 Canvas。反馈问题时建议附上日志里的 WebGL vendor / renderer。  
- **Linux AppImage / Wayland 白屏**：应用会先尝试加速 WebKitGTK 路径，若首屏在约 5 秒内未就绪，会自动以安全模式重启一次。若仍需手动排查，可设置环境变量 `OXIDETERM_LINUX_WEBVIEW_PROFILE=safe` 或 `accelerated` 对比行为，并附上会话类型（Wayland / X11）、打包形式（AppImage / 系统包）和 renderer 日志。  

更多已知问题见 [knownissues.md](./knownissues.md)。

---

## 相关文档

| 文档 | 说明 |
|------|------|
| [ARCHITECTURE.md](./ARCHITECTURE.md) | 系统架构（文头版本 v1.9.1，与约束文档对齐） |
| [SYSTEM_INVARIANTS.md](./SYSTEM_INVARIANTS.md) | 强同步、门禁、生命周期等不变量 |
| [PROTOCOL.md](./PROTOCOL.md) | 终端数据平面（WebSocket 帧与心跳等） |
| [OXIDETERM_CORE_REFERENCE.md](./OXIDETERM_CORE_REFERENCE.md) | 连接池、拓扑、重连、转发、SFTP、Agent、IDE 等合订参考 |
| [AICHAT.md](./AICHAT.md) | OxideSens 内联与侧边栏 |
| [TERMINAL_SEARCH.md](./TERMINAL_SEARCH.md) | 终端搜索双模式 |
| [LOCAL_TERMINAL.md](./LOCAL_TERMINAL.md) | 本地 PTY 终端 |
| [SERIALIZATION.md](./SERIALIZATION.md) | `.oxide` 格式与加解密要点 |
| [PLUGIN_SYSTEM.md](./PLUGIN_SYSTEM.md) / [插件开发资料包](../../plugin-development/README.zh-CN.md) | 插件运行时与开发 |
| [CUSTOM_THEMES.md](./CUSTOM_THEMES.md) | 自定义主题 |
| [GRAPHICS_FORWARDING.zh-CN.md](./GRAPHICS_FORWARDING.zh-CN.md) | WSL 图形转发 |
| [DEVELOPMENT.md](./DEVELOPMENT.md) | 开发与构建入门 |

---

## 获取帮助

- **GitHub Issues**: https://github.com/AnalyseDeCircuit/oxideterm/issues  
- **Discussions**: https://github.com/AnalyseDeCircuit/oxideterm/discussions  

欢迎通过 Issue / PR 反馈文档与产品问题。

---

*文档版本: v1.9.1 | 应用参考: 0.20.1 | 最后更新: 2026-03-24*
