# 资源监控器 (Resource Profiler)

> 实时采样远程主机的 CPU、内存、负载和网络指标，通过持久化 SSH Shell 通道实现低开销监控。

## 🎯 核心特性

| 特性 | 说明 |
|------|------|
| **持久化通道** | 整个生命周期仅打开 **1 个** Shell Channel，避免 MaxSessions 耗尽 |
| **轻量采样** | 精简命令输出 ~500-1.5KB（仅读取 `/proc` 中的关键行） |
| **自动生命周期** | SSH 断连 → 自动停止；重连 → 可重新启动 |
| **优雅降级** | 非 Linux 主机或连续失败后自动降级为 RTT-Only 模式 |
| **Delta 计算** | CPU% 和网络速率基于两次采样的差值，首次采样返回 `None` |

---

## 🏗️ 架构概览

```
┌──────────────────────────────────────────────────────────────┐
│                        Frontend                              │
│                                                              │
│  profilerStore ◄──── Tauri Event ◄──── "profiler:update:{id}"│
│  (Zustand)           (JSON payload)                          │
│       │                                                      │
│       ├─ metrics: ResourceMetrics | null                     │
│       ├─ history: ResourceMetrics[] (max 60)                 │
│       └─ isRunning / isEnabled / error                       │
│                                                              │
│  api.startResourceProfiler(connId)  ──► Tauri IPC ──►       │
│  api.stopResourceProfiler(connId)   ──► Tauri IPC ──►       │
│  api.getResourceMetrics(connId)     ──► Tauri IPC ──►       │
│  api.getResourceHistory(connId)     ──► Tauri IPC ──►       │
├──────────────────────────────────────────────────────────────┤
│                        Backend (Rust)                         │
│                                                              │
│  ProfilerRegistry (DashMap<String, ResourceProfiler>)        │
│       │                                                      │
│       └─ ResourceProfiler::spawn(connId, controller, app)    │
│              │                                               │
│              ├─ open_shell_channel()    → 1 persistent shell │
│              ├─ sampling_loop()        → 每 10s 一次采样     │
│              │     ├─ shell_sample()   → 写入命令 + 读取输出 │
│              │     ├─ parse_metrics()  → 解析 /proc 数据     │
│              │     └─ emit_metrics()   → AppHandle.emit()    │
│              │                                               │
│              └─ stop 信号: oneshot / disconnect_rx / 手动     │
└──────────────────────────────────────────────────────────────┘
```

---

## 📊 采集指标

### ResourceMetrics 数据结构

```typescript
interface ResourceMetrics {
  timestampMs: number;         // 采样时间戳 (ms since epoch)
  cpuPercent: number | null;   // CPU 使用率 (0-100)，首次无数据
  memoryUsed: number | null;   // 已用内存 (bytes)
  memoryTotal: number | null;  // 总内存 (bytes)
  memoryPercent: number | null;// 内存使用率 (0-100)
  loadAvg1: number | null;     // 1 分钟负载
  loadAvg5: number | null;     // 5 分钟负载
  loadAvg15: number | null;    // 15 分钟负载
  cpuCores: number | null;     // CPU 核心数
  netRxBytesPerSec: number | null; // 网络接收速率 (bytes/s)
  netTxBytesPerSec: number | null; // 网络发送速率 (bytes/s)
  sshRttMs: number | null;     // SSH RTT (ms)
  source: MetricsSource;       // 数据质量标识
}

type MetricsSource = 'full' | 'partial' | 'rtt_only' | 'failed';
```

### 数据源对照

| 指标 | 数据源 | 命令 |
|------|--------|------|
| CPU% | `/proc/stat` 首行 | `head -1 /proc/stat` |
| 内存 | `/proc/meminfo` 两行 | `grep -E '^(MemTotal\|MemAvailable):' /proc/meminfo` |
| 负载 | `/proc/loadavg` | `cat /proc/loadavg` |
| 网络 | `/proc/net/dev` 全文 | `cat /proc/net/dev`（排除 lo 回环接口） |
| 核心数 | `nproc` | `nproc` |

