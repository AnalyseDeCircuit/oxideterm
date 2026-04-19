# CLI 伴侣工具 — `oxt`

> OxideTerm CLI 伴侣工具的完整参考文档。

## 概述

`oxt` CLI 是一个独立的命令行二进制程序，通过 IPC（进程间通信）与正在运行的 OxideTerm GUI 进行通信。用户可以在终端或 shell 脚本中：

- 查询 OxideTerm 状态与健康信息
- 运行 `oxt doctor` 诊断安装、PATH、endpoint 与 CLI API 兼容性
- 列出连接、会话、本地终端与端口转发
- 执行连接、断开、聚焦、附着（mirror）等会话操作
- 创建/删除端口转发规则
- 读取配置（分组、连接详情）
- 使用 AI（`ask` / `exec`），支持流式输出、会话续聊与终端 Markdown 渲染

**主要设计决策：**

- **独立二进制**：`oxt` 是一个独立的 Rust crate（`cli/`），不编译进 Tauri 后端，保持 CLI 轻量（约 1 MB）且与 GUI 生命周期解耦。
- **内置不自动安装**：CLI 二进制文件作为 Tauri 资源分发在应用包内。用户通过 设置 → 通用 → CLI 伴侣工具 选择安装，安装后会在 Unix 上创建符号链接，在 Windows 上复制二进制到 `~/.local/bin/oxt`。
- **IPC 而非 HTTP**：通信使用 Unix Domain Socket（macOS/Linux）或命名管道（Windows）——无网络暴露、无端口冲突、无 TLS 开销。

## 架构

```
┌─────────────┐        JSON-RPC 2.0         ┌─────────────────┐
│  oxt CLI    │◄──── (换行符分隔) ──────────►│  OxideTerm GUI  │
│  (Rust bin) │                              │   cli_server    │
└──────┬──────┘                              └────────┬────────┘
       │                                              │
  Unix Socket                                    Tokio 异步
  ~/.oxideterm/oxt.sock                          accept 循环
       │                                              │
  Named Pipe（Windows）                          methods.rs
  \\.\pipe\OxideTerm-CLI-{user}                  ├─ 状态/列表/健康类方法
                                                 ├─ 会话与标签操作方法
                                                 ├─ 转发与配置方法
                                                 └─ ask（流式 AI）
```

### 模块结构

**服务端**（`src-tauri/src/cli_server/`）：

| 模块 | 职责 |
|---|---|
| `mod.rs` | `CliServer` — 生命周期管理（启动、接受连接循环、关闭） |
| `transport.rs` | `IpcListener` / `IpcStream` — Unix Socket 与命名管道的抽象层 |
| `handler.rs` | 单连接请求循环，含读取上限与空闲超时 |
| `protocol.rs` | JSON-RPC 2.0 的 `Request`、`Response`、`RpcError`、`Notification` 类型 |
| `methods.rs` | RPC 方法分发与具体实现 |

**客户端**（`cli/src/`）：

| 模块 | 职责 |
|---|---|
| `main.rs` | Clap CLI 入口，命令定义 |
| `connect.rs` | `IpcConnection` — 同步 IPC 客户端（Unix Socket / 命名管道） |
| `protocol.rs` | 客户端 JSON-RPC 请求/响应类型 |
| `output.rs` | `OutputMode` — 人类可读或 JSON 输出格式化 |

**GUI 集成**（`src-tauri/src/commands/cli.rs`）：

| 命令 | 职责 |
|---|---|
| `cli_get_status` | 返回打包/安装状态供设置界面使用 |
| `cli_install` | 在 Unix 上创建符号链接，在 Windows 上复制二进制到安装路径 |
| `cli_uninstall` | 删除已安装的 CLI 二进制文件 |

## 通信协议

### 传输格式

- **传输层**：Unix Domain Socket 或命名管道
- **帧格式**：换行符分隔 JSON（每行一个 JSON 对象，以 `\n` 结尾）
- **编码**：UTF-8

### JSON-RPC 2.0

请求示例：

```json
{"id": 1, "method": "status", "params": {}}
```

成功响应：

```json
{"id": 1, "result": {"version": "0.21.0", "sessions": 5, "connections": {"ssh": 3, "local": 2}, "pid": 12345}}
```

错误响应：

```json
{"id": 1, "error": {"code": -32601, "message": "Method not found: foo"}}
```

### 错误码

