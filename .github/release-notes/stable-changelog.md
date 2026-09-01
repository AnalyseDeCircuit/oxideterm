# OxideTerm Stable Changelog

Stable releases are listed newest first. The release workflow uses each versioned
section as the detailed changelog attached to the corresponding GitHub Release.

## 2.0.26

### English

OxideTerm 2.0.26 adds configurable terminal font weight and multiline Quick Command editing, improves native drag-and-drop and input handling across all three desktop platforms, and repairs macOS Keychain compatibility without reintroducing duplicate startup authentication.

#### ✨ Terminal Display and Quick Commands

- Added a terminal font-weight setting from 100 to 900. Regular text uses the selected weight, bold terminal cells remain at least bold, and fonts without an exact face may select their nearest available weight.
- Replaced the Quick Command body field with a multiline editor. Enter and pasted line breaks are preserved so shell continuations, heredocs, and other structured commands can be stored without flattening them into one line.
- Limited Quick Command template expansion to the explicit `{{param.*}}` and `{{ctx.*}}` namespaces. Other double-brace syntax, including Docker Go templates such as `{{.LogPath}}`, now remains literal.
- Added deletion for every custom Quick Command group. A confirmation explains the operation, and commands in the removed group move to the built-in default group instead of being deleted.

#### 🖥️ Desktop Input and Drag Reliability

- Added macOS file and folder drag-and-drop for local terminal panes. Dropped paths are inserted as POSIX-quoted shell words with a trailing space and are never executed automatically; remote and non-terminal sessions do not accept this drop path.
- Fixed Linux text input for XIM methods that send `COMPOUND_TEXT`; malformed or unsupported input is rejected safely instead of being accepted as invalid text.
- Fixed a Windows redraw loop that could occur while editing SFTP and local file-manager address fields.
- Fixed Windows tab dragging that could remain active after the mouse button was released and interfere with later clicks.

#### 🔐 macOS Keychain and Managed SSH Keys

- Preserved multiline managed SSH private keys when reading from newer macOS Keychain implementations, and added a guarded recovery path for entries previously stored as hexadecimal text. Unencrypted recovery is persisted only after the public-key fingerprint matches the saved metadata; encrypted recovery is not rewritten before it can be validated with its passphrase.
- Restored opening encrypted connection data with a single Touch ID request instead of a password prompt followed by Touch ID, while retaining correct multiline managed-key reads.

#### 🧰 Package Compatibility

- Moved Linux arm64 release builds to the Ubuntu 22.04 runner and added an ELF symbol-version check that rejects binaries requiring newer than glibc 2.35, matching the existing Linux compatibility baseline for distributions such as Kylin V11.
- Added complete Windows installer file properties, including the package version, application description, and copyright metadata.

### 中文

OxideTerm 2.0.26 新增可配置终端字重和多行快捷命令编辑，改进三个桌面平台的原生拖放与输入处理，并修复 macOS 钥匙串兼容性，同时避免重新引入启动时重复认证。

#### ✨ 终端显示与快捷命令

- 新增 100 至 900 的终端字重设置。普通文本使用所选字重，终端粗体单元格至少保持粗体；若字体没有完全对应的字重，系统可选择最接近的可用字重。
- 将快捷命令正文改为多行编辑器。Enter 和粘贴内容中的换行会被保留，因此 Shell 续行、heredoc 及其它结构化命令不再被压成一行。
- 将快捷命令模板展开限制在明确的 `{{param.*}}` 和 `{{ctx.*}}` 命名空间。其它双花括号语法会保持原文，包括 `{{.LogPath}}` 等 Docker Go 模板。
- 支持删除所有自定义快捷命令分组。操作前会显示确认说明，被删除分组中的命令会移至内置默认分组，而不会随分组一起删除。

#### 🖥️ 桌面输入与拖放可靠性

- 为 macOS 本地终端窗格新增文件和文件夹拖放。拖入路径会作为经过 POSIX 引用的 Shell 单词插入，并在末尾保留空格，但绝不会自动执行；远程及非终端会话不会接受该拖放路径。
- 修复使用 `COMPOUND_TEXT` 的 XIM 输入法在 Linux 上的文字输入；格式错误或不支持的输入会被安全拒绝，不再作为无效文字接收。
- 修复 Windows 编辑 SFTP 与本地文件管理器地址栏时可能出现的重绘循环。
- 修复 Windows 标签拖动在鼠标已经松开后仍可能保持活动并影响后续点击的问题。

#### 🔐 macOS 钥匙串与托管 SSH 密钥

- 在新版 macOS 钥匙串中完整保留多行托管 SSH 私钥，并为此前被保存为十六进制文本的条目增加受限恢复路径。未加密私钥只有在公钥指纹与已保存元数据一致后才会写回；加密私钥在使用口令完成验证前不会改写钥匙串。
- 恢复使用单次 Touch ID 打开加密连接数据，不再先出现密码提示再要求 Touch ID，同时继续正确读取多行托管密钥。

#### 🧰 软件包兼容性

- 将 Linux arm64 发布构建迁移至 Ubuntu 22.04 runner，并增加 ELF 符号版本检查，拒绝依赖高于 glibc 2.35 的二进制文件，使其与现有 Linux 兼容基线一致并覆盖银河麒麟 V11 等发行版。
- 补齐 Windows 安装器文件属性，包括软件包版本、应用说明和版权信息。

## 2.0.25

### English

OxideTerm 2.0.25 refreshes the native GPUI desktop runtime, adds shared and independently authenticated SSH terminal splitting, aligns reconnect behavior across standalone protocols, improves session and SFTP organization, and resolves several Windows pointer, text-selection, editor-font, and native-window regressions while further increasing terminal throughput.

#### ⚡ Terminal Performance and Pane Workflows

- Retained physical terminal rows across scroll damage, reused generation-aware glyph data, batched native rendering work, and reduced repeated snapshot and paint preparation while preserving the existing fast path when no extra projection is needed.
- In the current reproducible 16 MiB benchmark, median process-to-PTY throughput reached 89.959 MiB/s for plain text, 94.358 MiB/s for ANSI output, 77.642 MiB/s for Unicode output, and 100.820 MiB/s for long CSI sequences. Compared with the published 2.0.24 release results, these are further gains of 8.6%, 3.0%, 10.9%, and 1.2% respectively.
- The benchmark used one warm-up and the median of three measured runs. It measures process-to-PTY throughput rather than completed rendering or input latency.
- Added the normal split controls to eligible SSH terminals, removed the distracting active-pane background highlight, and made split dividers visible at 3 px when idle, 4 px on hover, and 5 px while dragging without enlarging the layout footprint.
- Preserved tmux split dimensions, restored the cursor to the home position after clearing the buffer, kept `Ctrl+L` shell clearing intact, and stopped OxideTerm history suggestions from consuming Up/Down when the shell's own completion UI should handle them.

#### 🔗 Shared Connections and Reconnect

- Added multiple SSH terminals on one registry-owned physical transport. Closing one pane no longer disconnects sibling terminals, SFTP, port forwards, or monitoring consumers that still hold the node.
- Added an SSH profile option for independently authenticated new terminals while retaining the same split workflow and stable saved-connection identity. Shared and dedicated channels now use explicit consumer leases and clean up failed consumers without transferring node ownership to a pane.
- Reworked Serial, Telnet, Mosh, RDP, and VNC session records so disconnected sessions retain their original profile identity and can use the same explicit reconnect model instead of being replaced by another target.
- Coalesced repeated reconnect requests, cancelled stale retries cleanly, and prevented restored SSH node identifiers from colliding with live nodes. Serial device-open failures are now surfaced instead of leaving an apparently connected but unusable session.

#### 🗂️ Session and File Management

- Added drag-and-drop organization for saved connections and groups, including nested folders, group-wide drop regions, explicit child-group creation, rename and delete actions, and context menus located beside the group being changed.
- Simplified the Session Manager toolbar to one top-level **New Group** action; group-specific maintenance remains in each folder's context and overflow menus, while the shared editor continues to handle creation and renaming consistently.
- Optimized recursive SFTP deletion and transfer progress delivery with bounded scheduling, less per-entry coordination, cancellable work, and coalesced progress updates; restored keyboard focus for SFTP file-row navigation.
- Reordered connection forms so transport and endpoint fields appear before optional name, group, and notes metadata, and removed the retired standalone connection-monitor and macOS Launchpad entries from the application chrome.

#### 🖥️ Native Desktop and Editor Reliability

- Updated the vendored GPUI runtime and completed the application migration across macOS, Linux, and Windows, including refreshed text, scene, input, accessibility, scheduling, and native-window integration.
- Linux client titlebars now respect the desktop button side and order, hide unsupported minimize or maximize controls, maximize on double-click, open the compositor window menu on right-click, activate buttons on completed clicks, and route Close through the normal close-request flow.
- Repaired Windows pointer-capture and cursor reconciliation when mouse-up or capture-loss messages are missing, restored scenes after window blur, completed edge and corner resize hit testing, and cleared stale caption-button state. This prevents the persistent I-beam cursor and application-wide click lock seen after interacting with selectable text and controls.
- Constrained selectable text to measured glyph hitboxes, stopped zero-length selections from swallowing unrelated clicks, and prevented page headers, empty states, and controls from entering read-only text-selection ownership.
- Restored the configured terminal/code font in the editor on Windows, bundled the code-font resources needed by the refreshed runtime, and retained UI-font fallback for non-Latin glyphs. Static AI tool-call names are now localized consistently across all 11 supported languages.

#### 🔒 Security, Sync, and Portable Runtime

- Sandboxed Kitty local-file transmission behind an explicit per-session approval. Approved sessions receive a private transfer directory whose path is copied only after confirmation; the permission and directory lifetime end with the terminal session.
- Restored Cloud Sync and encrypted `.oxide` compatibility for authenticated archives created before SSH algorithm-preference fields were added, while continuing to reject payloads whose protected connection data was actually modified.
- Added opt-in portable automatic unlock using the current device's system credential manager. The portable password remains required after moving the folder to another device, and stale device credentials are removed before falling back to normal password entry.
- The maintainer completed GUI validation for this release; localization catalogs, registered tool-name coverage, release metadata, and composed bilingual notes are validated by the release preparation checks.

### 中文

OxideTerm 2.0.25 更新原生 GPUI 桌面运行时，新增共享连接及独立认证两种 SSH 终端分屏模式，统一独立协议的断联重连行为，改进会话与 SFTP 管理，并修复多项 Windows 指针、文本选择、编辑器字体和原生窗口回归，同时继续提升终端吞吐量。

#### ⚡ 终端性能与窗格工作流

- 在滚动损伤期间保留终端物理行，复用带代次标记的字形数据，批量执行原生绘制工作，并减少重复快照与绘制准备；未启用额外投影时继续沿用原有快速路径。
- 在当前可复现的 16 MiB 基准中，纯文本、ANSI 输出、Unicode 输出及长 CSI 序列的进程到 PTY 吞吐量中位数分别达到 89.959、94.358、77.642 和 100.820 MiB/s。相较已发布的 2.0.24 数据，分别进一步提升 8.6%、3.0%、10.9% 和 1.2%。
- 基准采用一次预热及三次测量的中位数。该结果衡量进程到 PTY 的吞吐量，不代表绘制完成耗时或输入延迟。
- 为符合条件的 SSH 终端补齐与本地终端一致的分屏控件，移除干扰视觉的活动窗格背景高亮，并将分隔线调整为静止时 3 px、悬停时 4 px、拖动时 5 px，同时不扩大布局占用。
- 保留 tmux 分屏尺寸，清空缓冲区后将光标恢复到起始位置，维持 `Ctrl+L` Shell 清屏行为，并避免 OxideTerm 历史建议在 Shell 自身补全界面应处理上下键时抢占按键。

#### 🔗 共享连接与重连

- 支持在一个由连接注册表持有的物理 SSH 传输上创建多个终端。关闭单个窗格不会再断开仍由其它终端、SFTP、端口转发或监控消费者使用的节点。
- 新增为每个 SSH 新终端单独认证的配置，同时保留相同的分屏工作流和稳定的已保存连接身份。共享与独立通道均使用明确的消费者租约，并会在创建失败时清理消费者，而不会把节点所有权交给窗格。
- 重构串口、Telnet、Mosh、RDP 和 VNC 会话记录，使断联会话保留原始配置身份，并通过与 SSH 一致的明确重连模型恢复，而不是被其它目标替换。
- 合并重复重连请求并正确取消过期重试，防止恢复的 SSH 节点标识与活动节点冲突；串口设备打开失败现在会直接显示，不再留下看似已连接但无法使用的会话。

#### 🗂️ 会话与文件管理

- 为已保存连接和分组新增拖拽整理，支持嵌套文件夹、覆盖整个分组范围的放置区域、明确的新建子分组、重命名与删除操作，以及位于目标分组旁的右键菜单。
- 将会话管理器顶部收敛为一个“新建分组”入口；具体分组的维护继续由对应文件夹的右键及更多菜单承担，统一编辑器仍一致处理创建与重命名。
- 通过有界调度、更少的逐条目协调、可取消任务及合并进度更新优化 SFTP 递归删除和传输进度交付，并恢复 SFTP 文件行的键盘焦点导航。
- 调整连接表单顺序，使传输类型与目标端点字段位于可选名称、分组和备注之前；同时从应用界面移除已经退役的独立连接监控入口和 macOS 启动台。

#### 🖥️ 原生桌面与编辑器可靠性

