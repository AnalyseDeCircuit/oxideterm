# 云盘 / 虚拟文件系统融入本地文件管理系统计划

> 状态: Proposed
> 日期: 2026-04-08
> 目标: 在不破坏现有本地文件管理体验的前提下，把云盘和虚拟文件系统能力并入统一文件管理架构。

---

## 1. 背景

OxideTerm 现在已经有三块相近但没有完全统一的文件能力：

1. 本地文件管理器 `LocalFileManager`
2. SFTP 双栏文件视图 `SFTPView`
3. 各类“虚拟文件源”雏形，例如 archive 预览、IDE/Agent 文件访问、拖拽和跨端传输

这三块能力在 UI、选择逻辑、路径处理、权限错误、批量操作、预览和刷新策略上已经出现重复实现。继续为云盘再复制第四套逻辑，后续会产生以下问题：

1. 路径与权限语义进一步分叉
2. 复制、剪切、粘贴、压缩、解压等操作无法统一编排
3. 云盘鉴权、缓存、速率限制和分页逻辑会散落在多个组件中
4. 未来要把 SFTP、Archive、Cloud、Local 做跨源操作时，成本会急剧升高

因此，这件事不应被实现为“给本地文件管理器加几个云盘按钮”，而应被实现为：

**把现有本地文件管理器升级为一个统一的 File Pane + Provider + Operation Orchestrator 架构。**

---

## 2. 目标与非目标

### 2.1 目标

1. 在同一套文件面板中支持 Local、Cloud、Virtual 三类来源。
2. 统一列表、选中、批量选中、右键菜单、预览、属性、拖拽、粘贴、错误提示和刷新行为。
3. 支持跨 Provider 的复制、上传、下载、导入、导出。
4. 让“云盘 / 虚拟文件系统”以 Mount 的形式接入，而不是把业务写死到组件里。
5. 保持现有 UI 风格，不引入另一套文件管理页面。
6. 保持安全边界，凭证始终留在 OS keychain 或后端安全层，不暴露给前端页面脚本。

### 2.2 非目标

1. 第一阶段不追求 Finder / Explorer 那种系统级文件同步客户端。
2. 第一阶段不做离线双向同步引擎。
3. 第一阶段不要求所有 Provider 都支持压缩、解压、重命名、移动等全量能力。
4. 第一阶段不替换现有 SSH / SFTP 架构本身，只统一文件面板抽象。
5. 第一阶段不开放第三方不受信任 Provider 直接执行任意原生 I/O。

---

## 3. 设计原则

### 3.1 单一文件面板核心

本地、云盘、SFTP、Archive 不应各自维护一套文件列表交互逻辑。应该抽成一套统一的 `FilePaneCore`，只把“数据来源”和“能力差异”下沉到 Provider。

### 3.2 Provider 优先，而不是路径分支优先

不要在组件里到处写：

- `if (isRemote)`
- `if (isCloud)`
- `if (provider === 's3')`

应该把差异下沉到 Provider capability 和 operation handler。

### 3.3 规范化标识优先于原始 path

云盘和虚拟文件系统不一定有 POSIX/Windows 路径语义。

因此统一模型必须区分：

1. `entryId`: 稳定对象标识
2. `displayPath`: UI 展示路径
3. `canonicalPath`: Provider 返回的规范路径或 key
4. `parentId`: 用于导航和刷新，不依赖字符串切割

### 3.4 所有跨源操作必须经过编排层

跨源复制、剪切、粘贴、压缩、解压不能直接由组件互调。应由统一的 `FileOperationOrchestrator` 决定：

1. 是否允许该操作
2. 是否需要临时落地到本地 staging
3. 是否需要流式传输
4. 是否需要冲突解决
5. 是否需要部分失败恢复

### 3.5 鉴权和凭证只留在后端

云盘 OAuth token、S3 access key、WebDAV 密码等：

1. 存在 OS keychain 或安全存储
2. 前端只拿到 mount 状态和能力，不直接拿密钥
3. 文件数据流优先走 Tauri command / backend provider，不在前端直连第三方云 SDK

---

## 4. 目标形态

### 4.1 用户视角

文件管理系统左侧来源区域增加三组来源：

1. Local
2. Cloud
3. Virtual

其中：

- Local: 本地磁盘、外接卷、网络卷
- Cloud: WebDAV、S3 兼容对象存储、OneDrive、Dropbox、Google Drive 等
- Virtual: Archive mount、Recent transfers、临时导出区、Agent workspace mirror、后续也可纳入 SFTP provider

用户不再区分“这是哪个页面的文件树”，只区分“这是哪个挂载来源”。

### 4.2 UI 形态

