# WorkspaceApp 投递与生命周期所有权清单

## 用途

本清单记录 Phase 0 基线中所有根渲染入口、530ms 心跳入口和
`WorkspaceApp` 直接 receiver。它是迁移顺序的事实基线，不是最终架构。

通用现状：

- 除节点事件外，表中多数 `std::sync::mpsc` channel 为无界队列。
- “排空”表示当前入口使用 `try_recv` 直到空；Phase 1 必须为其加入统一预算。
- “主动投递”表示 Phase 2 需要由目标 Entity 等待 receiver，并直接更新自身。
- 页面隐藏只允许停止页面采样和刷新，不得停止节点、传输、隧道、插件宿主、
  AI 活动流或远程桌面必要生命周期事件。

## 根 `render` 的 28 个入口

| 入口 | 消息类别与生产者 | 当前取消、溢出和 generation | 目标所有者 |
| --- | --- | --- | --- |
| `poll_forwarding_worker_results` | 转发用户动作完成；转发 UI worker | 无界队列、排空；任务令牌和节点关系在现有转发状态中 | `ForwardingWorkspaceEntity` |
| `poll_graphics_worker_results` | 图形生命周期、帧和动作结果；图形 worker | 图形状态保存 worker generation；当前由根触发排空 | Graphics 表面 Entity |
| `poll_remote_desktop_worker_results` | 远程桌面生命周期、输入结果和帧；会话 worker | 现有 wake、generation、帧槽、数量与时间预算；会话关闭负责停止 | `RemoteDesktopSessionEntity` |
| `poll_connection_monitor_updates` | 连接监控快照；monitor sampler | 当前状态保存 sampler 生命周期；隐藏策略待显式化 | `HostToolsEntity` |
| `poll_host_gpu_updates` | GPU 高频快照；GPU sampler | snapshot 语义；当前由根触发读取 | `HostToolsEntity` |
| `poll_host_process_action_results` | 进程动作完成；Host Tools action worker | 不得丢失；当前排空 | `HostToolsEntity` |
| `poll_host_docker_action_results` | Docker 动作完成；Host Tools action worker | 不得丢失；当前排空 | `HostToolsEntity` |
| `poll_host_docker_logs_results` | Docker 日志结果；Host Tools log worker | 当前排空；跟随日志动作生命周期 | `HostToolsEntity` |
| `poll_host_service_action_results` | 服务动作完成；Host Tools action worker | 不得丢失；当前排空 | `HostToolsEntity` |
| `poll_host_service_logs_results` | 服务日志结果；Host Tools log worker | 当前排空；跟随日志动作生命周期 | `HostToolsEntity` |
| `poll_host_logs_snapshot_results` | 日志快照；Host Tools sampler | 高频快照；隐藏后应停止生产 | `HostToolsEntity` |
| `poll_host_tmux_snapshot_results` | tmux 快照；Host Tools sampler | 高频快照；隐藏后应停止生产 | `HostToolsEntity` |
| `poll_host_tmux_action_results` | tmux 动作完成；Host Tools action worker | 不得丢失；当前排空 | `HostToolsEntity` |
| `poll_host_ports_snapshot_results` | 端口快照；Host Tools sampler | 高频快照；隐藏后应停止生产 | `HostToolsEntity` |
| `poll_host_schedules_snapshot_results` | 计划任务快照；Host Tools sampler | 高频快照；隐藏后应停止生产 | `HostToolsEntity` |
| `poll_host_filesystems_snapshot_results` | 文件系统快照；Host Tools sampler | 高频快照；隐藏后应停止生产 | `HostToolsEntity` |
| `poll_host_packages_snapshot_results` | 软件包快照；Host Tools sampler | 高频快照；隐藏后应停止生产 | `HostToolsEntity` |
| `poll_host_schedule_logs_results` | 计划任务日志结果；Host Tools worker | 当前排空；动作结果不得丢失 | `HostToolsEntity` |
| `poll_host_schedule_action_results` | 计划任务动作完成；Host Tools worker | 不得丢失；当前排空 | `HostToolsEntity` |
| `maybe_refresh_connection_monitor` | 周期刷新调度；monitor sampler | 由时间和页面状态触发，不是 worker 完成消息 | `HostToolsEntity` |
| `poll_connection_trace_events` | 连接诊断生命周期；连接与节点路径生产 | 无界队列、排空；不得记录秘密或端点内容 | `WorkspaceRuntimeEntity` 输出到 `WorkspaceOverlayEntity` |
| `poll_terminal_notices` | 终端通知；终端、SFTP、转发、Host Tools 等生产 | 无界队列、排空；通知去重有界化待迁移 | `WorkspaceTerminalEntity` 输出到 `WorkspaceOverlayEntity` |
| `poll_native_plugin_terminal_ui_requests` | 插件终端 UI 请求；插件宿主 | 运行时队列；管理页隐藏不得停止宿主 | `PluginWorkspaceEntity` |
| `poll_native_plugin_product_ui_effects` | 插件产品副作用；插件宿主 | 当前暂存副作用；必须改为类型化宿主请求 | `PluginWorkspaceEntity` |
| `poll_ai_chat_stream_events` | AI 生命周期和流式文本；对话 worker | 已有每次 256 条预算、相邻文本合并和会话检查 | `AiWorkspaceEntity` |
| `poll_ai_compaction_results` | AI 压缩动作完成；压缩 worker | 用户动作结果不得丢失；当前根读取 | `AiWorkspaceEntity` |
| `poll_ai_model_selector_probe_results` | 模型探测结果；模型探测 worker | 旧探测结果需要 generation 检查；隐藏后停止非必要探测 | `AiWorkspaceEntity` |
| `poll_ai_model_refresh_results` | 模型刷新结果；设置/模型 worker | 用户刷新结果不得丢失；当前根读取 | `AiWorkspaceEntity` |