- 更新内置 GPUI 运行时并完成 macOS、Linux 和 Windows 上的应用迁移，覆盖文本、场景、输入、可访问性、调度及原生窗口集成。
- Linux 客户端标题栏现在遵循桌面按钮所在侧及顺序，隐藏不支持的最小化或最大化按钮，支持双击最大化和右键打开合成器窗口菜单，在完整点击后才执行按钮操作，并让关闭按钮经过正常关闭请求流程。
- 修复 Windows 在缺少鼠标松开或捕获丢失消息时的指针捕获与光标协调，恢复窗口失焦后的场景，补齐窗口边缘及四角缩放命中测试，并清理过期标题栏按钮状态；这可避免与可选文本或控件交互后鼠标持续显示工字形且整个应用无法点击。
- 将可选文本命中范围限制到实际测量的字形区域，避免零长度选区吞掉无关点击，并阻止页面标题、空状态及控件进入只读文本选择所有权。
- 恢复 Windows 编辑器对终端及代码字体配置的使用，为新运行时打包所需代码字体资源，并为非拉丁字符保留界面字体回退；全部静态 AI 工具调用名称现已在 11 个支持语言中保持完整本地化。

#### 🔒 安全、同步与便携运行时

- 将 Kitty 本地文件传输限制在逐会话明确授权之后。获准会话会得到私有传输目录，只有确认后才复制目录路径；权限及目录生命周期均在终端会话结束时终止。
- 恢复 Cloud Sync 和加密 `.oxide` 文件对新增 SSH 算法偏好字段之前所创建认证归档的兼容，同时继续拒绝受保护连接数据确实被修改的内容。
- 新增可选的便携模式自动解锁，通过当前设备的系统凭据管理器保存派生解锁密钥。将文件夹移动到其它设备后仍需便携密码，过期设备凭据会先被删除，再回退到正常密码输入。
- 维护者已完成本版本的 GUI 验证；发布准备检查同时验证语言目录、已注册工具名称覆盖、发布元数据及合成后的双语说明。

## 2.0.24

### English

OxideTerm 2.0.24 adds native tmux control-mode sessions, parameterized Quick Commands, configurable session logging, named broadcast groups, cross-platform Kerberos authentication, richer remote-desktop workflows, and scoped terminal history while further improving terminal throughput and saved-connection routing correctness.

#### ⚡ Further Terminal Performance

- Reduced repeated work across PTY draining, snapshot conversion, text layout, link preparation, and rendering; compacted snapshot-cell storage and reused unchanged terminal data more aggressively; and batched common ASCII parsing and grid writes in the vendored terminal stack.
- In OxideTerm's reproducible 16 MiB release-build benchmark, median process-to-PTY throughput increased from 69.031 to 82.864 MiB/s for plain text, from 88.384 to 91.609 MiB/s for ANSI output, from 68.711 to 69.988 MiB/s for Unicode output, and from 96.685 to 99.644 MiB/s for long CSI sequences. These are further gains of 20.0%, 3.6%, 1.9%, and 3.1% over 2.0.23 respectively.
- Across both optimization rounds, 2.0.24 reaches 26.36×, 12.50×, 15.93×, and 9.78× the original baseline throughput for plain text, ANSI output, Unicode output, and long CSI sequences respectively.
- The comparison used the same fixture size, one warm-up, and the median of three measured runs. It measures process-to-PTY throughput rather than completed rendering or input latency.
- Corrected precise touchpad smooth scrolling so a new gesture preserves its fractional visual position and consumes the first pixel delta instead of briefly snapping in the opposite direction.

#### ✨ Native tmux, Session Logs, Broadcast Groups, and History

- Added native tmux `-CC` control-mode sessions with session, window, and pane actions, synchronized pane layouts, draggable dividers, rename and command prompts, and explicit detach or close controls inside the terminal workspace.
- Added configurable terminal session logging with global and per-connection policies, file-name and line templates, unique, append, or overwrite modes, optional ANSI control sequences, retention and size limits, and start, pause, resume, stop, and open-file actions. Automatic logging remains disabled by default because terminal output may contain sensitive data.
- Added named broadcast groups alongside temporary target selection. Group membership remains explicit, closed targets are not silently replaced, and the command sender continues to skip unavailable members instead of widening the target set.
- Restored compact Rich Input as a single-line text field with scoped ghost-text completion and an Up/Down history menu. Terminal history suggestions remain in process memory, are scoped to the relevant local session or SSH node, and are not stored in the operating-system credential store.

#### 🧩 Parameterized Quick Commands

- Added text, choice, and secret parameters plus target-context tokens for host, username, port, current directory, connection, group, and selection. Templates support explicit POSIX shell quoting and are expanded independently for each selected target.
- Added protocol and host-pattern availability, per-target previews, unavailable-target reporting, bounded expansion, and per-command confirmation policies. Commands approved by the user are frozen before dispatch so later target or input changes cannot alter the authorized payload.
- Secret parameter values use short-lived protected buffers, stay out of stored Quick Command definitions and diagnostic output, and hide expanded previews that would expose them. Risk classification and confirmation are applied to the final expanded command.
- Expanded Quick Command management across the desktop UI, CLI, plugins, Public MCP, terminal triggers, Cloud Sync, and `.oxide` transfer. Categories and commands now support explicit ordering, and desktop exports use atomic snapshot replacement.

#### 🤖 AI Provider Compatibility

- Aligned OpenAI-compatible model discovery, selector readiness, and chat execution. Local and private endpoints now reuse optional stored credentials and the same `/v1` fallback during readiness checks, while compatible keyless gateways remain selectable.

#### 🔐 SSH, Kerberos, and Connection Identity

- Added Kerberos-preferred SSH authentication on macOS, Linux, and Windows through the operating system's current credentials, with conventional fallback authentication, optional server identity, credential-availability feedback, and an explicit delegation warning and control.
- Added editable SSH negotiation order for key exchange, host-key, cipher, MAC, and compression categories, together with stage-aware connection progress, redacted connection traces, and progress cards that can be dismissed without cancelling the connection.
- Added operating-system connection handlers and an opt-in setting for external `ssh://`, `telnet://`, `mosh://`, `rdp://`, and `vnc://` links, including safe delivery from a second application launch into the existing workspace.
- Corrected Saved Connection reuse so route endpoints, complete proxy paths, runtime ownership, and workspace ownership must all match before an existing logical node or terminal can be focused. Profiles that share a host, username, and port remain logically isolated, while different ports are also distinct physical connection identities.
- Preserved local authentication material when synchronized connection metadata adds or changes Kerberos settings, so a metadata update cannot discard device-local passwords or passphrases.

#### 🖥️ Remote Desktop and SFTP

- Expanded remote-desktop file transfer with remote browsing, root and parent navigation, downloads, progress and cancellation, destination selection, and overwrite, rename, or skip conflict policies.
- Expanded VNC security compatibility with VeNCrypt Plain and SASL authentication over TLS, including verified X.509 variants, credential-aware subtype selection, bounded protocol exchanges, and protected handling of usernames and passwords.
- Added RDP network-quality profiles for automatic detection, local networks, broadband, and low-bandwidth links so visual effects and bandwidth hints can match the connection.
- Improved SFTP upload buffering, hardened streamed archive transfers with bounded errors, idle timeouts, cancellation, cleanup, and unpacked-byte progress, and stopped slow terminal working-directory discovery from prematurely failing otherwise healthy SFTP workflows.

#### ☁️ Backup and Sync Workflows

- Moved local encrypted `.oxide` import and export into the Cloud Hub so local backups and cloud synchronization live in one place while remaining available without configuring a cloud provider.
- Extended synchronized Quick Command and SSH metadata while keeping device-local secret values outside portable snapshots and preserving them across compatible updates.

### 中文

OxideTerm 2.0.24 新增原生 tmux 控制模式、参数化快捷命令、可配置会话日志、命名广播组、跨平台 Kerberos 认证、更完整的远程桌面工作流及分作用域终端历史，同时继续提升终端吞吐量并修正已保存连接的路由复用。

#### ⚡ 终端性能进一步强化

- 减少 PTY 排空、快照转换、文本布局、链接准备及绘制中的重复工作；压缩终端快照单元存储并更积极地复用未变化数据；同时在内置终端栈中批量处理常见 ASCII 解析与网格写入。
- 在 OxideTerm 可复现的 16 MiB 发布构建基准中，纯文本的进程到 PTY 吞吐量中位数由 69.031 MiB/s 提升至 82.864 MiB/s，ANSI 输出由 88.384 MiB/s 提升至 91.609 MiB/s，Unicode 输出由 68.711 MiB/s 提升至 69.988 MiB/s，长 CSI 序列由 96.685 MiB/s 提升至 99.644 MiB/s；相较 2.0.23 分别进一步提升 20.0%、3.6%、1.9% 和 3.1%。
- 综合两轮优化后，2.0.24 的纯文本、ANSI 输出、Unicode 输出及长 CSI 序列吞吐量分别达到最初基线的 26.36 倍、12.50 倍、15.93 倍和 9.78 倍。
- 对比采用相同的测试数据规模、一次预热及三次测量的中位数。该结果衡量进程到 PTY 的吞吐量，不代表绘制完成耗时或输入延迟。
- 修正精确触控板的平滑滚动：新手势会保留原有的小数行视觉位置并消费首段像素增量，不再先向相反方向短暂回跳。

#### ✨ 原生 tmux、会话日志、广播组与历史记录

- 新增原生 tmux `-CC` 控制模式，支持会话、窗口与窗格操作、同步窗格布局、可拖拽分隔线、重命名与命令输入，以及在终端工作区内明确分离或关闭会话。
- 新增可配置终端会话日志，提供全局与单连接策略、文件名及行内容模板、创建唯一文件、追加或覆盖模式、可选 ANSI 控制序列、保留期限与大小上限，以及开始、暂停、继续、停止和打开日志操作。由于终端输出可能包含敏感数据，自动记录默认保持关闭。
- 新增命名广播组并保留临时目标选择。组成员始终需要明确指定，已关闭目标不会被其他终端静默替换，命令发送器会跳过不可用成员而不会擅自扩大目标范围。
- 将紧凑 Rich Input 恢复为单行文本框，并新增分作用域幽灵文本补全及上下键历史菜单。终端历史建议仅保存在当前进程内，按本地会话或 SSH 节点隔离，不写入操作系统凭据存储。

#### 🧩 参数化快捷命令

- 新增文本、选项及敏感参数，并提供主机、用户名、端口、当前目录、连接、分组和选区等目标上下文令牌。模板支持显式 POSIX Shell 转义，并会为每个选中目标分别展开。
- 新增协议与主机模式可用范围、逐目标预览、不可用目标提示、有界展开及单命令确认策略。用户批准后的命令会在分发前冻结，后续目标或输入变化无法修改已经授权的内容。
- 敏感参数值使用短生命周期受保护缓冲区，不进入持久化快捷命令定义或诊断输出；可能暴露敏感值的展开预览会被隐藏。风险分级与确认针对最终展开后的命令执行。
- 扩展桌面界面、CLI、插件、Public MCP、终端触发器、Cloud Sync 及 `.oxide` 传输中的快捷命令管理。分类和命令现可明确排序，桌面导出采用原子快照替换。

#### 🤖 AI 服务兼容性

- 统一 OpenAI-compatible 模型发现、模型选择器就绪状态与聊天执行。对本地及私网服务的就绪探测现会复用可选的系统凭据和相同的 `/v1` 回退规则，同时继续允许选择兼容的无密钥网关。

#### 🔐 SSH、Kerberos 与连接身份

- 在 macOS、Linux 和 Windows 上新增优先使用当前操作系统凭据的 Kerberos SSH 认证，并提供常规认证回退、可选服务端身份、凭据可用性提示，以及明确的凭据委派警告与开关。
- 新增密钥交换、主机密钥、加密算法、MAC 及压缩类别的 SSH 协商顺序编辑，并提供分阶段连接进度、已脱敏连接追踪，以及可在不取消连接的情况下关闭的进度卡片。
- 新增操作系统连接协议处理及外部链接开关，支持 `ssh://`、`telnet://`、`mosh://`、`rdp://` 和 `vnc://`，并可将第二次应用启动收到的链接安全交付到现有工作区。
- 修复已保存连接的复用规则：只有路由端点、完整代理路径、运行时所有者及工作区所有者全部匹配时，才会聚焦已有逻辑节点或终端。即使多个配置共用相同主机、用户名和端口，逻辑终端仍彼此隔离；端口不同的连接也会使用不同的物理连接身份。
- 在同步连接元数据新增或修改 Kerberos 设置时保留本机认证材料，避免元数据更新丢弃设备本地的密码或口令。

#### 🖥️ 远程桌面与 SFTP

- 扩展远程桌面文件传输，新增远端浏览、根目录与上级目录导航、下载、进度与取消、目标位置选择，以及覆盖、重命名或跳过冲突策略。
- 扩展 VNC 安全兼容性，支持 TLS 上的 VeNCrypt Plain 与 SASL 认证及经过验证的 X.509 变体，并根据凭据选择可用子类型，对协议交换设置边界，同时以受保护方式处理用户名和密码。
- 新增 RDP 网络质量配置，可选择自动检测、局域网、宽带或低带宽链路，使视觉效果与带宽提示更符合当前连接。
- 优化 SFTP 上传缓冲区；加强流式归档传输的有界错误、空闲超时、取消、清理及解包字节进度；同时避免较慢的终端工作目录探测过早中止本来正常的 SFTP 工作流。

#### ☁️ 备份与同步工作流

- 将本地加密 `.oxide` 导入与导出移入 Cloud Hub，使本地备份与云同步集中在同一位置，同时在未配置云服务商时仍可使用。
- 扩展同步的快捷命令与 SSH 元数据，并继续将设备本地敏感值排除在可移植快照之外，在兼容更新中保留这些值。

