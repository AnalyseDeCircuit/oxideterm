# OxideTerm 连接架构重构施工计划

> 版本: v1.0  
> 日期: 2026-02-04  
> 状态: 待执行

---

## 目录

1. [背景与问题分析](#一背景与问题分析)
2. [核心设计变更](#二核心设计变更)
3. [深度调查结果](#三深度调查结果)
4. [重构任务清单](#四重构任务清单)
5. [并发锁机制设计](#五并发锁机制设计)
6. [强化清理逻辑](#六强化清理逻辑)
7. [UI 适配性自检](#七ui-适配性自检)
8. [实施顺序与阶段](#八实施顺序与阶段)
9. [风险评估](#九风险评估)
10. [验收标准](#十验收标准)

---

## 一、背景与问题分析

### 1.1 现有架构问题

现有"后端递归建链"模式导致了以下严重问题：

| 问题 | 症状 | 根因 |
|------|------|------|
| **僵尸终端** | 重连后侧边栏出现多个无效终端项 | `closeTab` 不清理 `sessions` Map 和后端资源 |
| **AcceptTimeout** | 60 秒超时，前端无法连接 WebSocket | 前端请求过快，后端端口尚未绑定 |
| **状态昏厥** | UI 状态与后端物理连接不一致 | `terminalIds` 增量合并，残留旧值 |
| **竞态冲突** | 重复触发连接导致多个 Promise 链竞争 | 缺乏并发锁机制 |

### 1.2 问题复现路径

```
用户关闭 Tab
    ↓
appStore.closeTab()         ← 仅移除 tab 对象，不清理 session
    ↓
TerminalView 组件卸载
    ↓
WebSocket 断开 → "ClientClosed"
    ↓
后端不知道该清理什么（前端没调 closeTerminal）
    ↓
重新连接时，后端 terminal_ids 仍包含旧 ID
    ↓
前端 nodeTerminalMap 增量合并，出现重影
```

---

## 二、核心设计变更

### 2.1 后端：从"决策者"降级为"执行者"

- ❌ 废弃后端自动递归处理 ProxyChain 的逻辑
- ✅ `connect_tree_node` 指令仅负责**单一级别**的连接
- ✅ 后端在连接断开或 `ref_count` 归零时，**严格物理清理资源**
- ✅ 向前端广播准确的状态变更事件

### 2.2 前端：从"监听者"升级为"指挥官"

- ✅ 由 `sessionTreeStore` 负责预设链的遍历
- ✅ 通过 `async/await` 确保连接动作是**线性的、可控的**
- ✅ 引入**并发锁机制**，防止重复触发
- ✅ 连接前执行**焦土式清理**

---

## 三、深度调查结果

### 3.1 链路识别：后端递归建链的位置

**后端入口**: `src-tauri/src/commands/session_tree.rs#L505` - `connect_tree_node`

```
调用链：
connect_tree_node
├─> [单级连接] establish_tunneled_connection (有父节点时)
└─> [单级连接] registry.connect() (根节点时)
```

**关键发现**：`connect_tree_node` 本身已经是**单级连接**，不会递归。

**真正的递归在**: `session_tree.rs#L700` - `connect_manual_preset`
- 后端在此命令中循环遍历 `path_node_ids` 并逐个连接

**后端自动重连**: `connection_registry.rs#L1777` - `start_reconnect`
- 重连成功后调用 `cascade_reconnect_children` **自动级联重连子连接**

### 3.2 状态清理点：terminalIds 增量合并的位置

| 文件 | 行号 | 问题描述 |
|------|------|---------|
| `sessionTreeStore.ts` | L730 | `[...existing, terminalId]` 增量追加，不清理旧值 |
| `sessionTreeStore.ts` | L1384 | `rebuildUnifiedNodes` 从两个来源合并，不验证有效性 |
| `sessionTreeStore.ts` | L838-860 | `addKbiSession` 同样增量追加 |

### 3.3 生命周期钩子：closeTab 的断裂点

**当前 `closeTab` 实现** (`appStore.ts#L870`):

```typescript
closeTab: (tabId) => {
  set((state) => {
    const newTabs = state.tabs.filter(t => t.id !== tabId);
    // 仅更新 tabs 和 activeTabId
    return { tabs: newTabs, activeTabId: newActiveId };
  });
}
```

**断裂点清单**:

| 缺失的清理 | 后果 |
|-----------|------|
| ❌ 不清理 `sessions` Map | 僵尸 session 残留 |
| ❌ 不调用 `api.closeTerminal()` | 后端 terminal 不关闭 |
| ❌ 不清理 `sessionTreeStore.nodeTerminalMap` | 映射关系残留 |
| ❌ 不调用 `sshDisconnect()` | SSH 连接不断开 |

---

## 四、重构任务清单

### 4.1 前端任务

| ID | 任务 | 优先级 | 状态 |
|----|------|--------|------|
| F1 | 重写 `closeTab`，加入物理清理逻辑 | P0 | ✅ 已完成 |
| F2 | 新增 `resetNodeState`，全量覆盖状态 | P0 | ✅ 已完成 |
| F3 | 新增 `connectNodeWithAncestors` 线性连接器 | P0 | 待执行 |
| F4 | 新增 `isConnecting` 并发锁机制 | P0 | ✅ 已完成 |
| F5 | 修改 `rebuildUnifiedNodes` 验证 terminalIds 有效性 | P1 | 待执行 |
| F6 | 重写 `reconnectCascade` 使用线性连接器 | P1 | 待执行 |

### 4.2 后端任务

| ID | 任务 | 优先级 | 状态 |
|----|------|--------|------|
| B1 | `expand_manual_preset` 返回 `pathNodeIds`，移除循环连接 | P1 | ✅ 已完成 |
| B2 | 移除 `cascade_reconnect_children` 自动级联重连 | P1 | ✅ 已完成 |
| B3 | 新增 `destroy_node_sessions` 命令，物理销毁节点残余资源 | P0 | ✅ 已完成 |
| B4 | 心跳重连只广播事件，级联重连由前端决定 | P1 | ✅ 已完成 |

---

## 五、并发锁机制设计

### 5.1 状态定义

在 `sessionTreeStore` 中新增：

```typescript
interface SessionTreeStore {
  // ... existing fields
  
  /** 正在连接的节点 ID 集合（并发锁） */
  connectingNodeIds: Set<string>;
  
  /** 全局连接锁（防止多条链同时执行） */
  isConnectingChain: boolean;
}
```

### 5.2 锁的获取与释放

```typescript
/**
 * 尝试获取节点连接锁
 * @returns true 如果成功获取锁，false 如果节点已在连接中
 */
acquireConnectLock(nodeId: string): boolean {
  const { connectingNodeIds } = get();
  if (connectingNodeIds.has(nodeId)) {
    console.warn(`[Lock] Node ${nodeId} is already connecting, rejecting duplicate request`);
    return false;
  }
  
  set({ connectingNodeIds: new Set([...connectingNodeIds, nodeId]) });
  return true;
}

/**
 * 释放节点连接锁
 */
releaseConnectLock(nodeId: string): void {
  const { connectingNodeIds } = get();
  const newSet = new Set(connectingNodeIds);
  newSet.delete(nodeId);
  set({ connectingNodeIds: newSet });
}

/**
 * 尝试获取链式连接锁（全局唯一）
 */
acquireChainLock(): boolean {
  if (get().isConnectingChain) {
    console.warn('[Lock] A chain connection is already in progress');
    return false;
  }
  set({ isConnectingChain: true });
  return true;
}

releaseChainLock(): void {
  set({ isConnectingChain: false });
}
```

### 5.3 在 `connectNodeWithAncestors` 中使用锁

```typescript
async connectNodeWithAncestors(nodeId: string): Promise<void> {
  // 1. 获取链式锁
  if (!this.acquireChainLock()) {
    throw new Error('Another chain connection is in progress');
  }
  
  try {
    const path = await this.getNodePath(nodeId);
    
    // 2. 为路径上所有节点获取锁
    for (const node of path) {
      if (!this.acquireConnectLock(node.id)) {
        throw new Error(`Node ${node.id} is already connecting`);
      }
    }
    
    // 3. 连接前清理（焦土策略）
    for (const node of path) {
      await this.resetNodeState(node.id);
    }
    
    // 4. 线性连接
    for (const node of path) {
      // ... 连接逻辑
    }
  } finally {
    // 5. 释放所有锁
    const path = await this.getNodePath(nodeId);
    for (const node of path) {
      this.releaseConnectLock(node.id);
    }
    this.releaseChainLock();
  }
}
```

### 5.4 UI 锁定行为

当 `isConnectingChain === true` 或 `connectingNodeIds.has(nodeId)` 时：

- 侧边栏"连接"按钮**禁用**
- 显示连接中**遮罩或 spinner**
- 禁止关闭相关 Tab
- 禁止触发 DrillDown

---

## 六、强化清理逻辑

### 6.1 `resetNodeState` 完整实现

```typescript
/**
 * 重置节点状态（焦土式清理）
 * 
 * 执行顺序：
 * 1. 调用后端销毁残余资源
 * 2. 清理本地映射
 * 3. 重置节点状态为 pending
 */
async resetNodeState(nodeId: string): Promise<void> {
  const node = get().getRawNode(nodeId);
  if (!node) return;
  
  // ========== Phase 1: 后端物理销毁 ==========
  
  // 1a. 销毁该节点的所有终端
  const terminalIds = get().nodeTerminalMap.get(nodeId) || [];
  for (const terminalId of terminalIds) {
    try {
      await api.closeTerminal(terminalId);
    } catch (e) {
      console.warn(`Failed to close terminal ${terminalId}:`, e);
    }
  }
  
  // 1b. 如果有 SSH 连接，尝试断开（仅当无其他终端引用时）
  if (node.sshConnectionId) {
    try {
      // 调用新的 destroy_node_sessions 接口，让后端判断是否需要断开 SSH
      await api.destroyNodeSessions(nodeId);
    } catch (e) {
      console.warn(`Failed to destroy node sessions for ${nodeId}:`, e);
    }
  }
  
  // 1c. 等待短暂时间确保后端资源释放
  await new Promise(resolve => setTimeout(resolve, 100));
  
  // ========== Phase 2: 本地状态清理 ==========
  
  const { nodeTerminalMap, terminalNodeMap } = get();
  const newTerminalMap = new Map(nodeTerminalMap);
  const newNodeMap = new Map(terminalNodeMap);
  
  // 清理该节点的所有终端映射
  const oldTerminals = newTerminalMap.get(nodeId) || [];
  newTerminalMap.delete(nodeId);
  for (const tid of oldTerminals) {
    newNodeMap.delete(tid);
  }
  
  set({ 
    nodeTerminalMap: newTerminalMap, 
    terminalNodeMap: newNodeMap 
  });
  
  // ========== Phase 3: 重置节点状态 ==========
  
  set((state) => ({
    rawNodes: state.rawNodes.map(n => 
      n.id === nodeId 
        ? { 
            ...n, 
            state: { status: 'pending' as const },
            sshConnectionId: undefined,
            terminalSessionId: undefined,
            sftpSessionId: undefined,
          }
        : n
    )
  }));
  
  // 清除 link-down 标记
  const { linkDownNodeIds } = get();
  if (linkDownNodeIds.has(nodeId)) {
    const newLinkDownIds = new Set(linkDownNodeIds);
    newLinkDownIds.delete(nodeId);
    set({ linkDownNodeIds: newLinkDownIds });
  }
  
  get().rebuildUnifiedNodes();
}
```

### 6.2 后端 `destroy_node_sessions` 命令

```rust
/// 销毁节点关联的所有会话资源
/// 
/// 此命令用于前端"焦土式清理"，确保后端资源完全释放：
/// - 关闭所有关联的终端
/// - 关闭 SFTP 会话
/// - 清理 WebSocket bridges
/// - 如果 ref_count 归零，断开 SSH 连接
#[tauri::command]
pub async fn destroy_node_sessions(
    state: State<'_, Arc<SessionTreeState>>,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
    session_registry: State<'_, Arc<SessionRegistry>>,
    bridge_manager: State<'_, BridgeManager>,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
    node_id: String,
) -> Result<DestroyNodeSessionsResponse, String> {
    let mut destroyed_terminals = Vec::new();
    let mut ssh_disconnected = false;
    
    // 1. 获取节点信息
    let (ssh_connection_id, terminal_session_id, sftp_session_id) = {
        let tree = state.tree.read().await;
        let node = tree.get_node(&node_id)
            .ok_or_else(|| format!("Node not found: {}", node_id))?;
        (
            node.ssh_connection_id.clone(),
            node.terminal_session_id.clone(),
            node.sftp_session_id.clone(),
        )
    };
    
    // 2. 关闭终端
    if let Some(terminal_id) = terminal_session_id {
        bridge_manager.unregister(&terminal_id);
        session_registry.remove(&terminal_id);
        destroyed_terminals.push(terminal_id);
    }
    
    // 3. 关闭 SFTP
    if let Some(sftp_id) = sftp_session_id {
        sftp_registry.remove(&sftp_id);
    }
    
    // 4. 检查 SSH 连接是否需要断开
    if let Some(ssh_id) = ssh_connection_id {
        // 从连接中移除该终端
        if let Some(terminal_id) = &destroyed_terminals.first() {
            let _ = connection_registry.remove_terminal(&ssh_id, terminal_id).await;
        }
        
        // 检查剩余引用
        if let Some(info) = connection_registry.get_info(&ssh_id).await {
            if info.terminal_ids.is_empty() && info.sftp_session_id.is_none() {
                // 无剩余引用，断开 SSH
                let _ = connection_registry.disconnect(&ssh_id).await;
                ssh_disconnected = true;
            }
        }
    }
    
    // 5. 清理节点元数据
    {
        let mut tree = state.tree.write().await;
        if let Some(node) = tree.get_node_mut(&node_id) {
            node.terminal_session_id = None;
            node.sftp_session_id = None;
            if ssh_disconnected {
                node.ssh_connection_id = None;
                node.state = NodeState::Pending;
            }
        }
    }
    
    Ok(DestroyNodeSessionsResponse {
        destroyed_terminals,
        ssh_disconnected,
    })
}
```

---

## 七、UI 适配性自检

### 7.1 需要联动的组件清单

| 组件 | 文件位置 | 需要的改动 |
|------|---------|-----------|
| **SessionTreeNode** | `src/components/sessions/SessionTreeNode.tsx` | 读取 `connectingNodeIds` 显示 spinner |
| **Sidebar** | `src/components/layout/Sidebar.tsx` | 禁用"连接"按钮当 `isConnectingChain` |
| **ConnectionStatus** | `src/components/connections/ConnectionStatus.tsx` | 新增"连接中"状态显示 |
| **TerminalView** | `src/components/terminal/TerminalView.tsx` | 连接中显示遮罩 |
| **TabBar** | `src/components/layout/TabBar.tsx` | 连接中禁止关闭 Tab |
| **DrillDownDialog** | `src/components/modals/DrillDownDialog.tsx` | 连接中禁止触发 |
| **QuickConnect** | `src/components/modals/QuickConnectModal.tsx` | 连接中禁止新建连接 |

### 7.2 状态图标映射

```typescript
// 节点状态 → 图标/颜色映射
const STATUS_ICONS = {
  'idle':        { icon: '○', color: 'gray-400',   tooltip: '未连接' },
  'connecting':  { icon: '◐', color: 'yellow-500', tooltip: '连接中...', spin: true },
  'connected':   { icon: '●', color: 'green-500',  tooltip: '已连接' },
  'active':      { icon: '●', color: 'green-400',  tooltip: '活跃中' },
  'link-down':   { icon: '◉', color: 'orange-500', tooltip: '链路断开' },
  'error':       { icon: '✕', color: 'red-500',    tooltip: '连接失败' },
  'locked':      { icon: '🔒', color: 'blue-500',   tooltip: '操作锁定中' }, // 新增
};
```

### 7.3 连接中遮罩设计

```tsx
// src/components/ui/ConnectingOverlay.tsx
interface ConnectingOverlayProps {
  nodeId: string;
  message?: string;
}

export function ConnectingOverlay({ nodeId, message }: ConnectingOverlayProps) {
  const isConnecting = useSessionTreeStore(
    state => state.connectingNodeIds.has(nodeId)
  );
  
  if (!isConnecting) return null;
  
  return (
    <div className="absolute inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="flex flex-col items-center gap-2">
        <Spinner size="lg" />
        <span className="text-white text-sm">
          {message || '正在建立连接...'}
        </span>
      </div>
    </div>
  );
}
```

### 7.4 错误信息展示位置

| 错误类型 | 展示位置 | 展示方式 |
|---------|---------|---------|
| 连接失败 | 节点 tooltip + Toast | 红色图标 + 顶部 Toast |
| 链式连接中断 | Toast + 侧边栏详情 | 显示失败节点位置 |
| 锁冲突 | Toast | "另一个连接操作正在进行中" |
| 后端资源销毁失败 | Console + 静默重试 | 不阻塞用户操作 |

### 7.5 按钮禁用逻辑

```tsx
// Sidebar.tsx 中的连接按钮
const handleConnect = () => {
  const { isConnectingChain, connectingNodeIds } = useSessionTreeStore.getState();
  
  // 禁用条件
  if (isConnectingChain) {
    toast.warning('另一个连接操作正在进行中');
    return;
  }
  
  if (connectingNodeIds.has(selectedNodeId)) {
    toast.warning('该节点正在连接中');
    return;
  }
  
  // 执行连接
  sessionTreeStore.connectNodeWithAncestors(selectedNodeId);
};

// 按钮渲染
<Button 
  onClick={handleConnect}
  disabled={isConnectingChain || connectingNodeIds.has(selectedNodeId)}
>
  {connectingNodeIds.has(selectedNodeId) ? '连接中...' : '连接'}
</Button>
```

---

## 八、实施顺序与阶段

### Phase 1: 前端防御性修复（低风险，立竿见影）

```
预计耗时: 2-3 小时

├── 1.1 新增并发锁状态和方法
│   ├── connectingNodeIds: Set<string>
│   ├── isConnectingChain: boolean
│   ├── acquireConnectLock / releaseConnectLock
│   └── acquireChainLock / releaseChainLock
│
├── 1.2 重写 closeTab（加入物理清理）
│   ├── 清理 sessions Map
│   ├── 调用 api.closeTerminal()
│   ├── 清理 sessionTreeStore.nodeTerminalMap
│   └── 条件性调用 sshDisconnect()
│
├── 1.3 新增 resetNodeState（焦土清理）
│   ├── 调用后端 destroyNodeSessions
│   ├── 清理本地映射
│   └── 重置节点状态
│
└── 1.4 修改 connectNode 使用锁
    ├── 连接前获取锁
    ├── 连接完成/失败释放锁
    └── 重复请求直接拒绝
```

### Phase 2: 后端降级（中风险，解耦核心）

```
预计耗时: 3-4 小时

├── 2.1 新增 destroy_node_sessions 命令
│   ├── 关闭终端和 SFTP
│   ├── 清理 WebSocket bridges
│   └── 条件断开 SSH
│
├── 2.2 connect_manual_preset → expand_manual_preset
│   ├── 移除循环连接逻辑
│   └── 只保留树节点展开
│
├── 2.3 移除 cascade_reconnect_children
│   └── 重连成功只广播事件
│
└── 2.4 心跳重连行为修改
    └── 只广播 link_down，不自动重连
```

### Phase 3: 前端升级（高收益，完成闭环）

```
预计耗时: 3-4 小时

├── 3.1 实现 connectNodeWithAncestors 线性连接器
│   ├── 获取祖先路径
│   ├── 批量获取锁
│   ├── 批量 resetNodeState
│   ├── 线性 await 连接
│   └── finally 释放所有锁
│
├── 3.2 重写 reconnectCascade
│   └── 使用线性连接器
│
├── 3.3 UI 组件适配
│   ├── SessionTreeNode 显示 spinner
│   ├── Sidebar 禁用按钮
│   ├── ConnectingOverlay 遮罩
│   └── TabBar 禁止关闭
│
└── 3.4 适配新的 expand_manual_preset API
    └── 前端负责遍历调用 connect_tree_node
```

---

## 九、风险评估

| 影响功能 | 风险程度 | 应对措施 |
|---------|---------|---------|
| 自动重连 | 🟡 中 | 后端仍检测 link-down 并广播，前端监听后决定是否 `reconnectCascade` |
| DrillDown | 🟢 低 | `connect_tree_node` 单级连接不受影响 |
| 手工跳板链 | 🟠 较高 | `connect_manual_preset` 需改名，前端适配 |
| SFTP | 🟢 低 | 清理机制修复后更准确 |
| 端口转发 | 🟢 低 | 与终端类似 |
| 并发锁死锁 | 🟡 中 | 使用 try-finally 确保释放，加入超时机制 |

### 兼容性策略

1. **API 兼容**：保留旧命令名，内部重定向到新实现
2. **灰度发布**：先修复 `closeTab`，观察僵尸终端是否减少
3. **回滚计划**：保留旧逻辑开关，可通过 feature flag 切换

---

## 十、验收标准

### 10.1 僵尸终端测试

```
步骤:
1. 连接服务器 A
2. 打开 2 个终端 Tab
3. 关闭所有 Tab
4. 重新连接服务器 A

期望结果:
- 侧边栏只显示新终端，无重影
- 后端 terminal_ids 为空后才断开 SSH
- 连接池监控面板 total_terminals = 1
```

### 10.2 AcceptTimeout 测试

```
步骤:
1. 关闭 Tab
2. 2 秒内点击重连

期望结果:
- 无 60 秒超时
- WebSocket 正常建立
- 无 AcceptTimeout 错误
```

### 10.3 并发锁测试

```
步骤:
1. 快速双击"连接"按钮
2. 或：在 A 节点连接中时，尝试连接 B 节点的子节点

期望结果:
- 第二次点击被拒绝
- Toast 提示"操作进行中"
- 无重复 Promise 链
```

### 10.4 链式连接熔断测试

```
步骤:
1. 设置跳板链 A → B → C → D
2. 模拟 B 节点连接失败

期望结果:
- A 保持已连接状态
- B、C、D 显示失败状态
- 错误信息明确指出 B 是失败点
```

### 10.5 状态一致性测试

```
步骤:
1. 使用连接池监控面板
2. 执行各种连接/断开操作

期望结果:
- total_terminals 实时准确
- ref_count 与终端数一致
- 无孤儿 SSH 连接
```

---

## 附录

### A. 文件改动清单

| 文件 | 改动类型 | 描述 |
|------|---------|------|
| `src/store/sessionTreeStore.ts` | 修改 | 新增锁、resetNodeState、connectNodeWithAncestors |
| `src/store/appStore.ts` | 修改 | 重写 closeTab |
| `src/lib/api.ts` | 新增 | destroyNodeSessions 接口 |
| `src/components/sessions/SessionTreeNode.tsx` | 修改 | 显示连接中状态 |
| `src/components/layout/Sidebar.tsx` | 修改 | 按钮禁用逻辑 |
| `src/components/ui/ConnectingOverlay.tsx` | 新增 | 连接遮罩组件 |
| `src-tauri/src/commands/session_tree.rs` | 修改 | 新增 destroy_node_sessions，重构 connect_manual_preset |
| `src-tauri/src/ssh/connection_registry.rs` | 修改 | 移除 cascade_reconnect_children |

### B. 新增 API 接口

```typescript
// api.ts
interface Api {
  // 新增
  destroyNodeSessions(nodeId: string): Promise<DestroyNodeSessionsResponse>;
  expandManualPreset(request: ExpandManualPresetRequest): Promise<ExpandManualPresetResponse>;
}

interface DestroyNodeSessionsResponse {
  destroyedTerminals: string[];
  sshDisconnected: boolean;
}

interface ExpandManualPresetResponse {
  targetNodeId: string;
  pathNodeIds: string[];
}
```

---

*文档结束 - 准备执行 Phase 1*