| 错误码 | 常量 | 含义 |
|---|---|---|
| `-32600` | `ERR_INVALID_REQUEST` | JSON-RPC 请求格式错误 |
| `-32601` | `ERR_METHOD_NOT_FOUND` | 未知方法名 |
| `-32602` | `ERR_INVALID_PARAMS` | 方法参数无效 |
| `-32603` | `ERR_INTERNAL` | 内部服务器错误 |
| `1001` | `ERR_NOT_CONNECTED` | 无活跃连接（保留） |
| `1003` | `ERR_TIMEOUT` | 操作超时（保留） |

## 可用方法（22 个普通方法 + 1 个流式方法）

> 说明：`ask` 被归类为流式方法（服务端通过 `stream_chunk` 通知推送增量文本），其余为普通 JSON-RPC 请求/响应。`exec` 不是独立服务端 RPC，而是 CLI 端对 `ask + exec_mode=true` 的受控包装。

### 方法总览

| 分类 | 方法名 |
|---|---|
| 状态/探活 | `status`、`ping` |
| 列表/查询 | `list_saved_connections`、`list_sessions`、`list_active_connections`、`list_forwards`、`list_local_terminals`、`health` |
| 会话/标签操作 | `disconnect`、`connect`、`open_tab`、`focus_tab`、`attach` |
| 转发管理 | `create_forward`、`delete_forward` |
| 配置读取 | `config_list`、`config_get` |
| SFTP | `sftp_ls`、`sftp_get`、`sftp_put` |
| 导入 | `import_list`、`import_hosts` |
| AI（流式） | `ask` |

### `status`

返回 OxideTerm 实例的状态摘要。

**参数**：`{}`

**响应**：
```json
{
  "version": "0.21.0",
  "sessions": 5,
  "connections": {
    "ssh": 3,
    "local": 2
  },
  "pid": 12345
}
```

### `list_saved_connections`

返回所有已保存的连接配置（来自本地加密配置存储，由 `connections.json` 提供 envelope）。

**参数**：`{}`

**响应**：
```json
[
  {
    "name": "prod-server",
    "host": "10.0.1.5",
    "port": 22,
    "username": "deploy",
    "auth_type": "key"
  }
]
```

> 注意：响应中永远不包含密码和私钥内容。

### `list_sessions`

返回 `SessionRegistry` 中所有活跃会话。

**参数**：`{}`

**响应**：
```json
[
  {
    "id": "abc123...",
    "name": "prod-server",
    "host": "10.0.1.5",
    "state": "active",
    "uptime_secs": 3600
  }
]
```

### `list_active_connections`

返回 `SshConnectionRegistry` 中所有活跃 SSH 连接。

**参数**：`{}`

**响应**：
```json
[
  {
    "id": "conn-456...",
    "host": "10.0.1.5",
    "port": 22,
    "username": "deploy",
    "state": "active",
    "ref_count": 2
  }
]
```

### `list_forwards`

列出端口转发规则，可按会话过滤。

**参数**：`{ "session_id": "abc123" }` 或 `{}`（列出所有会话的转发）

**响应**：
```json
[
  {
    "session_id": "abc123...",
    "id": "fwd-456...",
    "forward_type": "local",
    "bind_address": "127.0.0.1",
    "bind_port": 8080,
    "target_host": "localhost",
    "target_port": 80,
    "status": "active",
    "description": "Web server"
  }
]
```

### `health`

获取连接健康状态。

**参数**：`{ "session_id": "abc123" }`（单个会话）或 `{}`（所有会话）

**单个会话响应**：
```json
{
  "session_id": "abc123...",
  "status": "healthy",
  "latency_ms": 42,
  "message": "Connected • 42ms"
}
```

**所有会话响应**：
```json
{
  "abc123...": {
    "session_id": "abc123...",
    "status": "healthy",
    "latency_ms": 42,
    "message": "Connected • 42ms"
  },
  "def456...": {
    "session_id": "def456...",
    "status": "degraded",
    "latency_ms": 350,
    "message": "High latency: 350ms"
  }
}
```

> 健康状态枚举值：`healthy`、`degraded`、`unresponsive`、`disconnected`、`unknown`

### `disconnect`

断开指定会话。支持按会话 ID 或名称匹配。

**参数**：`{ "target": "abc123" }` 或 `{ "target": "prod-server" }`

**响应**：
```json
{
  "success": true,
  "session_id": "abc123..."
}
```

