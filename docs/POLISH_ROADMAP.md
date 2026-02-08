# OxideTerm 功能深度打磨路线图

> **版本**: v1.0
> **日期**: 2026-02-08
> **目的**: 基于代码库深度分析，列出各功能域的具体改进点

---

## 目录

1. [终端核心体验](#1-终端核心体验)
2. [重连可靠性](#2-重连可靠性)
3. [SFTP 文件传输](#3-sftp-文件传输)
4. [端口转发](#4-端口转发)
5. [IDE 模式](#5-ide-模式)
6. [本地终端](#6-本地终端)
7. [设置与配置](#7-设置与配置)
8. [错误处理与用户反馈](#8-错误处理与用户反馈)
9. [跨平台一致性](#9-跨平台一致性)
10. [启动性能](#10-启动性能)

---

## 1. 终端核心体验

### 1.1 输入路径分析

**��前实现** (`src/components/terminal/TerminalView.tsx`):

```
term.onData (L1078)
  → inputLockedRef 检查 (L1078)
  → runInputPipeline (L1084) [pluginTerminalHooks.ts:29-66]
  → encodeDataFrame (L65-72)
  → ws.send (L1092)
```

**已有优化**:
- 无插件时快速路径：`if (interceptors.length === 0) return data;` (pluginTerminalHooks.ts:31)
- 插件管道同步执行，fail-open 错误处理

**可改进点**:

| 改进项 | 位置 | 说明 |
|--------|------|------|
| 输入延迟度量 | 新增 | 添加 keydown → ws.send 的时间戳记录，建立性能基线 |
| 插件超时警告 | pluginTerminalHooks.ts:43-50 | 当前 5ms 预算，超时仅 console.warn，考虑添加 UI 提示 |

### 1.2 输出路径分析

**当前实现** (`src/components/terminal/TerminalView.tsx:178-264`):

```
ws.onmessage
  → handleWsMessage (L178-264)
  → runOutputPipeline (L198)
  → RAF 批处理 (L200-248)
  → term.write (L217)
```

**平台差异**:
- **Windows** (L200-227): IME 组合状态分支处理，防止候选窗口闪烁
- **macOS/Linux** (L228-248): 始终使用 RAF 批处理

**可改进点**:

| 改进项 | 位置 | 说明 |
|--------|------|------|
| 输出洪流节流 | L200-248 | 当前无显式节流，大输出（`cat` 大文件）时可能卡顿。考虑在输出速率超过阈值时降低渲染频率 |
| 背压机制 | 无 | WebSocket 无背压，如果 xterm.js 渲染慢于数据到达，pendingDataRef 会无限增长 |
| 内存拷贝优化 | L193-195 | `slice()` 创建副本防止 ArrayBuffer 保留，但高频场景下 GC 压力大 |

### 1.3 渲染器配置

**当前实现** (`src/components/terminal/TerminalView.tsx:747-801`):

三层渲染器策略：
1. **Canvas 模式** (L747): 强制 CanvasAddon
2. **WebGL 模式** (L756): 强制 WebglAddon，带 context loss 回调
3. **Auto 模式** (L772): 先尝试 WebGL，失败回退 Canvas，最终回退 DOM

**已有优化**:
- WebGL context loss 自动恢复 (L762-768)
- Unicode11Addon 支持 Nerd Font 和 CJK (L710-714)
- 图片限制：16M 像素，64MB FIFO 缓存 (L698-699)

**可改进点**:

| 改进项 | 位置 | 说明 |
|--------|------|------|
| 渲染器切换 UI | SettingsView | 当前有设置项，确认用户能理解各模式差异 |
| WebGL 崩溃统计 | L762-768 | 记录 context loss 频率，帮助诊断 GPU 兼容性问题 |

### 1.4 选择与复制

**当前实现**:
- 使用 xterm.js 原生 `getSelection()` (L840)
- 多行粘贴确认保护 (L1657-1680)

**可改进点**:

| 改进项 | 说明 |
|--------|------|
| 复制格式选项 | 当前复制纯文本，可选保留 ANSI 颜色或 HTML 格式 |
| 双击选词优化 | 验证 URL、路径等特殊模式的选择行为 |
| 矩形选择 | Alt+拖拽的列选择模式（xterm.js 支持但需启用） |

### 1.5 关键常量

| 常量 | 位置 | 当前值 | 说明 |
|------|------|--------|------|
| scrollback | L686 | 5000 | 前端默认，后端 100,000 |
| 图片像素限制 | L698 | 16,777,216 | 4096×4096 |
| 图片缓存 | L699 | 64MB | SSH 终端 |
| 图片缓存 | LocalTerminalView:235 | 128MB | 本地终端 |
| Resize debounce | L1149 | 50ms | |
| 插件 hook 预算 | pluginTerminalHooks.ts:20 | 5ms | |

---

## 2. 重连可靠性

### 2.1 重连管道

**当前实现** (`src/store/reconnectOrchestratorStore.ts`):

```
snapshot → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → done
```

| 阶段 | 行号 | 行为 | 失败处理 |
|------|------|------|----------|
| snapshot | 473-564 | 捕获转发规则、未完成传输、IDE 状态 | 警告日志，继续 |
| ssh-connect | 568-621 | 调用 reconnectCascade()，3 次重试 | 可重试错误重试，否则失败 |
| await-terminal | 625-706 | 轮询新 terminalSessionId（10s 超时，500ms 间隔） | 警告日志，继续 |
| restore-forwards | 710-797 | 重建端口转发，检查重复 | 单个失败不阻塞其他 |
| resume-transfers | 801-890 | 恢复未完成 SFTP 传输 | 单个失败不阻塞其他 |
| restore-ide | 894-976 | 重新打开 IDE 项目和标签页 | 尊重用户意图 |

**关键参数**:
- `MAX_ATTEMPTS = 3`
- `RETRY_DELAY_MS = 2000`
- `DEBOUNCE_MS = 500`
- 可重试错误：`CHAIN_LOCK_BUSY`, `NODE_LOCK_BUSY`

### 2.2 用户反馈

**当前 Toast 通知** (L138-148):
- `connections.reconnect.starting` — 作业排队时
- `connections.reconnect.ssh_restored` — SSH 重连后
- `connections.reconnect.completed` — 完成，显示恢复数量
- `connections.reconnect.failed` — 失败，显示错误信息
- `connections.reconnect.cancelled` — 取消时

**可改进点**:

| 改进项 | 说明 |
|--------|------|
| 进度条 UI | 当前只有 Toast，考虑在终端标签页显示重连进度条 |
| 当前步骤显示 | 显示"正在恢复端口转发 (2/5)"等详细进度 |
| 手动取消按钮 | 在重连过程中允许用户取消 |
| 重连历史 | 记录最近重连的成功/失败，帮助诊断网络问题 |

### 2.3 边界情况

**跳板机链路** (`src/store/sessionTreeStore.ts:824-954`):
- `connectNodeWithAncestors()` 获取完整祖先路径
- 跳过已连接的前缀节点
- 获取链锁（全局，防止并发链路）
- 线性连接：root → intermediate → target

**中间节点断开** (L727-795):
- 在根节点调用 `reconnectCascade()`
- 然后重连 link-down 子节点
- **Gap**: 如果中间节点重连成功但目标节点失败，目标保持 link-down

**可改进点**:

| 改进项 | 位置 | 说明 |
|--------|------|------|
| 链锁超时 | L832 | 当前无超时，可能死锁 |
| 终端等待超时回退 | L114 | 10s 超时后无回退策略 |
| 部分链路恢复 | L767-770 | 父节点未连接时跳过子节点，考虑延迟重试 |

### 2.4 已知错误处理 Gap

| 文件 | 行号 | 问题 |
|------|------|------|
| sessionTreeStore.ts | 512-514 | `topologyResolver.unregister()` 错误被忽略 |
| sessionTreeStore.ts | 1072-1075 | 终端关闭失败被忽略 |
| sessionTreeStore.ts | 1087-1089 | SFTP 关闭失败被忽略 |
| sessionTreeStore.ts | 1097-1108 | appStore 会话清理失败被忽略 |
| forwarding/manager.rs | 518 | `stop_forward()` 错误静默忽略 |

---

## 3. SFTP 文件传输

### 3.1 当前能力

**UI 组件** (`src/components/sftp/`):
- `SFTPView.tsx` (2067 行) — 双窗格文件管理器
- `TransferQueue.tsx` (332 行) — 传输队列，支持暂停/恢复
- `TransferConflictDialog.tsx` (215 行) — 冲突解决

**已实现功能**:
- ✅ 拖拽上传/下载 (SFTPView.tsx:435-441, 1442-1458)
- ✅ 冲突解决：跳过/覆盖/重命名/跳过旧文件/取消 (TransferConflictDialog.tsx:16)
- ✅ 并排比较，高亮较新文件 (L109-158)
- ✅ "应用到全部"批量操作 (L161-172)
- ✅ 进度条显示 (TransferQueue.tsx:257)
- ✅ 暂停/恢复 (L282-304)
- ✅ 断点续传 (L89-112, L176-233)

**后端参数** (`src-tauri/src/sftp/transfer.rs`):
- 默认并发传输：3 (L77)
- 最大并发：10 (L74)
- 速度限制：可配置 (L111-120)

### 3.2 重试逻辑

**文件**: `src-tauri/src/sftp/retry.rs`

| 参数 | 值 |
|------|-----|
| 最大重试 | 3 次 |
| 退避策略 | 指数：1s → 2s → 4s，上限 30s |
| 可重试错误 | IoError, ChannelError, 超时/连接 ProtocolErrors |
| 不可重试 | PermissionDenied, FileNotFound 等 |

### 3.3 可改进点

| 改进项 | 当前状态 | 建议 |
|--------|----------|------|
| 校验和验证 | 未发现 | 大文件传输后可选 MD5/SHA256 校验 |
| 符号链接处理 | 需验证 | 确认是否正确处理，是否有选项跟随/保留 |
| 文件权限保留 | 需验证 | 确认 Unix 权限是否在传输中保留 |
| 目录传输进度 | 按文件 | 考虑显示总字节进度 |
| 传输速度平滑 | 需验证 | 速度显示是否有抖动 |

---

## 4. 端口转发

### 4.1 当前实现

**后端** (`src-tauri/src/forwarding/`):
- `manager.rs` — 转发生命周期管理
- `local.rs` — 本地端口转发
- `remote.rs` — 远程端口转发
- `dynamic.rs` — 动态转发 (SOCKS)

**通道参数** (local.rs:381-382):
- mpsc 通道容量：32 项
- 缓冲区大小：32KB (L395)
- 空闲超时：300 秒 (L340)

**统计跟踪** (manager.rs:76-85):
- `connection_count` — 总连接数
- `active_connections` — 活跃连接数
- `bytes_sent` / `bytes_received` — 流量统计

### 4.2 UI 状态

**发现**: 未找到独立的转发 UI 组件目录。转发管理可能嵌入在：
- 连接设置中
- 或通过其他入口访问

### 4.3 可改进点

| 改进项 | 说明 |
|--------|------|
| 独立转发管理 UI | 显示所有活跃转发、流量统计、连接数 |
| 实时流量监控 | 图表显示每个转发的带宽使用 |
| 转发日志 | 记录连接建立/断开事件 |
| 错误可见性 | 转发失败时的用户通知 |
| stopped_forwards 清理 | 当前无界增长，需要 TTL 或数量限制 |

---

## 5. IDE 模式

### 5.1 当前实现

**组件** (`src/components/ide/`):
- `IdeEditor.tsx` — 主编辑器（使用 CodeMirror）
- `IdeWorkspace.tsx` — 工作区布局
- `IdeEditorTabs.tsx` — 标签页管理
- `IdeStatusBar.tsx` — 状态栏
- `IdeTree.tsx` — 文件树
- `IdeSearchPanel.tsx` — 搜索面板
- `IdeTerminal.tsx` — 集成终端

**编辑器**: CodeMirror (IdeEditor.tsx:6)
- 语法高亮：根据文件类型自动检测 (L56)
- 保存：调用 ideStore.saveFile() (L33-40)
- 未保存检测：通过 contentVersion 跟踪 (L24)

### 5.2 可改进点

| 改进项 | 说明 |
|--------|------|
| 编辑器主题 | 与终端主题同步，或独立配置 |
| 字体设置 | 编辑器字体大小/字体族配置 |
| 自动保存 | 可选的定时自动保存 |
| 文件类型支持 | 确认支持的语言列表，考虑扩展 |
| 大文件处理 | 大文件打开时的性能和警告 |

---

## 6. 本地终端

### 6.1 与 SSH 终端的差异

**文件**: `src/components/terminal/LocalTerminalView.tsx`

| 差异点 | SSH 终端 | 本地终端 |
|--------|----------|----------|
| 数据通道 | WebSocket | Tauri IPC |
| 二进制输入 | 无 | term.onBinary() (L380-387) |
| 输入锁 | inputLockedRef | 无 |
| 图片缓存 | 64MB | 128MB |
| 搜索暂停 | 无 | 有 (L90-93, L528-544) |

**搜索暂停机制** (L528-544):
- 高频输出时暂停搜索更新
- 150ms 无输出后恢复
- 防止搜索结果"1→2→3→1"循环

### 6.2 可改进点

| 改进项 | 位置 | 说明 |
|--------|------|------|
| PTY 清理 | L477-481 | 注释提到 React StrictMode 双挂载问题，需验证清理完整性 |
| Shell 选择 | 需验证 | 确认用户能选择不同 shell |

---

## 7. 设置与配置

### 7.1 当前设置项

**文件**: `src/components/settings/SettingsView.tsx`

- ✅ 终端字体族、字体大小、光标样式、渲染器类型
- ✅ 主题选择（带预览）
- ✅ SSH 密钥管理
- ✅ SSH 主机配置
- ✅ SFTP 设置（并发数、速度限制、冲突处理）
- ✅ 本地终端设置
- ✅ 快捷键帮助

### 7.2 可能缺失的设置

| 设置项 | 说明 |
|--------|------|
| 滚动缓冲行数 | 后端有 BufferConfig.max_lines，前端是否暴露？ |
| 连接超时 | SSH 连接超时时间 |
| 重连延迟 | 自动重连的延迟配置 |
| IDE 编辑器主题 | 独立于终端主题 |
| 代理设置 | HTTP/SOCKS 代理配置 |

---

## 8. 错误处理与用户反馈

### 8.1 当前实现

**全局错误边界** (`src/components/ErrorBoundary.tsx`):
- 捕获 React 组件错误
- 显示错误信息、堆栈、组件栈
- 提供"尝试恢复"和"重新加载"选项
- 复制错误到剪贴板
- 链接到 GitHub Issues

**Toast 系统** (`src/App.tsx:62-70`):
- useToastStore 管理
- 变体：success, error, warning, default
- 可配置持续时间

### 8.2 可改进点

| 改进项 | 说明 |
|--------|------|
| 错误分类 | 区分网络错误、认证错误、权限错误，给出针对性建议 |
| 错误历史 | 保留最近错误列表，方便用户报告问题 |
| 离线模式提示 | 网络断开时的明确提示和重试选项 |
| 操作确认 | 危险操作（删除、覆盖）的二次确认 |

---

## 9. 跨平台一致性

### 9.1 Windows 特有处理

**IME 处理** (`src/components/terminal/TerminalView.tsx`):
- `isComposingRef` 跟踪组合状态 (L144)
- Windows 分支：组合时 RAF 缓冲，非组合时直接写入 (L200-227)
- 组合事件监听器 (L1053-1071)

### 9.2 可改进点

| 平台 | 改进项 | 说明 |
|------|--------|------|
| Windows | 高 DPI 缩放 | 验证不同缩放比例下的渲染 |
| Windows | 字体渲染 | WebGL 在不同 GPU 上的表现 |
| macOS | 触控板手势 | 双指滚动平滑度 |
| macOS | 原生菜单栏 | Tauri 菜单集成 |
| Linux | Wayland 支持 | 验证 Wayland 下的行为 |

---

## 10. 启动性能

### 10.1 当前启动序列

**文件**: `src/App.tsx`

1. 网络状态和连接事件监听器 (L24-25)
2. 加载 shells (L29-30)
3. 预加载终端字体（延迟 500ms）(L48-55)
4. 初始化插件系统 (L58-79)
5. 同步 SFTP 设置到后端 (L82-99)
6. 设置 SessionTree 订阅 (L174-177)

### 10.2 可改进点

| 改进项 | 说明 |
|--------|------|
| 启动时间度量 | 记录从点击图标到可交互的时间 |
| 懒加载 | 验证重型组件是否使用 React.lazy() |
| 并行初始化 | 检查哪些初始化可以并行执行 |
| 首屏优化 | 优先渲染用户最常用的界面 |

---

## 优先级建议

### P0 - 立即改进（影响每次使用）

1. **输出洪流处理** — 大输出时的节流机制
2. **重连进度 UI** — 比 Toast 更详细的进度显示
3. **stopped_forwards 清理** — 防止内存泄漏

### P1 - 短期改进（提升体验）

1. **端口转发管理 UI** — 独立的转发监控界面
2. **传输校验和** — 大文件传输后的完整性验证
3. **错误分类与建议** — 更有帮助的错误信息

### P2 - 中期改进（完善功能）

1. **IDE 编辑器配置** — 主题、字体等设置
2. **跨平台测试** — 系统性验证各平台行为
3. **性能度量** — 建立性能基线和监控

### P3 - 长期改进（锦上添花）

1. **复制格式选项** — 保留颜色或 HTML
2. **矩形选择** — 列选择模式
3. **会话广播** — 同时向多终端发送命令

---

*文档版本: v1.0 | 最后更新: 2026-02-08*