保留现有 `LocalFileManager` 风格，增强而不是重做：

1. 顶部仍是当前文件面板操作区
2. 左侧 sidebar 增加 Mount 列表和 Provider 分组
3. 中央仍是统一 `FileList`
4. 底部仍是操作进度区
5. 错误、鉴权失效、权限拒绝统一用 inline banner + toast 表达

---

## 5. 核心架构

### 5.1 前端层

新增或重构为以下结构：

```text
src/components/fileManager/
  FileManagerShell.tsx         # 页面级容器
  FilePaneCore.tsx             # 统一面板交互
  MountSidebar.tsx             # Mount 列表
  FileList.tsx                 # 继续作为共享列表组件
  PreviewRouter.tsx            # 按 capability / mime 路由预览

src/store/
  fileProviderStore.ts         # Provider registry / mount 状态
  filePaneStore.ts             # 当前 pane 导航、筛选、排序、选中
  fileOperationStore.ts        # 跨源操作编排、进度、冲突处理
```

### 5.2 后端层

新增统一 VFS Router，而不是让前端分别调用不同 Provider API：

```text
src-tauri/src/
  vfs/
    mod.rs
    router.rs                  # mountId -> provider dispatch
    types.rs                   # VfsEntry / VfsError / capabilities
    local_provider.rs
    archive_provider.rs
    webdav_provider.rs
    s3_provider.rs
    oauth_provider.rs          # OneDrive / Dropbox / Google Drive 的统一鉴权层
```

### 5.3 统一数据模型

```ts
type VfsProviderKind =
  | 'local'
  | 'sftp'
  | 'archive'
  | 'webdav'
  | 's3'
  | 'onedrive'
  | 'dropbox'
  | 'gdrive'
  | 'agent-workspace';

type VfsCapability = {
  list: boolean;
  read: boolean;
  write: boolean;
  delete: boolean;
  rename: boolean;
  mkdir: boolean;
  copyWithinMount: boolean;
  moveWithinMount: boolean;
  streamRead: boolean;
  streamWrite: boolean;
  archiveRead: boolean;
  archiveWrite: boolean;
  quickPreview: boolean;
};

type VfsLocation = {
  providerKind: VfsProviderKind;
  mountId: string;
  entryId: string;
  canonicalPath: string;
  displayPath: string;
  parentId?: string | null;
};
```

重点是：**导航、刷新、选中、批量选中以 `entryId` 和 `parentId` 为核心，不再把所有能力建立在 path 字符串切割上。**

---

## 6. Provider 分层策略

### 6.1 第一批 Provider

建议按复杂度从低到高推进：

1. `local`
2. `archive`
3. `webdav`
4. `s3`
5. `onedrive` / `dropbox` / `gdrive`

原因：

1. `local` 和 `archive` 可以先把抽象打通
2. `webdav` 是最通用、最接近文件语义的云 Provider
3. `s3` 能验证“对象存储不是目录树”的抽象是否足够好
4. OAuth 云盘放到后面，避免一开始就被授权流程和 SDK 绑定拖住

### 6.2 为什么不第一阶段就做全 Provider

不同来源的语义差异很大：

1. WebDAV 接近真实文件系统
2. S3 更像对象 key 空间，不是真目录
3. OneDrive/Dropbox 有速率限制、delta API、token 刷新和服务端回收站语义
4. Archive mount 通常是只读或“解包后写回”语义

如果第一阶段就做“统一支持全部读写能力”，计划会在抽象层直接失焦。

---

## 7. 关键能力设计

### 7.1 列表与导航

统一命令建议如下：

1. `vfs_list_dir(mountId, parentId | canonicalPath, cursor?)`
2. `vfs_stat(mountId, entryId)`
3. `vfs_resolve_path(mountId, inputPath)`
4. `vfs_get_children_page(mountId, parentId, pageToken)`

要求：

1. 支持分页或 cursor，不能假设目录一次性全量返回
2. 支持 provider 返回“部分 metadata 延迟加载”
3. 对权限拒绝目录返回结构化错误，而不是空列表

### 7.2 复制 / 剪切 / 粘贴

拆成三种模式：

1. 同 Mount 内复制 / 移动
2. 跨 Mount 导出 / 导入
3. Provider 不支持原地 move 时，降级为 copy + delete

统一入口：

`fileOperationStore.enqueue({ type: 'copy' | 'move', sources, destination })`

编排层负责：

1. 同目录 no-op 判定
2. 复制到自身 / 子目录阻断
3. 名称冲突策略
4. 部分失败统计
5. Provider 限流与重试

### 7.3 压缩 / 解压

第一阶段建议限制为：