> 断开操作会依次执行：持久化终端缓冲区 → 停止端口转发 → 关闭 SSH 会话 → 清理 WebSocket 桥接 → 清理 SFTP 缓存 → 清理健康检查器 → 释放连接池。

### `ping`

连通性检查，立即返回。

**参数**：`{}`

**响应**：
```json
{"pong": true}
```

### `list_local_terminals`

返回本地终端会话列表（`local-terminal` 特性开启时可用）。

**参数**：`{}`

**响应示例**：
```json
[
  {
    "id": "local-123...",
    "shell_name": "zsh",
    "cwd": "/Users/name/project"
  }
]
```

### `connect`

根据保存的连接名称/ID/主机匹配并发起连接（在 GUI 中打开会话）。

**参数**：`{ "target": "prod-server" }`

### `open_tab`

打开一个新的本地终端标签页。

**参数**：`{ "path": "/path/to/workdir" }`

### `focus_tab`

聚焦一个已存在标签页（SSH 会话或本地终端）。

**参数**：`{ "target": "session-id-or-name" }`

### `attach`

附着到一个正在运行的会话（SSH/本地），进行终端镜像。

**参数**：`{ "session_id": "abc123" }`

**响应示例**：
```json
{
  "ws_url": "ws://127.0.0.1:55321",
  "ws_token": "single-use-token",
  "terminal_type": "ssh",
  "cols": 160,
  "rows": 48
}
```

### `create_forward`

为会话创建端口转发规则（local / remote / dynamic）。

### `delete_forward`

删除指定端口转发规则。

### `config_list`

列出连接分组及统计信息。

### `config_get`

查询单个连接详情（不返回密码或私钥内容）。

### `sftp_ls`

列出远端目录内容。

### `sftp_get`

从远端下载文件到本地路径。

### `sftp_put`

把本地文件上传到远端路径。

### `import_list`

列出 `~/.ssh/config` 中可导入的主机条目及其导入状态。

### `import_hosts`

把一个或多个 SSH config host 导入为 OxideTerm 已保存连接。

### `ask`（流式）

AI 提问接口。支持：

- `context`（stdin 管道上下文）
- `session_id`（附带终端缓冲区上下文）
- `model`、`provider` 覆盖
- `stream` 流式输出
- `conversation_id` 续聊

流式过程中服务端会发送：

```json
{"method":"stream_chunk","params":{"text":"..."}}
```

最终响应示例：

```json
{
  "text": "...",
  "model": "moonshot-v1-8k",
  "done": true,
  "conversation_id": "b1d0..."
}
```

### `exec` 不是独立 RPC

`oxt exec` 是 CLI 层的受控包装，不是单独的服务端方法。

当前行为是：

1. CLI 仍然调用 `ask`
2. 额外传入 `exec_mode=true`
3. 服务端根据 `exec_mode` 切换更偏命令/代码生成的系统提示
4. CLI 直接流式输出结果，不在本地执行任何生成内容

这条边界在 Phase 0 固化，后续增强不能把 `exec` 偷偷扩展成自动执行入口。

## CLI 使用说明

### 安装

CLI 工具已捆绑在 OxideTerm 应用包内，安装步骤：

1. 打开 OxideTerm → 设置 → 通用
2. 找到 **CLI 伴侣工具** 区块
3. 点击 **安装**

这将在 macOS/Linux 上创建符号链接，在 Windows 上复制二进制文件到安装路径。

**安装路径：**
- macOS/Linux：`~/.local/bin/oxt`
- Windows：`%LOCALAPPDATA%\OxideTerm\bin\oxt.exe`

请确保安装目录已添加到 `$PATH`。

### 诊断

安装或升级后优先运行：

```bash
oxt doctor
oxt doctor --json
```

`doctor` 会固定检查下面几类问题：

1. 当前 CLI 二进制路径
2. `PATH` 是否能命中 `oxt`
3. 当前实际生效的 socket / pipe 解析结果
4. endpoint 是否存在、类型是否正确
5. endpoint ownership 是否匹配当前用户（Unix）
6. GUI 是否可连接
7. CLI API 是否与 GUI 兼容

检查项状态固定为 `ok`、`warn`、`fail`。即使 GUI 没启动，`doctor` 也会继续输出本地诊断结果。

### 命令

