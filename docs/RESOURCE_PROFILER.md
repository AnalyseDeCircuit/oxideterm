# OxideTerm 深度资源画像施工文档 (Resource Profiler)

> **目标**：实时远程主机资源监控（CPU/内存/负载/网络），以"性能胶囊"浮层形式呈现在终端视图顶部。
>
> **合规**：本功能属于 SYSTEM_INVARIANTS §6.1 明确允许的扩展（"新的健康检查指标"），不触及任何 §6.2 禁止修改区域。
>
> **v0.1.0 | 2026-02-06**

---

## 目录

1. [设计目标与约束](#1-设计目标与约束)
2. [后端架构](#2-后端架构)
3. [前端架构](#3-前端架构)
4. [数据流详解](#4-数据流详解)
5. [生命周期与不变量](#5-生命周期与不变量)
6. [安全与性能](#6-安全与性能)
7. [错误处理](#7-错误处理)
8. [i18n 键](#8-i18n-键)
9. [实施计划](#9-实施计划)
10. [检查清单](#10-检查清单)

---

## 1. 设计目标与约束

### 1.1 核心目标

1. **实时感知**：在终端视图顶部提供 CPU / 内存 / 负载 / RTT 的可视化胶囊
2. **零侵入**：不修改 Wire Protocol（0x00-0x03 不变），数据走 Tauri IPC（控制平面）
3. **低开销**：每 5s 采样一次远程指标，单次 exec 命令 < 50ms
4. **优雅降级**：远程无 `/proc` 时静默回退到仅 SSH RTT 展示
5. **可组合**：同时适用于 SSH 终端和未来的 Graphics 视图

### 1.2 不修改的区域

| 约束来源 | 不触碰 |
|---------|---------|
| §6.2 Wire Protocol | `MessageType 0x00-0x03` 帧格式不变 |
| §6.2 生命周期依赖 | Session / Channel / Forward 的依赖关系不变 |
| §6.2 资源清理顺序 | `lib.rs` 的 exit cleanup 六段式顺序不变 |
| §0.1 Strong Sync | `connection:update` 事件驱动不变 |

### 1.3 依赖的现有基础设施

| 组件 | 文件 | 复用方式 |
|------|------|---------|
| `HandleController` | `ssh/handle_owner.rs` | Clone 后调用 `open_session_channel()` 开 exec channel |
| `SshConnectionRegistry` | `ssh/connection_registry.rs` | `get_handle_controller(connection_id)` 获取控制器 |
| `HealthTracker` | `session/health.rs` | 扩展指标，复用 RTT / 丢包率 / 状态判定 |
| `HealthRegistry` | `commands/health.rs` | DashMap 注册表，已在 `lib.rs` 中 `.manage()` |
| `ide_exec_command` 模式 | `commands/ide.rs` L225-327 | 参考其 exec channel + timeout + 输出收集模式 |
| Tauri Event 系统 | `session/events.rs` | `app_handle.emit()` 推送采样数据到前端 |
| `api.ts` Health 接口 | `lib/api.ts` L625-660 | 扩展新命令的前端 wrapper |

---

## 2. 后端架构

### 2.1 模块结构

```
src-tauri/src/
  session/
    health.rs          ← 扩展：添加 ResourceMetrics 结构体
    profiler.rs        ← 新建：ResourceProfiler 采样器
  commands/
    health.rs          ← 扩展：新增 profiler 相关 Tauri 命令
```

**不需要新 Feature Gate**：Resource Profiler 依赖 SSH 连接（核心功能），不是可选平台特性。

### 2.2 ResourceMetrics 数据结构

在 `session/health.rs` 中新增：

```rust
/// 远程主机资源指标（单次采样）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMetrics {
    pub timestamp_ms: u64,
    pub cpu_percent: Option<f64>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub memory_percent: Option<f64>,
    pub load_avg_1: Option<f64>,
    pub load_avg_5: Option<f64>,
    pub load_avg_15: Option<f64>,
    pub cpu_cores: Option<u32>,
    pub net_rx_bytes_per_sec: Option<u64>,
    pub net_tx_bytes_per_sec: Option<u64>,
    pub ssh_rtt_ms: Option<u64>,
    pub source: MetricsSource,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MetricsSource {
    Full, Partial, RttOnly, Failed,
}
```

### 2.3 ResourceProfiler 采样器

核心设计：**单条命令采集全部指标**。每次 `open_session_channel()` 经过 HandleOwner mpsc → oneshot 往返 + SSH channel open 网络往返，分多条命令会触及 SSH server `MaxSessions` 限制。

采样命令：
```bash
echo '===STAT==='; cat /proc/stat 2>/dev/null;
echo '===MEMINFO==='; cat /proc/meminfo 2>/dev/null;
echo '===LOADAVG==='; cat /proc/loadavg 2>/dev/null;
echo '===NETDEV==='; cat /proc/net/dev 2>/dev/null;
echo '===NPROC==='; nproc 2>/dev/null;
echo '===END==='
```

CPU 和网络速率通过**两次采样 delta** 计算，首次返回 null。

生命周期绑定 SSH 连接：通过 `controller.subscribe_disconnect()` 自动停止。

### 2.4 ProfilerRegistry

DashMap 注册表，管理所有活跃的 profiler 实例。

### 2.5 Tauri 命令

- `start_resource_profiler(connection_id)` — 启动采样
- `stop_resource_profiler(connection_id)` — 停止采样
- `get_resource_metrics(connection_id)` — 获取最新指标
- `get_resource_history(connection_id)` — 获取历史（供 sparkline）

### 2.6 lib.rs 集成

- `.manage(ProfilerRegistry::new())`
- 两个 `#[cfg]` invoke_handler 块都添加 4 个命令
- Exit cleanup：ProfilerRegistry 在 BridgeManager **之前**清理

---

## 3. 前端架构

### 3.1 PerformanceCapsule 组件

毛玻璃浮层（`bg-theme-bg-panel/40 backdrop-blur-md`），绝对定位在终端右上角：

```
┌─ PerformanceCapsule ──────────────────────────────────────┐
│ CPU 23%  ▁▃▅▇▅▃▁  MEM 4.2/8G  ↓1.2MB/s  ↑340KB/s  12ms │
└───────────────────────────────────────────────────────────┘
```

- 点击展开详情面板（Load Avg、CPU 核心数、数据来源）
- MiniSparkline: SVG polyline（最近 12 个采样点）
- 颜色阈值：green(< 70%) / amber(70-90%) / red(> 90%)

### 3.2 useResourceProfiler Hook

监听 Tauri Event `profiler:update:{connectionId}` 被动更新，不主动轮询。

### 3.3 设置项

`settingsStore.showResourceProfiler` 开关（默认 true）。

---

## 4. 数据流详解

```
远程 /proc/*  ─SSH exec─▶  ResourceProfiler  ─watch+event─▶  useResourceProfiler  ─state─▶  PerformanceCapsule
               (5s tick)      (Rust)                            (React Hook)                   (React Component)
```

---

## 5. 生命周期与不变量

| # | 不变量 | 说明 |
|---|--------|------|
| P1 | Profiler 不持有连接强引用 | 仅持有 HandleController Clone |
| P2 | SSH 断开 → Profiler 自动停止 | `subscribe_disconnect()` |
| P3 | Profiler 停止不影响连接 | 不触发 `connection:update` |
| P4 | 单条命令采集 | 每次采样只开 1 个 channel |
| P5 | 首次采样 null | CPU/网络无 delta 基线 |
| P6 | Key-Driven Reset | 伴随 TerminalView 的 key 自动重建 |
| P7 | 清理顺序 | ProfilerRegistry 在 BridgeManager 之前 |

---

## 6. 安全与性能

- 只执行 `cat /proc/*` 和 `nproc`（只读操作）
- 不读 `/proc/*/environ`、`/proc/*/cmdline`
- 输出超 1MB 截断
- 原始输出不写日志
- 5s 间隔，单 channel，历史环形 60 点（~4KB）

---

## 7. 错误处理

- exec 超时（3s）→ 跳过本轮，下次重试
- 连续 3 次 channel open 失败 → 降级到 RttOnly
- 非 Linux 系统 → 自动 RttOnly 模式

---

## 8. i18n 键

命名空间 `profiler`，11 种语言文件 `src/locales/{lang}/profiler.json`。

---

## 9. 实施计划

### Phase 1: 后端核心（~3-4 天）
- 类型定义 + profiler.rs + ProfilerRegistry + Tauri 命令 + lib.rs 集成 + 单元测试

### Phase 2: 前端 PerformanceCapsule（~3-4 天）
- TypeScript 类型 + API + Hook + 组件 + TerminalView 集成 + 设置开关

### Phase 3: 完善（~2 天）
- Tauri Event 推送 + RttOnly 回退 + RTT 集成 + i18n + 边界测试

---

## 10. 检查清单

- [ ] 不修改 Wire Protocol 帧格式
- [ ] 不修改生命周期依赖关系
- [ ] ProfilerRegistry 在 BridgeManager 之前清理
- [ ] `open_session_channel()` 失败时优雅降级
- [ ] exec 3s 超时 + 1MB 输出截断
- [ ] 不日志记录 /proc 输出
- [ ] CPU/网络首次采样返回 null
- [ ] PerformanceCapsule 跟随 key 自动重建
- [ ] 两个 `#[cfg]` invoke_handler 块都添加新命令
- [ ] `cargo build` 和 `cargo build --no-default-features` 都通过
- [ ] `npm run i18n:check` 通过
- [ ] Profiler 通过 `subscribe_disconnect()` 绑定生命周期

---

*文档版本: v0.1.0 | 最后更新: 2026-02-06*
