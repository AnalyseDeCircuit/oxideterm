# OxideTerm 公开 MCP 服务端工具契约

> 状态：实施契约；服务端按本文的阶段和兼容边界逐步交付。
>
> 基线盘点：2026-08-15；目标协议为 MCP `2026-07-28`，兼容旧版 MCP 客户端。
>
> 本文的“公开”是产品公开、供外部 MCP 客户端使用的稳定接口，不是把用户桌面的端口暴露到互联网。

## 1. 决策、方向与边界

OxideTerm 应当作为 **MCP 服务端**，让经过用户授权的 Codex、Claude Desktop、IDE Agent 或其他兼容客户端调用本机正在运行的 OxideTerm。客户端是调用者；OxideTerm 是资源、会话与操作的唯一所有者。

这项能力必须与仓库内已有的两条相反或更窄的路径严格分开：

| 路径 | 当前职责 | 与本文的关系 |
|---|---|---|
| `oxideterm-ai` 的 `McpRegistry` | OxideTerm 的 AI 功能连接外部 MCP 服务 | 方向相反；不复用其连接、凭据或授权。 |
| `oxideterm-acp-host-tools` | 一个 ACP 会话临时拥有的回环 MCP 桥接 | 仅服务该 ACP 会话；不升级为公开服务端，也不共享其令牌。 |
| 本文的 Public MCP | 外部客户端调用本机 OxideTerm 的受控产品能力 | 新增独立网关、授权账本、句柄转换和审计边界。 |

设计参考了 Navop 已公开的能力范围：连接、活动会话、终端、文件、传输和工具组按需开放；但工具名、描述、分组、参数、协议、数据模型和实现边界均由 OxideTerm 独立制定，不能复制其实现或接口形态。