```bash
# 诊断安装、PATH、endpoint 与兼容性
oxt doctor

# 查看 OxideTerm 状态
oxt status

# 列出已保存的连接
oxt list connections

# 列出活跃会话
oxt list sessions

# 列出所有端口转发
oxt list forwards

# 列出指定会话的端口转发
oxt list forwards <session-id>

# 查看所有会话健康状态
oxt health

# 查看指定会话健康状态
oxt health <session-id>

# 断开会话（按 ID 或名称）
oxt disconnect <session-id-or-name>

# 连通性检查
oxt ping

# AI 提问（默认流式）
oxt ask "explain this log"

# 从 stdin 管道上下文
echo "$(cat app.log)" | oxt ask "find root cause"

# 强制原始文本输出（不做 Markdown 渲染）
oxt ask --raw "show me command"

# 续聊
oxt ask --continue <conversation-id> "give me safer variant"

# 代码/命令生成模式（代码优先输出）
oxt exec "write a bash script to rotate logs"

# 按名称连接并在 GUI 打开
oxt connect prod-server

# 打开本地终端标签（可指定路径）
oxt open ~/work

# 聚焦标签
oxt focus <session-id-or-name>

# 附着镜像会话
oxt attach <session-id-or-name>

# 创建/删除端口转发
oxt forward add 8080:localhost:80 --session <session-id>
oxt forward remove <forward-id> --session <session-id>

# SFTP
oxt sftp ls --session <session-id> /var/log
oxt sftp get --session <session-id> /remote/file ./local-file
oxt sftp put --session <session-id> ./local-file /remote/file

# 从 ~/.ssh/config 导入
oxt import list
oxt import add my-prod my-staging
oxt import add --all

# 配置查询
oxt config list
oxt config get <connection-name-or-id>

# 显示版本
oxt version

# 生成 Shell 补全脚本
oxt completions bash
oxt completions zsh
oxt completions fish
oxt completions powershell
```

### 全局参数

| 参数 | 默认值 | 说明 |
|---|---|---|
| `--json` | 自动检测 | 强制 JSON 输出（默认：管道时输出 JSON，终端时输出人类可读格式） |
| `--timeout <ms>` | `30000` | IPC 超时时间（毫秒） |
| `--socket <path>` | 平台默认值 | 自定义 socket/管道路径（用于调试） |

### 退出码（Phase 0 最小契约）

Phase 0 先固定最小退出码契约，后续阶段可以细化，但不能破坏下面三项：

| 退出码 | 含义 | 当前来源 |
|---|---|---|
| `0` | 成功执行，或正常显示 help/version | 正常命令路径、Clap help/version |
| `1` | 命令运行期失败 | IPC 失败、兼容性错误、doctor 发现失败项、RPC 返回错误、目标不存在等 |
| `2` | CLI 参数或用法错误 | Clap 参数解析失败、未知子命令、缺少必需参数 |

说明：

1. Phase 0 只承诺这三个最小出口。
2. 后续阶段若细化运行期错误码，只能在 `1` 的基础上拆分，不得改变 `0` 和 `2` 的含义。

### 输出模式

CLI 会自动检测 stdout 是否为终端：

- **终端** → 人类可读的格式化表格
- **管道/重定向** → 紧凑 JSON（单行）

对于 AI 输出（`oxt ask`）：

- **TTY 且未指定 `--raw`**：输出完成后按 Markdown 渲染（ANSI）
- **管道/重定向 或 `--raw`**：原始文本输出

使用 `--json` 可强制始终输出 JSON。

### 示例

