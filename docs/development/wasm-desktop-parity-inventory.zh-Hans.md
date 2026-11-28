# WASM 对齐审计附录：页面、设置和快捷键

与[主审计](wasm-desktop-parity.zh-Hans.md)使用同一提交 `4b7ada080`。这里回答“有没有入口、点进去是什么、设置背后有没有执行者”。Demo 与 WSS 分列：前者能打开页面不等于后者连得上真实服务。

## 工作区全部标签类型

依据：[标签类型](../../crates/oxideterm-workspace/src/lib.rs)、[桌面标签内容分派](../../crates/oxideterm-gpui-app/src/workspace/tabs/detach.rs)、[Web 的 action、page 与窗格挂载](../../crates/oxideterm-gpui-web/src/host.rs)、[Web 终端创建](../../crates/oxideterm-gpui-web/src/terminal.rs)、[文件页分派](../../crates/oxideterm-gpui-web/src/files.rs)。表中的“无入口”指通常的页面动作；例外单列。欢迎空态、右栏、弹窗不是新的标签类型。

| TabKind | 桌面用途 | 浏览器 Demo | 浏览器 WSS |
|---|---|---|---|
| `Workspace` | 组合不同内容的窗格树 | 可拖入目标窗格或从标签菜单合并；页面和终端可移回标签 | 同左；终端来源改为网关或设备 |
| `LocalTerminal` | 用户机器的 PTY/本地终端 | “local” 为模拟终端；BLE 也借用此类型并换图标 | “local” 是网关节点名，不是用户本机；BLE 独立直连 |
| `SshTerminal` | 绑定已认证节点的会话 | 示例节点、命令解释器和真实终端画布 | Open/Input/Output 走网关；客户端不做桌面 SSH 认证协商 |
| `MoshTerminal` | Mosh 会话 | 有草稿，无 Mosh 连接分支 | 无独立 Mosh 会话入口 |
| `FileManager` | 本机文件／卷、预览与操作 | 示例盘和每个授权本机目录分别开页，收藏按目录身份恢复；可浏览、预览、复制、拖入、下载、压缩和解压 | 同样可选本机目录，无需网关在线；不能借此读取网关远端文件 |
| `Graphics` | 图形内容页 | 无独立图形页 | 无独立图形页 |
| `Runtime` | 节点运行状态、消费者 | 按演示工作台的实际节点、终端与文件页汇总；概览、连接矩阵可切换 | 无网关运行时快照 |
| `ConnectionPool` | 连接池与节点生命周期 | 无独立页 | 无独立页 |
| `Topology` | 独立拓扑标签 | 独立标签与 Runtime 矩阵共用视图，显示当前 Demo 节点与文件端点 | 无网关拓扑服务，不显示入口 |
| `NotificationCenter` | 通知、事件日志与关联动作 | 连接与传输通知可回到原窗格或文件页；不显示无数据源的事件日志 | 可查看通知并返回出错的终端，无网关审计日志 |
| `Sftp` | 节点侧栏及独立单／双端 SFTP | 侧栏跟随或固定节点；独立标签保留双端点。单节点标签可选授权本机目录，在两栏间传输、编辑、比较文件并检查版本冲突 | 无远端文件页；Copy 消息不提供目录读写 |
| `Ide` | 项目、文件、编辑器与对应运行时 | 节点目录和浏览器授权的本机项目分别打开；共用桌面编解码器、搜索和保存冲突处理 | 可编辑本机授权项目；无远端 IDE 文件桥 |
| `Forwards` | 创建规则、启动监听、停止转发 | 节点各自拥有草稿页，可增删改，不开端口 | 无转发服务，不显示执行入口 |
| `SessionManager` | 多协议持久连接库 | 示例 SSH、独立 SFTP 和 BLE 配置保存在本站 IndexedDB，支持增删改、分组和连接 | 单独保留此模式下保存的 BLE 配置；命令面板可打开设备，不跳入 Demo 目录 |
| `PluginManager` | 插件市场、安装、审批与启停 | 官方 v2 市场可浏览、搜索、筛选；不伪造安装及运行状态 | 同 Demo，目录不依赖网关 |
| `Plugin` | 由插件 ID 和 tab ID 标识的贡献页 | 无插件页面渲染 | 无插件页面渲染 |
| `CloudSync` | 同步、版本、备份与冲突处理 | 配置预览，不执行同步、生成备份或宣称保存成功；历史为空 | 正常动作被挡 |
| `Knowledge` | 持久知识库、索引与检索 | 内存集合和关键词查询 | 正常动作被挡 |
| `RemoteDesktop` | RDP/VNC session、帧与输入 | 有连接草稿，无桌面视图 | 无页面和帧/输入协议 |
| `Settings` | 完整宿主设置 | portable 过滤，见下表 | 同一 portable 设置，不等于修改网关配置 |
| `Onboarding` | 初次引导与完成状态 | 首次显示；接受声明并完成后，刷新不重开，保留示例终端 | 共享引导与完成状态，不为引导创建网关终端 |