1. Local mount: 完整支持
2. Archive mount: 只读浏览，不支持原地“改包”
3. Cloud mount: 默认走 staging 策略

即：

1. 云端压缩: 拉流到本地临时区后打包，再上传目标 mount
2. 云端解压: 下载 archive 到 staging，安全解压，再批量上传回 mount

这样可以最大化复用现有本地 archive 安全逻辑，避免每个 provider 自己实现一套压缩/解压细节。

### 7.4 预览与打开

分成三层：

1. `quick metadata preview`
2. `streaming content preview`
3. `local temp materialization`

例如：

1. 文本文件优先流式读取
2. 图片 / PDF 可用 provider 临时 URL 或受控 temp file
3. Office / 大文件通过本地临时副本打开

### 7.5 权限拒绝 / 鉴权过期

统一错误码：

```text
PermissionDenied
AuthExpired
QuotaExceeded
RateLimited
NotFound
Conflict
UnsupportedOperation
TemporaryUnavailable
```

UI 表现统一：

1. 权限拒绝: inline error banner + retry
2. 鉴权过期: banner + reconnect / re-auth CTA
3. 限流: 显示 backoff 状态，不做高频 toast 轰炸

---

## 8. 与现有模块的关系

### 8.1 LocalFileManager

`LocalFileManager` 不应继续作为“只服务本地磁盘的独立实现”。它应演进为：

1. `FileManagerShell`
2. 默认挂载一个 `local` mount
3. 可额外挂载 `cloud` 和 `virtual` mounts

### 8.2 SFTPView

`SFTPView` 当前保留了一套本地/远程文件面板逻辑。长期看应收敛为：

1. 左 pane: `local` provider mount
2. 右 pane: `sftp` provider mount
3. 共享同一套 `FilePaneCore`

也就是说，**云盘计划的第一步，其实也是文件面板收敛计划。**

### 8.3 插件系统

第一阶段不开放不受信任插件直接注册原生 Provider。

可预留后续模式：

1. 插件声明 `virtual provider`
2. 插件只能提供 metadata / content proxy
3. 真正的原生 I/O 仍由后端 capability 白名单控制

---

## 9. 分阶段路线图

### Phase 0: 面板收敛与抽象准备

目标：先消灭“本地、SFTP、云盘各一套文件面板”趋势。

交付：

1. 抽出 `FilePaneCore`
2. 抽出共享 selection / filter / sort / context menu / error banner 行为
3. `LocalFileManager` 切到 shared core
4. `SFTPView` 的 local pane 和 remote pane 逐步切到 shared core

退出标准：

1. 新 Provider 不需要复制 `FileList + selection + dragdrop + preview` 逻辑
2. 本地和 SFTP 的 pane 行为一致度明显提升

### Phase 1: 引入 VFS Router 与 Local/Archive Provider

目标：把现有本地文件系统和 archive 能力接到统一 Provider 抽象。

交付：

1. `vfs_*` command 基础集合
2. `local` provider 完整迁移
3. `archive` provider 以 mount 形式只读浏览
4. mount sidebar 初版

退出标准：

1. 本地文件管理器已通过 mount 方式访问本地磁盘
2. archive 可作为虚拟目录打开，而不是只停留在预览弹窗

### Phase 2: 云盘 MVP - WebDAV First

目标：上线第一类真实云 Provider。

交付：

1. WebDAV 账号配置与 keychain 持久化
2. Mount / Unmount / Reconnect
3. 列表、读取、下载、上传、重命名、删除、mkdir
4. 权限拒绝和认证失败统一错误模型

退出标准：

1. 用户可以把 WebDAV 目录当作一个挂载卷使用
2. Local <-> WebDAV 复制链路稳定

### Phase 3: 对象存储与跨源编排

目标：验证“非目录语义”的 Provider 也能进入统一架构。

交付：

1. S3 兼容 provider
2. prefix / delimiter 目录投影视图
3. 跨 mount 复制编排、断点状态、部分失败恢复
4. staging policy 落地

退出标准：

1. 跨 local / webdav / s3 的 copy/export/import 语义稳定
2. 用户理解上仍是同一套文件操作体验

### Phase 4: OAuth 云盘

目标：接入用户最熟悉的消费级云盘。

候选：

1. OneDrive
2. Dropbox
3. Google Drive

交付：

1. OAuth 授权流
2. token refresh
3. mount reconnect
4. 大文件分片上传下载
5. API backoff / quota handling

### Phase 5: 高级虚拟文件系统

候选：

1. Agent workspace mount
2. Recent transfer staging mount
3. Search result virtual folder
4. Diff result virtual folder
5. Unified remote/local/cloud compare workspace