> CPU% 和网络速率采用 **Delta 计算**：需要两次采样之间的差值。因此首次采样的这两个指标为 `null`（参见[不变量 P5](#-不变量)）。

---

## 🔧 后端设计

### 持久化 Shell 通道

与常规的 `exec` 模式（每次命令打开新 Channel）不同，Profiler 采用**持久 Shell 通道**：

```
┌───────────────────────────────────┐
│  shell_channel (1 个, 存活全程)   │
│                                   │
│  1. request_shell(false)          │
│  2. init: PS1=''; stty -echo      │
│  3. 循环:                         │
│     → 写入 SAMPLE_COMMAND (stdin) │
│     ← 读取输出直到 ===END===     │
│     → 解析 → 计算 → 发射事件     │
│     → sleep 10s                   │
└───────────────────────────────────┘
```

**优势**：
- 避免频繁开关 Channel → 不触发 MaxSessions 限制
- 无额外的 shell 启动开销
- 输出通过 `===MARKER===` 分隔符精确提取

### 采样命令

```bash
echo '===STAT==='; head -1 /proc/stat 2>/dev/null
echo '===MEMINFO==='; grep -E '^(MemTotal|MemAvailable):' /proc/meminfo 2>/dev/null
echo '===LOADAVG==='; cat /proc/loadavg 2>/dev/null
echo '===NETDEV==='; cat /proc/net/dev 2>/dev/null
echo '===NPROC==='; nproc 2>/dev/null
echo '===END==='
```

- 总输出量：**~500-1.5KB**（相比读取完整 `/proc/stat` + `/proc/meminfo` 的 10-30KB，减少约 90%）
- 每条子命令均带 `2>/dev/null` → 非 Linux 系统上静默失败

### 性能参数

| 参数 | 值 | 说明 |
|------|-----|------|
| `DEFAULT_INTERVAL` | 10s | 采样间隔，平衡精度与 SSH 带宽开销 |
| `SAMPLE_TIMEOUT` | 5s | 单次采样读取超时 |
| `MAX_OUTPUT_SIZE` | 8KB | 输出截断上限，防止异常输出 |
| `HISTORY_CAPACITY` | 60 | 环形缓冲区大小（10 分钟历史） |
| `MAX_CONSECUTIVE_FAILURES` | 3 | 连续失败阈值 → 降级为 RttOnly |
| `CHANNEL_OPEN_TIMEOUT` | 10s | 初始 Shell Channel 打开超时 |

### 锁策略

使用 `std::sync::RwLock`（非 `tokio::sync::RwLock`），原因：
- 临界区极短（仅读写几个字段，无 await）
- 避免 async RwLock 的 Waker/调度开销
- 减少与终端 PTY I/O 的 tokio 调度器竞争

### 停止机制（三路信号）

```rust
tokio::select! {
    _ = interval.tick() => { /* 采样 */ }
    _ = disconnect_rx.recv() => { break; }  // SSH 断连
    _ = &mut stop_rx => { break; }          // 手动停止
}
```

1. **disconnect_rx** — `HandleController::subscribe_disconnect()` 的广播，SSH 物理断连时触发
2. **stop_rx** — `tokio::sync::oneshot`，调用 `profiler.stop()` 时发送
3. **ProfilerRegistry::stop_all()** — 应用退出时统一清理

---

## 💻 前端设计

### profilerStore (Zustand)

```
src/store/profilerStore.ts
```

每个连接独立状态：`ConnectionProfilerState { metrics, history, isRunning, isEnabled, error }`

**关键操作**：

| 方法 | 说明 |
|------|------|
| `startProfiler(connId)` | 调用后端 API + 订阅 Tauri 事件 |
| `stopProfiler(connId)` | 取消订阅 + 调用后端 API + 清理状态 |
| `_updateMetrics(connId, m)` | 原地修改 Map + 浅拷贝触发 Zustand 更新 |
| `getSparklineHistory(connId)` | 返回最近 12 个数据点用于迷你图 |

**渲染优化**：
- `_updateMetrics` 采用原地修改 Map 后浅拷贝引用 — 仅触发订阅了对应 connectionId 数据的组件
- 避免全量 Map 深拷贝导致的无关连接组件重渲染

### API 层

```typescript
// src/lib/api.ts
api.startResourceProfiler(connectionId: string): Promise<void>
api.stopResourceProfiler(connectionId: string): Promise<void>
api.getResourceMetrics(connectionId: string): Promise<ResourceMetrics | null>
api.getResourceHistory(connectionId: string): Promise<ResourceMetrics[]>
```

### 事件通道

```
事件名: "profiler:update:{connectionId}"
载荷: ResourceMetrics (JSON)
方向: Backend → Frontend (单向)
频率: 每 10 秒
```

---

## 🛡️ 不变量

| 编号 | 不变量 | 说明 |
|------|--------|------|
| **P1** | 无强引用 | Profiler 不持有连接的强引用，仅通过 `HandleController`（弱引用）操作 |
| **P2** | 断连自停 | SSH 断连 → `disconnect_rx` 触发 → Profiler 自动停止并释放 Channel |
| **P3** | 单通道 | 整个生命周期仅打开 1 个 Shell Channel，不会导致 MaxSessions 耗尽 |
| **P5** | 首采空值 | 首次采样的 CPU% 和网络速率为 `None`（无 Delta 基线） |

---

## 🧪 测试

后端包含 8+ 单元测试，覆盖：

```bash
cd src-tauri && cargo test profiler    # 运行 profiler 相关测试
```

| 测试 | 验证内容 |
|------|---------|
| `test_parse_cpu_snapshot` | `/proc/stat` 解析正确性 |
| `test_parse_meminfo` | MemTotal / MemAvailable 计算 |
| `test_parse_loadavg` | 负载平均值解析 |
| `test_parse_net_snapshot` | 网络接口聚合（排除 lo） |
| `test_parse_nproc` | CPU 核心数解析 |
| `test_parse_metrics_first_sample_no_delta` | P5：首次无 CPU%/网络速率 |
| `test_parse_metrics_with_delta` | Delta 计算正确性 |
| `test_extract_section` | 标记分隔符提取 |
| `test_empty_output` | 空输出 → RttOnly 降级 |

---

## 📁 文件清单

| 文件 | 职责 |
|------|------|
| `src-tauri/src/session/profiler.rs` | 核心采样引擎（~760 行） |
| `src-tauri/src/session/health.rs` | `ResourceMetrics` / `MetricsSource` 类型定义 |
| `src-tauri/src/commands/health.rs` | `ProfilerRegistry` + 4 个 Tauri 命令 |
| `src-tauri/src/lib.rs` | `.manage(ProfilerRegistry)` + 命令注册 + 退出清理 |
| `src/store/profilerStore.ts` | 前端 Zustand Store |
| `src/lib/api.ts` | 4 个 API 包装函数 |
| `src/types/index.ts` | TypeScript 类型定义 |
| `src/locales/*/profiler.json` | 11 种语言的 i18n 文件 |

---

## ⚡ 性能影响

### SSH 带宽

- 每次采样命令 + 输出：**~1-2 KB**
- 10s 间隔 → **~6-12 KB/min** 额外带宽
- 远低于终端 PTY 的典型吞吐量（滚屏时可达 MB/s 级）

### 系统资源

- **1 个 Shell Channel** — 不占用额外的 SSH Session 额度
- **std::sync::RwLock** — 极低锁开销，不与 tokio 调度器竞争
- **环形缓冲区**（60 条）— 内存占用恒定（~30 KB/连接）

### 降级策略

连续 3 次采样失败后自动降级：
- ⚠️ **RttOnly 模式**：停止 `/proc` 采样，仅保留 SSH RTT 数据
- 降级后仍会每 10s 发射一次空指标（前端可据此显示降级状态）
- Channel 关闭时会尝试一次重开（`open_shell_channel`）

---

## 🔌 集成示例

```typescript
import { useProfilerStore } from '../store/profilerStore';

// 启动监控
await useProfilerStore.getState().startProfiler(connectionId);

// 读取最新指标
const metrics = useProfilerStore.getState().connections.get(connectionId)?.metrics;
if (metrics?.cpuPercent !== null) {
  console.log(`CPU: ${metrics.cpuPercent.toFixed(1)}%`);
}

// 停止监控
await useProfilerStore.getState().stopProfiler(connectionId);
```

> **幂等性**：`startProfiler` 和 `stopProfiler` 均为幂等操作，重复调用不会产生副作用。React StrictMode 的双重挂载也不会产生重复 Profiler。
