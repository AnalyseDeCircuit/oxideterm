# OxideTerm 技术债务审查报告

> 审查日期：2026-01-29  
> 审查范围：前端 (React/TypeScript) + 后端 (Rust/Tauri)  
> 状态：📋 待修复

## 目录

1. [概述](#概述)
2. [问题汇总](#问题汇总)
3. [关键问题 (Critical)](#1-关键问题-critical-)
4. [重要问题 (High)](#2-重要问题-high-)
5. [中等问题 (Medium)](#3-中等问题-medium-)
6. [建议改进 (Low)](#4-建议改进-low-)
7. [修复计划](#修复计划)
8. [依赖关系图](#依赖关系图)

---

## 概述

OxideTerm 是一个使用 Tauri + React + Rust 构建的 SSH 终端客户端。整体架构设计合理，代码质量较高，但存在一些需要关注的技术债务。本文档旨在系统性地记录这些问题并提供修复方案。

### 统计

| 严重级别 | 数量 | 建议时间窗口 |
|---------|-----|-------------|
| 🔴 Critical | 3 | 立即修复 |
| 🟠 High | 5 | 1-2 周内 |
| 🟡 Medium | 5 | 1-2 月内 |
| 💚 Low | 5 | 长期改进 |

---

## 问题汇总

### 快速索引

| ID | 严重性 | 问题 | 文件 | 状态 |
|----|--------|------|------|------|
| C-1 | 🔴 | Rust `unwrap()` 滥用 | `kbi.rs`, `registry.rs`, `parser.rs` | [x] ✅ 2026-01-29 |
| C-2 | 🔴 | `expect()` 在关键路径 | `main.rs`, `transfer.rs` | [x] ✅ 2026-01-29 |
| C-3 | 🔴 | WebSocket Token 有效期过短 | `bridge/server.rs` | [x] ✅ 2026-01-29 |
| H-1 | 🟠 | SFTPView 组件过度复杂 | `SFTPView.tsx` (1946行) | [ ] |
| H-2 | 🟠 | TerminalView 状态管理复杂 | `TerminalView.tsx` (1345行) | [ ] |
| H-3 | 🟠 | appStore 过于集中 | `appStore.ts` (780行) | [ ] |
| H-4 | 🟠 | 事件监听器内存泄漏风险 | 多个组件 | [x] ✅ 2026-01-29 |
| H-5 | 🟠 | Rust 连接池死锁风险 | `connection_registry.rs` | [x] ✅ 2026-01-29 |
| M-1 | 🟡 | 传输冲突处理逻辑重复 | `SFTPView.tsx` | [ ] |
| M-2 | 🟡 | 缺少请求取消机制 | `api.ts` | [ ] |
| M-3 | 🟡 | 事件监听器清理不完整 | 多处 | [ ] |
| M-4 | 🟡 | 硬编码的超时和重试值 | 多处 | [ ] |
| M-5 | 🟡 | 前端缺少错误边界 | 组件层 | [ ] |
| L-1 | 💚 | TypeScript 类型安全改进 | 多处 | [ ] |
| L-2 | 💚 | 缺少单元测试 | - | [ ] |
| L-3 | 💚 | i18n 键类型安全 | 多处 | [ ] |
| L-4 | 💚 | 日志级别优化 | 多处 | [ ] |
| L-5 | 💚 | 废弃 API 清理 | `api.ts`, `appStore.ts` | [ ] |

---

## 1. 关键问题 (Critical) 🔴

### C-1: Rust `unwrap()` 滥用可能导致 Panic ✅ 已修复

> **修复日期**: 2026-01-29  
> **修复内容**: 将 `std::sync::Mutex` 替换为 `parking_lot::Mutex`，移除所有 `.unwrap()` 调用

**问题描述**

后端代码中存在多处 `lock().unwrap()` 调用，当锁被污染（poisoned）时会导致 panic，使整个应用崩溃。

**影响范围**
- 高并发场景下锁竞争
- 异常线程终止后锁污染
- 生产环境稳定性

**问题位置**

```
src-tauri/src/ssh/kbi.rs
├── Line ~45: PENDING_REQUESTS.lock().unwrap()
├── Line ~67: PENDING_REQUESTS.lock().unwrap()
└── Line ~89: PENDING_REQUESTS.lock().unwrap()

src-tauri/src/session/registry.rs
├── Line ~112: sessions.lock().unwrap()
└── Line ~156: sessions.lock().unwrap()

src-tauri/src/forwarding/manager.rs
└── Line ~78: forwardings.lock().unwrap()
```

**修复方案**

**方案 A：返回 Result 错误（推荐）**

```rust
// 定义锁错误类型
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("Lock poisoned: {0}")]
    Poisoned(String),
}

// 修改前
let mut pending = PENDING_REQUESTS.lock().unwrap();

// 修改后
let mut pending = PENDING_REQUESTS.lock()
    .map_err(|e| LockError::Poisoned(e.to_string()))?;
```

**方案 B：使用 parking_lot（性能更好，无毒锁）**

```toml
# Cargo.toml
[dependencies]
parking_lot = "0.12"
```

```rust
use parking_lot::Mutex;

// parking_lot 的 Mutex 不会被污染，lock() 直接返回 Guard
let mut pending = PENDING_REQUESTS.lock();
```

**推荐方案**：方案 B（parking_lot）

理由：
1. 无锁污染问题，API 更简洁
2. 性能比 std::sync::Mutex 更好
3. 支持超时锁定，便于死锁检测

**修复步骤**

1. 在 `Cargo.toml` 添加 `parking_lot = "0.12"`
2. 全局替换 `std::sync::Mutex` → `parking_lot::Mutex`
3. 全局替换 `std::sync::RwLock` → `parking_lot::RwLock`
4. 移除所有 `.lock().unwrap()` 中的 `.unwrap()`
5. 运行测试确保行为一致

**依赖关系**：无，可独立修复

---

### C-2: `expect()` 在关键路径可能导致 Panic ✅ 已修复

> **修复日期**: 2026-01-29  
> **修复内容**: 
> - `lib.rs`: 使用 `map_err()` + `ok()` + `map()` 处理构建错误，显示友好对话框
> - `transfer.rs`: 使用 `unwrap_or_else()` 并添加详细 panic 信息
> - `storage.rs`: 使用 `unwrap_or_else()` 并改进错误信息

**问题描述**

关键代码路径上使用 `expect()` 可能在异常情况下导致应用崩溃，而非优雅降级。

**影响范围**
- 应用启动失败
- 传输过程中断
- 用户体验

**问题位置**

```
src-tauri/src/main.rs
└── Line ~89: .expect("error while building tauri application")

src-tauri/src/sftp/transfer.rs
├── Line ~156: .expect("Semaphore closed unexpectedly")
└── Line ~234: .expect("Failed to send progress")

src-tauri/src/bridge/server.rs
└── Line ~67: .expect("Failed to bind WebSocket")
```

**修复方案**

**1. main.rs - 应用启动**

```rust
// 修改前
tauri::Builder::default()
    // ...
    .run(tauri::generate_context!())
    .expect("error while building tauri application");

// 修改后
fn main() {
    if let Err(e) = run_app() {
        // 使用已有的 show_startup_error 函数
        show_startup_error(&format!("Failed to start application: {}", e));
        std::process::exit(1);
    }
}

fn run_app() -> Result<(), Box<dyn std::error::Error>> {
    tauri::Builder::default()
        // ...
        .run(tauri::generate_context!())?;
    Ok(())
}
```

**2. transfer.rs - 信号量获取**

```rust
// 修改前
let permit = self.semaphore.acquire().await
    .expect("Semaphore closed unexpectedly");

// 修改后
let permit = self.semaphore.acquire().await
    .map_err(|_| TransferError::SemaphoreClosed)?;
```

**3. bridge/server.rs - WebSocket 绑定**

```rust
// 修改前
let listener = TcpListener::bind(addr).await
    .expect("Failed to bind WebSocket");

// 修改后
let listener = TcpListener::bind(addr).await
    .map_err(|e| BridgeError::BindFailed(e.to_string()))?;
```

**修复步骤**

1. 为每个模块定义适当的错误类型
2. 将 `expect()` 替换为 `?` 运算符
3. 在调用链顶层处理错误
4. 对于启动错误，显示友好的错误对话框

**依赖关系**：依赖 C-1（统一错误处理模式）

---

### C-3: WebSocket Token 有效期过短 ✅ 已修复

> **修复日期**: 2026-01-29  
> **修复内容**: 将 `TOKEN_VALIDITY_SECS` 从 60 秒延长到 300 秒（5 分钟）

**问题描述**

WebSocket 认证 Token 有效期仅 60 秒，在以下场景可能导致合法连接认证失败：
- 高延迟网络（卫星连接、跨国网络）
- 系统高负载
- 用户操作缓慢

**影响范围**
- 部分用户无法建立终端连接
- 认证失败错误难以诊断
- 用户体验

**问题位置**

```
src-tauri/src/bridge/server.rs
└── Line ~23: const TOKEN_VALIDITY_SECS: u64 = 60;
```

**修复方案**

**方案 A：延长有效期（简单）**

```rust
// 修改前
const TOKEN_VALIDITY_SECS: u64 = 60;

// 修改后 - 延长到 5 分钟
const TOKEN_VALIDITY_SECS: u64 = 300;
```

**方案 B：实现 Token 刷新机制（完整）**

```rust
// 新增刷新 Token 的命令
#[tauri::command]
pub async fn refresh_ws_token(session_id: String) -> Result<String, String> {
    let registry = SESSION_REGISTRY.read().await;
    if let Some(session) = registry.get(&session_id) {
        let new_token = generate_secure_token();
        session.update_ws_token(new_token.clone()).await;
        Ok(new_token)
    } else {
        Err("Session not found".to_string())
    }
}

// 前端在连接前检查 Token 是否即将过期
const isTokenExpiringSoon = (tokenTimestamp: number) => {
    const now = Date.now() / 1000;
    const remaining = tokenTimestamp + TOKEN_VALIDITY_SECS - now;
    return remaining < 30; // 剩余不足 30 秒时刷新
};
```

**推荐方案**：方案 A（先延长到 300 秒）

理由：
1. 修改简单，风险低
2. 300 秒足够覆盖绝大多数场景
3. Token 是一次性使用，延长有效期安全影响有限

**修复步骤**

1. 将 `TOKEN_VALIDITY_SECS` 改为 300
2. （可选）在前端添加 Token 剩余时间检查
3. （可选）实现 Token 刷新 API

**依赖关系**：无，可独立修复

---

## 2. 重要问题 (High) 🟠

### H-1: SFTPView 组件过度复杂

**问题描述**

`SFTPView.tsx` 单文件达 1946 行，包含：
- 文件列表渲染
- 文件预览对话框
- 传输逻辑
- 重命名/新建/删除对话框
- 拖拽处理
- 右键菜单

**影响范围**
- 维护困难，修改风险高
- 测试困难
- 首次渲染性能
- 代码复用性差

**问题位置**

```
src/components/sftp/SFTPView.tsx (1946 lines)
├── Lines 1-85: FileList 内部组件
├── Lines 86-580: FileList 实现（应提取）
├── Lines 581-970: SFTPView 主组件状态
├── Lines 971-1400: 传输和文件操作逻辑
├── Lines 1401-1700: 对话框渲染
└── Lines 1701-1946: 主渲染
```

**修复方案**

**目标结构**

```
src/components/sftp/
├── SFTPView.tsx          (~400 lines) - 主容器，布局编排
├── FileList.tsx          (~350 lines) - 文件列表组件
├── FileListItem.tsx      (~150 lines) - 单个文件项
├── PreviewDialog.tsx     (~300 lines) - 预览对话框
├── TransferConflictDialog.tsx (~200 lines) - 冲突处理
├── FileOperationDialogs.tsx (~200 lines) - 重命名/新建/删除
├── hooks/
│   ├── useSFTPNavigation.ts   - 路径导航逻辑
│   ├── useSFTPTransfer.ts     - 传输逻辑
│   ├── useSFTPSelection.ts    - 选择逻辑
│   └── useSFTPDragDrop.ts     - 拖拽逻辑
└── types.ts              - SFTP 相关类型
```

**拆分步骤**

```typescript
// Step 1: 提取 FileList 为独立组件
// src/components/sftp/FileList.tsx

interface FileListProps {
  title: string;
  path: string;
  files: FileInfo[];
  selected: Set<string>;
  onNavigate: (path: string) => void;
  onSelect: (names: string[], multi: boolean) => void;
  onTransfer: (files: string[], direction: 'upload' | 'download') => void;
  // ... 其他 props
}

export const FileList: React.FC<FileListProps> = (props) => {
  // 从 SFTPView 提取的逻辑
};
```

```typescript
// Step 2: 提取传输逻辑为 hook
// src/components/sftp/hooks/useSFTPTransfer.ts

interface UseSFTPTransferOptions {
  sessionId: string;
  localPath: string;
  remotePath: string;
  onProgress: (progress: TransferProgress) => void;
  onComplete: () => void;
}

export function useSFTPTransfer(options: UseSFTPTransferOptions) {
  const [transfers, setTransfers] = useState<Transfer[]>([]);
  const [conflicts, setConflicts] = useState<ConflictInfo[]>([]);
  
  const startTransfer = useCallback(async (files: string[], direction: Direction) => {
    // 传输逻辑
  }, []);
  
  const resolveConflict = useCallback((resolution: ConflictResolution) => {
    // 冲突解决逻辑
  }, []);
  
  return { transfers, conflicts, startTransfer, resolveConflict };
}
```

```typescript
// Step 3: 简化后的 SFTPView
// src/components/sftp/SFTPView.tsx (~400 lines)

export const SFTPView: React.FC<{ sessionId: string }> = ({ sessionId }) => {
  // 使用提取的 hooks
  const navigation = useSFTPNavigation(sessionId);
  const transfer = useSFTPTransfer({ sessionId, ...navigation });
  const selection = useSFTPSelection();
  const dragDrop = useSFTPDragDrop({ onDrop: transfer.startTransfer });
  
  return (
    <div className="sftp-view">
      <FileList
        side="local"
        {...navigation.local}
        {...selection.local}
        {...dragDrop.local}
      />
      <FileList
        side="remote"
        {...navigation.remote}
        {...selection.remote}
        {...dragDrop.remote}
      />
      <PreviewDialog {...preview} />
      <TransferConflictDialog {...transfer.conflicts} />
    </div>
  );
};
```

**修复步骤**

1. 创建 `src/components/sftp/types.ts`，提取所有类型定义
2. 创建 `FileList.tsx`，移动文件列表相关代码
3. 创建 `hooks/useSFTPTransfer.ts`，移动传输逻辑
4. 创建 `hooks/useSFTPNavigation.ts`，移动导航逻辑
5. 创建 `PreviewDialog.tsx`，移动预览相关代码
6. 创建 `FileOperationDialogs.tsx`，移动对话框
7. 重构 `SFTPView.tsx` 为组合容器
8. 添加单元测试

**依赖关系**
- 与 M-1（传输冲突处理）一起修复效率更高
- 与 H-3（appStore 拆分）配合，可将传输状态移至独立 store

---

### H-2: TerminalView 状态管理复杂

**问题描述**

`TerminalView.tsx` 有 1345 行代码，包含：
- 30+ 个 `useRef`
- 20+ 个 `useState`
- 复杂的 WebSocket 连接管理
- 搜索功能
- AI 面板
- 粘贴保护
- IME 处理

**影响范围**
- 难以追踪状态变化
- 难以测试
- React StrictMode 双挂载处理复杂
- 性能优化困难

**问题位置**

```
src/components/terminal/TerminalView.tsx (1345 lines)
├── Lines 1-80: 导入和常量
├── Lines 81-200: 状态定义（过多）
├── Lines 201-400: WebSocket 连接管理
├── Lines 401-600: xterm 初始化
├── Lines 601-900: 事件处理
├── Lines 901-1100: 搜索功能
├── Lines 1101-1345: 渲染和清理
```

**修复方案**

**目标结构**

```
src/components/terminal/
├── TerminalView.tsx        (~500 lines) - 主组件
├── TerminalCanvas.tsx      (~200 lines) - xterm 渲染层
├── SearchBar.tsx           (现有)
├── AiInlinePanel.tsx       (现有)
├── PasteConfirmOverlay.tsx (现有)
├── hooks/
│   ├── useTerminalWebSocket.ts  - WebSocket 连接管理
│   ├── useTerminalSearch.ts     - 搜索逻辑
│   ├── useTerminalRenderer.ts   - xterm 初始化
│   └── useTerminalInput.ts      - 输入处理（IME、粘贴）
└── lib/
    └── terminalProtocol.ts      - 协议编解码
```

**提取 WebSocket 管理**

```typescript
// src/components/terminal/hooks/useTerminalWebSocket.ts

interface UseTerminalWebSocketOptions {
  sessionId: string;
  wsUrl: string | null;
  wsToken: string | null;
  onData: (data: Uint8Array) => void;
  onError: (error: string) => void;
  onStatusChange: (status: ConnectionStatus) => void;
}

interface UseTerminalWebSocketReturn {
  isConnected: boolean;
  send: (data: Uint8Array) => void;
  sendResize: (cols: number, rows: number) => void;
  reconnect: () => Promise<void>;
}

export function useTerminalWebSocket(
  options: UseTerminalWebSocketOptions
): UseTerminalWebSocketReturn {
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectingRef = useRef(false);
  const [isConnected, setIsConnected] = useState(false);
  
  // WebSocket 连接逻辑...
  
  // 返回稳定的 API
  return useMemo(() => ({
    isConnected,
    send: (data) => wsRef.current?.send(data),
    sendResize: (cols, rows) => { /* ... */ },
    reconnect: async () => { /* ... */ },
  }), [isConnected]);
}
```

**提取协议编解码**

```typescript
// src/components/terminal/lib/terminalProtocol.ts

// 协议常量
export const MSG_TYPE = {
  DATA: 0x00,
  RESIZE: 0x01,
  HEARTBEAT: 0x02,
  ERROR: 0x03,
} as const;

export const HEADER_SIZE = 5;

// 编码函数
export function encodeDataFrame(payload: Uint8Array): Uint8Array { /* ... */ }
export function encodeResizeFrame(cols: number, rows: number): Uint8Array { /* ... */ }
export function encodeHeartbeatFrame(seq: number): Uint8Array { /* ... */ }

// 解码函数
export function decodeFrame(buffer: ArrayBuffer): DecodedFrame { /* ... */ }
```

**修复步骤**

1. 创建 `lib/terminalProtocol.ts`，提取协议相关代码
2. 创建 `hooks/useTerminalWebSocket.ts`，提取 WebSocket 管理
3. 创建 `hooks/useTerminalRenderer.ts`，提取 xterm 初始化
4. 创建 `hooks/useTerminalInput.ts`，提取输入处理
5. 重构 `TerminalView.tsx` 为组合容器
6. 添加 hook 单元测试

**依赖关系**
- 与 H-4（事件监听器泄漏）一起修复
- 协议提取可独立进行

---

### H-3: appStore 状态过度集中

**问题描述**

`appStore.ts` 达 1264 行，管理所有全局状态：
- Tab 管理
- Session 管理
- Connection 配置
- 分屏状态
- Workspace 状态

**影响范围**
- 任何状态变化可能触发不必要的重渲染
- 难以进行状态逻辑的单元测试
- 与其他 store 存在循环调用

**问题位置**

```
src/store/appStore.ts (1264 lines)
├── Lines 1-100: 类型定义
├── Lines 101-300: Tab 管理
├── Lines 301-500: Session 管理
├── Lines 501-700: Connection 管理
├── Lines 701-900: 分屏逻辑
├── Lines 901-1100: Workspace 逻辑
└── Lines 1101-1264: 辅助函数
```

**修复方案**

**目标结构**

```
src/store/
├── appStore.ts           (~200 lines) - 组合入口，简单状态
├── tabStore.ts           (~300 lines) - Tab 管理
├── sessionStore.ts       (~250 lines) - Session 管理
├── connectionStore.ts    (~300 lines) - Connection 配置
├── workspaceStore.ts     (~200 lines) - Workspace 状态
└── index.ts              - 导出聚合
```

**Store 拆分示例**

```typescript
// src/store/tabStore.ts
import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';

interface TabState {
  tabs: Tab[];
  activeTabId: string | null;
}

interface TabActions {
  createTab: (type: TabType, options?: TabOptions) => string;
  closeTab: (tabId: string) => void;
  setActiveTab: (tabId: string) => void;
  updateTab: (tabId: string, updates: Partial<Tab>) => void;
}

export const useTabStore = create<TabState & TabActions>()(
  subscribeWithSelector((set, get) => ({
    tabs: [],
    activeTabId: null,
    
    createTab: (type, options) => {
      const id = crypto.randomUUID();
      set(state => ({
        tabs: [...state.tabs, { id, type, ...options }],
        activeTabId: id,
      }));
      return id;
    },
    
    closeTab: (tabId) => {
      set(state => {
        const newTabs = state.tabs.filter(t => t.id !== tabId);
        const newActiveId = state.activeTabId === tabId
          ? newTabs[newTabs.length - 1]?.id ?? null
          : state.activeTabId;
        return { tabs: newTabs, activeTabId: newActiveId };
      });
    },
    
    // ... 其他 actions
  }))
);
```

```typescript
// src/store/sessionStore.ts
import { create } from 'zustand';

interface SessionState {
  sessions: Map<string, SessionInfo>;
}

interface SessionActions {
  addSession: (session: SessionInfo) => void;
  removeSession: (sessionId: string) => void;
  updateSession: (sessionId: string, updates: Partial<SessionInfo>) => void;
  getSession: (sessionId: string) => SessionInfo | undefined;
}

export const useSessionStore = create<SessionState & SessionActions>((set, get) => ({
  sessions: new Map(),
  
  addSession: (session) => {
    set(state => {
      const newSessions = new Map(state.sessions);
      newSessions.set(session.id, session);
      return { sessions: newSessions };
    });
  },
  
  // ... 其他 actions
}));
```

```typescript
// src/store/index.ts - 组合导出
export { useTabStore } from './tabStore';
export { useSessionStore } from './sessionStore';
export { useConnectionStore } from './connectionStore';
export { useWorkspaceStore } from './workspaceStore';

// 兼容性：保留 useAppStore 作为聚合（可选）
export const useAppStore = () => {
  const tabs = useTabStore();
  const sessions = useSessionStore();
  const connections = useConnectionStore();
  return { ...tabs, ...sessions, ...connections };
};
```

**修复步骤**

1. 创建 `tabStore.ts`，迁移 Tab 相关状态和 actions
2. 创建 `sessionStore.ts`，迁移 Session 相关状态
3. 创建 `connectionStore.ts`，迁移 Connection 配置
4. 更新组件导入，使用细分的 store
5. 保留 `useAppStore` 作为兼容层（可后续移除）
6. 移除 `localTerminalStore` 中的 `useAppStore` 调用

**依赖关系**
- 应在 H-1、H-2 之前完成，为组件拆分提供更好的状态管理
- 需要更新所有使用 `useAppStore` 的组件

---

### H-4: 事件监听器内存泄漏风险 ✅ 已修复

> **修复日期**: 2026-01-29  
> **修复内容**: 
> - 创建 `src/hooks/useTauriListener.ts` 通用安全监听器 hook
> - 重构 `useConnectionEvents.ts` 使用 `mounted` 标志和 `unlisteners` 数组模式
> - 修复 `TerminalView.tsx` 中 `connection_status_changed` 监听器
> - 修复 `LocalTerminalView.tsx` 中 `data`、`closed`、`ai-insert-command` 监听器
> - 修复 `SFTPView.tsx` 中 `sftp:progress` 和 `sftp:complete` 监听器
> - 修复 `KbiDialog.tsx` 中 `ssh_kbi_prompt` 和 `ssh_kbi_result` 监听器
>
> **关键修复模式**：避免使用 `async/await` 在 useEffect 中设置监听器，改用 `.then()` 回调，并在回调中检查 `mounted` 标志

**问题描述**

`useNetworkStatus.ts` 中 Tauri 的 `listen()` 返回的 Promise 在组件快速卸载时可能导致监听器泄漏。

**影响范围**
- 长时间使用累积泄漏
- 可能导致重复事件处理
- 性能下降

**问题位置**

```typescript
// src/hooks/useNetworkStatus.ts
useEffect(() => {
  const unlistenStatus = listen('connection_status_changed', handler);
  const unlistenProgress = listen('reconnect_progress', handler);
  
  return () => {
    // 问题：Promise 可能在卸载后 resolve
    unlistenStatus.then((fn) => fn());
    unlistenProgress.then((fn) => fn());
  };
}, []);
```

**修复方案**

```typescript
// src/hooks/useNetworkStatus.ts - 修复版

export function useNetworkStatus() {
  const [status, setStatus] = useState<NetworkStatus>('online');
  
  useEffect(() => {
    let mounted = true;
    const unlisteners: Array<() => void> = [];
    
    const setupListeners = async () => {
      try {
        // 设置监听器
        const unlistenStatus = await listen('connection_status_changed', (event) => {
          if (mounted) {
            setStatus(event.payload.status);
          }
        });
        
        // 只有在组件仍挂载时才保存 unlisten 函数
        if (mounted) {
          unlisteners.push(unlistenStatus);
        } else {
          // 组件已卸载，立即清理
          unlistenStatus();
        }
        
        // 其他监听器同理...
      } catch (error) {
        console.error('Failed to setup listeners:', error);
      }
    };
    
    setupListeners();
    
    return () => {
      mounted = false;
      // 清理已注册的监听器
      unlisteners.forEach(unlisten => unlisten());
    };
  }, []);
  
  return status;
}
```

**通用模式：创建 useTauriListener hook**

```typescript
// src/hooks/useTauriListener.ts

import { useEffect, useRef } from 'react';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

export function useTauriListener<T>(
  event: string,
  handler: (payload: T) => void,
  deps: React.DependencyList = []
) {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;
  
  useEffect(() => {
    let mounted = true;
    let unlisten: UnlistenFn | null = null;
    
    listen<T>(event, (e) => {
      if (mounted) {
        handlerRef.current(e.payload);
      }
    }).then((fn) => {
      if (mounted) {
        unlisten = fn;
      } else {
        fn(); // 组件已卸载，立即清理
      }
    });
    
    return () => {
      mounted = false;
      unlisten?.();
    };
  }, [event, ...deps]);
}

// 使用示例
function MyComponent() {
  useTauriListener('connection_status_changed', (status) => {
    console.log('Status:', status);
  });
}
```

**修复步骤**

1. 创建 `useTauriListener.ts` 通用 hook
2. 重构 `useNetworkStatus.ts` 使用新 hook
3. 审查所有使用 `listen()` 的地方，统一使用新模式
4. 在 `TerminalView.tsx` 中应用相同修复

**依赖关系**
- 与 H-2（TerminalView 重构）一起修复效率更高

---

### H-5: Rust 连接池死锁风险 ✅ 已修复

> **修复日期**: 2026-01-29  
> **审查结论**: 经过详细代码审查，该文件整体设计良好，**未发现严重死锁风险**
> **改进内容**:
> - 为 `ConnectionEntry` 结构体添加锁获取顺序文档
> - 优化 `replace_handle_controller` 方法，先收集数据再释放 DashMap 引用
>
> **良好实践已确认**:
> - 使用 `DashMap` 而非 `HashMap<_, Mutex>` 管理连接
> - 大部分方法只获取单个锁
> - 使用 `AtomicU32`/`AtomicU64`/`AtomicBool` 处理简单计数器
> - 多处显式 `drop()` 释放锁
> - 使用 `try_read()` 避免潜在死锁

**问题描述**

`ConnectionEntry` 中使用多个 `RwLock` 和 `Mutex`，在特定调用顺序下可能导致死锁：

```rust
pub struct ConnectionEntry {
    state: RwLock<ConnectionState>,
    keep_alive: RwLock<bool>,
    idle_timer: Mutex<Option<JoinHandle<()>>>,
    terminal_ids: RwLock<Vec<String>>,
    sftp_initialized: RwLock<bool>,
    // ...
}
```

**影响范围**
- 高并发操作时应用可能卡死
- 难以复现和调试
- 影响用户体验

**问题位置**

```
src-tauri/src/state/pool.rs
├── ConnectionEntry 结构体定义
└── 多处同时获取多个锁的代码
```

**修复方案**

**方案 A：定义锁获取顺序（简单）**

```rust
// 在文档中定义并强制执行锁获取顺序
// 顺序: state -> keep_alive -> terminal_ids -> sftp_initialized -> idle_timer

impl ConnectionEntry {
    /// 安全地更新连接状态
    /// 锁获取顺序: state -> keep_alive
    pub async fn update_state(&self, new_state: ConnectionState) {
        let mut state = self.state.write().await;
        let mut keep_alive = self.keep_alive.write().await;
        
        *state = new_state;
        if matches!(new_state, ConnectionState::Disconnected) {
            *keep_alive = false;
        }
    }
}
```

**方案 B：使用单一锁保护整个状态（更安全）**

```rust
// 将所有可变状态合并到一个结构体中
#[derive(Debug)]
struct ConnectionInner {
    state: ConnectionState,
    keep_alive: bool,
    terminal_ids: Vec<String>,
    sftp_initialized: bool,
    idle_timer: Option<JoinHandle<()>>,
}

pub struct ConnectionEntry {
    inner: RwLock<ConnectionInner>,
    // 不可变字段不需要锁
    id: String,
    config: ConnectionConfig,
}

impl ConnectionEntry {
    pub async fn with_state<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&ConnectionInner) -> R,
    {
        let inner = self.inner.read().await;
        f(&inner)
    }
    
    pub async fn with_state_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut ConnectionInner) -> R,
    {
        let mut inner = self.inner.write().await;
        f(&mut inner)
    }
}
```

**方案 C：使用 parking_lot 的超时锁（推荐，与 C-1 配合）**

```rust
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;

impl ConnectionEntry {
    pub fn try_read_state(&self, timeout: Duration) -> Option<RwLockReadGuard<ConnectionState>> {
        self.state.try_read_for(timeout)
    }
    
    pub fn try_write_state(&self, timeout: Duration) -> Option<RwLockWriteGuard<ConnectionState>> {
        self.state.try_write_for(timeout)
    }
}

// 使用示例
if let Some(mut state) = entry.try_write_state(Duration::from_secs(5)) {
    *state = ConnectionState::Connected;
} else {
    log::error!("Potential deadlock detected when acquiring state lock");
    // 处理超时情况
}
```

**推荐方案**：方案 C（parking_lot 超时锁）+ 方案 A（定义顺序）

**修复步骤**

1. 在 C-1 修复中统一使用 parking_lot
2. 为 `ConnectionEntry` 定义锁获取顺序文档
3. 实现 `try_*_for` 超时方法
4. 添加死锁检测日志
5. 考虑长期重构为方案 B

**依赖关系**
- 依赖 C-1（parking_lot 引入）

---

## 3. 中等问题 (Medium) 🟡

### M-1: 传输冲突处理逻辑重复

**问题描述**

传输冲突检测和解决逻辑在 `SFTPView.tsx` 中多处重复。

**问题位置**

```
src/components/sftp/SFTPView.tsx
├── Lines ~1050-1120: 上传冲突检测
├── Lines ~1150-1220: 下载冲突检测
└── Lines ~1250-1320: 冲突解决逻辑
```

**修复方案**

在 H-1 重构中提取 `useTransferConflictResolver` hook。

**依赖关系**：与 H-1 一起修复

---

### M-2: 缺少请求取消机制

**问题描述**

API 请求没有实现取消机制，长时间操作无法中断。

**问题位置**

```
src/lib/api.ts - 所有 SFTP 操作
```

**修复方案**

```typescript
// 使用 Tauri 的 cancel 功能
import { invoke, InvokeArgs } from '@tauri-apps/api/core';

class CancellableRequest<T> {
  private aborted = false;
  
  async invoke(cmd: string, args: InvokeArgs): Promise<T> {
    if (this.aborted) {
      throw new Error('Request cancelled');
    }
    return invoke(cmd, args);
  }
  
  cancel() {
    this.aborted = true;
  }
}

// 使用示例
const request = new CancellableRequest<FileInfo[]>();
const files = await request.invoke('sftp_list_dir', { sessionId, path });

// 取消
request.cancel();
```

**依赖关系**：无

---

### M-3: 事件监听器清理不完整

**问题描述**

部分组件的事件监听器在清理函数中有遗漏。

**问题位置**

```
src/components/terminal/TerminalView.tsx
├── ResizeObserver 清理
└── window.resize 监听器

src/components/sftp/SFTPView.tsx
└── Tauri 事件监听器
```

**修复方案**

在 H-2 和 H-4 修复中统一处理。

**依赖关系**：与 H-2、H-4 一起修复

---

### M-4: 硬编码的超时和重试值

**问题描述**

网络相关的超时和重试次数硬编码在代码中。

**问题位置**

```
src/components/terminal/TerminalView.tsx
├── 心跳间隔: 30000ms
└── 重连延迟: 1000-5000ms

src-tauri/src/bridge/server.rs
├── 心跳超时: 60s
└── 连接超时: 30s
```

**修复方案**

```typescript
// src/lib/config.ts
export const NETWORK_CONFIG = {
  heartbeat: {
    interval: 30000,
    timeout: 60000,
  },
  reconnect: {
    initialDelay: 1000,
    maxDelay: 5000,
    maxAttempts: 10,
  },
  connection: {
    timeout: 30000,
  },
};

// 允许用户在设置中覆盖
```

**依赖关系**：无

---

### M-5: 前端缺少错误边界

**问题描述**

大部分组件没有错误边界保护。

**问题位置**

```
src/components/ErrorBoundary.tsx - 存在但未广泛使用
```

**修复方案**

```tsx
// 在关键组件外包裹 ErrorBoundary
<ErrorBoundary fallback={<TerminalErrorFallback />}>
  <TerminalView sessionId={sessionId} />
</ErrorBoundary>

<ErrorBoundary fallback={<SFTPErrorFallback />}>
  <SFTPView sessionId={sessionId} />
</ErrorBoundary>
```

**依赖关系**：无

---

## 4. 建议改进 (Low) 💚

### L-1: TypeScript 类型安全改进

使用类型守卫替代 `as` 断言。

### L-2: 添加单元测试

为 stores 和 hooks 添加测试覆盖。

### L-3: i18n 键类型安全

生成 i18n 键的 TypeScript 类型定义。

### L-4: 日志级别优化

使用条件编译控制生产环境日志。

### L-5: 废弃 API 清理

移除标记为 `@deprecated` 的 API。

---

## 修复计划

### Phase 1：关键问题（1 周）

```
Week 1:
├── Day 1-2: C-1 (parking_lot 替换 unwrap)
├── Day 3-4: C-2 (expect 替换)
├── Day 5: C-3 (Token 有效期)
└── Day 6-7: 测试和验证
```

### Phase 2：重要问题（2 周）

```
Week 2:
├── Day 1-2: H-3 (appStore 拆分)
├── Day 3-4: H-4 (事件监听器修复)
└── Day 5-7: H-5 (连接池死锁修复)

Week 3:
├── Day 1-3: H-1 (SFTPView 拆分)
└── Day 4-7: H-2 (TerminalView 重构)
```

### Phase 3：中等问题（2 周）

```
Week 4-5:
├── M-1: 与 H-1 一起完成
├── M-2: 请求取消机制
├── M-3: 与 H-2、H-4 一起完成
├── M-4: 配置提取
└── M-5: 错误边界
```

### Phase 4：低优先级改进（持续）

按需处理 L-1 到 L-5。

---

## 依赖关系图

```
C-1 (parking_lot) ──┬──> H-5 (死锁修复)
                    │
C-2 (expect) ───────┤
                    │
H-3 (appStore) ─────┼──> H-1 (SFTPView)
                    │         │
H-4 (监听器) ───────┼──> H-2 (TerminalView)
                    │         │
                    │    M-1 (冲突逻辑)
                    │    M-3 (监听器清理)
                    │
C-3 (Token) ────────┘ (独立)

M-2, M-4, M-5: 独立，可随时修复
L-1 ~ L-5: 独立，持续改进
```

---

## 附录

### A. 代码规范建议

1. **Rust 错误处理**
   - 使用 `thiserror` 定义错误类型
   - 避免 `unwrap()`，使用 `?` 运算符
   - 关键路径提供友好错误信息

2. **React 组件**
   - 单文件不超过 500 行
   - 复杂逻辑提取为 hooks
   - 使用 ErrorBoundary 保护

3. **状态管理**
   - 按领域拆分 store
   - 使用 `subscribeWithSelector` 优化
   - 避免跨 store 循环调用

### B. 测试覆盖目标

| 模块 | 目标覆盖率 |
|------|-----------|
| Stores | 80% |
| Hooks | 70% |
| Utils | 90% |
| Components | 50% |

### C. 监控建议

1. 添加性能监控（React Profiler）
2. 添加错误追踪（Sentry 或类似服务）
3. 添加锁竞争监控日志
