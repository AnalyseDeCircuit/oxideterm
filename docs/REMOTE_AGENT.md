# 远程代理 (Remote Agent)

> OxideTerm 可以将一个轻量级代理程序自动部署到远程 Linux 主机，显著提升 IDE 模式下的文件操作性能和体验。

## 概述

OxideTerm Agent 是一个无依赖、静态链接的 Rust 单体二进制（~600 KB），通过 SSH 通道与主应用通信。它直接在远程服务器上执行文件操作，避免了传统 SFTP 方案的逐条往返延迟。

**核心优势**：
- **原子文件写入** — 先写临时文件再 rename，网络中断不会产生半成品文件
- **inotify 实时监视** — 文件在远端被修改时，IDE 内即时刷新（无需轮询）
- **哈希冲突检测** — 写入前校验 SHA-256，防止覆盖外部修改
- **服务器端搜索** — grep 直接在远端执行，仅传回匹配结果
- **深层目录树预取** — 一次请求获取 3 层目录结构，减少展开等待

## 架构

```
┌─────────────────────────────────────────────────────────────┐
│  OxideTerm 应用 (Tauri + React)                               │
│  ┌─────────────┐    ┌─────────────────────────────────────┐ │
│  │ IDE 前端组件  │    │  Rust 后端                            │ │
│  │  IdeTree     │◄──►│  AgentTransport (stdin/stdout)      │ │
│  │  IdeEditor   │    │  AgentDeployer (自动部署)              │ │
│  │  IdeSearch   │    │  AgentRegistry (多会话管理)            │ │
│  └─────────────┘    └──────────────┬──────────────────────┘ │
└────────────────────────────────────┼────────────────────────┘
                                     │ SSH exec 通道
                                     │ (stdin/stdout JSON-RPC)
                                     ▼
                          ┌─────────────────────┐
                          │  远程 Linux 主机       │
                          │  ~/.oxideterm/         │
                          │    oxideterm-agent     │
                          │    (静态 musl 二进制)    │
                          └─────────────────────┘
```

## 通信协议

Agent 使用 **行分隔 JSON-RPC** 协议，通过 SSH exec 通道的 stdin/stdout 传输。

### 请求格式

```json
{"id": 1, "method": "fs/readFile", "params": {"path": "/home/user/example.txt"}}
```

### 响应格式

```json
{"id": 1, "result": {"content": "file content...", "hash": "sha256:abc123..."}}
```

### 错误响应

```json
{"id": 1, "error": {"code": -2, "message": "File not found: /path/to/file"}}
```

### 通知（服务端主动推送）

```json
{"method": "watch/event", "params": {"path": "/home/user/file.txt", "kind": "modify"}}
```

## 支持的方法

### 文件系统操作 (`fs/*`)

| 方法 | 说明 | 参数 |
|------|------|------|
| `fs/readFile` | 读取文件内容 | `path`, `max_size?` |
| `fs/writeFile` | 原子写入文件 | `path`, `content`, `expected_hash?` |
| `fs/stat` | 获取文件元信息 | `path` |
| `fs/list` | 列出目录内容 | `path` |
| `fs/listTree` | 递归获取目录树 | `path`, `depth?`, `max_entries?` |
| `fs/mkdir` | 创建目录 | `path` |
| `fs/remove` | 删除文件或目录 | `path`, `recursive?` |
| `fs/rename` | 重命名/移动 | `from`, `to` |

### 搜索操作 (`search/*`)

| 方法 | 说明 | 参数 |
|------|------|------|
| `search/grep` | 在文件/目录中搜索文本 | `pattern`, `path`, `case_sensitive?`, `max_results?`, `ignore?` |

### 文件监视 (`watch/*`)

| 方法 | 说明 | 参数 |
|------|------|------|
| `watch/start` | 开始监视目录变更 | `path`, `ignore?` |
| `watch/stop` | 停止监视 | `path` |

### 系统操作 (`sys/*`)

| 方法 | 说明 | 参数 |
|------|------|------|
| `sys/info` | 返回 Agent 版本和系统信息 | 无 |
| `sys/shutdown` | 优雅关闭 Agent 进程 | 无 |

## 部署流程

Agent 的部署完全自动化，无需用户手动操作：

```
1. 检测远程架构 ──► uname -m
2. 版本检查     ──► 运行 oxideterm-agent --version (如已存在)
3. 上传二进制   ──► 通过 SFTP 传输到 ~/.oxideterm/oxideterm-agent
4. 设置权限     ──► chmod +x
5. 启动代理     ──► SSH exec 通道执行
6. 握手验证     ──► 发送 sys/info 确认通信正常
```

如果版本一致，跳过步骤 3-4，直接启动。

### 支持的目标架构

| 架构 | 二进制文件名 | 大小 |
|------|-------------|------|
| x86_64 (Intel/AMD) | `oxideterm-agent-x86_64-linux-musl` | ~670 KB |
| aarch64 (ARM64) | `oxideterm-agent-aarch64-linux-musl` | ~600 KB |

不支持的架构（如 32 位 ARM、MIPS 等）会自动回退到 SFTP 模式。

## 安全性

- **进程隔离** — Agent 以当前 SSH 用户权限运行，无 root 提权
- **自清理** — SSH 连接断开时 stdin EOF 触发自动退出
- **无网络监听** — 不开放任何端口，仅通过 SSH 通道通信
- **最小权限** — 仅访问用户有权限的文件
- **静态链接** — 不依赖目标系统的共享库