能力范围的外部参考是 [Navop 的 Public MCP 使用说明](https://docs.navop.dev/en-US/guide/public-mcp)；该参考只用于识别产品域，不构成本契约的接口或安全模型来源。

### 1.1 成功标准

在用户已授权对应客户端、工具组和具体操作后，客户端可以完成真实自动化闭环，例如：

1. 找到一个已保存 SSH 连接，建立一个由 NodeRouter 拥有的节点；
2. 打开终端或 SFTP 消费者，读取所需结果并发送实际输入；
3. 上传、验证、原子替换一个远端配置文件，获得可用时的撤销句柄；
4. 建立一个端口转发，使其在终端消费者关闭后继续运行；
5. 预览云同步差异、在应用内确认后应用或回滚；
6. 查看 RDP/VNC 帧、发送键鼠输入及按授权同步剪贴板。

安全控制不能把这些已获授权的核心工作流删成只读演示。它们使用默认关闭的工具组、按客户端授权、不可变操作确认、可审计结果和可撤销句柄来约束。

### 1.2 硬边界

以下边界不提供例外授权：

- 不返回已存在的密码、私钥、口令、令牌、认证头、受保护存储的原值或云同步密钥。
- 不暴露 `NodeId`、连接池键、`TabId`、GPUI `Entity`、Tokio task、helper PID、SSH/SFTP transport、OS keychain account 等内部身份或对象。
- 不提供“调用任意内部函数”“执行任意插件方法”“转发任意 helper JSON”的工具。插件能力只经产品级、声明式的适配器暴露。
- 不把外部客户端的文件路径、命令文字、终端内容、剪贴板、截图或错误对象直接写入日志、遥测、诊断包或审计正文。
- 不因关闭一个终端、SFTP、IDE、Host Tools 或 MCP 客户端消费者而隐式断开共享 SSH 物理节点。

### 1.3 非目标

- 第一阶段不提供公网监听、局域网发现或远程控制中继。未来若需要远程入口，必须另立 TLS、设备身份、网络 ACL 和威胁建模设计，不能把本机令牌复用于网络服务。
- 不把所有 Rust crate 的 public API 变成 MCP API。
- 不替代 OxideTerm 的 GUI：需要人机验证、设备所有者认证或显示许可时，应用负责显示交互；MCP 只接收可验证的结果。
- 不承诺虚假的跨协议能力。Mosh、Telnet、串口和本地终端只暴露它们实际拥有的终端能力；本地终端绝不成为可保存连接。

### 1.4 当前已落地的首个垂直切片

当前代码已经实现回环 Streamable HTTP 入口，并将主要自动化领域接入真实产品所有者：

- 设置页可创建、停用和撤销独立外部客户端；一次性 Bearer 凭据只向用户显示一次，设备端只持久化 SHA-256 摘要。
- 每个客户端独立选择普通模式或完全权限模式，并逐项启用工具组。普通模式保留应用内动作批准；完全权限模式只跳过已勾选工具组的逐动作批准，不会绕过 Bearer 认证、应用锁、秘密硬边界、审计或未授权工具组。
- `tools/list` 按客户端工具组裁剪；当前实际发布基础、连接目录/详情/管理、凭据管理、节点租约、真实终端会话/观察/输入/录制、RDP/VNC 会话/画面/键鼠/剪贴板、命令执行/观察、临时 artifact、后台 SFTP 传输、远端 IDE 工作区、当前客户端审计、类型化 Host Tools、快速命令、插件生命周期、端口转发、SFTP 文件和云同步工具。
- `connections_browse` 已覆盖保存的 SSH、串口、Telnet、Mosh、RDP 和 VNC 配置，只返回目录投影；精确端点及协议选项由单独授权的 `connections_describe` 返回，既有凭据只显示存在性。
- `connections_save` 使用严格 tagged profile 写入上述六类真实保存配置，更新必须携带 `connections_describe` 返回的 revision；schema 不包含秘密字段，本地终端没有保存分支。`connections_remove` 遇到受保护凭据时要求显式 `forget_credentials=true`，因此不会静默删除秘密或制造无法管理的孤儿引用。
- `credentials_status` 只返回槽位、认证类型、可写性和存在标记；`credentials_store` 把新的零化输入直接交给连接受保护存储，`credentials_forget` 删除指定槽位。三者都不返回既有值或内部存储引用，更新凭据也不会污染最近连接顺序。
- Telnet profile 已作为独立分区进入 `.oxide` 预览/导入/导出和结构化云同步。SSH 上游代理的设备本地受保护存储引用在构造云快照时剥离，应用远端记录时只保留本机目标完全匹配的引用。
- 连接、节点和命令句柄均为随机且绑定客户端的外部引用；首个切片会在应用重启后重新生成连接引用，客户端需要重新浏览目录，不能缓存或构造内部连接身份。
- `nodes_connect` 使用保存的 SSH 配置和现有受保护凭据，通过 NodeRouter 建立或复用物理节点；代理链也沿用现有节点树展开与连接顺序。
- `commands_start` 使用当前物理 SSH 连接的独立 exec channel，返回真实退出码、有界输出和可取消 `command_ref`。
- `artifacts_stage`/`artifacts_read` 使用客户端隔离、容量受限且会过期的临时文件，不接受任意本机路径；`mcp_audit_search` 只能读取调用客户端自身的脱敏记录。
- `hosttools_catalog`/`hosttools_capture`/`hosttools_operate` 只接产品已有的类型化资源与固定动作，不暴露自由 shell 或插件调用。
- 快速命令目录和正文分别授权；保存和删除采用存储 revision 冲突检查并刷新应用内状态。执行只读取已保存的精确命令，并再次校验 revision、节点所有权和 `host_pattern`。当前模型没有参数 schema，因此 `arguments` 必须为空；当前执行目标仅支持 `node_ref`，终端目标随终端句柄阶段接入。
- 插件目录和插件管理分别授权。插件安装只接受客户端自己的临时 artifact、必填 SHA-256 和预期插件身份，在 staging 阶段核对身份后才替换安装目录；启停和卸载复用应用已有注册表及运行时停用路径。返回值只含 manifest 公开身份、状态、权限请求和声明式贡献数量，绝不返回安装路径、配置路径、任意 `api.invoke`、运行时命令或 Host Monitor 命令正文。
- 端口转发读取和管理分别授权。`forwards_*` 复用应用唯一的 `ForwardingRuntimeService` 和独立 `PortForward` 消费者，支持类型化创建、带 revision 的受控修改、停止、重启、删除、统计和单次端口发现。关闭终端或释放 Public MCP 节点租约不会停止转发；显式物理节点断开、客户端撤销或关闭转发管理授权才按所有权清理。外部只看到客户端作用域的 `forward_ref`。
- SFTP 文件读取和修改分别授权。`files_open` 为规范化远端根目录登记独立 SFTP 消费者，后续列表、元数据、分段读取、比较、写入、移动和删除都重新校验规范化路径边界。正文只经客户端自己的有界 artifact 传递；重连时先取得当前连接的消费者再释放旧消费者，`files_close` 不断开共享 SSH 节点。
- `transfers_start` 在同一授权根内启动真实后台 SFTP 单文件上传或下载，除传输数据组外，上传还要求远端文件写入组，下载还要求远端文件读取组。进度和取消复用应用的传输控制器。上传源与下载产物都只能是客户端私有 artifact，本机临时路径不会进入结果；客户端撤销、工具组关闭、文件会话关闭或物理节点断开会取消仍在运行的传输。当前 artifact 数据面上限为 64 MiB，尚未保留可跨请求重启的部分文件，因此 `resume=true` 会明确拒绝；上传失败或取消时会如实报告可能存在远端部分文件。
- IDE 工作区读取和结构化编辑分别授权。`workspaces_mount` 必须从客户端已有的 `file_session_ref` 派生，并再次规范化项目根；它创建独立的 Node Agent/SFTP IDE owner，不借用可见编辑器标签页，也不暴露 NodeId、TabId 或 GPUI Entity。树、文本读取和搜索都有硬上限；编辑使用编辑器核心校验 UTF-8 字节区间，并将客户端先前观察到的 revision 映射回真实 `SavedFileVersion` 做冲突检测。多文件写入不是服务端原子事务，后续文件失败时会尝试用刚写入的版本回滚前项，并明确报告回滚是否完整，不虚构持久 `undo_ref`。关闭工作区、父文件会话、客户端授权或物理节点只释放该工作区自己的 IDE consumer。
- 终端会话、内容观察和输入控制分别授权。`terminals_open` 只创建应用真实持有的可见终端页：SSH 必须使用已取得的 `node_ref`，保存的 Mosh、Telnet 和串口使用 `connection_ref`，本地终端只允许一次性启动且不会进入保存、导出或同步。读取仅返回有界屏幕快照与 generation cursor；搜索复用终端后端；输入正文不进入审计，普通模式批准页只显示输入类型、长度与回车意图。关闭 SSH 终端只关闭自己的 terminal consumer，不等同于物理节点断开。
- 终端录制控制和录制内容分别授权。`recordings_control` 复用真实 `TerminalPane` recorder，同一终端不会覆盖既有应用录制；当前后端只支持 output-only，因此 `capture_input=true` 会明确拒绝。停止时保留有界 asciicast 正文，搜索只给有界片段，导出只生成客户端私有且会过期的 artifact，不接受任意本机路径。终端关闭或控制授权关闭会先停止活动录制；客户端撤销会零化保留正文并撤销导出 artifact。
- 远程桌面会话、画面观察、键鼠控制和剪贴板分别授权。`desktops_open` 只解析保存的 RDP/VNC `connection_ref`，凭据由设备受保护存储直接移交真实 provider，会话仍由可见标签页和 `RemoteDesktopSessionEntity` 持有。隐藏标签页只有在客户端启用画面观察时才继续消费最新帧；`desktops_frame` 把有界 CPU framebuffer 在后台编码成客户端私有 PNG artifact。输入必须携带当前 graphics epoch，坐标按 server framebuffer 校验；远端剪贴板来自会话内零化缓存，绝不读取系统剪贴板冒充远端内容。关闭或撤权会释放所有输入、关闭 helper，并撤销该桌面的帧和剪贴板 artifact。
- 云同步按配置范围或调用方显式选择的分区冻结 pull/publish 预览；`sync_plan_ref` 绑定客户端、十分钟过期且只能消费一次。应用前同时比较完整本地状态和远端 revision/etag/content hash，任一变化都会拒绝陈旧计划。pull 只在能精确恢复的分区返回十五分钟有效的 `undo_ref`；SSH、Mosh 和凭据分区仍可保留产品已有的加密本地恢复备份，但不会把它伪装成严格撤销。publish 是远端写入，成功后明确不返回撤销句柄。
- 连接和命令执行、物理节点断开先冻结原参数并进入应用内批准；批准票据绑定客户端、五分钟过期且只能消费一次。批准界面显示实际客户端、目标和命令，但命令不进入 MCP 批准结果或审计。
- 停用或撤销客户端会撤销待批准动作、取消命令并释放其 Public MCP 消费者；`nodes_release` 不会把其他终端、SFTP 或转发消费者仍在使用的物理节点断开。
- 应用锁定时会拒绝新的 MCP 领域请求、撤销待批准动作、取消 MCP 命令并释放 MCP 节点消费者；解锁后客户端凭据仍有效，但必须重新取得运行时句柄。

stdio bridge 仍按后续阶段实施。已接入领域尚未支持的目标或动作会明确拒绝；其余领域在真实 broker 和生命周期接通前不会出现在 `tools/list`，也不会用空壳结果冒充支持。

## 2. 服务形态与协议

### 2.1 传输和发现

公开服务端建议由新的应用内 `PublicMcpRuntime` 统一拥有，提供两种等价入口：

| 入口 | 使用情形 | 约束 |
|---|---|---|
| 受管 stdio bridge | MCP 客户端只能启动子进程 | bridge 从受保护的本地发现记录取得一次性连接材料，随后把 stdio 转给应用；它不持有业务权限。 |
| 回环 Streamable HTTP | 长驻客户端或原生 MCP 配置 | 仅绑定 `127.0.0.1`、`::1` 或受当前用户保护的本地 socket；每个客户端配置有独立连接材料。 |

启用 Public MCP 后，应用为每个已批准客户端生成独立的 `client_ref` 和不可导出的连接凭据。凭据只写入应用生成的客户端配置、受保护发现记录或 OS 凭据存储；不会出现在工具结果、审计、日志、终端或同步对象里。停用客户端、轮换凭据、退出应用或撤销客户端会立即使相关会话失效。

MCP `2026-07-28` 请求使用 `server/discover` 做可选发现，并在每次请求的 `_meta.io.modelcontextprotocol/*` 中携带协议版本、客户端信息和能力。Streamable HTTP 同时校验 `Mcp-Method`、`Mcp-Name` 与正文一致；回环监听必须校验 Host、Origin 和对具体客户端签发的认证材料。兼容旧版客户端时才进入 `initialize` / `initialized` 生命周期，旧连接不得改变新的授权模型。

客户端声明的名称、版本、PID/启动方式等信息只能用于展示和审计，不能单独构成信任。每个请求都从认证材料解析 `client_ref`，再把声明身份与已批准客户端记录核对；首次连接或身份变化时由应用重新确认绑定。应用生成的发现记录、令牌和授权账本是设备本地安全状态，不进入 `.oxide` 导入导出、云同步、普通设置导出或支持诊断包。

### 2.2 工具发现

服务端不能把全部高风险工具无差别放进 `tools/list`。工具可见性为：

```text
服务端已启用
  ∩ 当前客户端已获工具组授权
  ∩ 当前策略允许的传输和平台能力
  ∩ 当前应用实际可用的功能
```

基础目录工具始终可见；其余工具组由用户对具体客户端授予后才出现在 `tools/list`。客户端可调用 `mcp_catalog` 获得未开放组的名称、风险级别、原因和请求方式，但不会得到隐藏工具的完整 schema。`tools/list` 必须按稳定顺序返回，并使用私有 cache scope 和与授权有效期一致的 `ttlMs`。授权变化后，支持新协议的客户端通过订阅或重新拉取目录观察变化；兼容旧协议时发送 `notifications/tools/list_changed`。

### 2.3 统一请求和结果信封

每个工具的业务参数外，以下通用字段按需适用：

| 字段 | 含义 |
|---|---|
| `request_key` | 客户端生成的幂等键；创建、写入、上传和提交操作必须支持。 |
| `operation_ref` | 继续查询、等待或取消一个异步操作的外部句柄。 |
| `approval_ref` | 对一份不可变动作摘要的一次性批准票据；不能与新参数混用。 |
| `expected_revision` | 乐观并发条件；文件、配置、同步计划和快速命令写入应优先使用。 |
| `dry_run` | 仅生成计划、冲突、影响范围和所需授权；绝不产生副作用。 |

工具结果的 `structuredContent` 使用以下稳定形状；另附简短文本以兼容只消费文本的 MCP 客户端：

```json
{
  "outcome": "completed | queued | approval_required | conflict | rejected | failed",
  "operation_ref": "op_…",
  "approval_ref": "apr_…",
  "undo_ref": "undo_…",
  "revision": "rev_…",
  "data": {},
  "warnings": [],
  "error": { "code": "…", "message": "…", "retryable": false }
}
```

`operation_ref` 只表示服务端跟踪的工作，不等价于内部任务句柄。长时间工作必须脱离 MCP 请求生命周期，由应用运行时保留取消、完成和清理所有权。

### 2.5 内容传递与状态变化

第一版以版本 cursor、`mcp_operation` 的有界等待和再次读取为可靠状态获取方式；客户端不得假定自己一定能接收自定义 server notification。服务端在 MCP `2026-07-28` 下可声明官方 Tasks/Subscriptions 扩展，但 `operation_ref` 始终是产品级事实来源，不依赖扩展才能查询或取消。Node、terminal、command、forward、transfer、desktop 与同步的内部事件会更新各自的 revision/cursor，但不直接把内部事件 DTO 变成公共协议。

二进制和大内容必须作为 MCP image content 或带大小/过期限制的 `artifact_ref` 返回。画面、录制、文件和剪贴板不能以无限制 base64 JSON 塞入普通结果。`artifacts_read` 分段读取，且所有 artifact 都继承产生它的客户端和内容授权范围。

### 2.4 确认、提交和撤销

高风险工具的首次调用只固化动作摘要，返回 `approval_required` 和 `approval_ref`。应用内确认页显示客户端、目标、影响、是否可撤销、敏感参数的存在性及经脱敏后的摘要。用户在应用中批准后，客户端调用 `mcp_commit_action`，只传入该 `approval_ref`，服务端才执行原始、不可变的动作。新协议的 MRTR `input_required` 可改善客户端交互，但不能代替设备所有者在 OxideTerm 内的批准。

批准票据默认五分钟有效、仅能提交一次，并在客户端断开、权限撤销、目标 revision 变化或应用锁定时失效。`mcp_commit_action` 不接受替换后的命令、路径、内容或目标，因此不能把一次确认挪作另一项操作。

可真实回滚的副作用返回 `undo_ref`；`mcp_revert` 会在必要时再次确认。不能可靠回滚的操作必须显式标为 `non_reversible`，并在确认页中说明。例如已经发送的终端输入不能假装可撤销，而带保留备份的单文件替换可以。

## 3. 外部句柄与生命周期

所有 `*_ref` 都是服务端随机生成、带版本和客户端作用域的外部标识。它们不可推导、不可跨客户端使用，不编码内部 ID，也不得由客户端猜测或构造。

| 句柄 | 指向 | 有效期与失效 | 重要语义 |
|---|---|---|---|
| `client_ref` | 已批准的外部客户端 | 用户撤销、轮换或删除时失效 | 不是认证秘密。 |
| `connection_ref` | 已保存连接的安全投影 | 记录删除、权限变更时失效 | 不等于连接记录 UUID；可映射为多个运行时节点。 |
| `node_ref` | NodeRouter 拥有的 SSH 逻辑节点 | 显式断开、租约释放、节点失败或客户端撤销 | 指向共享物理节点而不是某个终端。 |
| `terminal_ref` | 一个终端消费者 | 关闭、后端退出、租约释放时失效 | 关闭它只移除消费者，绝不等价于 `node_disconnect`。 |
| `recording_ref` | 一个客户端拥有的终端录制 | 客户端撤销或有界句柄回收时失效 | 活动录制绑定 `terminal_ref`；停止后正文与终端生命周期分离，内容授权关闭时不可读取。 |
| `command_ref` | 一次受跟踪的命令执行 | 完成后保留只读状态至审计保留期 | SSH exec 可提供真实退出码；交互终端按 Shell Integration/命令标记报告可靠性。 |
| `file_session_ref` | 一个 NodeRouter 取得的 SFTP/IDE 消费者 | 节点断开、会话关闭、客户端撤销时失效 | 通过真实 SFTP channel，而不是 shell 兼容路径。 |
| `workspace_ref` | IDE 工作区视图 | 挂载关闭、来源文件会话失效时失效 | 不是 GPUI 编辑器实体。 |
| `transfer_ref` | 有进度、可取消的传输 | 完成后保留只读状态至审计保留期 | 传输的文件数据另由 artifact 句柄管理。 |
| `forward_ref` | 端口转发规则和其 listener/bridge 所有者 | 删除、所属节点断开或撤销时失效 | 终端关闭后仍存活；显式停止或节点断开才终止。 |
| `desktop_ref` | RDP/VNC 实时会话 | 关闭、provider 终止或授权撤销时失效 | 不暴露 helper 进程或 framebuffer 内部对象。 |
| `sync_plan_ref` | 带远端 revision 的同步预览 | 远端/本地变更、过期或应用后失效 | 只能按预览内容应用。 |
| `artifact_ref` | 有大小和时间限制的临时输入/输出内容 | 消费、过期或客户端撤销时删除 | 用于大文件、导出和截图，不能指向任意本地路径。 |
| `approval_ref` / `undo_ref` | 不可变待批准动作 / 可回滚记录 | 单次、过期、对象变更或撤销时失效 | 只适用于原动作与原客户端。 |
| `operation_ref` | 后台操作状态 | 取消、完成并过期或客户端撤销时失效 | 可查询、等待、取消；不是系统任务 ID。 |

### 3.1 节点和消费者的必守规则

当 `nodes_connect` 成功后，Public MCP 注册一个明确的 `McpNodeLease`；该租约是 NodeRouter 的消费者/所有者记录，而不是终端 pane 的替身。随后 `terminals_open`、`files_open`、`forwards_open` 和 Host Tools 各自再注册相应消费者。

```text
PublicMcpRuntime 的节点租约
        │
        ▼
NodeRouter / connection registry ── 物理 SSH node
        ├── terminal_ref 消费者
        ├── file_session_ref 消费者
        ├── forward_ref listener 与 bridge
        ├── Host Tools 采样或操作
        └── reconnect / health 所有者
```

- `terminals_close`、`files_close`、`workspaces_close` 只释放自己的消费者。
- `nodes_disconnect` 才要求 NodeRouter 按父子拓扑断开物理节点并停止相关转发、SFTP 与子节点工作。
- `nodes_release` 只释放 Public MCP 的节点租约；若仍有其他产品消费者，节点继续存活。若该租约是最后一个明确 owner，应用按节点策略进行有界清理，绝不由 pane 关闭偶然触发。
- 重连、健康检查、forward listener、helper process 和传输都必须由应用/节点/会话记录持有取消与完成句柄；MCP HTTP 请求结束不能销毁它们。

## 4. 完整工具目录

表中的权限写法为 `工具组 / 权限等级 / 确认`。等级含义：`D` 是脱敏默认读取，`R` 是敏感读取，`W` 是普通写入，`X` 是高风险且必须确认。所有写入仍会审计。没有列出的工具不属于公开契约。

### 4.1 服务、授权、操作和审计

| 工具 | 权限 | 关键参数 | 关键结果与约束 |
|---|---|---|---|
| `mcp_overview` | 基础 / D / 否 | 无 | 服务版本、传输状态、当前客户端可见工具组、平台能力、受限原因；不含端口或令牌。 |
| `mcp_catalog` | 基础 / D / 否 | `group?` | 当前可用工具的 schema 摘要，及未开放组的用途/风险/请求条件。 |
| `mcp_request_access` | 基础 / W / 应用内批准 | `groups`、`scope`、`duration`、`reason` | 创建客户端授权请求；结果为 `approval_ref`，用户批准后才改变 `tools/list`。 |
| `mcp_access_state` | 基础 / D / 否 | `approval_ref?` | 客户端自己的已授予、待处理、过期与拒绝授权。 |
| `mcp_revoke_access` | 基础 / W / 应用内批准 | `grant_ref` | 撤销该客户端自己的可撤销授权，立即收回相关句柄。 |
| `mcp_commit_action` | 基础 / X / 已批准票据 | `approval_ref` | 提交一份不可变高风险动作；返回 `operation_ref`、结果或 `undo_ref`。 |
| `mcp_operation` | 基础 / D / 否 | `operation_ref`、`wait_ms?` | 阶段、进度、可取消性、脱敏错误与产出句柄；`wait_ms` 有上限。 |
| `mcp_cancel_operation` | 基础 / W / 否 | `operation_ref` | 请求有界取消；返回是否已产生不可逆副作用及可能的 `undo_ref`。 |
| `mcp_revert` | 基础 / X / 应用内确认 | `undo_ref`、`request_key` | 只回滚记录的原始动作；目标 revision 不匹配时返回冲突。 |
| `mcp_audit_search` | 审计读取 / R / 否 | `time_range`、`tool?`、`target_ref?`、`cursor?` | 客户端自身的审计记录：动作、批准、状态、参数摘要哈希和结果摘要；不返回秘密或原始终端/文件内容。 |

### 4.2 连接、凭据和 SSH 节点

| 工具 | 权限 | 关键参数 | 关键结果与约束 |
|---|---|---|---|
| `connections_browse` | 连接目录 / D / 否 | `query?`、`connection_types?` | SSH、Mosh、Telnet、串口、RDP/VNC 的 `connection_ref`、显示名、传输类型、分组、标签和最近使用时间；不返回精确端点或凭据。 |
| `connections_describe` | 连接读取 / R / 否 | `connection_ref` | 经授权的主机/端口、用户名、跳板与代理元数据、保存转发摘要；秘密字段仅以存在标记表示。 |
| `connections_save` | 连接管理 / W / 确认新建或覆盖 | `connection_ref?`、`profile`、`expected_revision?` | 新建或更新可保存的 SSH、Mosh、Telnet、串口、RDP/VNC 配置；返回新 `connection_ref`、revision 和可用时 `undo_ref`。`profile` 中的秘密字段一律拒绝，必须使用 `credentials_store`；`local` 一律拒绝保存。 |
| `connections_remove` | 连接管理 / X / 必须 | `connection_ref`、`forget_credentials?` | 删除保存配置及其公开映射；若存在受保护凭据，默认拒绝删除，只有显式设为 `true` 才同时遗忘凭据，避免静默删除或留下孤儿引用。 |
| `credentials_status` | 凭据管理 / D / 否 | `connection_ref` | 各认证槽是否存在、来源是否为受保护存储、最后更新摘要；不返回值或内部存储键。 |
| `credentials_store` | 凭据管理 / X / 必须 | `connection_ref`、`slot`、`new_secret` | 将新值直接写入指定受保护存储槽，并尽快归零输入；结果只报告 `stored`。不允许读取或替换为任意内部引用。 |
| `credentials_forget` | 凭据管理 / X / 必须 | `connection_ref`、`slot` | 删除指定受保护槽；返回影响的配置摘要。 |
| `nodes_connect` | 节点会话 / X / 必须 | `connection_ref` 或短暂 `ssh_profile`、`lease`、`request_key` | 通过 NodeRouter 建立/复用物理 SSH node，返回 `node_ref`、状态、协商能力和认证等待状态。短暂 profile 不会自动保存。 |
| `nodes_inspect` | 节点会话 / R / 否 | `node_ref` | readiness、父子拓扑的外部引用、消费者计数、SFTP/转发可用性、重连状态；不返回 NodeId、pool key 或 transport。 |
| `nodes_release` | 节点会话 / W / 否 | `node_ref` | 释放该客户端的节点租约；其他消费者仍可保留节点。 |
| `nodes_disconnect` | 节点会话 / X / 必须 | `node_ref`、`cascade` | 显式断开该节点，按记录的父子关系级联并撤销依赖句柄；不是关闭某个终端的别名。 |

`credentials_store` 接受新的秘密是为了让用户已授权的自动化能够完成配置和连接；它绝不提供相反方向的读取工具。连接时优先让应用经受保护 broker 使用已有秘密。若需要键盘交互认证，结果进入 `waiting_for_user_auth`，由应用拥有的认证界面完成，不将旧秘密回传给 MCP 客户端。

### 4.3 终端与非 SSH 终端

| 工具 | 权限 | 关键参数 | 关键结果与约束 |
|---|---|---|---|
| `terminals_open` | 终端会话 / W / 确认新会话 | `source`、`cols`、`rows`、`title?` | `source` 为 `{kind:"node",node_ref}`、`{kind:"connection",connection_ref}` 或一次性 `{kind:"local"}`；保存的 Mosh、Telnet、串口经连接句柄解析，SSH 必须先取得 node。返回 `terminal_ref`、实际 transport、生命周期和能力。 |
| `terminals_state` | 终端观察 / D / 否 | `terminal_ref` | 生命周期、尺寸、标题、编码、交互性、缓冲区计数与实际能力；不含屏幕文字。 |
| `terminals_read` | 终端观察 / R / 否 | `terminal_ref`、`cursor?`、`line_limit`、`tail?` | 有界终端快照、增量 cursor、截断标记；原始内容按敏感数据处理。 |
| `terminals_find` | 终端观察 / R / 否 | `terminal_ref`、`query`、`options` | 有界匹配位置与行片段，支持大小写、正则和整词；不返回完整缓冲区。 |
| `commands_start` | 命令执行 / X / 必须 | `target`、`command`、`cwd?`、`timeout_ms?`、`output_limit?`、`request_key` | `target` 严格为 `node_ref` 或 `terminal_ref`。SSH node 使用现有物理连接的独立 exec channel；terminal 使用真实可见会话和命令标记。返回 `command_ref`、`operation_ref` 与 `completion_reliability`。 |
| `commands_state` | 命令观察 / R / 否 | `command_ref` | `queued/running/waiting_for_input/completed/cancelled/failed`、可靠性、可取消性、实际可用时的退出码与截断状态。不得把输出稳定误报为进程成功。 |
| `commands_output` | 命令观察 / R / 否 | `command_ref`、`cursor?`、`max_chars?` | SSH exec 分离 stdout/stderr；交互终端返回有界增量和命令标记范围。内容按终端敏感读取处理。 |
| `commands_cancel` | 命令执行 / W / 视能力确认 | `command_ref` | SSH exec 关闭该 command channel；交互终端默认只停止等待，发送 interrupt 必须显式调用 `terminals_control`。返回真实取消结果。 |
| `terminals_submit` | 终端输入 / X / 必须 | `terminal_ref`、`text` 或 `bytes_base64`、`append_enter`、`request_key` | 向真实 PTY/SSH/Mosh/Telnet/串口写入精确输入；不宣称 shell 执行、退出码或命令回滚。确认页展示经保护的输入摘要。 |
| `terminals_resize` | 终端会话 / W / 否 | `terminal_ref`、`cols`、`rows` | 调整真实终端；返回最终尺寸。 |
| `terminals_control` | 终端输入 / X / 必须 | `terminal_ref`、`action` | `interrupt`、`terminate`、`kill`、`serial_break`、`serial_line`、`telnet_control` 等只在真实支持时可用。 |
| `terminals_close` | 终端会话 / W / 确认有活动任务时 | `terminal_ref` | 关闭终端消费者/后端；SSH node 是否继续存活只由节点 owner 和其他消费者决定。 |

能力矩阵必须由运行时返回，不由工具名暗示：

| 传输 | 可用能力 | 明确不可用或不模拟的能力 |
|---|---|---|
| SSH | 终端、NodeRouter 节点、SFTP、IDE、转发、Host Tools、X11（如协商） | 不把 terminal 当作物理 node owner。 |
| Mosh | 真实 Mosh 终端、输入、尺寸、状态、录制 | 不提供 SFTP、SSH 转发或 NodeRouter 复用的伪能力。 |
| Telnet | 真实 Telnet 终端、输入、尺寸、Telnet 控制 | 不提供 SSH/SFTP/转发/Host Tools。 |
| 串口 | 真实端口、输入、控制线、break、运行时编码和显示模式 | 不提供网络节点、SFTP、转发。 |
| 本地终端 | 实际本地 PTY、输入、尺寸、录制 | 一次性启动，不可 `connections_save`，不伪装为远端或可同步连接。 |

`commands_start` 的完成语义不能跨传输伪造。SSH `node_ref` 通过现有连接的 exec channel 返回真实 stdout、stderr 与退出状态。终端目标优先使用 Shell Integration 的结束标记和退出状态；只有命令标记时报告 `tracked_without_exit_status`；仅观察到输出稳定时报告 `output_stable` 而不是 `completed`。串口和 Telnet 没有 shell 证明时只能报告 `sent`、`waiting_for_input` 或 `output_stable`。

### 4.4 SFTP、IDE 和文件传输

| 工具 | 权限 | 关键参数 | 关键结果与约束 |
|---|---|---|---|
| `files_open` | 远端文件 / W / 否 | `node_ref`、`root?` | 取得真实 node-backed SFTP consumer，返回 `file_session_ref` 和规范化授权根目录。 |
| `files_close` | 远端文件 / W / 否 | `file_session_ref` | 释放 SFTP/IDE 消费者，不断开 node。 |
| `files_list` | 远端文件读取 / R / 否 | `file_session_ref`、`path`、`cursor?`、`limit` | 目录项、类型、大小、修改时间、链接信息和分页 cursor。 |
| `files_stat` | 远端文件读取 / R / 否 | `file_session_ref`、`path` | 精确元数据和可操作能力。 |
| `files_read` | 远端文件读取 / R / 否 | `file_session_ref`、`path`、`offset?`、`maximum_bytes?` | 将有界范围写入 `artifact_ref`，连同总大小和下一偏移量；大文件不塞入无限制工具结果。 |
| `files_compare` | 远端文件读取 / R / 否 | `file_session_ref`、`path`、`artifact_ref` | 在大小上限内比较二进制正文与摘要，返回远端 revision；不改变文件。 |
| `files_write` | 远端文件写入 / X / 必须 | `file_session_ref`、`path`、`artifact_ref`、`overwrite`、`expected_revision?` | 写入或尽可能原子替换；先检查 metadata revision，并准确报告 `atomic_write`。当前没有可靠远端备份，因此不虚构 `undo_ref`。 |
| `files_move` | 远端文件写入 / X / 必须 | `file_session_ref`、`source_path`、`destination_path`、`overwrite`、`expected_revision?` | 在同一授权根内重命名/移动。当前不承诺跨服务端实现都可靠的反向撤销。 |
| `files_remove` | 远端文件删除 / X / 必须 | `file_session_ref`、`path`、`recursive`、`expected_revision?` | 永久删除文件或目录；递归意图进入冻结批准参数，不把永久删除称为回收站，也不返回虚假撤销。 |
| `artifacts_stage` | 传输数据 / W / 否 | `content` 或 `bytes_base64`、`media_type`、`name?` | 将客户端提供的有界数据写入临时 artifact，返回 `artifact_ref`、大小和 digest；不接受任意本机路径。 |
| `artifacts_read` | 传输数据 / R / 否 | `artifact_ref`、`offset?`、`length?` | 有界下载、媒体类型、digest；客户端只可读取自己有权访问的 artifact。 |
| `transfers_start` | 传输数据 / X / 上传必须 | 严格 tagged `direction`；上传含 `file_session_ref`、`remote_path`、`artifact_ref`、`overwrite`、`resume`，下载不接受本机路径 | 基于真实 SFTP 传输控制器启动客户端私有单文件作业，返回 `transfer_ref`。当前最多 64 MiB 且 `resume` 必须为 `false`；目录传输等待显式清单协议后再开放。 |
| `transfers_status` | 传输数据 / D / 否 | `transfer_ref` | `pending/running/completed/cancelled/failed`、字节数、速度、脱敏错误码、远端残留标记和完成后的下载 artifact；不返回内部节点或临时路径。 |
| `transfers_cancel` | 传输数据 / W / 否 | `transfer_ref` | 请求现有传输控制器取消；上传会准确报告可能的远端部分文件，关闭传输不等于断开共享 SSH 节点。 |
| `workspaces_mount` | IDE 工作区读取 / W / 否 | `file_session_ref`、`root?`；同时要求远端文件读取组 | 在既有 SFTP 授权根下规范化子目录，创建独立 IDE owner，返回 `workspace_ref`、项目名、Git 摘要和真实能力。 |
| `workspaces_tree` | IDE 工作区读取 / R / 否 | `workspace_ref`、`path?`、`cursor?`、`limit?` | 有界目录页、相对路径、类型、版本和基础元数据；不泄露内部节点或 UI 实体。 |
| `workspaces_read` | IDE 工作区读取 / R / 否 | `workspace_ref`、`path` | 最多 4 MiB 的可编辑 UTF-8 文本和 conflict revision；二进制和超限正文明确拒绝，不塞进 JSON base64。 |
| `workspaces_apply_edits` | IDE 工作区编辑 / X / 必须 | `workspace_ref`、`files[]`，每项含 `path`、`expected_revision`、UTF-8 字节范围 edits；同时要求远端文件写入组 | 先预检全部文件，再用编辑核心应用结构化事务并带真实远端版本写回；跨文件不宣称原子，失败时尝试受版本保护的补偿回滚，不返回虚假 `undo_ref`。 |
| `workspaces_search` | IDE 工作区读取 / R / 否 | `workspace_ref`、字面量 `pattern`、`root?`、`case_sensitive`、`maximum_results?` | 复用 Node Agent 字面量搜索；不可用时走已有有界远端 grep fallback。结果限制 500 条、片段限制 4 KiB，路径仍受工作区根约束；当前不虚构未贯通的正则、glob 或隐藏文件策略。 |
| `workspaces_close` | IDE 工作区读取 / W / 否 | `workspace_ref` | 关闭 headless IDE owner 并只释放它自己的 consumer，不断开共享物理节点。 |

### 4.5 Host Tools 和端口转发

| 工具 | 权限 | 关键参数 | 关键结果与约束 |
|---|---|---|---|
| `hosttools_catalog` | Host Tools 观察 / D / 否 | `node_ref` | 节点的已支持、类型化资源：系统、进程、服务、容器、端口、文件系统、软件包、计划任务、日志、tmux。 |
| `hosttools_capture` | Host Tools 观察 / R / 否 | `node_ref`、`resource`、`options` | 通过 Host Tools broker 的类型化快照；原始日志和命令参数按敏感内容限额返回。 |
| `hosttools_operate` | Host Tools 操作 / X / 必须 | `node_ref`、`resource`、`action`、`typed_target` | 如服务 start/stop/restart、进程 signal、容器 action、计划任务 action。只允许 catalog 中的类型化操作，绝无自由 shell 字符串。 |
| `forwards_list` | 转发 / R / 否 | `node_ref?`、`include_stopped?` | 外部 `forward_ref`、类型、绑定/目标、状态、保存状态和统计摘要。 |
| `forwards_open` | 转发 / X / 必须 | `node_ref`、`kind`、`bind`、`target?`、`persist?` | 创建 local、remote 或 dynamic forward，返回 `forward_ref`。listener/bridge 由转发 owner 持有，终端关闭不停止它。公开绑定与持久化需要更高风险提示。 |
| `forwards_change` | 转发 / X / 必须 | `forward_ref`、`patch`、`expected_revision` | 更新已停止规则或受控重启活动规则；返回新 revision。当前没有跨监听器副作用的可靠撤销，因此不虚构 `undo_ref`。 |
| `forwards_stop` | 转发 / W / 必须 | `forward_ref` | 停止 listener 和 bridge；保存规则可随后重启。 |
| `forwards_restart` | 转发 / W / 必须 | `forward_ref` | 重新建立原规则，返回实际状态。 |
| `forwards_remove` | 转发 / X / 必须 | `forward_ref`、`remove_saved` | 删除运行时规则，可选删除保存定义。 |
| `forwards_metrics` | 转发 / R / 否 | `forward_ref` | 连接数、活跃数、收发字节和状态。 |
| `forwards_discover_ports` | 转发 / R / 否 | `node_ref` | Host Tools 端口发现快照；不创建转发。 |

Host Tools 的已安装插件扩展不能通过 Public MCP 取得任意调用入口。只有经过产品审核、具有稳定 typed schema、明确标注 `public_mcp_exposable` 的声明式监视器，才可被并入 `hosttools_catalog`；它们仍通过同一个 Host Tools broker 执行，而不是暴露插件 RPC。

### 4.6 RDP、VNC、画面、键鼠和剪贴板

| 工具 | 权限 | 关键参数 | 关键结果与约束 |
|---|---|---|---|
| `desktops_open` | 远程桌面 / X / 必须 | `connection_ref` | 开启保存的真实 RDP/VNC provider 会话，返回 `desktop_ref`、连接状态、协商能力、安全状态和 framebuffer epoch；认证只通过设备受保护槽移交，不接受秘密明文或内部凭据引用。短暂 profile 待凭据交互契约完成后再开放。 |
| `desktops_state` | 远程桌面观察 / D / 否 | `desktop_ref` | provider、连接状态、尺寸、epoch、加密/证书状态、支持的输入和剪贴板方向；不返回画面。 |
| `desktops_frame` | 远程桌面观察 / R / 否 | `desktop_ref`、`after_generation?` | 最新完整帧编码为客户端私有 PNG `artifact_ref`，并返回 generation、graphics epoch 和尺寸；generation 未变化时返回 `unchanged`，不在 JSON 中塞入 framebuffer Base64。 |
| `desktops_input` | 远程桌面控制 / X / 必须 | `desktop_ref`、`graphics_epoch`、`event` | 严格联合类型：鼠标移动/按键/滚轮、键按下/松开、文本、释放全部输入。坐标以 server framebuffer 为准；不转发任意 helper 协议消息。 |
| `desktops_resize` | 远程桌面控制 / W / 否 | `desktop_ref`、`width`、`height` | 请求远端 resize，实际结果以协商能力为准。 |
| `desktops_clipboard_read` | 远程桌面剪贴板 / R / 否 | `desktop_ref`、`kind` | 在双方都授权且 provider 支持时读取会话实际收到的最新远端文本或图像 artifact；绝不把系统剪贴板当成远端内容，文件剪贴板尚不公开。 |
| `desktops_clipboard_write` | 远程桌面剪贴板 / X / 必须 | `desktop_ref`、严格 `payload`（文本或图像 `artifact_ref` + 格式） | 写入远端剪贴板；校验 artifact media type 和 16 MiB 限额，不把剪贴板正文写进审计。 |
| `desktops_reconnect` | 远程桌面 / X / 必须 | `desktop_ref` | 用原配置请求受控重连；继续由会话所有者持有 helper 生命周期与秘密。 |
| `desktops_close` | 远程桌面 / W / 确认未保存工作提示时 | `desktop_ref` | 关闭 provider/helper 及会话，撤销帧和剪贴板句柄。 |

RDP/VNC helper 的 JSON line、二进制帧、证书材料、凭据和进程控制都是内部实现。Public MCP 只看到版本化 DTO、图像/ artifact 和严格输入事件。若会话借助 SSH tunnel，tunnel 仍归相应 SSH node/forward owner 所有，而非远程桌面视图或 MCP HTTP 请求。

### 4.7 云同步、插件、快速命令和终端录制

| 工具 | 权限 | 关键参数 | 关键结果与约束 |
|---|---|---|---|
| `sync_status` | 云同步 / D / 否 | 无 | 后端类型、是否配置、最近 revision、是否有本地脏数据和秘密存在标记；不返回 token、密码、远端 URL 或受保护引用。 |
| `sync_pull_preview` | 云同步 / R / 否 | `selection.sections?`、`conflict_strategy` | 下载并构造结构化预览，返回客户端私有、十分钟有效且只可消费一次的 `sync_plan_ref`、分区和冲突摘要；凭据引用在预览前剥离。 |
| `sync_publish_preview` | 云同步 / R / 否 | `selection.sections?`、`force?` | 生成本地上传预检、脏分区、冲突和预期远端 revision；返回与 pull 相同生命周期的 `sync_plan_ref`。 |
| `sync_apply_plan` | 云同步 / X / 必须 | `sync_plan_ref` | 应用已预览的 pull 或 publish 计划；再次比较完整本地状态和远端 revision/etag/content hash，失配即拒绝。只有能精确恢复的本地 pull 分区返回 `undo_ref`；publish 不可撤销。 |
| `sync_restore` | 云同步 / X / 必须 | `undo_ref` | 在本地状态仍等于 apply 后快照时恢复严格 checkpoint；句柄绑定客户端、十五分钟过期且只可消费一次。不把远端写入或无法恢复的密钥删除伪装成可恢复。 |
| `addons_list` | 插件管理 / D / 否 | `include_disabled?` | 插件 ID、版本、来源类别、启用状态、声明能力和公开适配器摘要；不列出任意内部 host function。 |
| `addons_install` | 插件管理 / X / 必须 | `artifact_ref` 或受管 `source`、`expected_identity` | 经现有签名/完整性/权限审核安装插件；返回 `operation_ref` 和可用时 `undo_ref`。不执行客户端提供的任意二进制命令。 |
| `addons_set_enabled` | 插件管理 / X / 必须 | `addon_ref`、`enabled` | 启用或禁用，重新核对所需权限并返回状态。 |
| `addons_remove` | 插件管理 / X / 必须 | `addon_ref`、`retain_settings` | 卸载插件，选择保留或删除其设置；绝不调用插件自定义 RPC。 |
| `quickcommands_list` | 快速命令 / R / 否 | `scope?` | 名称、描述、参数 schema、风险分类和 revision；命令正文按敏感命令读取权限另行返回。 |
| `quickcommands_describe` | 快速命令 / R / 否 | `quickcommand_ref`、`include_body?` | 结构化模板、声明参数和目标限制；含正文时必须已授权敏感读取。 |
| `quickcommands_save` | 快速命令 / X / 必须 | `quickcommand_ref?`、`definition`、`expected_revision?` | 新建或更新保存的固定模板，返回 revision/undo。此工具不执行模板。 |
| `quickcommands_remove` | 快速命令 / X / 必须 | `quickcommand_ref` | 删除保存模板，可在保留快照时返回 `undo_ref`。 |
| `quickcommands_run` | 快速命令 / X / 必须 | `quickcommand_ref`、`terminal_ref` 或 `node_ref`、`arguments` | 只渲染已保存模板的声明参数后投递给真实终端；确认页展示目标和最终渲染摘要。不能借此执行未保存的任意插件函数。 |
| `recordings_control` | 录制控制 / X / 仅 `start` 必须 | 严格 tagged action：`start {terminal_ref,title?,capture_input}`，或 `pause/resume/stop {recording_ref}` | 返回 `recording_ref`、状态和时长；当前真实 recorder 为 output-only，拒绝 `capture_input=true`。录制可能包含敏感终端文本，默认不公开。 |
| `recordings_status` | 录制控制 / D / 否 | `target` 严格为 `{kind:"recording",recording_ref}` 或 `{kind:"terminal",terminal_ref}` | 状态、时长、尺寸、事件数、是否由当前客户端管理、是否可读取正文及截断状态，不含事件正文。 |
| `recordings_search` | 终端录制 / R / 否 | `recording_ref`、`query`、`limit` | 有界时间点与片段；内容按终端敏感读取策略处理。 |
| `recordings_export` | 录制内容 / X / 必须 | `recording_ref`、`format:"asciicast_v2"`、`name?` | 将停止后的真实 asciicast 导出到受限 `artifact_ref`；不接受任意路径，确认页明确该记录可能含凭据或业务数据。 |

## 5. 授权、确认和审计策略

### 5.1 工具组默认值

| 工具组 | 默认 | 范围 | 说明 |
|---|---:|---|---|
| 基础目录、操作状态 | 开启 | 当前客户端 | 只含服务状态和脱敏目录。 |
| 连接目录 | 开启 | 当前客户端 | 仅脱敏 metadata；精确端点需要连接读取。 |
| 连接读取、连接管理、凭据管理、节点会话 | 关闭 | 客户端 + 指定连接或节点 | 配置写入与秘密写入分组授权；使用已存秘密不等于可读取秘密。 |
| 终端观察、终端输入、录制 | 关闭 | 客户端 + 指定 terminal/transport | 输入和录制分别授权。 |
| 远端文件读取、写入、IDE 工作区读取、IDE 工作区编辑、传输 | 关闭 | 客户端 + `connection_ref`/根目录 | IDE 必须从已授权 SFTP 根派生；读取与结构化编辑独立授予。 |
| Host Tools 观察、Host Tools 操作 | 关闭 | 客户端 + 指定 node | 操作必须来自固定 typed catalog。 |
| 转发 | 关闭 | 客户端 + 指定 node | 公网/非回环 bind 和保存规则加重确认。 |
| 远程桌面观察、控制、剪贴板 | 关闭 | 客户端 + 指定 desktop/profile | 画面、输入、双向剪贴板独立授予。 |
| 云同步 | 关闭 | 客户端 + 当前账户 | 任何应用、上传、恢复都再确认。 |
| 插件、快速命令 | 关闭 | 客户端 + 资源范围 | 不给任意插件调用权。 |
| 审计读取 | 关闭 | 当前客户端自身 | 不允许跨客户端审计浏览。 |

用户授权由客户端、工具组、资源范围、过期时间和允许的操作组合构成，不存在自动开放所有工具组的通配授权。完全权限是逐动作批准策略，不是工具组通配：未勾选的组仍不会出现在 `tools/list`，直接调用也会被拒绝。应用可提供一次任务、至应用重启、固定时长和持久授权；默认推荐普通模式。

### 5.2 哪些行为必须动作确认

普通模式下，除工具组授权外，以下必须经 `approval_ref` 的动作确认。用户把具体客户端显式切换为完全权限模式后，这些动作可在已勾选工具组内直接执行，但仍会审计，并继续受应用锁、句柄所有权、参数 schema、秘密边界和生命周期规则约束：

- 创建共享 node、打开新终端、写入新凭据、保存/删除连接；
- 终端提交、kill/terminate、串口 break 与 Telnet 控制；
- 文件写入、覆盖、移动、删除、递归删除和目录传输覆盖；
- Host Tools 的状态改变、公开或持久化端口转发；
- RDP/VNC 会话打开、键鼠/剪贴板写入、重连；
- 云同步应用、发布、恢复；
- 安装、启用、卸载插件；
- 创建/编辑/运行/删除快速命令；
- 开始含内容的录制与导出。

低风险、已授权且不改变状态的读取可直接运行。当前转发停止和重启也按状态改变处理：普通模式逐次确认，完全权限模式只在已勾选转发管理组内跳过确认；两种模式都要求审计和明确目标句柄。

### 5.3 审计记录

每次工具调用记录：`audit_ref`、时间、`client_ref`、工具、外部目标句柄、权限/批准引用、结果状态、耗时、可撤销性、参数结构摘要和内容 digest。敏感正文只记录存在性、大小和单向 digest；不记录秘密、完整命令、终端文本、文件内容、剪贴板或画面像素。

审计策略还必须做到：

- 请求、结果、MCP protocol error 和取消路径在构造日志前脱敏；
- 凭据写入和云同步的秘密只记录“使用了受保护输入”，不记录值、存储账户或密文；
- `mcp_audit_search` 只能查询当前授权客户端自己的记录，且再做字段投影；
- 句柄撤销、客户端停用、服务关闭和应用退出均为可审计事件。

## 6. 数据与秘密规则

1. 所有保存的连接和远程桌面 DTO 都使用专门的 Public MCP 投影；绝不序列化现有 Rust domain struct。
2. `credentials_store`、连接、RDP/VNC、代理、云同步等含新秘密输入的工具，在网关边界转为 `Zeroizing` 所有者，交给 `oxideterm-secret-store` 或现有受保护连接存储后即清空。不会为审计、重试或异步任务克隆普通 `String`。
3. 对现有秘密只能做存在性检查、选择受保护槽、删除，或由应用 broker 在认证动作中消费。没有 `credentials_read`、`secret_export`、`sync_key_read` 等工具。
4. SFTP 文件、终端、录制、日志、截图和剪贴板不是“秘密字段”，但可能包含秘密；它们一律是需要显式读取许可的敏感内容，并受大小、分页、过期和审计摘要限制。
5. Public MCP 事件、通知、工具结果、错误、遥测、诊断和插件边界均在发送前做同一份红线检查。

## 7. 现有实现映射

本文不要求重写现有产品能力。公开契约应在新 adapter 层翻译稳定 DTO 与外部句柄，内部 crate 保持既有职责。

| 契约领域 | 当前实现证据 | Public MCP adapter 的职责 |
|---|---|---|
| MCP 协议与临时 HTTP 处理 | 官方 Rust SDK `rmcp` 支持 MCP `2026-07-28`；`oxideterm-acp-host-tools` 已有旧协议回环 listener、限流、取消和授权头经验 | Public MCP 使用官方 SDK 和独立公共运行时；不得共享 ACP 会话 listener、工具定义或 token。ACP handler 不作为公共协议实现。 |
| 反向 MCP 客户端 | `oxideterm-ai`、`McpRegistry` 和应用设置中的 MCP 配置 | 明确隔离，防止 Public MCP 的 grant 或 client credential 被外部 MCP 配置使用。 |
| 连接与凭据 | `oxideterm-connections`、`oxideterm-secret-store`、`SecretString` | 做脱敏 profile 投影、外部 `connection_ref` 映射、受保护存储写入 broker 和 revision/undo 账本。 |
| SSH node | `oxideterm-ssh::NodeRouter`、`SshConnectionRegistry`、node runtime store | 为每个 MCP lease 建立专用 `ConnectionConsumer`；只通过 `acquire_connection`/`release_consumer` 使用共享物理 node。 |
| 终端与命令 | `oxideterm-terminal::TerminalSession` facade、命令标记账本、`SshTransportClient::run_command_capture`、local/SSH/Mosh/Telnet/serial backend | 建立 `terminal_ref`/`command_ref` 表、串行输入队列、受限快照/搜索投影和生命周期事件；SSH exec 通过 NodeRouter 当前物理连接，交互命令按实际可靠性报告；不从 tab 推断活性。 |
| SFTP/传输 | `oxideterm-sftp`、connection registry 的 SFTP acquire、`oxideterm-trzsz`、`oxideterm-modem-transfer` | 通过真实 SFTP session 建立 file consumer；把大内容改为有界 artifact 和 transfer operation。 |
| IDE | `oxideterm-ide-fs`、`oxideterm-ide-core`、`oxideterm-editor-core` | 创建非 GPUI 的 `workspace_ref` 投影，采用结构化 edits 与 revision，而不暴露 editor entity。 |
| Host Tools | `oxideterm-connection-monitor`、`oxideterm-acp-host-tools`、插件 host-tools adapter | 映射为固定 catalog/capture/operate；禁止把 `runExtension` 或 shell command 原样公开。 |
| 转发 | `oxideterm-forwarding::ForwardingManager`、registry、事件、profiler | `forward_ref` 映射到规则所有者；listener/bridge 的取消路径归 forward owner，节点断开时级联。 |
| RDP/VNC | `oxideterm-remote-desktop`、`oxideterm-gpui-remote-desktop`、RDP/VNC helper | 提取应用拥有的会话 broker，向 MCP 投影 frame、状态、严格 input 和剪贴板；不穿透 helper 协议。 |
| 云同步 | `oxideterm-cloud-sync` 的 preview/apply/upload、`oxideterm-gpui-cloud-sync` | 将 preview 固化为带 revision 的 `sync_plan_ref`，应用前比较 revision，保留实际存在的 checkpoint。 |
| 插件 | plugin manifest/registry/runtime/host API crate | 仅管理插件生命周期和已审核声明式 adapter；绝不暴露 `api.invoke` 或插件自定义方法。 |
| 快速命令 | `oxideterm-quick-commands`、应用 product host calls | 公开保存模板和声明参数；运行经确认后投递至真正 terminal，不建立任意函数调用通道。 |
| 录制 | `oxideterm-terminal-recording` | `recording_ref` 管理状态、搜索与有明确内容授权的 export artifact。 |
| 审计与批准 | 应用已有 AI tool approval 生命周期可作 UI 所有权参考 | 新建 Public MCP 专用 grant、不可变 action、audit 和 undo store；不要共用 AI 对话批准。 |

实现时应新增独立 crate/模块边界，例如 `oxideterm-public-mcp`，由应用运行时持有。它依赖领域 broker trait，而不是直接依赖 GPUI view、internal registry map 或 helper 进程。应用 crate 只负责装配 broker、GUI 授权界面和生命周期；领域 crate 继续拥有协议、会话和秘密。

## 8. 分阶段实施顺序与验收

以下顺序同时约束当前实现和后续扩展，防止先暴露高风险入口、后补生命周期。

| 阶段 | 交付边界 | 关键验收 |
|---|---|---|
| 0. 契约冻结 | MCP `2026-07-28` 与旧版兼容、句柄编码、DTO schema、错误码、授权账本、审计字段、client discovery 和 threat model | 评审确认方向是“外部客户端调用 OxideTerm”；验证没有复用反向 MCP 或 ACP 会话的 token；新旧协议得到相同的产品授权结果。 |
| 1. 服务骨架 | `PublicMcpRuntime`、stdio/回环入口、client registration、基础目录、动态工具可见性、operation/approval/audit store | 未获授权的客户端只看到基础目录；撤销能断开会话并使所有其句柄不可用；Host/Origin/认证校验完整；日志中没有 token/参数正文。 |
| 2. 连接与节点 | 连接投影、凭据只写 broker、`nodes_*`、节点租约 | 打开和关闭最后一个 MCP terminal 后，另一个 SFTP/forward consumer 仍能使用同一 node；显式 node disconnect 才级联停止。 |
| 3. 终端、命令与文件 | `terminals_*`、`commands_*`、真实 SFTP、artifact、transfer、IDE、revision/undo | SSH exec 返回真实退出码；交互终端明确完成可靠性；Mosh、Telnet、串口和本地 terminal 只报告真实能力；本地 terminal 保存被拒绝；文件覆盖冲突、取消和实际备份回滚可验证。 |
| 4. Host Tools 与转发 | typed Host Tools catalog、操作确认、forward 生命周期/统计/端口发现 | 无自由 shell/plugin RPC；forward 在 terminal 关闭后仍存活，在节点断开时停止；公开 bind 的确认信息正确。 |
| 5. 远程桌面 | desktop session broker、frame artifact、input epoch、剪贴板方向授权 | 不泄露 helper protocol；过期 frame epoch 不接受坐标输入；关闭会话销毁 helper、frame 与 clipboard 句柄。 |
| 6. 同步和产品资产 | sync plan/apply/restore、插件生命周期、快速命令、录制 export | 同步 preview 与 apply revision 不一致时拒绝；秘密引用不出现在 preview；无法真实回滚的外部写入不会返回 `undo_ref`。 |
| 7. 稳定化 | schema versioning、迁移、压力/断线测试、跨平台 credential 验证、文档和客户端配置向导 | 每组工具都有授权拒绝、批准、取消、撤销、客户端断线、应用退出、重连和审计脱敏测试；在 macOS/Windows/Linux 实际验证受保护存储路径。 |

每一阶段都必须先实现相应的 ownership、撤销和审计，再放开工具组。功能不完整时应从 `mcp_catalog` 报告 `unavailable` 原因，而不是伪造成功、降级为未声明 shell 路径或暴露内部实现。

## 9. 明确拒绝的接口形态

以下接口即使实现方便，也不进入公开 MCP：

```text
call_internal(function_name, args)
plugin_invoke(plugin_id, method, payload)
helper_passthrough(desktop_ref, raw_json)
credential_get(connection_ref, slot)
connection_internal_ids(connection_ref)
terminal_by_tab(tab_id)
node_transport(node_id)
read_any_local_path(path)
```

它们要么泄露内部身份/协议，要么绕开版本化 schema、节点所有权、秘密边界、用户确认和审计。需要新能力时，应添加一个有明确资源模型、参数、结果、权限、失败语义和实现 owner 的专用工具，而不是扩展万能逃生口。