## 设置：14 个标签下的全部分区

来源：[分区枚举与动态路由](../../crates/oxideterm-settings-model/src/navigation.rs)、[Web 白名单与导航](../../crates/oxideterm-gpui-settings-view/src/surface/portable.rs)、[各卡片实际实现](../../crates/oxideterm-gpui-settings-view/src/surface/portable/pages.rs)。**列入**指在浏览器工作区有实际入口；终端、AI 的子页和快捷键范围还要匹配当前路由。**未列**指该分区没有常规页面/搜索入口，不代表相应操作在浏览器不可实现。

| 分区 ID | Web 页面 | 实际行为或缺口 |
|---|---|---|
| `GeneralStartup` | 未列 | 启动与恢复：Web 重新建立工作区；Demo 自动开例程，不用桌面启动项。 |
| `GeneralWindowBehavior` | 未列 | 驻留托盘属于桌面窗口。 |
| `GeneralAppLock` | 未列 | 应用锁：网页会话锁与系统认证需要不同实现。 |
| `GeneralLanguage` | 列入 | 首次使用浏览器语言；手动切换后按站点恢复。 |
| `GeneralExternalConnectionUris` | 未列 | 系统 URI 处理器属于桌面；应用内连接链接是独立功能。 |
| `GeneralDataDirectory` | 未列 | 数据目录：不应向网页提供迁移桌面数据目录的操作。 |
| `GeneralCliCompanion` | 未列 | CLI：不提供在用户机器安装命令行伴侣的操作。 |
| `PortableRuntime` | 未列 | 网页不激活桌面便携安装。 |
| `AppearanceTheme` | 列入 | 主题：可选主题、编辑并更新外壳；未配置明暗方案时从标题栏进入外观设置。 |
| `AppearanceBackground` | 列入 | 内置背景图和站点图库共用启停、透明度和范围设置，分屏按窗格类型生效。支持导入、选择和删除 PNG、JPEG、GIF、WebP；网关断线仍可用。视频、特效及其余图像调节见 W05c。 |
| `AppearanceLayout` | 列入 | 工作台密度、字体、圆角与侧栏宽度等显示偏好在刷新后恢复。 |
| `AppearanceEffects` | 列入 | 效果：有卡片；Web 外壳固定用色处不会得到桌面透明效果。 |
| `AppearanceAppIcon` | 列入 | 应用图标：改变 favicon，不改变访问者的系统应用图标。 |
| `TerminalPageSwitcher` | 列入 | 终端子页切换器：已共享；下面的卡片按当前子页列出。 |
| `TerminalDisplayFont` | 列入 | 预览与终端使用相同的字体栈、CJK 回退、字重、连字和度量；字体偏好刷新后保留。 |
| `TerminalDisplayBehavior` | 列入 | 显示行为：有共享卡片；宿主更新画布的主要路径只下发主题与度量。 |
| `TerminalDisplayCursor` | 列入 | 形状和闪烁作用于已打开的终端；终端应用隐藏光标时仍不显示。 |
| `TerminalDisplayCommandMarks` | 列入 | 命令标记：有卡片，Web 未装配完整桌面命令区。 |
| `TerminalDisplayBuffer` | 列入 | 历史行数控制实际滚动缓冲，缩小上限会裁去最早的历史行。 |
| `TerminalInputClipboard` | 列入 | 选中复制、Ctrl+C 智能复制、OSC 52 写入开关和多行粘贴确认已接入；终端不能读取浏览器剪贴板来回复远端 OSC 52 查询。 |
| `TerminalInputMouseLinks` | 列入 | 可设 Shift 选区、链接的修饰键、中键或右键粘贴，右键可尊重终端鼠标报告。HTTP(S) 链接可打开；不将远端文件路径当成本机链接。 |
| `TerminalInputKeyboard` | 列入 | 键盘：有共享卡片和终端协议输入；不等于 Web 工作台接了同名快捷键。 |
| `TerminalLocalShell` | 未列 | 浏览器不启动访问者的本机 shell。 |
| `TerminalLocalShellProfile` | 未列 | 网页不加载本机 shell 配置。 |
| `TerminalLocalPrivilegeCredentials` | 未列 | 浏览器不管理系统提权凭据。 |
| `TerminalTransferInBand` | 未列 | 浏览器终端没有带内文件传输，不显示相关设置。 |
| `TerminalTransferXmodem` | 未列 | 浏览器终端没有 XMODEM 接收或发送任务。 |
| `TerminalCommandBarControls` | 列入 | 浏览器使用共享命令发送器；显示开关刷新后保留。 |
| `TerminalCommandBarFocusHandoff` | 列入 | 焦点交接：有卡片，缺相同的工作台发送栏和终端焦点交接路径。 |
| `TerminalCommandBarQuickCommands` | 列入 | Demo 快捷命令可插入发送草稿或执行；终端发送栏使用共享的调度器和展开面板。 |
| `TerminalAwarenessControls` | 列入 | 目录感知和显示开关作用于有目录数据的 Demo 会话；Git 和项目任务见 E07。 |
| `TerminalAwarenessShellIntegration` | 列入 | Shell Integration：有设置区；宿主未提供桌面的完整 shell 集成。 |
| `TerminalAwarenessTriggers` | 列入 | 触发器：有设置区，Web 没有对应的桌面触发器操作入口。 |
| `TerminalLoggingSession` | 未列 | 网页未提供持续写入桌面日志文件的服务。 |
| `TerminalHighlightSelection` | 列入 | 选区高亮：有卡片，需与普通终端选区行为分别比对。 |
| `TerminalHighlightSemantic` | 列入 | 语义高亮：可以编辑选项；实际生效范围取决于终端画布。 |
| `TerminalHighlightRules` | 列入 | 规则高亮：可编辑规则；Web 设置刷新无桌面配置持久化合同。 |
| `ConnectionsDefaults` | 未列 | 连接默认值：普通 Web SSH 表单未连完整桌面连接运行时。 |
| `ConnectionsIdleTimeout` | 未列 | 空闲超时：应由拥有连接的网关提供，不能靠页面计时推断。 |
| `ConnectionsRetention` | 未列 | 连接保留：网关要能区分节点与单一标签，才能挂此策略。 |
| `ConnectionsReconnect` | 未列 | 重连：Web 消息没有完整桌面节点重连状态机。 |
| `ConnectionsSshKeys` | 未列 | SSH 密钥：没有桌面钥匙串/密钥文件管理；网关授权身份可另接。 |
| `ConnectionsKnownHosts` | 未列 | 需标明归属网关并接入其信任记录。 |
| `ConnectionsSshConfigImport` | 设置未列 | Demo 欢迎页和会话目录可选取单个 OpenSSH 文件，预览、勾选并导入主机；不扫描主目录。 |
| `DiscoverySources` | 未列 | 动态发现：桌面 #872 可配，portable 显式排除整个 Discovery 标签。 |
| `ConnectionsImporters` | 未列 | 导入器：未挂；用户选文件、解析和预览不必隐藏。 |
| `PrivilegeCredentials` | 未列 | 系统提权凭据由桌面服务管理。 |
| `PrivilegeTotp` | 未列 | TOTP：未挂载桌面凭据区；不要用浏览器普通偏好保存种子。 |
| `NetworkSharedProxy` | 未列 | 浏览器请求不消费桌面代理配置；未接网关探测。 |
| `NetworkRouting` | 未列 | 当前 WebSocket 与 fetch 使用浏览器网络栈。 |
| `NetworkPublicMcp` | 未列 | 对外 MCP 服务：网页不监听本机端口；连接远端 MCP 是不同能力。 |
| `SftpOpening` | 列入 | Demo 支持每次询问、侧栏和标签；已有同节点标签时复用。选择保存在当前站点。WSS 缺文件服务。 |
| `SftpPerformance` | 列入 | 传输性能：有设置卡；示例内存复制不能体现实际并发、限速或吞吐。 |
| `SftpConflict` | 列入 | Demo 双栏传输、浏览器选择器与拖入文件共用重名策略，可逐项选择或应用到其余冲突。站点保存默认策略；网关传输尚未接入。 |
| `IdeEditing` | 列入 | 自动保存和折行设置应用于新建和已打开的 Demo 项目。 |
| `IdeTypography` | 列入 | 字体、字重、字号、CJK 回退、行高生效；未覆盖时沿用终端设置。 |
| `IdeAgent` | 未列 | 无管理远端 IDE agent 的服务。 |
| `IdeAgentPrivacy` | 未列 | 没有相应 agent，不显示其设置。 |
| `AiPageSwitcher` | 列入 | AI 子页切换：已共享，助手侧栏是否挂载仍由宿主模式决定。 |
| `AiGeneralSettings` | 列入 | AI 基本设置可编辑。Demo 和 WSS 都可从右栏打开助手；WSS 不要求网关在线。 |
| `AiGeneralPrivacy` | 列入 | AI 隐私：可编辑；真实浏览器模型请求与示例回答需区分。 |
| `AiProvidersSettings` | 列入 | 可配置端点、会话内密钥和模型，选择模型后对话使用对应端点。网页不显示尚不可用的模型刷新按钮；第三方请求需要端点支持跨域。 |
| `AiContextControls` | 列入 | 上下文来源：开关可编辑；Web 未提供终端、IDE、SFTP 活动上下文。 |
| `AiContextSystemPrompt` | 列入 | 系统提示：卡片有字段；Web Anthropic 请求过滤 system 消息且请求体未传 system 字段。 |
| `AiContextMemory` | 列入 | 记忆：设置与聊天历史不同；当前浏览器聊天历史放在内存。 |
| `AiContextModelWindows` | 列入 | 模型上下文窗口：可编辑预算，不等于已接桌面上下文收集。 |
| `AiToolsToolUse` | 未列 | 没有注册到当前聊天的工具。 |
| `AiToolsWebSearch` | 未列 | 没有当前聊天可调用的网页搜索工具。 |
| `AiToolsAgents` | 未列 | 未接入可运行的代理。 |
| `AiToolsSkills` | 未列 | 浏览器工作区无技能加载器。 |
| `AiToolsMcpServers` | 未列 | 不在浏览器启动本机 MCP 子进程。 |
| `KnowledgeError` | 未列 | 知识库错误：portable 状态固定没有错误，不出现此分区。 |
| `KnowledgeCollections` | 列入 | 复用知识库导航的集合列表；未接入的网关不显示 Demo 数据。 |
| `KnowledgeEmbeddingConfig` | 列入 | Demo 显示关键词索引状态，仅支持关键词搜索；没有嵌入任务入口。 |
| `KnowledgeDocuments` | 未列 | 文档：设置路由固定未选集合；知识库主页面仍可编辑示例文档。 |
| `KeybindingsToolbar` | 列入 | 改键或解绑在刷新后保留；录制组合键时不触发工作台动作。各范围的执行情况见下节。 |
| `KeybindingsScope(usize)` | 列入 | 仅展示有处理程序的动作与范围；无文件服务的网关不列出 SFTP、文件管理和预览。 |
| `HelpVersion` | 列入 | 显示当前 Web 构建版本，不提供桌面更新通道。 |
| `HelpDiagnostics` | 列入 | 导出工作区模式、连接状态、会话数量、视口与渲染信息。报告不读取终端文字、路径、连接地址或凭据。 |
| `HelpTechStack` | 列入 | 只列出网页实际使用的 Rust、GPUI、WebAssembly、wgpu 和 alacritty_terminal。 |
| `HelpResources` | 列入 | 资源：URL 可交给浏览器打开。 |
| `HelpSafety` | 列入 | 安全：保留帮助信息，不因此获得本地系统权限。 |
| `HelpLegal` | 列入 | 许可：可在页内打开许可与第三方文档。 |