```bash
# 先做诊断
$ oxt doctor
Doctor summary: 5 ok, 2 warning(s), 0 failed

# 机器可读诊断
$ oxt doctor --json | jq '.items[] | select(.status == "fail")'

# 人类可读格式的状态信息
$ oxt status
OxideTerm v0.21.0
  Sessions:      5 active
  Connections:   3 SSH, 2 local

# JSON 格式输出（适合脚本使用）
$ oxt status --json
{"version":"0.21.0","sessions":5,"connections":{"ssh":3,"local":2},"pid":12345}

# 在脚本中列出连接
$ oxt list connections --json | jq '.[].host'
"10.0.1.5"
"192.168.1.100"

# 查看所有会话健康状态
$ oxt health
  SESSION        STATUS         LATENCY    MESSAGE
  abc123de...    healthy        42ms       Connected • 42ms
  def456gh...    degraded       350ms      High latency: 350ms

# 在监控脚本中检查是否有不健康的会话
$ oxt health --json | jq 'to_entries[] | select(.value.status != "healthy")'

# 列出端口转发
$ oxt list forwards
  SESSION    TYPE     BIND                     TARGET                   STATUS     DESC
  abc123de   local    127.0.0.1:8080           localhost:80             active     Web server
  abc123de   dynamic  127.0.0.1:1080           SOCKS5                   active     SOCKS5 Proxy

# SFTP：查看目录与传输文件
$ oxt sftp ls --session abc123 /var/log
$ oxt sftp get --session abc123 /var/log/app.log ./app.log
$ oxt sftp put --session abc123 ./nginx.conf /etc/nginx/nginx.conf

# 导入 ~/.ssh/config
$ oxt import list
$ oxt import add prod jumpbox
$ oxt import add --all

# 断开会话（按名称）
$ oxt disconnect prod-server
Disconnected session: abc123...

# 检查 OxideTerm 是否正在运行
$ oxt ping && echo "正在运行" || echo "未运行"

# 生成 zsh 补全并安装
$ oxt completions zsh > ~/.zfunc/_oxt

# 自定义超时时间
$ oxt status --timeout 5000

# AI：流式 + Markdown 终端渲染
$ oxt ask "explain pwd command"

# AI：用于脚本/管道时建议 raw
$ oxt ask --raw "generate curl command" | pbcopy

# AI：多轮续聊
$ oxt ask "summarize this" --provider openai
...
Conversation: b1d0...
$ oxt ask --continue b1d0... "give me concise version"

# Exec：仍然走 ask RPC，只是切换 exec_mode
$ oxt exec "write a bash script to rotate logs"

# Attach：会话镜像，`~.` 退出
$ oxt attach prod-server
```

### 环境变量

| 变量 | 平台 | 说明 |
|---|---|---|
| `OXIDETERM_SOCK` | macOS/Linux | 覆盖默认 socket 路径 |
| `OXIDETERM_PIPE` | Windows | 覆盖默认命名管道路径 |

`oxt doctor` 会把 `--socket` 或环境变量覆写直接显示在诊断结果中，方便排查“为什么命中了意外 endpoint”。

## 安全性

### IPC 传输安全

- **Unix Socket**：创建于 `~/.oxideterm/oxt.sock`，权限为 `0o600`（仅所有者可读写）
- **命名管道**：`\\.\pipe\OxideTerm-CLI-{username}`——作用域限定于当前用户
- **过期 socket 检测**：启动时服务端会探测已存在的 socket。若不可达则删除过期 socket；若可达则启动失败并提示「另一个 OxideTerm 实例正在运行」。

### 资源限制

| 限制项 | 数值 | 目的 |
|---|---|---|
| 最大并发连接数 | 16 | 防止资源耗尽（Semaphore 信号量） |
| 最大请求大小 | 1 MB | 有界行读取，防止内存滥用 |
| 最大响应大小 | 4 MB | 客户端 `.take()` 限制 |
| 空闲超时 | 60 秒 | 断开不活跃的客户端 |

AI 相关补充限制：

| 限制项 | 数值 | 说明 |
|---|---|---|
| `MAX_PROMPT_SIZE` | 50 KB | 单次 prompt 上限 |
| `MAX_CONTEXT_SIZE` | 500 KB | stdin 上下文上限 |
| `MAX_TERMINAL_BUFFER_LINES` | 2000 行 | 会话缓冲区注入上限 |
| 流式读取超时 | 180 秒 | `oxt ask` 客户端读取超时 |

### 数据暴露原则

- 任何 RPC 方法**均不**返回密码
- `list_saved_connections` 只暴露私钥文件路径，不包含私钥内容
- IPC 服务端仅绑定到本地 IPC，不开放任何网络可访问端口

## 构建与分发

### 本地构建

```bash
# 编译检查
cd cli && cargo check

# 正式构建（体积优化）
cd cli && cargo build --release
```

Release 配置使用 `lto = true`、`strip = true`、`opt-level = "z"`，最终二进制约 1 MB。

### CI 集成

CLI 在 GitHub Actions 中自动为全部 6 个平台目标构建并打包：

1. **构建步骤**：在 `cli/` 目录执行 `cargo build --release --target ${{ matrix.target }}`
2. **复制**：二进制放入 `src-tauri/cli-bin/`
3. **打包**：`tauri.conf.json` 的 `bundle.resources` 包含 `"cli-bin/*"` glob，Tauri 自动将匹配到的 CLI 二进制打包为资源
4. **分发**：Tauri 将 CLI 二进制包含在最终的 `.app` / `.deb` / `.exe` 安装包中