---

## 10. 性能策略

云盘与虚拟文件系统接入后，性能问题会比本地更敏感，必须前置设计。

### 10.1 列表性能

1. 支持分页 / cursor，不假设全量返回
2. 支持 metadata lazy fetch
3. 目录排序优先在 provider 端完成，前端尽量避免对超大列表二次全量加工

### 10.2 预览性能

1. 文本预览优先 range / stream
2. 二进制预览优先临时 URL 或分块读取
3. 图片缩略图采用异步生成和 LRU 缓存

### 10.3 操作性能

1. 大文件跨云复制采用流式管道
2. 多文件批量操作采用带并发上限的 operation queue
3. 每个 Provider 自报建议并发度，避免 WebDAV/S3/OAuth API 被打爆

### 10.4 刷新性能

1. 保留 request generation / stale result guard
2. 云 Provider 再加 cancellation token / generation gate
3. mount 级别缓存 TTL，避免每次切 pane 都重列目录

---

## 11. 安全要求

1. 所有云盘凭证进入 OS keychain，不写入明文配置。
2. 前端不直接持有长期 access token。
3. Provider 下载到本地 staging 的文件必须可清理、可追踪、可过期回收。
4. 压缩 / 解压继续沿用现有本地安全策略：拒绝覆盖、拒绝路径逃逸、拒绝符号链接污染。
5. 对 WebDAV / OAuth 返回的外链 URL，必须审查是否会绕开当前 Tauri 资产安全边界。

---

## 12. 测试策略

### 12.1 Provider Contract Tests

为每个 Provider 跑同一套契约测试：

1. list/stat/read/mkdir/rename/delete
2. 权限拒绝
3. 不存在
4. 限流
5. 名称冲突
6. 大目录分页

### 12.2 Cross-Provider Integration Tests

重点覆盖：

1. local -> cloud copy
2. cloud -> local export
3. cloud -> cloud copy via staging
4. 同 mount / 跨 mount 剪切粘贴
5. 权限拒绝目录进入后的 UI 和恢复行为
6. 大批量选择 + 批量粘贴 + 部分失败

### 12.3 Regression Tests

必须保住本地文件管理器现有高风险用例：

1. Windows drive / UNC 路径
2. 同目录 cut/paste no-op
3. 目录复制到自身/子目录阻断
4. archive overwrite protection
5. symlink loop protection

---

## 13. 主要风险

### 13.1 抽象过度

风险：过早做一个“万能文件系统接口”，最后每个 Provider 都要逃逸出抽象。

应对：

1. 用 capability model
2. 先做 Local / Archive / WebDAV 三种差异明显但仍可控的来源

### 13.2 S3 不是文件系统

风险：把对象存储硬套进目录树，导致 rename、move、mtime、empty folder 全部语义混乱。

应对：

1. 显式把 S3 视为“prefix projected VFS”
2. 某些能力可标记 unsupported，不强行伪造

### 13.3 OAuth 和第三方 SDK 复杂度

风险：早期被 Google Drive / OneDrive SDK、token refresh 和 API 配额问题拖住。

应对：

1. WebDAV first
2. OAuth provider 延后到 Phase 4

### 13.4 继续复制旧逻辑

风险：为了赶进度，把云盘直接塞进 `LocalFileManager` 或 `SFTPView`，形成第四套逻辑。

应对：

1. Phase 0 先做 `FilePaneCore`
2. 新 Provider 接入前必须先接共享 pane

---

## 14. 建议的落地顺序

按性价比，建议优先级如下：

1. 先做 `FilePaneCore` 和 VFS 抽象，不先做任何消费级云盘接入
2. 把 `local` 和 `archive` 接成第一批 provider，验证抽象是否站得住
3. 上 WebDAV，验证真实网络文件系统
4. 再做 S3，验证对象存储语义
5. 最后再引入 OAuth 云盘

---

## 15. 结论

这项工作本质上不是“给本地文件管理器增加云盘入口”，而是：

**把 OxideTerm 的文件管理能力升级为统一的虚拟文件系统平台。**

如果只做表层入口，短期会看起来上线更快，但很快会在以下方面反噬：

1. 逻辑重复
2. 权限和错误处理不一致
3. 跨源操作不可维护
4. 云盘、SFTP、Archive 各自演化

如果按本计划推进，最终可以形成一套统一文件管理底座，让本地磁盘、SFTP、Archive、Cloud、Agent workspace 都进入同一套 pane 和 operation system。

这条路线的第一里程碑不是某个具体云盘，而是：

**先把文件面板抽象收敛成一套可承载多 Provider 的核心。**