## 快捷键：共享动作目录与 Web 实际入口

来源：[共享目录](../../crates/oxideterm-gpui-shell/src/shortcuts/definitions.rs)、[Web 根键盘处理](../../crates/oxideterm-gpui-web/src/host.rs)、[Web 命令面板](../../crates/oxideterm-gpui-web/src/palette.rs)。目录包括按平台或终端状态生效的动作。以下对照的是 **Web 工作台根处理器**，不是浏览器默认键或控件内部的事件；例如终端能够粘贴、IDE 能编辑，不能因为根处理器没消费对应 ID 就判定功能不存在。

**已由 Web 工作台派发：** 新建终端/连接、关闭当前或其他标签、上/下一标签及 `app.goToTab1…9`、设置、命令面板、左栏、禅模式、全屏、后退/前进、水平/垂直分屏、切换与关闭窗格、终端搜索、字号增减与重置、快捷键设置、AI 侧栏。文件页与预览另按当前控件解析改键，包括打开、预览、翻页、缩放和旋转。命令面板和快捷键执行同一动作；连接、分屏及助手取决于宿主和当前会话。

**其余目录项按作用域列出。** 表内省略第一列给出的前缀；`goToTab1…9` 是九个实际动作，不是一个占位操作。

| 作用域 | 根处理器未按该 ID 分派的操作 | 从哪里判断实际可用性 |
|---|---|---|
| `app.` | `shellLauncher` | 网页不能启动访问者的本机 shell。 |
| `terminal.` | `copy`、`cut`、`paste`、`clearScreen`、`aiPanel`、`recording`、`toggleFreeTypeMode`、`closePanel`、`suggestPrevious`、`suggestNext`、`wordBackward`、`wordForward`、`scrollPageUp`、`scrollPageDown`、`scrollLineUp`、`scrollLineDown`、`scrollTop`、`scrollBottom`、`pasteAlternate`、`copyAlternate`、`terminateTask`、`killTask`、`aiSubmit`、`aiInsert` | 终端视图按实际绑定处理复制、粘贴、六种历史滚动及普通屏幕按词移动；录制、建议、AI 与任务操作要看工作台是否挂载。 |
| `palette.` | `eventLog`、`broadcast` | 面板能展示通知和 Demo 助手；事件日志、广播不能只凭候选名称判断。 |
| `editor.` | `save`、`copy`、`cut`、`paste`、`selectAll`、`undo`、`redo`、`addNextMatch`、`find` | 由获得焦点的共享编辑器按用户绑定处理。例如给「全选」改键后，展开的发送草稿使用新键位。 |
| `sftp.` | `revealTerminalDirectory` 由文件视图分派 | 按所选栏执行文件操作。Demo 可跳到同节点最近活动的终端目录，也可将浏览位置同步给该会话；独立配置和没有目录来源的连接不提供同步。 |
| `fileManager.` | 当前文件页 | 路径、筛选和对话框支持选区、粘贴和输入法；选择、复制、剪切、粘贴、预览、改名和确认删除作用于当前文件页。授权目录的剪切／改名取决于浏览器对所选类型的移动支持。 |
| `preview.` | 无 | 文件管理器预览按类型处理上一项／下一项、元数据、Markdown 源文及图片缩放、复位、旋转。 |
| `remoteDesktop.` | `copy`、`paste` | Web 未挂远程桌面页，无远程剪贴板目的地。 |