本地 `pnpm tauri dev` 时 `cli-bin/` 目录不存在，glob 匹配零个文件，不会报错。

### 支持的目标平台

| 平台 | 构建目标 | 二进制名 |
|---|---|---|
| macOS（ARM） | `aarch64-apple-darwin` | `oxt` |
| macOS（Intel） | `x86_64-apple-darwin` | `oxt` |
| Linux（x64） | `x86_64-unknown-linux-gnu` | `oxt` |
| Linux（ARM） | `aarch64-unknown-linux-gnu` | `oxt` |
| Windows（x64） | `x86_64-pc-windows-msvc` | `oxt.exe` |
| Windows（ARM） | `aarch64-pc-windows-msvc` | `oxt.exe` |

### 版本同步

CLI 版本通过 `pnpm version:bump` 与 OxideTerm 保持同步，该脚本会同步更新 `cli/Cargo.toml`、`package.json`、`src-tauri/Cargo.toml` 和 `src-tauri/tauri.conf.json`。

## GUI 集成

### 设置界面

CLI 伴侣工具区块位于 设置 → 通用：

- **状态徽章**：显示「已安装」、「未安装」或「未打包」
- **安装按钮**：将内置 CLI 符号链接/复制到 `~/.local/bin/oxt`
- **卸载按钮**：删除已安装的 CLI 二进制文件
- **自动检测**：打开「通用」选项卡时自动刷新状态

### Tauri 命令

| IPC 命令 | 说明 |
|---|---|
| `cli_get_status` | 返回 `{ bundled: bool, installed: bool, install_path: string }` |
| `cli_install` | 将内置资源中的 CLI 安装到安装路径 |
| `cli_uninstall` | 删除已安装的 CLI 二进制文件 |

### 资源解析顺序

`find_bundled_cli()` 按以下顺序查找 CLI 二进制：

1. Tauri 资源路径：`cli-bin/{binary_name}`（来自 `tauri.conf.json` 静态配置）
2. 直接资源路径：`{binary_name}`（备用）
3. 主程序同目录：`{exe_dir}/{binary_name}`（开发环境备用）

## 服务端生命周期

### 启动

CLI 服务端在 OxideTerm 的 Tauri `setup` 回调中自动启动：

```
应用启动 → setup() → CliServer::start(app_handle) → 生成 accept 循环
```

启动失败不影响主程序：若 IPC 服务端绑定失败（例如另一个实例正在运行），GUI 照常运行，错误仅记录日志。

### 关闭

应用退出时，CLI 服务端优雅关闭：

```
应用退出 → server.shutdown() → oneshot 信号 → accept 循环退出 → socket 清理
```

关闭采用 `Mutex<Option<oneshot::Sender>>` 模式，确保单次信号的可靠传递。

### 连接处理

每个 CLI 连接在独立的 Tokio 任务中运行：

```
接受连接 → Semaphore 许可 → 生成任务 → 读取/分发/响应循环 → 释放许可
```

Semaphore 将并发连接数限制为 16，超出的连接会被立即拒绝。

## Shell 补全

`oxt` 通过 `clap_complete` 内置了 Shell 补全脚本生成功能，支持 bash、zsh、fish 和 PowerShell。

### 安装补全

**Bash**：
```bash
oxt completions bash > ~/.local/share/bash-completion/completions/oxt
```

**Zsh**：
```bash
# 确保 ~/.zfunc 在 fpath 中（在 .zshrc 中添加 fpath=(~/.zfunc $fpath)）
oxt completions zsh > ~/.zfunc/_oxt
```

**Fish**：
```bash
oxt completions fish > ~/.config/fish/completions/oxt.fish
```

**PowerShell**：
```powershell
oxt completions powershell >> $PROFILE
```

安装后重新加载 Shell 即可使用 Tab 补全。

## 未来规划

后续版本可能新增的功能：

- `oxt transfer <name> <local> <remote>` — SFTP 文件传输
- `oxt config set` — 命令行修改连接配置
- `oxt chat` — 交互式多轮 REPL（当前为 `ask --continue`）
- 服务端主动推送通知（事件流）
- 动态 Shell 补全（实时查询连接名、会话 ID）