## 设计原则

1. **零异步运行时** — 不使用 tokio，仅 `std::thread` + 阻塞 I/O，最小化二进制体积
2. **最少依赖** — 只有 `serde`、`serde_json`、`inotify`（Linux）、`libc`
3. **静态 musl 链接** — 在任何 Linux 发行版上无需安装依赖即可运行
4. **优雅降级** — 如果部署失败或架构不支持，自动回退到 SFTP，不影响正常使用

## 原子写入机制

传统 SFTP 写入在网络中断时可能产生不完整文件。Agent 使用原子写入避免此问题：

```
1. 写入临时文件    ──► /path/to/.file.tmp.{random}
2. 验证内容完整    ──► 检查写入字节数
3. 原子替换       ──► rename(tmp, target)  ← 操作系统保证原子性
```

如果客户端还提供了 `expected_hash`（之前读取时获得的 SHA-256），Agent 会在写入前检查目标文件的当前哈希。如果不一致，说明文件在此期间被外部修改，写入将被拒绝并返回冲突错误。

## inotify 文件监视

在 Linux 上，Agent 使用内核的 inotify 机制实现零延迟文件监视：

- 递归监视所有子目录（自动添加新目录）
- 支持 `.gitignore` 风格的排除模式
- inotify 文件描述符设置为非阻塞模式，确保停止信号可及时响应
- 非 Linux 系统不支持文件监视（此功能仅在远程主机为 Linux 时可用）

## 构建

### 前置条件

```bash
# 安装 musl 交叉编译目标
rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl

# macOS 需要安装交叉编译工具链
brew install filosottile/musl-cross/musl-cross
brew install messense/macos-cross-toolchains/aarch64-unknown-linux-musl

# 或者使用 cross（需要 Docker）
cargo install cross
```

### 手动构建

```bash
# 构建两个架构
./scripts/build-agent.sh

# 只构建 x86_64
./scripts/build-agent.sh x86_64

# 使用 cross 构建（需要 Docker 运行）
USE_CROSS=1 ./scripts/build-agent.sh
```

构建产物输出到 `src-tauri/agents/`。

### CI 自动构建

在 GitHub Actions 中，Agent 在 `ubuntu-24.04` runner 上自动交叉编译：

1. x86_64 — 原生编译（runner 即为 x86_64)
2. aarch64 — 通过 `cross`（基于 Docker）交叉编译
3. 编译产物复制到 `src-tauri/agents/` 供 Tauri 打包

macOS 和 Windows 构建使用空占位文件（Agent 仅在远端 Linux 主机运行，客户端平台不需要本地运行）。

## 前端集成

### 状态指示

IDE 模式下，状态栏左侧显示当前传输模式：

- 🟢 **Agent** — Agent 已连接，享受全部增强功能
- 🟡 **Deploying** — 正在部署 Agent 到远程主机
- ⚪ **SFTP** — 使用传统 SFTP（Agent 不可用或架构不支持）

### agentService 门面

前端通过 `src/lib/agentService.ts` 门面层访问 Agent 功能：

```typescript
import * as agentService from '@/lib/agentService';

// 读取文件（自动选择 Agent 或 SFTP）
const content = await agentService.readFile(nodeId, '/path/to/file');

// 原子写入
await agentService.writeFile(nodeId, '/path/to/file', content, expectedHash);

// 搜索
const results = await agentService.grep(nodeId, 'pattern', '/path', options);

// 获取目录树（深度预取）
const tree = await agentService.listTree(nodeId, '/root', depth);
```

### 按需加载

文件树采用纯按需加载策略（不做深度预取）。每次展开目录时调用 `agentService.listDir()`，该方法内部使用 `fs/listTree`（depth=0）获取单层目录内容。并发展开同一目录时，`AbortController` 自动取消旧请求。

### 路径解析

Agent 内置 `resolve_path()` 函数，自动将 `~` 和 `~/...` 展开为 `$HOME` 绝对路径。所有文件系统操作（readFile、writeFile、stat、listDir、listTree、mkdir、remove、rename、chmod、grep、git_status）以及符号操作（symbols/index、symbols/complete、symbols/definitions）均经过此函数处理。

> ⚠️ Linux 内核不认识 `~`，只有 Shell 认识。`Path::new("~/file")` 会被解析为当前目录下名为 `~` 的子目录，导致静默失败。

## 故障排查

| 症状 | 原因 | 解决方案 |
|------|------|----------|
| 状态栏始终显示 SFTP | 架构不支持（非 x86_64/aarch64 Linux） | 正常行为，无需处理 |
| 部署后立即失败 | SELinux 或 noexec 挂载点 | 检查 `~/.oxideterm/` 所在分区权限 |
| 文件监视不工作 | inotify watch 数量达到系统限制 | 增大 `fs.inotify.max_user_watches` |
| Agent 未自动退出 | SSH 连接未正常关闭 | Agent 会在 stdin EOF 后自动退出 |

## 相关文档

- [系统架构](ARCHITECTURE.md) — 整体架构设计
- [IDE 模式](IDE_MODE.md) — IDE 功能详解
- [SFTP 实现](SFTP.md) — SFTP 回退方案
- [系统不变量](SYSTEM_INVARIANTS.md) — 状态一致性规则
