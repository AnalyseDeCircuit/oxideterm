# Changelog

## v1.9.2

### ✨ 新功能

#### 跨平台驱动器/卷检测 (Cross-Platform Volume Detection)
- 使用 `sysinfo` crate 替代简单的 `["/"]` 返回值，实现真正的跨平台挂载卷检测
- 返回结构化 `DriveInfo`：路径、名称、类型（system/removable/network）、容量、是否只读
- macOS：自动检测系统盘 + 外接卷，APFS firmlink 去重（基于 `dev_id` 而非 `canonicalize()`）
- Linux：解析 `/proc/mounts`，过滤伪文件系统（/proc、/sys、/dev 等），放行 `/run/media/`、`/run/mount/`、`/run/user/*/gvfs/` 等真实挂载
- Windows：枚举所有盘符（C:\、D:\ 等）
- 驱动器选择对话框展示容量进度条（>70% 琥珀色、>90% 红色）和驱动器类型图标
- 只读卷显示琥珀色「只读」徽章（macOS SSV 系统卷做特判，避免误报）
- 所有平台均可使用驱动器按钮（移除了原有的 `platform.isWindows` 限制）

#### 文件属性对话框 (File Properties Dialog)
- 新增文件属性详情面板，支持查看文件大小、类型、时间戳、权限等元数据
- 色彩化 Unix 权限显示：`r` 绿色 / `w` 橙色 / `x` 蓝色，一目了然
- 平台自适应标题：macOS 显示「显示简介」，Windows/Linux 显示「属性」
- 支持按需计算文件校验和（MD5 / SHA-256），64KB 流式读取，大文件友好
- 目录内容统计：自动递归统计文件数、子目录数和总大小
- 深度 MIME 检测：基于 `infer` crate 的 magic bytes 识别，不再仅依赖扩展名

#### OSC 52 剪贴板支持
- 远程程序（如 tmux、vim）可通过 OSC 52 序列直接写入本地系统剪贴板
- 支持在设置中开关此功能，默认启用
- 本地终端与远程 SSH 终端均已适配

#### WSL Graphics 会话管理增强
- 应用程序会话自动清理机制，防止僵尸进程残留
- 新增信号通知机制，优化会话生命周期管理
- 明确回滚操作以清理异常退出的 WSL 会话

### 🔧 改进

#### 网络韧性 (Network Resilience)
- WebSocket 发送超时从 5s 提升至 15s，适配高延迟网络
- 连接恢复尝试次数从 5 次提升至 15 次
- 最大退避时间从 3s 提升至 15s
- 新增网络恢复检测逻辑，弱网环境下连接更稳定

#### SFTP TransferGuard
- 新增 `TransferGuard` 机制，防止传输控制句柄泄漏
- 确保异常退出时正确注销传输会话，避免资源泄露

#### 文件管理器上下文菜单
- 文件列表右键菜单功能增强，新增复制/剪切/粘贴、压缩/解压、属性查看等操作
- 支持多选文件批量操作

### 🐛 修复

#### 驱动器去重索引越界 (P1)
- 修复 Unix 上 `seen_dev_ids.insert()` 在伪文件系统过滤之前执行的问题
- 被过滤的挂载点（如 `/run/*/snap`）会占用索引但不推入 `drives` 数组，导致同设备后续挂载点触发 `drives[existing_idx]` 越界 panic
- 将索引注册延迟到所有过滤检查通过之后

#### 文件属性异步数据竞态 (P2)
- 修复 `handleProperties` 中异步 metadata/dirStats/checksum 回调覆盖当前文件数据的问题
- 添加 `propertiesPathRef` 请求令牌守卫，所有异步回调在应用结果前检查路径匹配
- 覆盖 metadata、dirStats、checksum 三个异步流

#### 粘贴操作错误信息丢失
- 修复 `useFileClipboard.paste()` 仅显示失败计数而不包含错误原因的问题
- 现在 toast 显示第一个失败文件名 + 具体错误（如 `Permission denied`）
- `handlePaste`、`handleCompress`、`handleExtract` 添加外层 try-catch，防止未捕获异常

#### `/run` 路径过滤过于严格
- 放宽 Linux 上 `/run` 的过滤策略，允许真实挂载路径通过
- 放行 `/run/media/*`（udisks2）、`/run/mount/*`、`/run/user/*/gvfs/*`（GNOME）

#### macOS 系统卷只读误报
- macOS Catalina+ 的 SSV（签名系统卷）挂载在 `/` 技术上是只读的，但用户通过 firmlink 实际可写
- 对 macOS `/` 做特判：检测 `/Users` 目录是否可写来决定 `isReadOnly`

### 🌐 国际化

- 新增 44+ 个 i18n 键值，覆盖文件属性、校验和、目录统计、驱动器选择等新功能
- 新增 `available`（可用空间）和 `readOnly`（只读）键，覆盖 fileManager + sftp 双场景
- 全部 11 个语言完整翻译：en、zh-CN、zh-TW、ko、ja、es-ES、de、fr-FR、it、pt-BR、vi

### 📦 依赖变更

- 新增 `sysinfo = { version = "0.33", features = ["disk"] }` — 跨平台磁盘/卷检测
- 新增 `infer = "0.16"` — 基于 magic bytes 的文件类型检测
- 新增 `md-5 = "0.10"` — MD5 校验和计算

### 📊 统计

- 变更文件：93 个
- 新增代码：~1,795 行
- 删除代码：~459 行