![OxideTerm 2.0.24 release highlights: native tmux, parameterized Quick Commands, session logging, Kerberos SSH, and a faster terminal](https://raw.githubusercontent.com/AnalyseDeCircuit/oxideterm/v2.0.24/.github/release-notes/assets/oxideterm-2.0.24-release-highlights.png)

![OxideTerm terminal throughput before optimization, in 2.0.23, and in 2.0.24](https://raw.githubusercontent.com/AnalyseDeCircuit/oxideterm/main/.github/release-notes/assets/terminal-performance-2.0.24-comparison.png)

## 2.0.23

### English

OxideTerm 2.0.23 adds terminal-output triggers, substantially improves terminal throughput, strengthens cross-device Cloud Sync conflict handling, and fixes several SSH, Quick Command, semantic-highlighting, editor, and native-window workflows.

#### ✨ Terminal Triggers and Toolbar Workflows

- Added a global trigger rule library with per-session temporary toggles. Rules can be managed from Terminal settings and opened directly from the terminal toolbar, context menu, or Command Palette without losing the source pane.
- Added literal and regular-expression matching with case sensitivity, whole-word matching, named captures, immediate or next-line execution, optional delay, and a per-rule cooldown to prevent echo loops and repeated bursts.
- Added trigger actions for sending text to the matching terminal, running an existing Quick Command, and launching a local program. Direct-program arguments preserve argument boundaries, while Shell execution requires a separate explicit authorization and warning.
- Added scopes for all terminals, local terminals, or selected saved SSH, Telnet, Mosh, and Serial connections. Delayed and confirmed actions remain attached to the pane that produced the match and are cancelled if the pane, session, or rule is no longer valid.
- Kept trigger scanning incremental and bounded across fragmented UTF-8, ANSI control sequences, line rewrites, and chunk boundaries. Sessions without active rules retain a dedicated no-op path and do not enable full terminal-output events.
- Consolidated **Record Session** and **Open Recording** into one terminal-toolbar control with a compact action popover.

#### ⚡ Terminal Performance

- Optimized terminal output processing and high-density scrolling by eliminating repeated scanning, copying, scheduling, layout, and rendering work while preserving protocol detection and terminal compatibility.
- In OxideTerm's reproducible 16 MiB release-build benchmark, median process-to-PTY throughput increased from 3.144 to 69.031 MiB/s for plain text, from 7.330 to 88.384 MiB/s for ANSI output, from 4.393 to 68.711 MiB/s for Unicode output, and from 10.191 to 96.685 MiB/s for long CSI sequences. These results correspond to speedups of 21.96×, 12.06×, 15.64×, and 9.49× respectively.
- The comparison used the same fixture size, one warm-up, and the median of three measured runs. It measures process-to-PTY throughput rather than completed rendering or input latency.

#### ☁️ Cloud Sync Reliability

- Corrected structured synchronization when another device publishes an empty section, so deletions and empty baselines are applied instead of being skipped and later reported as false conflicts.
- Repaired conflict preview selection for empty connection, Quick Command, forwarding, profile, credential, application-setting, and plugin-setting sections.
- Kept **Pull & Preview** available when a real conflict needs inspection, and added a separately confirmed **Force Upload** choice for users who intentionally want the local state to replace the remote state.

#### 🔐 SSH and Connection Workflows

- Imported OpenSSH `RemoteCommand` into the existing post-connect command field, including supported token expansion and source-aware refresh behavior when `~/.ssh/config` changes.
- Fixed editing saved password authentication: typing or pasting now replaces the keychain-backed value without loading it into the form, while clearing an unfinished replacement restores the protected saved-password placeholder.
- Preserved jump-host and proxy-chain routes when saved connections are edited, synchronized, imported, exported, or opened through supported public connection workflows.
- Reorganized advanced SSH explanations into the existing information popovers for connection timeout, per-terminal authentication, agent forwarding, X11 forwarding, and legacy-server compatibility, reducing permanent form text without removing guidance.

#### 🛠️ Terminal, Editor, and Window Fixes

- Preserved semantic highlighting across wide and multi-column characters, including complete localized dates such as `6月22` instead of styling only part of the value.
- Restored scrollable Quick Command category lists and the confirmation view for commands that require approval, including long commands that exceed the visible panel height.
- Restored persisted normal, maximized, and full-screen window state after Wayland and macOS window-state transitions.
- Painted editor selections across empty lines and clipped horizontally scrolled editor content at the gutter boundary.

#### 🧰 Linux Build Maintenance

- Removed obsolete GTK, JavaScriptCoreGTK, Soup, and WebKitGTK development packages from the Linux dependency installer while retaining the DBus development files required by system-keyring integration. OxideTerm's native GPUI interface continues to render directly to the GPU and does not require a WebView runtime.

### 中文

OxideTerm 2.0.23 新增终端输出触发器，显著提升终端吞吐量，加强跨设备云同步冲突处理，并修复多项 SSH、快捷命令、语义高亮、编辑器和原生窗口问题。

#### ✨ 终端触发器与工具栏工作流

- 新增全局触发器规则库及当前会话临时开关。用户可在终端设置中管理规则，也可从终端工具栏、右键菜单或命令面板直接进入，并保持来源窗格不变。
- 新增普通文本与正则表达式匹配，支持区分大小写、完整单词、命名捕获、立即或下一次换行时执行、可选延迟，以及用于避免回显循环和短时重复触发的规则级最小间隔。
- 新增向匹配终端发送文本、运行已有快捷命令及启动本机程序的触发动作。直接程序模式会保持参数边界；Shell 执行则必须单独明确授权并经过风险提示。
- 新增全部终端、本地终端或指定已保存 SSH、Telnet、Mosh、串口连接的适用范围。延迟及待确认动作始终绑定产生匹配的窗格；当窗格、会话或规则不再有效时会自动取消。
- 触发器扫描可在拆分的 UTF-8、ANSI 控制序列、行覆写及输出分块之间进行有界增量处理。没有活动规则的会话继续使用专用空操作路径，也不会开启完整终端输出事件。
- 将“录制会话”和“打开录制”合并为一个终端工具栏控件，并通过紧凑气泡菜单选择具体操作。

#### ⚡ 终端性能

- 优化终端输出处理及高密度滚动，减少重复扫描、复制、调度、布局和绘制工作，同时保持协议探测能力及终端兼容性。
- 在 OxideTerm 可复现的 16 MiB 发布构建基准中，纯文本的进程到 PTY 吞吐量中位数由 3.144 MiB/s 提升至 69.031 MiB/s，ANSI 输出由 7.330 MiB/s 提升至 88.384 MiB/s，Unicode 输出由 4.393 MiB/s 提升至 68.711 MiB/s，长 CSI 序列由 10.191 MiB/s 提升至 96.685 MiB/s，分别达到原来的 21.96 倍、12.06 倍、15.64 倍和 9.49 倍。
- 对比采用相同的测试数据规模、一次预热及三次测量的中位数。该结果衡量进程到 PTY 的吞吐量，不代表绘制完成耗时或输入延迟。

#### ☁️ 云同步可靠性

- 修复其他设备发布空结构化分区时的同步行为，使删除结果和空基线能够正确应用，不再被跳过并在后续同步中误报冲突。
- 修复空连接、快捷命令、端口转发、各类配置、凭据、应用设置及插件设置分区的冲突预览选择。
- 当真实冲突需要检查时保持“拉取并预览”可用，并为确实希望用本地状态替换远端状态的用户新增需要单独确认的“强制上传”选项。

#### 🔐 SSH 与连接工作流

- 将 OpenSSH 的 `RemoteCommand` 导入现有连接后命令字段，并支持相应的令牌展开，以及 `~/.ssh/config` 变化时按配置来源更新。
- 修复已保存密码认证的编辑流程：输入或粘贴新密码时会替换钥匙串中的值，而不会先将旧密码载入表单；清空尚未完成的替换内容时会恢复受保护的已保存密码占位状态。
- 在编辑、同步、导入导出已保存连接，以及通过受支持的公共连接工作流打开连接时，完整保留跳板主机与代理链路由。
- 将连接超时、每个终端单独认证、SSH Agent 转发、X11 转发及旧版服务器兼容性的高级说明统一收进现有信息提示中，在保留帮助内容的同时减少表单中的常驻文字。

#### 🛠️ 终端、编辑器与窗口修复

- 修复宽字符和多列字符两侧语义高亮丢失的问题，使 `6月22` 等本地化日期能够完整着色，而不是只处理其中一部分。
- 恢复快捷命令分类列表的滚动及需要批准的命令确认界面，并正确处理超过面板可见高度的长命令。
- 修复 Wayland 与 macOS 窗口状态切换后普通、最大化和全屏状态无法正确持久化的问题。
- 修复编辑器选区跨越空行时未完整绘制的问题，并在水平滚动时将编辑器内容正确裁剪在行号栏边界内。

#### 🧰 Linux 构建维护

- 从 Linux 依赖安装脚本中移除已不再需要的 GTK、JavaScriptCoreGTK、Soup 和 WebKitGTK 开发包，同时保留系统钥匙串集成所需的 DBus 开发文件。OxideTerm 的原生 GPUI 界面继续直接通过 GPU 绘制，不依赖 WebView 运行时。

![OxideTerm release-build terminal throughput before and after optimization](https://raw.githubusercontent.com/AnalyseDeCircuit/oxideterm/v2.0.23/.github/release-notes/assets/terminal-performance-release-comparison.png)

## 2.0.22

### English

OxideTerm 2.0.22 is a feature-heavy update with several subsystem-level additions. It introduces independent and remote-to-remote SFTP workflows, a complete configurable terminal semantic-coloring system, richer saved connection profiles, and broad terminal, remote-desktop, editor, and native-window improvements while intentionally remaining on the 2.0 release line.

#### ✨ Standalone SFTP and Connection Workflows

- Added standalone SFTP profiles for cases where an SFTP endpoint must not reuse a saved SSH host, including servers with different SSH and SFTP ports or credentials.
- Added both local-to-remote and remote-to-remote transfer modes. Remote-to-remote profiles expose two independently authenticated endpoints, each with its own host, port, username, authentication method, initial directory, timeout, `ProxyCommand`, and upstream proxy settings.
- Added a dedicated advanced SFTP workspace that reuses the dual-pane file-transfer experience, keeps endpoint settings visually separated, and supports creating, saving, reopening, and editing standalone profiles.
- Added application-managed relays between two remote SFTP endpoints with bounded buffering and concurrency, so large transfers do not require loading an entire file or directory into memory.
- Added persisted restart recovery for single-file remote relays. Resume validates the profile, both endpoints, source identity, partial destination, and staging metadata before continuing; directory relays retain bounded in-session scheduling but do not yet resume after an application restart.
- Integrated standalone SFTP profiles with the Session Manager and connection-opening flows and preserved them through `.oxide` import and export, Cloud Sync, and the shared connection data model.
- Added a compact editable path field to the SFTP sidebar with remote directory completion, keyboard navigation, direct path submission, and synchronization with normal folder navigation without using the wider breadcrumb component.
- Added optional multiline notes to every saved connection type except local terminals. Notes survive editing, import and export, Cloud Sync, and public or command-line connection data paths, with a clear warning not to store passwords, private keys, or other credentials.
- Added SSH `ProxyCommand` configuration and persistence, and reorganized connection forms so transport selection precedes the endpoint-specific fields it controls.

#### 🎨 Terminal Semantic Coloring and Highlighting

- Added a semantic-coloring engine for otherwise unstyled terminal text while preserving explicit ANSI foreground colors, backgrounds, and text attributes emitted by terminal applications.
- Added distinct semantic classes and colors for commands, keywords, options, operators, strings, variables, comments, links, paths, network addresses, timestamps, numbers, errors, warnings, successes, and informational values instead of routing unrelated matches through one accent color.
- Expanded structured recognition for IPv4 and IPv6 addresses, MAC addresses, UUIDs, POSIX and Windows paths, URLs, localized dates, 12-hour and 24-hour times, elapsed and long-duration values, assignments, `key=value` fields, and common log status expressions.
- Added command syntax recognition for Bash, Zsh, Fish, and PowerShell and a specialized `ps` output parser that distinguishes process identifiers, resource values, states, dates, elapsed times, options, assignments, commands, and paths by column and context.
- Added punctuation and operator styling for pipes, assignment signs, separators, asterisks, and related shell symbols, including separate punctuation inside structured values such as the colon in a time.
- Added balanced multi-level bracket coloring for parentheses, square brackets, braces, and supported full-width pairs. Nested levels cycle through distinct colors, and unmatched delimiters are handled without recursively scanning the terminal buffer.
- Added built-in balanced and conservative schemes plus user-managed custom semantic schemes with per-class colors and create, import, export, select, and delete workflows.
- Added transient command-context highlighting that extracts literal query terms from `grep` and `rg`, limits each term to the corresponding command output block, and automatically expires it for later commands instead of creating a persistent rule.
- Reorganized the terminal highlight menu into semantic coloring, persistent keyword or regular-expression rules, and `grep` or `rg` command-context highlighting. Each layer can be controlled independently and overridden for the active session.
- Semantic coloring is disabled by default and remains available as an explicit global or per-session choice from the terminal highlight controls.
- Updated terminal row timestamps with clearer bracketed presentation while keeping timestamp display separate from text semantic coloring.

#### ⚙️ Compatibility, Performance, and Reliability

- Reduced redundant terminal scheduling, layout, and rendering work; bounded output draining and byte-aware backpressure; and reused scroll snapshots, rows, and layout results more effectively during high-output sessions.
- Kept semantic analysis limited to relevant visible logical lines, reused compiled schemes and cached layouts, and skipped semantic work entirely when the feature is disabled or the terminal is in an alternate-screen application.
- Added an RDP graphics-pipeline compatibility option for servers that render Progressive RemoteFX incorrectly, allowing those profiles to fall back to bitmap updates without changing the endpoint itself.
- Compactly reorganized remote-desktop session feature controls while retaining saved RDP and VNC behavior.
- Rebuilt stale GPU scenes after graphics-device recovery, stabilized Windows fallback-font identity, and applied the configured system-font fallback consistently.
- Restored native Linux client-window edge resizing and added a horizontal overflow scrollbar to the editor for long lines and wide content.
- Restored printable terminal input when the active shell remains visible but Workspace temporarily owns keyboard focus, matching the existing Enter, Tab, navigation, and protocol-key fallback without intercepting shortcuts or IME composition.
- Preserved shifted text correctly in Kitty keyboard mode, kept long Quick Commands editable, and prevented forwarding statistics events from feeding back into repeated updates.
- Preserved stored terminal options when command-line connection specifications are cloned.
- Preserved backslashes in Windows drive-letter and UNC private-key paths imported from SSH configuration files.
- Added copyable external MCP stdio JSON configuration so supported external clients can be connected without manually reconstructing the launch configuration.
- Refined the local-terminal form and editor selection behavior for consistency with the other connection workflows.

#### 🧰 Release Maintenance

- Consolidated redundant and low-value tests, replaced repeated setup with shared defaults or builders, and moved runtime-dependent PTY, network, thread, and platform behavior into integration coverage while retaining security, credential, persistence, migration, protocol-compatibility, and data-loss protection.
- Restored standalone SFTP synchronization snapshots and localization completeness checks across supported languages.
- Made stable release notes bilingual, with aligned English and Chinese sections for each new release.

### 中文

OxideTerm 2.0.22 是一次包含多项子系统级新增能力的功能密集型更新。在继续保持 2.0 版本线的同时，本次更新引入独立及双远程 SFTP 工作流、完整且可配置的终端语义着色系统、更丰富的连接配置，并广泛改进终端、远程桌面、编辑器和原生窗口体验。

#### ✨ 独立 SFTP 与连接工作流

- 新增独立 SFTP 配置，适用于 SFTP 端点不应复用已保存 SSH 主机的场景，包括 SSH 与 SFTP 端口或凭据不同的服务器。
- 新增本地与远程、远程与远程两种传输方式。双远程配置包含两个独立认证的端点，每个端点都可分别设置主机、端口、用户名、认证方式、初始目录、超时、`ProxyCommand` 和上游代理。
- 新增专用高级 SFTP 工作区，复用双栏文件传输体验，清晰分隔不同端点的设置，并支持创建、保存、重新打开和编辑独立配置。
- 新增两个远程 SFTP 端点之间由应用管理的中继，并限制缓冲区及并发量，传输大文件或大型目录时无需将全部内容一次性载入内存。
- 新增单文件远程中继的持久化重启恢复。续传前会校验配置、两个端点、源文件身份、部分目标文件及暂存元数据；目录中继继续使用会话内的有界调度，但暂不支持应用重启后续传。
- 将独立 SFTP 配置接入会话管理器和连接打开流程，并在 `.oxide` 导入导出、云同步及共享连接数据模型中完整保留。
- 在 SFTP 侧栏新增紧凑的可编辑路径，支持远程目录补全、键盘导航、直接提交路径，并与普通目录导航同步，无需复用占用空间较大的面包屑组件。
- 为除本地终端外的所有已保存连接类型新增可选多行备注。备注可在编辑、导入导出、云同步及公共或命令行连接数据链路中保留，并明确提醒不要存储密码、私钥或其他凭据。
- 新增 SSH `ProxyCommand` 的配置和持久化，并重新组织连接表单，使传输方式选择先于其控制的端点表单展开。

#### 🎨 终端语义着色与高亮

- 新增用于处理终端无样式文本的语义着色引擎，同时保留终端应用显式发送的 ANSI 前景色、背景色及文字属性。
- 为命令、关键字、选项、运算符、字符串、变量、注释、链接、路径、网络地址、时间戳、数字、错误、警告、成功和信息值提供独立语义类别与颜色，不再让无关内容共用同一种强调色。
- 扩展 IPv4、IPv6、MAC 地址、UUID、POSIX 与 Windows 路径、网址、本地化日期、十二小时制与二十四小时制时间、运行时长、长耗时、赋值、`key=value` 字段及常见日志状态短语的结构化识别。
- 新增 Bash、Zsh、Fish 和 PowerShell 命令语法识别，并加入专用的 `ps` 输出解析器，可按列和上下文区分进程标识、资源数值、状态、日期、运行时间、选项、赋值、命令和路径。
- 新增管道符、赋值号、分隔符、星号及相关 Shell 符号的标点和运算符着色，并可独立处理时间中冒号等结构化值内部的标点。
- 新增圆括号、方括号、花括号和受支持全角括号的多层平衡着色。嵌套层级会循环使用不同颜色，未配对分隔符也不会触发对整个终端缓冲区的递归扫描。
- 新增内置的均衡与保守方案，并支持用户管理自定义语义方案，可按类别设置颜色以及创建、导入、导出、选择和删除方案。
- 新增临时命令上下文高亮，可从 `grep` 和 `rg` 提取字面查询词，仅在对应命令的输出块内生效，并在后续命令中自动失效，而不是创建永久规则。
- 将终端高亮菜单重新组织为语义着色、持久化关键词或正则规则、`grep` 或 `rg` 命令上下文高亮三层。每层均可独立控制，并可对当前会话进行覆盖。
- 语义着色默认关闭，用户可从终端高亮控制中明确选择全局开启或仅对当前会话开启。
- 更新终端行时间戳的括号样式，同时继续将时间戳界面与文本语义着色分开处理。

#### ⚙️ 兼容性、性能与可靠性

- 减少终端调度、布局和渲染中的重复工作，限制高输出场景的单次排空量并采用按字节计算的背压，同时更有效地复用滚动快照、行和布局结果。
- 将语义分析限制在相关的可见逻辑行，复用已编译方案和布局缓存，并在功能关闭或终端运行全屏交互应用时完全跳过语义处理。
- 新增 RDP 图形管线兼容选项；当服务器无法正确渲染 Progressive RemoteFX 时，可让相应配置回退到位图更新，而无需修改端点本身。
- 紧凑重组远程桌面会话功能控制，同时保留已保存的 RDP 与 VNC 行为。
- 修复图形设备恢复后 GPU 场景未正确重建的问题，稳定 Windows 后备字体标识，并一致应用用户配置的系统字体回退。
- 恢复 Linux 原生客户端窗口的边缘缩放，并为编辑器长行和宽内容增加横向溢出滚动条。
- 修复活动终端仍可见但键盘焦点暂时由工作区持有时普通字符无法输入的问题，使其与已有的回车、制表、导航和协议按键后备路径一致，同时不拦截快捷键或输入法组合输入。
- 修复 Kitty 键盘模式中的移位文本保留，确保较长的快捷命令仍可编辑，并阻止端口转发统计事件形成重复更新回路。
- 在复制命令行连接规格时保留已存储的终端选项。
- 修复从 SSH 配置导入 Windows 盘符路径和 UNC 私钥路径时反斜杠丢失的问题。
- 新增可复制的外部 MCP 标准输入输出 JSON 配置，使受支持的外部客户端无需手工重建启动参数即可连接。
- 调整本地终端表单和编辑器选区行为，使其与其他连接工作流保持一致。

#### 🧰 发布维护

- 合并重复及低价值测试，使用共享默认值或构建器替代重复初始化，并将依赖 PTY、网络、线程和平台运行时的行为移至集成测试，同时保留安全、凭据、持久化、迁移、协议兼容和数据丢失防护测试。
- 恢复独立 SFTP 的同步快照及全部受支持语言的本地化完整性检查。
- 将稳定版发布说明改为中英双语，并让每个新版本的英文与中文内容逐项对应。

## 2.0.21

OxideTerm 2.0.21 adds controlled external MCP access, expands ACP and plugin interoperability, and improves SFTP, remote-desktop, Cloud Sync, and native window workflows.

### ✨ Highlights

- Added authenticated External MCP Control for editors, command-line agents, and other clients, with loopback HTTP and stdio access to explicitly granted connection, terminal, SFTP, transfer, forwarding, remote-desktop, recording, IDE workspace, Quick Command, addon, Cloud Sync, and Host Tools workflows.
- Added ACP presets for Gemini CLI and OpenCode, and allowed the built-in Codex and Claude Code adapters to consume session-scoped HTTP MCP servers without placing authorization values in process arguments.
- Added a searchable native plugin marketplace with compatibility, platform, checksum, installation, and update checks, and moved the complete Host Tools Dashboard example into the official plugin catalog.
- Added an embedded SFTP sidebar browser with a remembered sidebar, tab, or ask-each-time presentation preference while keeping transfers and shared SSH connections owned independently from the visible surface.
- Added SSH jump and proxy-chain support to saved RDP and VNC connections, direct CLI opening of saved SSH connections, and complete Telnet profile coverage in `.oxide` import, export, and Cloud Sync.

### 🛠️ Fixes

- Restored the main window's normal bounds, maximized state, and full-screen state across launches, retained valid secondary-display placement, and recentered windows that would otherwise reopen off screen.
- Initialized the OneDrive application folder before creating nested Cloud Sync objects, reused existing parents, and recovered cleanly when another client created the same folder concurrently.
- Preserved every saved connection profile type during structured synchronization and kept background MCP mutations, cancellations, revocations, and dependent refreshes aligned with their committed application state.
- Removed the unintended divider above installed plugins and retired the obsolete downloadable Wasm runtime installer from builds that do not include Wasm plugin support.

### 🔒 Security

- Kept external MCP credentials digest-only and device-local, required explicit tool-group grants, retained in-app approval for elevated access requests, and cancelled active work and released capabilities immediately when access is revoked.

## 2.0.20

OxideTerm 2.0.20 adds native Mosh and local-terminal workflows, refreshes session and start-page management, improves remote-desktop and Cloud Sync reliability, and expands terminal feedback and SSH controls.

### ✨ Highlights

- Added native Mosh sessions with SSH bootstrap, adaptive prediction, saved-profile editing, Session Manager and sidebar integration, `.xoide` import and export, and Cloud Sync support.
- Redesigned the start page around recent connections, local terminals, imports, Session Manager, and Cloud Sync, with responsive layouts for narrow workspaces and a clearer view of standalone active connections.
- Expanded Session Manager selection and batch group operations to saved serial, Telnet, Mosh, and remote-desktop profiles, and added complete in-place editing for serial and Telnet settings without disturbing active sessions.
- Added millisecond terminal timestamps, richer matched-text or logical-line highlight controls, and an optional accent for background tabs with unread terminal output.
- Added direct local-terminal launch from the connection flow, refined the terminal Git workflow with inline commit messages and clearer action grouping, and updated DeepSeek v4 reasoning support.
- Redesigned the connection runtime overview around pool usage, consumers, health attention, and refresh status, and allowed approved OxideSens tools to update non-secret Cloud Sync configuration.

### 🛠️ Fixes

- Improved RDP automatic reconnect and frame uploads, preserved sparse update regions through presentation, and reduced RDP and VNC work for incremental framebuffer changes.
- Repaired HTTP JSON Cloud Sync revision baselines, preserved configuration drafts across provider changes, reused snapshot encryption keys within one operation, and batched macOS credential authorization during imports.
- Made SSH connection timeouts configurable across saved connections, SSH config import, `.xoide` transfer, Cloud Sync, and CLI specifications while preserving imported timeout values.
- Preserved serial and Telnet profile identity and automatic-open metadata when editing, and ensured Telnet changes mark Cloud Sync state dirty.

## 2.0.19

OxideTerm 2.0.19 improves native window and terminal behavior, repairs Windows PowerShell and OneDrive compatibility, and simplifies supported update channels.

### ✨ Highlights

- Added native full-screen shortcuts with F11 on Windows and Linux and Control-Command-F on macOS, exposed the action in shortcut settings, and made macOS title-bar double-clicks follow the system preference.

### 🛠️ Fixes

- Prevented Windows PowerShell 5.1 from displaying OSC 7 directory-reporting bytes as visible `e]7;...` text in local and remote Shell integration.
- Made detached tab windows movable through native Windows caption handling, and made closing a detached window close its tab and local terminal instead of leaving the terminal active in the background; shared SSH node connections remain independently owned.
- Kept typed keys and pasted text flowing to selected broadcast terminals without re-entering the GPUI workspace entity during synchronous input delivery.
- Repaired OneDrive app-folder hierarchy creation by sending Microsoft Graph conflict behavior through the supported request URL annotation.

### 🧰 Release Maintenance

- Retired the GPUI Preview channel from runtime settings, updater selection, help content, and plugin compatibility while preserving Stable and Beta updates and safely migrating older saved channel settings.

## 2.0.18

OxideTerm 2.0.18 makes remote Shell integration safer across terminal clients, restores terminal and SSH interaction workflows, and repairs Windows Cloud Sync and OneDrive behavior.

### ✨ Highlights

- Migrated remote SSH directory reporting to standard OSC 7 while retaining legacy protocol parsing and in-place upgrades for existing integration packages; private editor enhancements now activate only for marked OxideTerm channels and safely remain unavailable when the server rejects the optional marker.
- Added complete editing for saved SSH proxy chains, preserving unchanged hop credentials, copying credentials into independent owners when needed, and resolving saved proxy authentication when connecting.

### 🛠️ Fixes

- Restored terminal input broadcasting for typed text, protocol keys, and paste operations without rebroadcast loops or widening AI and credential input scope.
- Prevented Tab Rename input from reaching the terminal behind its dialog, restored pane focus after the dialog closes, and stopped UTF-8 prompt glyphs such as `❯` from being mistaken for terminal control bytes.
- Preserved terminal-driven SFTP directory navigation when shared-session readiness or an older listing completes after a newer path request.
- Recovered valid Windows credentials when malformed chunk metadata exists, allowed damaged metadata to be replaced or deleted, and retained safe platform error context for Cloud Sync diagnosis.
- Repaired OneDrive app-folder uploads with supported create-conflict semantics, more precise permission and conflict classification, and actionable operation and request identifiers.

### 🔒 Security

- Suppressed private editor OSC messages in tmux, GNU screen, and Zellij shared panes, removed private OSC clipboard payloads from terminal recordings across fragmented and oversized messages, and zeroized captured payload buffers after use.

## 2.0.17

OxideTerm 2.0.17 strengthens Windows SSH, SFTP, and RDP workflows, adds terminal tab naming and optional SSH close-confirmation suppression, and improves AI provider compatibility and settings polish.

### ✨ Highlights

- Added terminal tab renaming from attached or detached windows while preserving the existing pane and session.
- Added direct custom model ID entry for AI providers and consolidated system monitoring into the Host Tools sidebar instead of a duplicate full-page health view.
- Added an opt-out checkbox to SSH close and disconnect confirmations, persisting the preference only after the user confirms the action.

### 🛠️ Fixes

- Preserved credentials entered during SSH connection tests when saving a connection, allowed unavailable X11 forwarding to fall back to a normal shell, and applied saved X11 policy changes to subsequent shells without restarting the application.
- Made RDP display resizing recover through a controlled reconnect with stale-frame isolation instead of leaving the remote view stalled or incorrectly sized.
- Prevented SFTP downloads from silently overwriting local files, strengthened collision and resume validation, and restored text-editor keyboard input on Windows.
- Stored oversized Windows Credential Manager values, including long OneDrive tokens, in bounded chunks and restored them transparently.
- Kept AI model chips visible in narrow provider cards, showed successful SSH connection tests with the success color, and stopped broad conversational wording from forcing unnecessary AI tool calls.

### 🧰 Release Maintenance

- Added an artifact-repair workflow that can republish successfully built native packages without rebuilding unaffected platforms.
- Updated issue intake to close reports from superseded major versions with a bilingual upgrade notice while retaining reminders for older stable patch releases.

## 2.0.16

OxideTerm 2.0.16 adds searchable settings, contextual nested session groups, and secure SSH X11 forwarding while improving terminal startup, remote previews, remote-desktop compatibility, and modem transfers.

### ✨ Highlights

- Added localized settings search with ranked results and direct navigation to matching sections and controls.
- Expanded Session Manager group management with contextual creation, child-group creation without typing full paths, rename and delete actions, and made the tree layout the default and first view option.
- Added persistent trusted and untrusted X11 forwarding for SSH connections, including OpenSSH configuration import and synchronization, local display and xauth preparation, spoofed-cookie handling, forwarding timeouts, and connection-owned channel routing.
- Allowed user-configured MCP server commands without a built-in executable allowlist while retaining structural validation and explicit settings ownership.

### 🛠️ Fixes

- Restored output-independent terminal polling so first and subsequent SSH terminal panes no longer wait for remote output or the deferred PTY timeout before applying their real viewport size.
- Kept terminal processing bounded under heavy output with incremental snapshots, asynchronous search and image preparation, render caches, and byte-aware backpressure while preserving responsive startup scheduling.
- Reused the full text editor for SFTP text and Markdown previews, restoring consistent keyboard, selection, scrolling, and rendering behavior.
- Accepted RDP progressive graphics updates that legitimately omit an optional context and launched local Zsh sessions as non-login shells to avoid duplicate login initialization.
- Strengthened X/Y/ZMODEM transfers with transactional downloads, collision-safe file persistence, stricter frame validation, peer cancellation, resume handling, and safer filename processing.

## 2.0.15

OxideTerm 2.0.15 substantially expands OxideSens with application-aware tools, background tasks, Agent Skills, and modern MCP interoperability while improving AI context accuracy, terminal efficiency, and remote-session reliability.

### ✨ Highlights

- Expanded OxideSens tools across interactive terminal control, prompt and command completion waits, RDP, VNC, serial and Telnet sessions, Host Tools, forwarding, plugins, Cloud Sync, credentials, and scoped memory, with the same capabilities available to ACP backends through the application bridge.
- Added cancellable one-shot, interval, and condition-based background tasks owned by each conversation, including visible task state and bounded execution rules.
- Added bounded Agent Skills discovery, explicit skill and resource loading, chat completion, settings visibility, and a portable `skills` directory that is preserved across application updates.
- Added dual-era MCP negotiation that discovers modern servers first and falls back to legacy initialization, with protocol-specific tool and resource handling for compatibility across server generations.

### 🛠️ Fixes

- Corrected AI context accounting across system instructions, tool definitions, skills, ACP handoff, compaction, and continuation turns, and changed conversation history badges to report logical user turns instead of raw stored message rows.
- Improved the AI input surface with correctly layered skill and mention completion, clearer context-capacity feedback, responsive status controls, stable IME composition, and multiline placeholders that no longer interfere with editing.
- Added terminal control and navigation keys, TUI-aware output waits, and tracked command completion and exit status so AI workflows can operate interactive programs without relying on blind text submission.
- Reduced terminal memory and high-output pressure by loading the optional CJK font only when required, coalescing redundant wakeups, and bounding layout caching.
- Restored authentication for additional SSH terminal sessions, SFTP editor keyboard ownership, and Cloud Sync form focus, and updated the remote-desktop codec dependency for broader progressive-codec compatibility.

### 🔒 Security

- Added a read-only AI safety mode and category-level tool policy controls, keeping observation separate from mutating, destructive, credential, and interactive operations while preserving explicit approval boundaries.

## 2.0.14

OxideTerm 2.0.14 unifies ACP and ordinary AI conversations, makes long chats and privilege prompts more reliable, and improves portable credentials, SSH Agent routing, terminal input, and remote compatibility.

### ✨ Highlights

- Integrated ACP agents into the existing OxideSens chat surface, allowing ordinary models and ACP backends to alternate within one conversation while preserving message ownership, incremental handoff, cancellation, and session lifecycle.
- Added an authoritative, capability-scoped AI runtime context so approved terminal, SFTP, IDE, connection, and workspace state is projected consistently without exposing internal runtime handles or unredacted secret-bearing data.
- Added configurable custom SSH Agent endpoints to new and edited SSH connections, including imported `IdentityAgent` values, while preserving automatic system Agent discovery when no override is selected.

### 🛠️ Fixes

- Rebuilt sudo and su password-prompt detection at the decoded local and SSH session boundary, so the first prompt is available immediately even for shell-history commands, split output, localized prompts, and retry flows without leaking full terminal output into the UI event path.
- Virtualized long AI conversation rendering, preserved per-message backend attribution, and limited tool-result progress indicators to the tool call that is actually being summarized.
- Migrated portable Vault credentials to stable account identifiers, allowing managed SSH keys, connection passwords, private-key passphrases, and privilege credentials to remain accessible after moving the portable directory to another machine or operating-system account.
- Restored clipboard editing in managed SSH key dialogs and added an optional right-click terminal paste setting alongside the existing middle-click behavior.
- Restored the compact command input layout and cursor behavior, cleared submitted commands correctly, and kept QuickBar commands on a single horizontal row.
- Updated the RDP ClearCodec dependency to decode valid short vertical-band regions that previously failed during compatible remote-desktop sessions.

### 🔒 Security

- Kept privilege-prompt classification independent from credential retrieval: terminal sessions emit only bounded semantic prompt events, while scoped secret lookup, confirmation, zeroization, and direct PTY writes remain owned by the application boundary.

## 2.0.13

OxideTerm 2.0.13 introduces an advanced terminal command sender, completes portable-mode updates and storage, and substantially strengthens native workspace ownership while resolving important Windows, remote-desktop, font, and AI compatibility issues.

### ✨ Highlights

- Added a unified terminal command sender with compact and expanded layouts, multiline text and hexadecimal input, line or character pacing, configurable intervals and repeat counts, multiple sender documents, progress and cancellation, and current, all, or explicitly selected terminal targets.
- Added per-connection terminal behavior overrides for text encoding and Backspace and Delete sequences, with persistence, import, cloud synchronization, and editing support while retaining application defaults when no override is selected.
- Enabled automatic updates for portable Windows builds and completed portable initialization for application data, secrets, plugins, logs, update staging, and runtime paths without writing those resources into installed-mode locations.
- Reworked native workspace ownership across terminals, tabs, settings, Host Tools, plugins, forwarding, AI, remote desktop, connection monitoring, and updates so background work is actively delivered and cancelled by its owning GPUI entity instead of being polled from the root renderer.

### 🛠️ Fixes

- Fixed Windows Server 2022 RDP sessions that failed while decoding valid single-color ClearCodec RLEX regions, and preserved the original protocol error instead of replacing it with a later closed-pipe message.
- Fixed Windows local file navigation when switching drives and kept keyboard path-completion selection visible as it moves beyond the current popup viewport.
- Corrected bundled JetBrains Mono family resolution and glyph metrics across native text backends, preventing abnormal terminal spacing when the built-in font is selected.
- Preserved Gemini signed tool-call parts across streaming and continuation turns, preventing valid tool workflows from losing provider-required metadata.
- Added AI conversation renaming, Enter-to-confirm editing, dated conversation and message timestamps, and multiline editing for memory and system prompts.
- Restored toggle behavior for active sidebar sections, preserved the previous sidebar state when opening Settings, centered compact command input, and moved the terminal performance overlay to the upper-right corner.
- Applied the OxideTerm application icon to the Windows installer and uninstaller interfaces as well as installed-app registration.

### 🧰 Release Maintenance

- Made terminal playback, SSH keepalive, and reconnect timers explicitly owned and cancelled by their GPUI entities, eliminating timing-dependent cross-test scheduler failures seen in parallel CI.
- Added the missing localized generic error status used by remote Shell integration and expanded packaging regression coverage for Windows installer icons and portable updates.

## 2.0.12

OxideTerm 2.0.12 substantially expands native remote desktop and SSH Agent workflows, adds richer asset and first-run appearance controls, and fixes several terminal, connection, and cloud-sync interactions.

### ✨ Highlights

- Expanded native RDP and VNC support with persisted session options, negotiated capability reporting, stronger VNC security policies, improved display and input handling, audio redirection, and richer clipboard interoperability.
- Added saved RDP and VNC assets to the session manager, including editing, selection, custom icons with independent foreground and background colors, and cloud synchronization alongside existing connection resources.
- Completed SSH Agent authentication and forwarding across supported endpoint types, including OpenSSH certificate identities, `ForwardAgent` and `IdentityAgent` imports, macOS Agent discovery, Windows named-pipe discovery, connection-pool isolation, and visible forwarding rejection errors.
- Expanded first-run appearance setup with bundled and custom terminal fonts, light and dark themes, background guidance, animation controls, and a continuous corner-radius slider.
- Added built-in light palettes and made password and private-key passphrase fields revealable while users enter or edit their own credentials.

### 🛠️ Fixes

- Fixed abnormal character spacing when the bundled JetBrains Mono font is selected on macOS by aligning embedded font metadata with the runtime family name.
- Preserved SSH Agent authentication and asset appearance settings when editing saved connections and other managed resources.
- Kept file-manager path completion navigation intact and added a terminal setting for recognizing local file paths as clickable links.
- Made keybindings explicitly removable without forcing a replacement shortcut.
- Corrected the cloud-sync automatic-upload toggle so the form reflects and saves the selected behavior consistently.

### 🧰 Release Maintenance

- Hardened GPUI draw lifetimes and platform dispatch behavior, expanded platform-focused checks, and documented the maintained GPUI-CE patch set.
- Added recoverable issue-maintenance automation and refined contribution and issue templates around the repository's current support policy.

## 2.0.11

OxideTerm 2.0.11 adds resilient SCP fallback and editor-aware Free Type Mode, improves IDE and terminal reliability, and avoids eager Metal memory allocation on macOS.

### ✨ Highlights

- Added legacy SCP as a compatibility transfer protocol for POSIX hosts, with an Auto mode that continues to prefer SFTP, explicit protocol settings, file and directory transfers, progress and cancellation, restart-aware retry behavior, and capability-gated plugin APIs.
- Expanded Free Type Mode with configurable Copy, Cut, and Paste actions, matched-bracket selection, true movement or copying of command selections, and insertion from command history while keeping the remote line editor authoritative.
- Validated ordinary, Emacs, and Vi command editing against real PTY-backed Bash, Zsh, and Fish sessions, and added opt-in Vim, Neovim, and Emacs adapters that expose bounded editor state without weakening alternate-screen or mouse-tracking protections.

### 🛠️ Fixes

- Prevented Sixel, Kitty graphics, iTerm image, and other terminal control-string payloads from being mistaken for ZMODEM, XMODEM, or YMODEM transfers, while preserving detection immediately after the control string ends.
- Completed the IDE folder path field with focus, IME composition, selection, clipboard actions, grapheme-safe deletion, and pointer positioning; aligned editor cursors and selections with shaped CJK and emoji text and made conflict overwrite reliably bypass stale version preconditions.
- Preserved Ctrl+B as the tmux prefix on Windows and Linux by moving the default broadcast shortcut to Ctrl+Shift+B.
- Made compact Host Tools monitoring responsive at narrow widths, kept network rates structured and readable, and reduced horizontal padding to 12 pixels at every sidebar width.
- Added operating-system name and version, architecture, boot time, and uptime to Host Tools system information.
- Fixed AI conversation titles collapsing to an ellipsis, prevented completed tool messages from retaining an active loading indicator, and kept streaming conversations pinned to the newest content unless the user scrolls upward.
- Allocated Metal path, scene, blur, and filter-group textures only when a frame needs them, avoiding full-window intermediate texture reservation for ordinary idle scenes and invalidating old resources without eager recreation after resize.

### 🧰 Release Maintenance

- Documented the Metal intermediate-texture lifecycle in the GPUI-CE vendor patch ledger and added regression coverage for ordinary scenes, independent path and filter allocation, and resize invalidation.
- Aligned every localized README with the main product overview, native-runtime comparison, CLI examples, technology stack, and host-key security guidance.
- Normalized the leading release-summary paragraph during note composition so source wrapping cannot create artificial line breaks in the GitHub Release editor.
- Made macOS DMG packaging track and detach the root disk-image device so APFS volume teardown cannot race the final compression step.

## 2.0.10

OxideTerm 2.0.10 opens a substantially broader native plugin platform, expands connection migration, and improves terminal, SFTP, and IDE workflows across the app.

### ✨ Highlights

- Expanded the native plugin API with useful redacted data by default plus capability-gated access to connection lifecycle, terminals, Host Tools, notifications, quick commands, complete theme tokens, IDE and AI operations, cloud sync, transfers, and other product controls.
- Added versioned, host-rendered plugin UI components together with plugin tabs, sidebar panels, activity-bar actions, bundled icons, and a complete Host Tools Dashboard example that demonstrates approved custom monitors.
- Added connection import for SecureCRT XML exports, Electerm JSON bookmarks, and FinalShell data folders, with a reviewable preview that excludes passwords and other secret material.
- Promoted Free Type Mode for ordinary shell command lines, including mouse cursor placement, selection replacement, drag editing, a configurable shortcut, and explicit Backspace and Delete sequence settings.
- Added a cross-platform window-opacity slider with persisted settings.
- Added authenticated application-owned Host Tools for ACP agents so approved tools can target explicit OxideTerm terminal sessions without exposing internal runtime handles.

### 🛠️ Fixes

- Added local and remote path completion to SFTP and local file-manager address bars, kept long breadcrumbs horizontally reachable, and prevented completion popups from leaking clicks or wheel input to the file list behind them.
- Made the SFTP local/remote split and transfer-queue height draggable, preserved independent pane sizing, documented the left/right transfer shortcuts in the queue, and refreshed local files without disturbing the remote session.
- Reworked IDE project search around native text input with focus, IME, selection, clipboard, and Unicode-safe deletion; reopening an existing IDE tab no longer asks for a folder again.
- Fixed IDE folder-picker cancellation, added missing toolbar hints and file-tree scrolling, and kept overflowing editor tabs horizontally scrollable with a visible draggable scrollbar.
- Corrected editor cursor, selection, and wrapping calculations for CJK text, emoji, and combining characters; added a visible document scrollbar and consistent monospace fallbacks.
- Aligned syntax-block guide lines with their closing braces instead of shifting them one indentation level into the block.
- Preserved legacy SSH compatibility fields while editing saved connections and improved session-manager selection, tab closing, and stale-action handling.
- Kept the right companion sidebar resizable against the actual maximized viewport after live Host Tool updates.
- Persisted welcome-flow legal acceptance, blocked click-through to the workspace behind the welcome layer, and added a direct entry for importing connections from other applications.

### 🧰 Release Maintenance

- Added policy coverage so manually reopened maintainer issues remain open instead of being closed again by the quality workflow.
- Added release-version consistency checks and expanded focused coverage for plugin permissions, connection imports, SFTP navigation, editor input, and Host Tools.

## 2.0.9

OxideTerm 2.0.9 adds multi-vendor GPU and NPU monitoring, expands native Markdown
compatibility, and improves reliability across sidebars, remote editing, and terminals.

### ✨ Highlights

- Added a GPU / NPU Host Tool with available per-device utilization, memory, temperature, power, health, and process visibility for NVIDIA, AMD ROCm, Hygon DCU, Huawei Ascend, Intel XPU, Moore Threads MUSA, and Cambricon accelerators.
- Added safe native rendering for common inline and block HTML in Markdown, including headings, tables, lists, links, details, alignment, highlighting, and subscript or superscript content without executing active HTML.

### 🛠️ Fixes

- Kept the right companion sidebar freely resizable after live Host Tool refreshes and removed the visible seam between the main workspace and sidebar.
- Stopped high-frequency accelerator polling after a host proves that no supported device is available, while retaining manual refresh for re-detection.
- Rebuilt IDE rename input around the shared text-input contract so the existing name, base-name selection, IME input, clipboard actions, validation, and localized errors behave consistently.
- Preserved CRLF line endings when opening, editing, and saving remote text files through the SFTP editor.
- Returned scrolled terminals to live output when the user types, without changing the viewport for terminal-owned protocol traffic.
- Restored the OxideTerm icon in Windows installed-app and uninstall surfaces.

### 🧰 Release Maintenance

- Split accelerator sampling into provider-owned adapters with focused parsing coverage for each supported vendor family.
- Added a standalone CC BY 4.0 license alongside the bundled background resources.

## 2.0.8

OxideTerm 2.0.8 unifies application proxy routing, strengthens SSH forwarding and
SFTP transfer ownership, and makes network configuration easier to understand.

### ✨ Highlights

- Reorganized Network & Proxy settings around one reusable upstream proxy and explicit routing choices for SSH connections, application requests, and update checks.
- Added system, direct, and shared-proxy routing for application HTTP traffic, covering OxideSens, cloud sync, plugin services, and runtime downloads.

### 🛠️ Fixes

- Preserved explicitly requested remote-forwarding ports when SSH servers return an empty allocation field, while continuing to use server-assigned ports for dynamic allocation.
- Kept local, dynamic, and remote forwarding work owned by their rules so stopping a forward also cancels its listeners, handshakes, bridges, and server registration cleanly.
- Corrected remote-forward health checks to validate the local target and restored uploads to virtual SFTP gateways that reject temporary sibling files.
- Preserved active SFTP transfer ownership across session replacement and reconnects, including paused, cancelled, and resumable transfer state.
- Corrected the disabled OxideSens prompt so it points to Settings → OxideSens instead of the former AI settings label.

### 🧰 Release Maintenance

- Moved proxy policy, settings conversion, credential hydration, and HTTP client construction into a dedicated network-proxy crate with focused security and compatibility tests.
- Moved local directory, project probing, and plugin-settings persistence rules out of the GPUI application layer into their owning domain crates.
- Added end-to-end SSH forwarding coverage for explicit remote ports and shutdown behavior.

## 2.0.7

OxideTerm 2.0.7 expands SSH configuration interoperability and startup controls,
fixes cross-platform terminal rendering defects, and improves workspace reliability.

### ✨ Highlights

- Added automatic SSH Config synchronization that imports new hosts and updates previously imported connections without overwriting same-name manual connections or user-managed metadata.
- Expanded imported SSH routes with recursive `ProxyJump` resolution and opt-in direct `ProxyCommand` execution; shell operators remain rejected and command values stay out of persistence and diagnostics.
- Added launch-at-login controls for Windows and Linux, with a direct link to the system Login Items settings for ad-hoc signed macOS builds.

### 🛠️ Fixes

- Prevented Vim terminal queries from being misclassified as Sixel images, eliminating the repeated black artifact blocks they could leave across supported platforms.
- Preserved a full-screen terminal application's hidden-cursor state instead of replacing it with the configured visible cursor shape.
- Fixed Windows DirectWrite text corruption caused by mutable shaping buffers being reused after their callback lifetime ended.
- Kept all four welcome-screen shortcut hints on one row in standard Windows layouts while preserving wrapping in narrow windows.
- Restored Host Tools when OxideSens AI is disabled and corrected saved-connection search focus so hidden inputs no longer intercept terminal keys.
- Fixed keyboard and IME input routing in portable connection-transfer dialogs.
- Preserved ACP agent context across tool-call continuations and restored the configured default-key fallback when agent authentication is unavailable.
- Added the complete ANSI palette to asciicast v2 recordings for compatibility with conforming players.
- Reported stale saved-connection actions explicitly instead of silently operating on missing entries.

### 🧰 Release Maintenance

- Made issue-quality checks recoverable and covered their policy decisions with focused tests.
- Published repository-owned release, SSH session-ownership, and secret-handling skills for contributors and coding agents.
- Simplified stable release notes so the GitHub Release title is not repeated in the body and change details appear before download links.

## 2.0.6

OxideTerm 2.0.6 improves terminal input and command-mark reliability, adds
customizable settings navigation, and expands Linux window-manager integration.

### Highlights

- Added drag-and-drop settings navigation so pages and groups can be reordered, regrouped, saved, or restored to the default layout.
- Added a Linux-only option to hide the application titlebar for tiling compositors while leaving macOS and Windows behavior unchanged.

### Fixes

- Prevented the terminal command bar from crashing when IME composition creates overflow inside its compact input surface.
- Restored the blinking caret before the placeholder when an empty terminal command bar is focused.
- Reset visual command marks when saved history is cleared or terminal resizing reflows the grid, while preserving completed command facts.
- Improved Fish Shell command capture and command-line placement when shell integration events arrive without the usual prompt-start event.

### Release Maintenance

- Refreshed the terminal, SFTP, port-forwarding, and Mini IDE documentation screenshots.

## 2.0.5

OxideTerm 2.0.5 restores several native interface details and improves terminal
selection, glyph rendering, appearance controls, and packaging reliability.

### Fixes

- Fixed custom-drawn Powerline separators and related terminal glyph geometry across supported shapes and cell sizes.
- Fixed copying terminal selections that span multiple wrapped or visible pages.
- Restored the proven grayscale text path on Windows to prevent overlapping or duplicated glyphs.
- Restored onboarding step content that could collapse to zero height in the GPUI layout.
- Refined select-menu hover and selected surfaces, and added localized tooltips to icon-only terminal actions.
- Expanded the managed appearance gallery to three protected bundled backgrounds and extended background opacity control to the full supported range.
- Corrected the introduction page to describe the current WASM plugin system instead of the legacy ESM runtime.

### Release Maintenance

- Made macOS DMG packaging recover from transient busy-volume detach failures.
- Shipped provenance and CC BY 4.0 licensing for bundled background artwork, and limited remote-agent CI to relevant agent changes.

## 2.0.4

OxideTerm 2.0.4 refreshes the native GPUI runtime, improves Linux and virtualized
graphics compatibility, and polishes remote desktop, transfers, scrolling, and
desktop integration.

### Highlights

- Migrated the native desktop layer to a pinned, vendored GPUI-CE baseline while preserving OxideTerm's platform integration across macOS, Windows, and Linux.
- Improved Linux and virtual-machine graphics startup with deterministic Vulkan/OpenGL adapter selection, virtual-GPU handling, recoverable WGPU device resets, and actionable failure diagnostics.
- Updated remote desktop rendering to reuse a stable dynamic texture, apply incremental framebuffer updates, restore content after renderer resets, and hide the local pointer when displaying a remote cursor.

### Fixes

- Fell back from Wayland to X11 when the selected compositor is missing required capabilities, including affected WSLg environments, and reported both backend failures when neither can start.
- Prevented one trackpad or mouse-wheel gesture from scrolling nested inner and outer surfaces at the same time.
- Improved title-bar window controls and wrapped long graphics-startup diagnostics so they remain readable.
- Enlarged the small Windows taskbar and Linux desktop-panel icon artwork without changing macOS application icons or larger launcher assets.
- Recovered interrupted active SFTP transfers as paused work after restart and removed stale completed or cancelled progress records.
- Limited native application logs to one bounded 10 MiB file while retaining the most recent complete entries.
- Refined cloud-sync metadata layout, background-scope selection motion, and localized remote-agent size information.

### Release Maintenance

- Added a pinned GPUI-CE provenance ledger, vendored-source verification, refreshed third-party notices, and packaged license validation.

## 2.0.3

OxideTerm 2.0.3 improves terminal input compatibility, remote directory awareness,
Linux desktop integration, and interface scaling across the native workspace.

### Highlights

- Added user-controlled remote Shell integration for exact SSH working-directory awareness, with inspectable hook files, ask/always/disabled deployment policies, and explicit install, repair, and removal actions.
- Added an application-wide UI font-size setting and a temporary on-screen indicator when terminal font-size shortcuts are used.
- Added a terminal link activation preference that requires Ctrl + click by default on Linux and Windows, or Command + click on macOS, while retaining direct-click activation as an option.

### Fixes

- Restored Ctrl + Insert and Shift + Insert clipboard shortcuts in terminal panes.
- Fixed Linux window identification so the running application matches the stable desktop entry in Ubuntu Dock and other desktop shells.
- Fixed terminal mouse hit testing during fractional smooth scrolling so selections and command blocks align with the painted rows.
- Kept terminal-grid completion owned by the active Shell or TUI application by limiting OxideTerm ghost text to privilege prompts.
- Prevented edited saved connections and jump-host routes from reusing stale SSH nodes that point to an earlier endpoint.

## 2.0.2

OxideTerm 2.0.2 improves application privacy controls, terminal session ownership,
and visual consistency across the native workspace.

### Highlights

- Added application locking with macOS Touch ID and Windows Hello unlock support, plus a setting to hide the lock action from the activity bar.
- Improved terminal, pane, tab, and SSH node ownership so independent consumers keep shared sessions alive and terminal endpoints remain traceable.
- Added animated authentication selection while preserving the existing connection form appearance and reduced-motion behavior.

### Fixes

- Fixed modal backdrops so confirmations dim the complete application window instead of only the current content surface.
- Refined plugin manager typography, selected connection styling, sidebar selection motion, and other native workspace details.
- Improved terminal graphics cache ownership, rendering state, session cleanup, and transfer integration.
- Fixed local terminal background lifecycle handling and several tab, split-pane, SFTP, forwarding, cloud-sync, and plugin host edge cases.

## 2.0.1

OxideTerm 2.0.1 is a maintenance release focused on Linux startup reliability,
cross-version updater compatibility, and settings navigation consistency.

### Fixes

- Fixed a Linux startup panic caused by a Rust/WGSL backdrop-blur structure name mismatch in the Blade renderer.
- Kept stable updater manifests compatible with 1.x clients, including the gzip-compressed macOS application archives and installer-specific platform keys expected by the legacy Tauri updater.
- Fixed settings navigation selection and hover surfaces stretching vertically when the window had spare height.

### Release Maintenance

- Stable releases now become GitHub's Latest release automatically, while prereleases remain excluded from Latest promotion.
- Added release-time validation for the legacy updater package contract so future 2.x releases remain reachable from 1.x installations.
- Removed obsolete preview-status messaging from the localized READMEs and refreshed third-party notices.

## 2.0.0

OxideTerm 2.0 is the largest release in the project's history. The desktop application has been rebuilt around Rust and GPUI, replacing the bundled WebView application shell with a GPU-rendered workspace while preserving OxideTerm's local-first approach to remote operations.

This release brings terminals, saved connections, SFTP, remote editing, port forwarding, Host Tools, RDP/VNC, serial devices, cloud sync, plugins, the `oxideterm` CLI, and OxideSens AI into one shared workspace and runtime model.

### Highlights

- A new GPUI desktop workspace for macOS, Windows, and Linux, with no Electron or bundled browser runtime.
- Replaced the Tauri WebView and xterm.js terminal path with a direct Rust implementation built around `alacritty_terminal`, `portable-pty`, and `russh`, rendered by GPUI.
- Reduced measured idle memory from roughly 300 MB in 1.x to just over 100 MB in the current 2.0 build.
- One SSH node can now serve terminals, SFTP, remote editing, port forwarding, Host Tools, and downstream connections without tying their lifetime to one terminal tab.
- Grace Period reconnect can preserve an existing SSH runtime across short network interruptions when the original connection recovers in time.
- Host Tools add monitoring, processes, services, logs, ports, scheduled tasks, disks, packages, containers, and tmux workflows beside the active connection.
- Built-in RDP and VNC sessions, plus a Windows WSL Graphics connection flow.
- OxideSens now operates inside the native workspace with BYOK providers, MCP, local knowledge retrieval, risk-aware tools, and user-controlled action policy.
- A new native plugin model supports manifest-only, Wasm, and external process runtimes with capability-scoped host APIs.
- Oxide cloud sync is now a built-in workspace feature with multiple user-owned storage backends, conflict preview, history, and rollback.
- The old `oxt` desktop RPC client is replaced by a standalone `oxideterm` CLI for configuration, automation, diagnostics, migration, and recovery.

### Desktop Workspace

- Rebuilt the activity bar, saved-session sidebar, tab strip, auxiliary sidebar, dialogs, overlays, command palette, and settings surfaces in GPUI.
- Added a Session Manager for searching, sorting, grouping, importing, exporting, and editing saved connections.
- Separated saved connections, active SSH nodes, terminal panes, SFTP sessions, forwarding rules, IDE workspaces, and Host Tools so closing one view does not implicitly destroy unrelated runtime owners.
- Added connection topology and runtime views for jump-host relationships, downstream nodes, active consumers, reconnect state, and connection capabilities.
- Added split terminal panes with draggable dividers, pane focus, pane close behavior, and layout restoration.
- Added horizontal tab overflow with wheel-to-horizontal mapping, a visible scrollbar, pointer dragging, and automatic reveal of the active tab.
- Added matching overflow behavior to Host Tools and other compact tool strips.
- Added resizable and persistent sidebars, including resize behavior that remains available after virtualized content loads.
- Added page, dialog, tab-close, sidebar, toast, and popover motion with Normal, Fast, Reduced, and Off profiles.
- Added Zen mode, workspace restore, notification center, connection-status surfaces, diagnostics, and command-palette navigation.
- Added single-instance application handling and responsive system-tray behavior on Windows.
- Restored reopening from the Dock after the last window closes on macOS.

### Terminal and Local Shell

- Replaced the WebView/xterm.js terminal path with a Rust terminal model rendered directly by GPUI.
- Added local and SSH terminals with shared selection, search, scrollback, hyperlink, context-menu, clipboard, and encoding behavior.
- Added terminal search, command marks, shell integration, scrollback viewing, command playback metadata, and clickable links.
- Added Kitty, Sixel, and iTerm-style terminal graphics infrastructure.
- Added IME composition, Unicode bidirectional text handling, CJK font fallback, selectable terminal encodings, and improved wide-character layout.
- Added configurable cursor shape and blink behavior, terminal fonts, separate CJK fonts, font preview, and an opt-in font-ligatures setting.
- Added smooth scrolling, draggable scrollbars, double-click word selection, shift selection, and native terminal context menus.
- Added a multiline command bar with command history, path suggestions, Quick Commands, completion specifications, and risk-aware execution.
- Added terminal-aware current-directory controls and hooks for Git and project context.
- Added optional `sudo` and `su` credential helpers with scoped prompt detection.
- Added in-band trzsz transfers and modem transfer paths for X/Y/ZMODEM workflows.
- Added configurable automatic pane close after the underlying terminal exits.
- Added local shell discovery and configuration for common Unix shells, Command Prompt, Windows PowerShell, PowerShell Core, Git Bash, and Nushell.
- Added shell integration for Bash, Zsh, Fish, Nushell, and PowerShell so local and remote working-directory metadata does not depend on prompt parsing.
- Suppressed background console windows for local shell discovery and helper commands on Windows.

### SSH, Authentication, and Connection Management

- Moved desktop SSH transport to the Rust `russh` stack with `ring` cryptography and no OpenSSL/libssh2 dependency in the SSH implementation.
- Added password, private-key, OpenSSH certificate, managed-key, SSH Agent, and Keyboard-Interactive authentication flows.
- Added support for keyboard-interactive prompts used by common one-time-password, hardware-token, and challenge-response systems.
- Added managed SSH keys that can be imported or pasted, referenced by saved connections, and optionally moved through encrypted `.oxide` bundles.
- Added SSH Agent forwarding.
- Added multi-hop connection trees with independent host, port, username, and authentication settings at each hop.
- Added reuse of saved connections as jump hosts and next-hop nodes.
- Added HTTP CONNECT and SOCKS5 upstream proxies with global, per-connection, and force-direct policies.
- Added strict host-key confirmation, saved host-key removal, and clearer host-key mismatch handling.
- Added a per-connection legacy SSH compatibility option for servers that cannot negotiate the default algorithms.
- Added more specific algorithm-negotiation and authentication diagnostics without including credential values.
- Added optional post-connect commands.
- Improved OpenSSH config parsing for `Match` blocks and multiple aliases in one `Host` declaration.
- Added import flows for OpenSSH config and supported third-party connection managers, including import preview, unsupported-field warnings, duplicate handling, and source groups.
- Added temporary SSH connections that do not need to be saved first.

### Grace Period Reconnect and Runtime Ownership

- Added a node-level Grace Period reconnect pipeline.
- When a supported SSH connection appears lost, OxideTerm probes the original connection for up to 30 seconds before replacing it.
- If the original connection recovers during the grace period, existing terminal programs can continue on that runtime.
- If replacement is required, OxideTerm updates the node runtime and lets supported consumers reacquire the new transport.
- SFTP and remote editing retain their node identity while reacquiring SSH-backed capabilities.
- Saved and active forwarding rules can be restored after a node reconnect, subject to local port availability, permissions, and remote bind acceptance.
- Downstream nodes now observe jump-host link loss and enter the corresponding disconnected or reconnecting state.
- Added clearer runtime and connection-monitor status for connecting, active, idle, link-down, and reconnecting states.
- Removed duplicate reconnect messages from terminal output.

### Directory, Git, and Project Awareness

- Added current-working-directory awareness for local and SSH terminals.
- Added remote shell integration that reports directory, host, Git, and project metadata without scraping the visible prompt.
- Preserved SSH login banners, MOTD output, and last-login text while staging shell integration outside the visible interactive input stream.
- Added current-directory navigation, parent and child browsing, path insertion, and workspace search entry points.
- Added local and remote Git repository detection, branch or detached-HEAD identity, upstream state, ahead/behind counts, staged changes, modifications, untracked files, and conflicts.
- Added time-bounded remote Git status scans that return repository identity before slower working-tree details.
- Added branches, worktrees, changes, staging, history, references, sync, and conflict-oriented Git views.
- Added detection of merge, rebase, cherry-pick, and revert operations in progress.
- Added project-root and project-type detection, project task discovery, task search, and task execution.
- Isolated Git and project snapshots by host, terminal runtime, and working directory to prevent state from leaking between sessions.

### SFTP, Files, and Remote Editing

- Rebuilt SFTP as a node-level capability rather than a terminal-session attachment.
- Added remote navigation, path editing, refresh, selection, bookmarks, and independently opened SFTP views.
- Added single-file and directory upload/download with background queues, parallelism controls, speed limits, progress, throughput, and ETA.
- Added pause, resume, retry, and cancellation for supported transfer stages.
- Added archive-based directory transfer for suitable workloads with fallback to ordinary recursive transfer.
- Added remote archive extraction.
- Added overwrite, skip, rename, and apply-to-all conflict strategies.
- Improved directory progress accounting, remote modification-time handling, short-read recovery, symlink classification, and Windows/POSIX path normalization.
- Made SFTP paths selectable and copyable.
- Added local file management with navigation, drives, sorting, filtering, favorites, creation, copy, cut, paste, rename, delete, drag-and-drop, and context-menu actions.
- Added previews for supported text, source code, Markdown, images, audio, video, hexadecimal data, and fonts.
- Added font specimen and glyph coverage views for code fonts, CJK fonts, and Nerd Font symbols.
- Added a lightweight local and remote editor with project trees, multiple tabs, syntax highlighting, line wrapping, dirty-buffer tracking, save conflicts, safe writes, and workspace state.
- Added symbol indexing and completion support to remote-agent-backed project workflows.

### Port Forwarding and Network Tools

- Added local (`-L`), remote (`-R`), and dynamic SOCKS5 (`-D`) forwarding in the native runtime.
- Bound forwarding ownership to SSH nodes and exposed running, failed, stopped, paused, and restoring states.
- Added saved forwarding rules, optional connection-time startup, pause/resume, reconnect-aware restore, and actionable failure details.
- Added remote listening-port discovery and connection-topology entry points.
- Improved IPv4, IPv6, host-and-port normalization, local port conflict reporting, permission errors, and remote bind diagnostics.
- Added X11 forwarding infrastructure with DISPLAY allocation, Xauthority management, and remote `xauth` setup.
- Added update and SSH proxy settings without exposing proxy credentials in ordinary diagnostics.

### Telnet and Serial

- Added native Telnet sessions with option negotiation, binary mode, echo, terminal type, and window-size negotiation.
- Added local serial terminals with device enumeration or manual device paths, configurable baud rate, data bits, stop bits, parity, and flow control.
- Added saved, editable, importable, and exportable serial profiles with classified device, permission, busy-port, parameter, and disconnect errors.

### Host Tools and Runtime Views

- Added node-scoped Host Tools that remain available independently of any one terminal pane.
- Added CPU, memory, swap, disk, load, network, mount, interface, process, GPU-when-available, and RTT monitoring.
- Added process search, filtering, sorting, TERM/KILL, stop/continue, and nice-value actions where the remote platform permits them.
- Added service discovery and supported lifecycle operations for systemd, launchd, BSD services, and Windows services.
- Added Docker container status, metadata, ports, start, stop, restart, and log actions.
- Added host logs with presets, snapshots, and follow mode.
- Added tmux session, window, and pane discovery with create, attach, rename, close, and send-command operations.
- Added listening-port inspection with process association and public-exposure hints.
- Added filesystem and mount views with capacity, usage, read-only state, and low-space indicators.
- Added scheduled-task discovery and supported run, enable, disable, and log operations across Linux, macOS, and Windows.
- Added remote package-manager discovery, package lists, status, and package detail views.
- Isolated samplers and parsers by capability so one unsupported tool does not mark the SSH node itself as disconnected.
- Added responsive and virtualized tables that prioritize entity names at narrow sidebar widths.
- Host Tools degrade to partial or unavailable states when the remote operating system, command-line utilities, or privileges do not provide a capability.

### Remote Desktop and Graphics

- Added built-in RDP and VNC workspace sessions with separate helper-process boundaries.
- Added keyboard, mouse, clipboard, scaling, reconnect, and viewport-aware rendering paths.
- Added dynamic remote resolution handling for supported RDP sessions.
- Added VNC decoding for Raw, CopyRect, Hextile, and ZRLE server updates.
- Added a Windows WSL Graphics connection flow for discovering and opening WSLg graphical sessions.
- Added workspace graphics surfaces for terminal-owned images and supported remote graphical workflows.

### OxideSens AI

- Moved OxideSens into the Rust workspace with access to user-selected terminal, file, connection, and workspace context.
- Kept the BYOK model with OpenAI, Anthropic, Gemini, DeepSeek, Ollama, and custom OpenAI-compatible providers.
- Added a unified workspace tool layer for target selection, terminal observation, command execution, file operations, transfers, navigation, and preference changes.
- Added risk classes for read, write, execute, interactive, destructive, and credential-related operations.
- Added command-policy detection for destructive filesystem operations, formatting, reboot, privilege escalation, container deletion, and Kubernetes resource deletion.
- Read-only tools may execute directly; other actions follow the user's configured approval and safety policy.
- Added streaming output, conversation persistence, message branching, follow-up suggestions, tool-result compaction, and context-window budgeting.
- Added ACP agent integration with configurable external processes and presets for supported coding-agent CLIs.
- Added provider-bound context redaction for common private keys, authorization headers, database URLs, tokens, and credential-like values as a defense-in-depth measure.

### MCP and Knowledge

- Added native MCP transports for local stdio, Streamable HTTP, and Legacy SSE servers.
- Added MCP tool discovery and invocation, resource listing and reading, authentication headers, custom headers, environment variables, retry, and runtime status.
- Redacted MCP authentication values, headers, and environment secrets from ordinary debug output.
- Added document collections, document editing, scopes, index rebuilds, and knowledge search.
- Added BM25 and persistent HNSW vector retrieval with Reciprocal Rank Fusion and duplicate-reduction ranking.
- Added BM25 fallback when no embedding provider is available.
- Added character-bigram tokenization for Chinese, Japanese, and Korean retrieval.

### Plugins

- Introduced the 2.0 plugin model with manifest-only, Wasm, and external process runtimes.
- Bundled the Wasm executor in standard desktop packages.
- Added manifest contributions for custom tabs, sidebars, settings, terminal hooks, connection hooks, AI tools, and scoped host API access.
- Added host-rendered declarative plugin UI instead of loading plugin React, CSS, or WebView pages.
- Added capability and namespace checks for terminal, SFTP, forwarding, IDE, settings, sync, application state, and plugin-secret APIs.
- Added plugin discovery, install, enable, disable, update, settings, compatibility, health, and runtime status flows.
- Added protocol, guest ABI, WASI profile, host channel, host version, platform target, and checksum compatibility checks for Wasm runtimes.
- Added stable, validated keychain account identifiers for plugin secrets.
- Legacy Tauri/Web plugins can be discovered for information or removal, but their JavaScript entry points do not execute in 2.0.

### Oxide Cloud Sync

- Moved cloud sync from an optional 1.x plugin into the built-in 2.0 workspace.
- Added WebDAV, HTTP JSON, Dropbox, OneDrive, Google Drive, GitHub Gist, S3, and Git backends.
- Added manual upload, remote inspection, pull preview, conflict handling, automatic upload, history, and rollback backups.
- Added independent sync scopes for connections, forwards, Quick Commands, serial profiles, application settings, and plugin settings.
- Sensitive credentials and local-terminal environment variables remain excluded by default and require explicit opt-in.
- Added partition revisions, baselines, and tombstones for incremental updates and deletion tracking.
- Added pre-apply checkpoints and best-effort whole-operation rollback when connection, setting, forwarding, or plugin-setting writes fail.
- Added local rollback retention and bounded sync history.
- Fixed managed SSH keys blocking GitHub Gist upload preflight.
- Tightened upload, pull, conflict, tombstone, delivery-state, and rollback transitions.

### Encrypted `.oxide` Bundles and Portable Workflows

- Expanded and rebuilt `.oxide` import/export for connections, forwarding rules, application settings, Quick Commands, serial profiles, plugin settings, managed SSH keys, and optional portable secrets.
- Added content preview and per-resource selection before import.
- Added rename, skip, replace, and merge conflict policies, with rename as the conservative default.
- Added managed-key fingerprint reuse and explicit choices for restoring managed keys and passphrases.
- Kept saved server passwords, portable secrets, and managed-key passphrases out of ordinary exports unless the user explicitly includes them.
- Added validation and storage checkpoints before applying imports, with rollback when later stages fail.
- Continued ChaCha20-Poly1305 payload encryption and added the current Argon2id KDF profile while retaining support for older `.oxide` KDF files.
- Added portable profile locking, status, keystore, and recovery workflows.

### Standalone `oxideterm` CLI

- Replaced the 1.x `oxt` desktop JSON-RPC client with the standalone `oxideterm` command.
- The new CLI links directly to Rust domain modules and does not require the desktop app to be running.
- Added commands for settings, connections, temporary SSH, forwarding, Quick Commands, plugins, portable profiles, secrets, `.oxide`, cloud sync, paths, diagnostics, doctor checks, backups, batches, reports, completion, and error lookup.
- Added structured JSON output and machine-readable error codes for scripts and CI.
- Added dry-run plans and `--yes` guards to state-changing and high-impact operations where supported.
- Added redacted diagnostic reports and support bundles.
- Added shell completion for Bash, Zsh, Fish, PowerShell, and Elvish.

### Security and Secret Handling

- Unified passwords, key passphrases, AI keys, cloud credentials, plugin secrets, and portable secrets behind the OS keychain or portable keystore boundaries.
- Added encrypted local storage for saved connection metadata, with the local encryption key protected by the platform credential store.
- Added `SecretString`, `Zeroizing`, and redacted `Debug` handling across major secret-owning Rust boundaries.
- Kept credential values out of connection diagnostics, plugin status, cloud-sync summaries, CLI reports, and structured logs.
- Added standard-input and environment-based CLI secret input so scripts do not need to place secrets in process arguments.
- Continued strict SSH host-key verification with rejection of unexpected key changes.
- Added risk-aware AI tools and command-policy checks while keeping approval behavior user-configurable.

### Appearance, Settings, and Internationalization

- Rebuilt settings with native controls, categorized navigation, virtualized long pages, validation, and search-oriented organization.
- Added custom theme editing, interface fonts, terminal fonts, separate CJK font selection, application icon selection, and terminal highlight rules.
- Added background image libraries, opacity, blur, fit modes, per-surface selection, and content-only or full-window background scope.
- Added platform visual-material settings where supported.
- Added configurable shortcuts and native keybinding recording.
- Added update proxy, SSH proxy, SFTP, reconnect, terminal, AI, plugin, cloud-sync, and privacy-oriented settings surfaces.
- Added 11 shipped interface languages across the major 2.0 workflows.
- Added reduced and disabled animation modes; disabling motion removes transition delays instead of scheduling zero-duration exits.

### Packaging, Installation, and Updates

- Added six release targets: macOS arm64/x64, Windows arm64/x64, and Linux arm64/x64.
- Added macOS DMG, app archive, and portable archive outputs.
- Added Windows NSIS installers and portable ZIP outputs.
- Added Linux AppImage, DEB, RPM, and portable archive outputs.
- Added signed updater metadata and SHA-256 release checksums.
- Added Windows installer options for Start Menu and optional desktop shortcuts.
- Added a dedicated Windows update helper that stages the installer, waits for OxideTerm to exit, uses Restart Manager on a best-effort basis, keeps an `old` rollback directory, and completes replacement outside the running app.
- Added no-window process creation for Windows shell discovery, Git helpers, PowerShell, updater helpers, and other background commands.
- Added stable, beta, and GPUI Preview update-channel boundaries.
- Stable 2.0 updates use the GitHub Latest manifest; the old Preview-facing stable manifest remains frozen to prevent unintended cross-channel replacement.

### Important Fixes Since the GPUI Previews

- Restored Ubuntu MOTD, login banners, and last-login output during SSH shell integration startup.
- Fixed initial remote directory metadata reporting `~` instead of the actual directory.
- Fixed remote directory awareness becoming unavailable after the next prompt.
- Fixed remote Git and project detection ordering and bounded slow status scans.
- Fixed Host Tools showing counts while rows remained empty.
- Fixed Host Tools becoming impossible to resize after monitoring or process data arrived.
- Fixed tab and Host Tools scrollbars that were visible but could not be dragged.
- Fixed Windows background shell and Git discovery repeatedly opening console windows.
- Fixed Windows auto-update uninstall/reinstall sequencing with the dedicated update helper.
- Fixed Windows system-tray interaction after minimizing the app.
- Fixed saved connections failing to switch from key or Agent authentication to a newly entered password.
- Fixed keychain-backed passwords being read before the user explicitly requested to reveal them.
- Fixed SFTP path selection, Windows home-directory handling, remote extraction, transfer short reads, and several progress-state stalls.
- Fixed terminal IME focus, text-selection drag, Windows editing shortcuts, duplicate paste handling, pane-close cleanup, and duplicate reconnect output.
- Fixed remote desktop frame updates, sizing, clipboard, and reconnect edge cases.
- Fixed plugin runtime packaging so standard builds include Wasm execution again.
- Fixed cloud-sync managed-key preflight, conflict accounting, tombstone handling, and rollback state.
- Fixed multiple modal, toolbar, dropdown, toast, sidebar, and narrow-window interaction regressions across the GPUI workspace.

### Breaking Changes

- **CLI:** `oxt`, its JSON-RPC protocol, Unix socket, Windows named pipe, and old command syntax are not compatible with 2.0. Automation must move to the standalone `oxideterm` subcommands.
- **CLI scope:** the new CLI focuses on configuration, automation, diagnostics, migration, and recovery; not every live desktop-session RPC operation from `oxt` has a direct replacement.
- **Plugins:** Tauri/Web plugins that depend on `main.js`, React components, CSS injection, WebView APIs, or arbitrary Tauri commands do not execute in 2.0.
- **Plugin migration:** plugins must move to the 2.0 manifest, declarative UI, Wasm, or process protocol and request explicit host capabilities.
- **Cloud sync:** cloud sync is now built in; the old cloud-sync plugin is no longer the feature owner.
- **Preview updates:** GPUI Preview builds do not update directly to Stable 2.0 through the Stable channel.

### Upgrading to 2.0

#### Before You Upgrade

- Close active terminals, transfers, port forwards, Host Tools actions, and remote desktop sessions before installing the update. Active runtimes and in-progress operations do not survive the required application restart.
- Keep a current backup or encrypted export of important connections and settings.
- On first launch, OxideTerm creates a one-time snapshot of the existing data directory before loading and migrating mutable settings and connection data.
- The migration snapshot is a recovery copy, not an automatic rollback mechanism.
- OxideTerm continues to use the existing default data directory and honors a custom data directory selected through `bootstrap.json`.

#### From OxideTerm 1.x Stable

- Installed macOS releases can use the Stable update after 2.0 is promoted, using a compatibility archive understood by the 1.x updater.
- Current-user Windows installations can use the Stable update; the 2.0 installer detects the existing per-user installation and upgrades it in place.
- Linux AppImage installations can use the application update path, which replaces the AppImage after OxideTerm exits.
- Linux DEB or RPM users should install the matching 2.0 package manually. OxideTerm 2.0 publishes both package formats alongside AppImage and portable archives.
- Portable installations do not update themselves. Extract the 2.0 portable package separately and preserve the existing portable data directory before replacing files.
- If a GPUI Preview is installed alongside 1.x, verify that the 1.x stable application is the one performing an automatic Stable upgrade.
- Existing connection and settings data is migrated into the 2.0 storage model where supported, but review connections, authentication, cloud sync, plugins, and AI provider settings after first launch.
- Legacy keychain entries may be migrated to protected 2.0 entries on first read; the operating system may display a one-time credential access prompt.
- Existing keychain passwords remain unloaded in edit forms until the user explicitly reveals them.
- Strict host-key verification remains active after upgrade.
- Third-party connection imports may report fields that cannot be represented exactly; review the import preview before applying it.
- Data written by 2.0 is not guaranteed to remain fully understandable to a subsequently launched 1.x application.

#### From a GPUI Preview

- GPUI Preview cannot update directly to Stable 2.0 through the Stable update channel.
- Install the final 2.0 package manually from the stable GitHub Release, or use an installed 1.x stable build as the supported automatic-upgrade origin.
- Older Preview builds use a frozen `updater-stable` manifest and will not receive 2.0 Stable in place.
- Preview and Stable use separate application identities and can remain installed side by side while Stable is verified, but they share the same OxideTerm data directory.
- After confirming that Stable opens the existing data correctly, the GPUI Preview application can be removed.

### Compatibility and Known Boundaries

- Release packages are provided for macOS, Windows, and Linux on x64 and arm64.
- The optional remote project agent is packaged only for Linux x86_64 and Linux aarch64. OxideTerm asks before deployment; other remote architectures require a separately built agent or SFTP-compatible fallback behavior.
- Host Tools depend on the remote operating system, installed command-line tools, service manager, and privileges. Unsupported capabilities are shown as partial or unavailable.
- Grace Period reconnect is best-effort and depends on the original SSH runtime recovering or supported consumers successfully acquiring a replacement transport; it is not a guarantee of lossless recovery for every network failure.
- Unsaved editor buffers still require an explicit save, reload, or discard decision and should not be treated as remotely persisted state.
- Port-forward restore can fail when local ports are occupied, privileges are insufficient, or the remote server rejects binding.
- RDP and VNC support common interactive workflows but do not claim parity with every platform-specific enterprise client or extension.
- VNC uses the server framebuffer and scales it into the local viewport; dynamic remote resolution behavior primarily applies to supported RDP sessions.
- File preview supports selected text, code, Markdown, image, audio, video, hexadecimal, and font formats. PDF preview is not included in 2.0.
- Configurable keyboard shortcuts are included; vi-mode is not a 2.0 feature.
- Shell directory, Git, and project awareness is available through supported shell integration or explicit probing. Restricted shells, unusual startup files, or custom environments may reduce available metadata.
- Wasm plugins run inside Wasmtime; external process plugins are separate executables and should not be described as OS-sandboxed.
- Sensitive cloud-sync sections are disabled by default but can be explicitly included by the user in the encrypted sync payload.
- `.oxide` payload content is encrypted, but non-secret file metadata used to identify and describe a bundle is not a secret-storage boundary.
- OxideSens context redaction is defense in depth, not a formal guarantee that arbitrary user content can never contain an undiscovered secret pattern.
- AI conversation persistence is application-wide in 2.0; it is not guaranteed to remain semantically bound to the lifetime of an individual tab or workspace surface.