## 530ms 心跳的 39 个入口

| 入口 | 当前性质 | Phase 2/4 归属 |
| --- | --- | --- |
| `poll_ssh_worker_results` | worker 完成投递 | `WorkspaceRuntimeEntity` / `ConnectionFlowEntity` 主动投递 |
| `poll_node_events` | 节点生命周期；有界 256，单次预算 64 | `WorkspaceRuntimeEntity` 主动投递 |
| `poll_reconnect_worker_results` | worker 完成投递 | `WorkspaceRuntimeEntity` 主动投递 |
| `poll_launcher_worker_results` | worker 完成投递 | Launcher 表面 Entity 主动投递 |
| `poll_graphics_worker_results` | worker 完成投递；与根渲染重复 | Graphics 表面 Entity 主动投递 |
| `poll_connection_monitor_updates` | worker 完成投递；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_gpu_updates` | worker 完成投递；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_process_action_results` | 用户动作完成；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_docker_action_results` | 用户动作完成；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_docker_logs_results` | 日志结果；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_service_action_results` | 用户动作完成；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_service_logs_results` | 日志结果；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_logs_snapshot_results` | 高频快照；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_tmux_snapshot_results` | 高频快照；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_tmux_action_results` | 用户动作完成；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_ports_snapshot_results` | 高频快照；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_schedules_snapshot_results` | 高频快照；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_filesystems_snapshot_results` | 高频快照；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_packages_snapshot_results` | 高频快照；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_schedule_logs_results` | 日志结果；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_host_schedule_action_results` | 用户动作完成；与根渲染重复 | `HostToolsEntity` 主动投递 |
| `poll_external_settings_store_changes` | 周期文件变更检查 | `SettingsWorkspaceEntity` 定时任务 |
| `poll_terminal_cwd_results` | worker 完成投递 | `WorkspaceTerminalEntity` 主动投递 |
| `poll_terminal_git_results` | worker 完成投递 | `WorkspaceTerminalEntity` 主动投递 |
| `poll_terminal_project_results` | worker 完成投递 | `WorkspaceTerminalEntity` 主动投递 |
| `maybe_refresh_connection_monitor` | 周期刷新；与根渲染重复 | `HostToolsEntity` 可见性定时任务 |
| `maybe_refresh_active_terminal_git` | 周期刷新 | `WorkspaceTerminalEntity` 按活动终端需求运行 |
| `maybe_refresh_active_terminal_project` | 周期刷新 | `WorkspaceTerminalEntity` 按活动终端需求运行 |
| `poll_forwarding_worker_results` | worker 完成投递；与根渲染重复 | `ForwardingWorkspaceEntity` 主动投递 |
| `poll_forwarding_events` | 隧道生命周期 | 转发运行时服务主动投递到 `ForwardingWorkspaceEntity` |
| `sync_ssh_node_lifecycle` | 节点生命周期协调 | `WorkspaceRuntimeEntity` |
| `maybe_probe_active_ssh_connections` | 周期活动探测 | `WorkspaceRuntimeEntity` 定时任务 |
| `maybe_start_forwards_port_scan` | 页面专属周期探测 | `ForwardingWorkspaceEntity` 可见性定时任务 |
| `maybe_refresh_forwards_stats` | 页面专属周期刷新 | `ForwardingWorkspaceEntity` 可见性定时任务 |
| `any_terminal_recording_active` | 展示刷新判断 | `WorkspaceTerminalEntity` 的录制生命周期 |
| `handle_active_privilege_prompt_submit_request` | 用户输入结果 | `WorkspaceTerminalEntity` 输入路由；秘密仍走专用零化边界 |
| `handle_active_terminal_context_action_request` | 用户输入结果 | `WorkspaceTerminalEntity` 输入路由 |
| `sync_active_privilege_prompt_inline_hint` | 展示状态同步 | `WorkspaceTerminalEntity`；不得读取或记录秘密内容 |
| `active_ime_target_blinks_caret` | 展示定时状态 | 当前输入所有者；不属于 worker 投递 |

## `WorkspaceApp` 的 14 个直接 receiver

| receiver | 生产者与消息类别 | 当前溢出、取消和 drop | 目标所有者 |
| --- | --- | --- | --- |
| `terminal_cwd_rx` | CWD 探测 worker；快照/动作结果 | 无界；根排空；页面/终端 generation 负责丢弃旧结果 | `WorkspaceTerminalEntity` |
| `terminal_git_rx` | Git 探测 worker；快照/动作结果 | 无界；根排空；store revision 约束旧结果 | `WorkspaceTerminalEntity` |
| `terminal_project_rx` | 项目探测 worker；快照/动作结果 | 无界；根排空；禁用时清空积压 | `WorkspaceTerminalEntity` |
| `native_update_rx` | 更新检查或下载 worker；生命周期和进度 | 可替换的无界 receiver；`AtomicBool` 取消；完成后置空 | `SettingsWorkspaceEntity` |
| `desktop_presence_rx` | 托盘/系统回调；应用动作 | 无界；100ms UI 轮询；channel 断开时停止 | `WorkspaceOverlayEntity` 或窗口壳层窄适配器 |
| `single_instance_rx` | 单实例监听线程；应用动作 | 共享无界 receiver；100ms UI 轮询；应用拥有源端 | 窗口壳层窄适配器 |
| `ssh_worker_rx` | 连接、认证和保存流程 worker；生命周期/用户结果 | 无界；根排空；认证流程保存 generation 与取消状态 | `WorkspaceRuntimeEntity` / `ConnectionFlowEntity` |
| `node_event_rx` | `NodeRouter` 事件发射器；节点生命周期 | 容量 256；订阅 token 取消；单次预算 64 | `WorkspaceRuntimeEntity` |
| `reconnect_worker_rx` | 重连 worker；节点生命周期/完成 | 无界；根排空；节点 generation、重连 token 和编排状态约束 | `WorkspaceRuntimeEntity` |
| `forwarding_worker_rx` | 转发 UI worker；用户动作完成 | 无界；根排空；规则/节点令牌控制任务 | `ForwardingWorkspaceEntity` |
| `forwarding_event_rx` | `ForwardingRegistry`；隧道生命周期 | 无界；根排空；registry 和规则拥有运行时 | 转发运行时服务 / `ForwardingWorkspaceEntity` |
| `remote_desktop_worker_rx` | 远程桌面会话 worker；生命周期、输入、帧 | 无界 delivery 通道；generation、wake、帧槽和会话停止路径已存在 | `RemoteDesktopSessionEntity` |
| `terminal_notice_rx` | 多个产品表面；通知 | 无界；根排空；当前没有统一预算 | `WorkspaceOverlayEntity` |
| `connection_trace_rx` | 连接和节点诊断路径；生命周期 | 无界；根排空；不得携带秘密或原始端点内容 | `WorkspaceRuntimeEntity` 输出到 `WorkspaceOverlayEntity` |

## 不属于直接 receiver 计数但必须迁移的队列

- SFTP 的 receiver 已由构造时 `cx.spawn` 主动等待；sender 仍由根保存。Phase 4E 迁移
  SFTP 表面时保留这一主动投递方向。
- Host Tools、AI 和插件的若干 receiver、`VecDeque` 或状态内队列嵌在各自状态结构中，
  因而不计入 14 个根直接 receiver；它们仍在根渲染或心跳入口中被消费。
- 远程桌面帧槽和插件产品副作用队列不是普通 receiver 字段，但分别属于帧连续性和
  类型化宿主请求迁移范围。
