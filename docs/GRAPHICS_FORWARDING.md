# OxideTerm 图形转发施工参考文档 (Graphics Forwarding)

> **⚠️ 设计文档 — 功能尚未实现**
> 
> 本文档描述的所有功能均为设计规划，代码库中不存在对应实现。
> 后端无 `src-tauri/src/graphics/` 模块，前端无 `src/components/graphics/` 组件，
> Cargo.toml 无 `graphics-forwarding` feature flag。

> **版本**: v0.1.0 (Draft)
> **日期**: 2026-02-06
> **状态**: 设计文档，待实施
> **前置依赖**: SYSTEM_INVARIANTS.md v1.4.0

---

## 0. 设计目标与约束

### 0.1 目标

将 OxideTerm 从"终端引擎"扩展为"Linux 环境接管站"：

- **WSL 图形回传**：在 Windows 上直接查看/操作 WSL 内运行的 GUI 应用
- **远程图形转发**：通过已有 SSH 连接查看远程 Linux 桌面/应用
- **统一管线**：WSL 和远程 SSH 共用同一套渲染/交互管线

### 0.2 选型决策

| 层级 | 选型 | 否决方案 | 否决原因 |
|------|------|---------|---------|
| 图形协议 | **VNC (RFB)** | Wayland compositor | 需实现完整 wl_compositor 客户端，等同写半个 smithay |
| 传输层 | **localhost TCP** | AF_VSOCK | 无成熟 Rust crate，需大量 unsafe FFI + 管理员权限 |
| 代理模式 | **WebSocket ↔ TCP 透传** (websockify) | 自研像素流协议 | 复用 noVNC 已有的 RFB 编解码，零解析开销 |
| 渲染层 | **noVNC (Phase 1)** → 自研 WebGL (Phase 2) | 自研 Canvas 渲染 | noVNC MIT 协议，Phase 1 零开发渲染层 |
| SSH 隧道 | **复用 LocalForward** | 新建传输层 | 已有成熟端口转发实现，零新代码 |

### 0.3 与现有架构的共存原则

```
                  现有系统（不可触碰）              新增系统
                 ┌─────────────────────┐     ┌──────────────────────┐
  Wire Protocol  │ WsBridge (0x00-0x03)│     │ GraphicsBridge       │
  (INVARIANT§6.2)│ 终端字符 I/O        │     │ VNC RFB 二进制透传    │
                 │ 有界队列 4096/16384 │     │ 独立有界队列          │
                 └─────────────────────┘     └──────────────────────┘
                          ↑                            ↑
                       不修改                       全新模块
```

**铁律**：GraphicsBridge 是独立模块，不修改现有 Wire Protocol 帧格式（`MessageType 0x00-0x03`），不复用 `WsBridge` 代码路径，不干扰终端数据平面。

---

## 1. 三道防火墙：QoS / 优雅降级 / 心跳解耦

### 1.1 防火墙 #1：优先级调度（QoS）

**问题**：图形流带宽可达数十 MB/s，若与终端字符争抢系统资源（CPU/网络/内存），`Ctrl+C` 可能延迟数秒才到达。

**设计**：

```
┌─────────────────────────────────────────────────────────────┐
│ GraphicsBridge 内部架构                                      │
│                                                              │
│  VNC TCP ──read──▶ [Ingress Buffer: bounded(256)]           │
│                          │                                   │
│                    ┌─────▼─────┐                            │
│                    │ QoS Gate  │ ◄── bandwidth_budget        │
│                    └─────┬─────┘                            │
│                          │                                   │
│                    [Egress Queue: bounded(128)]              │
│                          │                                   │
│                    WebSocket.send(Binary)                    │
│                                                              │
│  全局限速器: Arc<AtomicU64> max_bytes_per_sec               │
│  终端优先: terminal WsBridge 不受限速影响                    │
└─────────────────────────────────────────────────────────────┘
```

**实现要点**：

```rust
/// GraphicsBridge 的 QoS 配置
pub struct GraphicsQosConfig {
    /// 图形流最大带宽 (bytes/sec)，默认 50MB/s
    /// 留出系统带宽给终端 I/O
    pub max_bandwidth_bps: u64,

    /// 单帧最大延迟 (ms)，超过则丢弃当前帧等下一个关键帧
    /// 保证 WebSocket event loop 不被大帧阻塞
    pub max_frame_latency_ms: u64,

    /// Egress 队列容量，满时丢弃最旧帧（图形可容忍丢帧）
    pub egress_queue_capacity: usize,

    /// 拥塞检测窗口 (ms)
    pub congestion_window_ms: u64,
}

impl Default for GraphicsQosConfig {
    fn default() -> Self {
        Self {
            max_bandwidth_bps: 50 * 1024 * 1024,  // 50 MB/s
            max_frame_latency_ms: 100,              // 100ms 最大帧延迟
            egress_queue_capacity: 128,             // 128 帧 egress 队列
            congestion_window_ms: 1000,             // 1s 拥塞检测窗口
        }
    }
}
```

**关键机制**：

1. **令牌桶限速**：每秒向桶中注入 `max_bandwidth_bps` 字节的令牌，发送前扣减。令牌不足时阻塞图形发送，但**终端 WsBridge 完全不经过此限速器**。

2. **帧丢弃策略**：Egress 队列满时，丢弃队列头部的旧帧。VNC RFB 协议天然支持增量更新，丢弃旧帧不会导致画面撕裂（下一次 FramebufferUpdate 会覆盖）。

3. **物理隔离**：GraphicsBridge 和 WsBridge 运行在不同的 tokio task 中，拥有独立的 TCP listener、独立的 WebSocket 连接、独立的 mpsc channel。即使 GraphicsBridge 的 task 完全阻塞，WsBridge 的终端 I/O 不受任何影响。

4. **`Ctrl+C` 保证**：终端字符通过 WsBridge（`FRAME_CHANNEL_CAPACITY = 4096/16384`）独立传输，GraphicsBridge 的任何拥塞都不会阻塞终端通道。这是物理保证，不是逻辑优先级——两条管道从 TCP listener 到 WebSocket 连接完全独立。

### 1.2 防火墙 #2：优雅降级（Adaptive Quality）

**问题**：远程 SSH 场景下网络带宽有限。若图形流占满带宽，SSH 心跳超时会导致整个 Session 断开。

**设计**：

```
拥塞检测器 (CongestionDetector)
    │
    ├─ 指标1: egress_queue_fill_ratio > 0.7  ──▶ 触发降级
    ├─ 指标2: frame_drop_count / window > 10  ──▶ 触发降级
    ├─ 指标3: ssh_heartbeat_rtt > 2x baseline ──▶ 紧急降级
    │
    ▼
降级动作 (QualityLevel 状态机)
    │
    ├─ Level 0 (Full)    : 原始质量，无压缩干预
    ├─ Level 1 (Reduced) : 请求 VNC server 切换 Tight JPEG quality=5
    ├─ Level 2 (Low)     : 降低 JPEG quality=2 + 限制 FPS 到 15
    ├─ Level 3 (Minimal) : 灰度模式 + 限制 FPS 到 5 + 降分辨率 50%
    └─ Level 4 (Paused)  : 暂停图形流，只保活 VNC 连接
```

**状态机规则**：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityLevel {
    Full,      // 无干预
    Reduced,   // JPEG quality 降低
    Low,       // JPEG quality + FPS 限制
    Minimal,   // 灰度 + 低FPS + 半分辨率
    Paused,    // 暂停图形流，仅保活
}

impl QualityLevel {
    /// 降级（逐级下降）
    pub fn degrade(&self) -> Self {
        match self {
            Self::Full => Self::Reduced,
            Self::Reduced => Self::Low,
            Self::Low => Self::Minimal,
            Self::Minimal => Self::Paused,
            Self::Paused => Self::Paused,
        }
    }

    /// 升级（逐级恢复，恢复速度慢于降级）
    pub fn upgrade(&self) -> Self {
        match self {
            Self::Paused => Self::Minimal,
            Self::Minimal => Self::Low,
            Self::Low => Self::Reduced,
            Self::Reduced => Self::Full,
            Self::Full => Self::Full,
        }
    }
}
```

**降级触发条件**：

| 条件 | 动作 | 恢复条件 |
|------|------|---------|
| Egress 队列 > 70% 满 | 降一级 | 队列 < 30% 持续 5s |
| 1s 内丢弃 > 10 帧 | 降一级 | 连续 10s 无丢帧 |
| SSH 心跳 RTT > 基线 2 倍 | **紧急降至 Paused** | RTT 恢复到基线 1.5 倍以内 |
| 用户手动设置 | 锁定到指定级别 | 用户解锁 |

**紧急降级的 SSH 心跳保护**：

```rust
/// 监控 SSH 心跳 RTT，保护核心信道
async fn monitor_ssh_health(
    session_id: &str,
    health_registry: &HealthRegistry,
    quality_tx: mpsc::Sender<QualityCommand>,
) {
    let baseline_rtt = measure_baseline_rtt(session_id, health_registry).await;

    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;

        let current_rtt = health_registry.get_rtt(session_id).await;
        if let (Some(baseline), Some(current)) = (baseline_rtt, current_rtt) {
            if current > baseline * 2 {
                // SSH 核心信道受压，紧急暂停图形流
                warn!(
                    "[GraphicsQoS] SSH RTT spike: {}ms (baseline: {}ms), pausing graphics",
                    current.as_millis(),
                    baseline.as_millis()
                );
                let _ = quality_tx.send(QualityCommand::EmergencyPause).await;
            }
        }
    }
}
```

**VNC 编码协商**：

降级通过标准 VNC RFB 协议的 `SetEncodings` 和 `SetPixelFormat` 消息实现：
- GraphicsBridge 在透传模式下不解析 RFB 内容
- 降级指令通过**注入 RFB 控制消息**到 VNC TCP 连接的写入端
- 这些消息是标准 VNC 客户端行为，VNC server 原生支持

```rust
/// 注入 RFB SetPixelFormat 消息到 VNC TCP 连接
/// 格式: [type=0][padding=3][PixelFormat=16bytes]
fn inject_set_pixel_format(quality: QualityLevel) -> Vec<u8> {
    let mut msg = vec![0u8; 20]; // type(1) + padding(3) + pixel_format(16)
    msg[0] = 0; // SetPixelFormat message type
    match quality {
        QualityLevel::Minimal => {
            // 8-bit grayscale
            msg[4] = 8;   // bits-per-pixel
            msg[5] = 8;   // depth
            msg[7] = 1;   // true-colour
            msg[8..10].copy_from_slice(&0u16.to_be_bytes()); // red-max (unused for 8bpp)
        }
        _ => {
            // 32-bit RGBA (full color)
            msg[4] = 32;  // bits-per-pixel
            msg[5] = 24;  // depth
            msg[7] = 1;   // true-colour
            msg[8..10].copy_from_slice(&255u16.to_be_bytes());  // red-max
            msg[10..12].copy_from_slice(&255u16.to_be_bytes()); // green-max
            msg[12..14].copy_from_slice(&255u16.to_be_bytes()); // blue-max
            msg[14] = 16; // red-shift
            msg[15] = 8;  // green-shift
            msg[16] = 0;  // blue-shift
        }
    }
    msg
}
```

### 1.3 防火墙 #3：心跳解耦（Independent Heartbeat）

**问题**：现有 WsBridge 心跳（`HEARTBEAT_INTERVAL_SECS = 30`）绑定到 SSH Session 生命周期。若图形通道断开触发心跳超时，不应该杀死整个 Session。

**设计**：

```
Session (SSH)
  ├─ WsBridge [心跳 A: 30s/90s] ──▶ 失败 → Session LinkDown
  ├─ SFTP Channel                ──▶ 独立于心跳
  ├─ Forward Channels            ──▶ 独立于心跳
  └─ GraphicsBridge [心跳 B: 10s/30s] ──▶ 失败 → 仅标记图形断开
                                              ──▶ 后台静默重连图形通道
                                              ──▶ Session 不受影响
```

**不变量**：

1. **GraphicsBridge 心跳失败 ≠ Session 故障**：图形通道断开只触发 GraphicsBridge 重连，不触发 `connection:update` 事件，不影响 `connectionState`
2. **GraphicsBridge 心跳独立于 WsBridge 心跳**：两套定时器，两个 `ConnectionState` 实例，互不干扰
3. **Session 断开 → 必须关闭 GraphicsBridge**：依赖方向单向（Session 是父，GraphicsBridge 是子），通过 `disconnect_rx` broadcast channel 传播

```rust
/// GraphicsBridge 心跳配置（独立于终端 WsBridge）
pub struct GraphicsHeartbeatConfig {
    /// 心跳间隔：比终端更频繁（图形对延迟更敏感）
    pub interval: Duration,    // 默认 10s
    /// 超时阈值：比终端更宽松（图形断开影响小）
    pub timeout: Duration,     // 默认 30s
    /// 断开后重连次数
    pub max_reconnect: u32,    // 默认 3 次
    /// 重连间隔（指数退避基数）
    pub reconnect_base: Duration, // 默认 2s
}

impl Default for GraphicsHeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(10),
            timeout: Duration::from_secs(30),
            max_reconnect: 3,
            reconnect_base: Duration::from_secs(2),
        }
    }
}
```

**GraphicsBridge 重连流程**：

```
GraphicsBridge 心跳超时
    │
    ├─ 1. 标记 GraphicsSession.status = Reconnecting
    ├─ 2. emit Tauri event: graphics:status_changed { status: "reconnecting" }
    ├─ 3. 前端 GraphicsView 显示 "图形连接中断，正在重连..."（叠加在画面上）
    ├─ 4. 关闭旧 WebSocket + 旧 VNC TCP 连接
    ├─ 5. 检查父 Session 是否仍 active（State Gating）
    │      ├─ 否 → 放弃重连，标记 Disconnected
    │      └─ 是 → 继续
    ├─ 6. 重新建立 VNC TCP 连接（WSL: localhost / Remote: 复用已有 LocalForward）
    ├─ 7. 重新创建 GraphicsBridge WebSocket
    ├─ 8. 返回新的 { wsPort, wsToken } 给前端
    ├─ 9. 前端 noVNC 用新 URL 重连
    └─ 10. 标记 GraphicsSession.status = Active
```

**关键**：GraphicsBridge 重连**不生成新的 connectionId**（与 SSH Session 重连不同），因为图形通道是 Session 的子资源，不触发 Key-Driven Reset。但如果 Session 本身重连（生成新 connectionId），GraphicsView 会因 `key={graphics-${sessionId}-${connectionId}}` 自动销毁重建。

---

## 2. 后端架构

### 2.1 模块结构

```
src-tauri/src/
├── graphics/                       # 新增模块 (feature: graphics-forwarding)
│   ├── mod.rs                      # 模块导出
│   ├── bridge.rs                   # GraphicsBridge: WebSocket ↔ VNC TCP 代理
│   ├── qos.rs                      # QoS 限速器 + 拥塞检测器
│   ├── quality.rs                  # 自适应质量控制 (QualityLevel 状态机)
│   ├── heartbeat.rs                # 独立心跳监测
│   ├── wsl.rs                      # WSL 图形服务管理 (VNC 探测/启动)
│   ├── registry.rs                 # GraphicsSession 注册表
│   └── error.rs                    # GraphicsError 类型
├── commands/
│   └── graphics.rs                 # Tauri 命令 (feature-gated)
```

### 2.2 Feature Gate

参照 `local-terminal` 的六处守卫模式：

```toml
# src-tauri/Cargo.toml
[features]
default = ["local-terminal"]
local-terminal = ["dep:portable-pty"]
graphics-forwarding = []  # Phase 1 无新依赖（纯 TCP 代理）
```

六处 `#[cfg(feature = "graphics-forwarding")]` 守卫：

| 位置 | 文件 | 作用 |
|------|------|------|
| 1. 模块声明 | `lib.rs` | `#[cfg(feature = "graphics-forwarding")] pub mod graphics;` |
| 2. 命令模块 | `commands/mod.rs` | `#[cfg(feature = "graphics-forwarding")] pub mod graphics;` |
| 3. 状态创建 | `lib.rs` | `let graphics_state = Arc::new(GraphicsState::new());` |
| 4. Tauri 管理 | `lib.rs` | `builder.manage(graphics_state)` |
| 5. 命令注册 | `lib.rs` | `invoke_handler` 中注册图形命令 |
| 6. 退出清理 | `lib.rs` | `graphics_state.close_all().await` |

### 2.3 GraphicsBridge 核心（bridge.rs）

与 `WsBridge` 的关键区别：

| 维度 | WsBridge (终端) | GraphicsBridge (图形) |
|------|----------------|---------------------|
| 协议 | Wire Protocol v1 (解析帧) | **原始二进制透传**（不解析 RFB） |
| 方向 | SSH Channel ↔ WebSocket | VNC TCP ↔ WebSocket |
| 队列 | `FRAME_CHANNEL_CAPACITY` | 独立 `egress_queue_capacity` |
| 心跳 | 嵌入 Wire Protocol (0x02) | **WebSocket Ping/Pong 原生心跳** |
| 限速 | 无 | 令牌桶 QoS |
| 丢帧 | 不允许 | 允许（图形可容忍） |
| 重连 | 触发 Session 状态变更 | **仅重连图形通道** |

```rust
//! GraphicsBridge: WebSocket ↔ VNC TCP 二进制双向代理
//!
//! 不解析 VNC RFB 协议内容（透传模式），仅提供：
//! - WebSocket 认证 (复用 generate_token 机制)
//! - QoS 限速
//! - 独立心跳
//! - 质量降级指令注入

pub struct GraphicsBridge;

impl GraphicsBridge {
    /// 启动 GraphicsBridge
    ///
    /// # Arguments
    /// * `vnc_target` - VNC server 的 TCP 地址 (e.g. "localhost:5901")
    /// * `qos_config` - QoS 配置
    /// * `session_disconnect_rx` - SSH Session 断开通知（单向依赖）
    ///
    /// # Returns
    /// * `(graphics_id, ws_port, ws_token)` - 前端连接信息
    pub async fn start(
        vnc_target: &str,
        qos_config: GraphicsQosConfig,
        session_disconnect_rx: broadcast::Receiver<()>,
    ) -> Result<(String, u16, String), GraphicsError> {
        let graphics_id = Uuid::new_v4().to_string();
        let token = generate_token(); // 复用现有 token 生成

        // 1. 连接 VNC server
        let vnc_stream = TcpStream::connect(vnc_target).await?;
        vnc_stream.set_nodelay(true)?;

        // 2. 绑定 WebSocket listener
        let ws_listener = TcpListener::bind("localhost:0").await?;
        let ws_port = ws_listener.local_addr()?.port();

        // 3. 启动代理任务
        tokio::spawn(Self::run_proxy(
            graphics_id.clone(),
            ws_listener,
            vnc_stream,
            token.clone(),
            qos_config,
            session_disconnect_rx,
        ));

        Ok((graphics_id, ws_port, token))
    }

    async fn run_proxy(
        graphics_id: String,
        ws_listener: TcpListener,
        vnc_stream: TcpStream,
        expected_token: String,
        qos_config: GraphicsQosConfig,
        mut session_disconnect_rx: broadcast::Receiver<()>,
    ) {
        // Accept WebSocket 连接（带超时）
        let ws_stream = tokio::select! {
            result = tokio::time::timeout(
                Duration::from_secs(60),
                ws_listener.accept()
            ) => {
                match result {
                    Ok(Ok((stream, _))) => {
                        stream.set_nodelay(true).ok();
                        stream
                    }
                    _ => return,
                }
            }
            _ = session_disconnect_rx.recv() => {
                info!("[GraphicsBridge {}] Session disconnected before WS connect", graphics_id);
                return;
            }
        };

        // WebSocket 握手 + Token 认证
        let ws = match accept_async(ws_stream).await {
            Ok(ws) => ws,
            Err(e) => {
                error!("[GraphicsBridge {}] WS handshake failed: {}", graphics_id, e);
                return;
            }
        };

        // 认证（复用 validate_token）
        // ... (与 WsBridge handle_connection_v1 相同的 token 验证逻辑)

        // 进入双向透传循环
        Self::proxy_loop(
            graphics_id, ws, vnc_stream, qos_config, session_disconnect_rx
        ).await;
    }

    /// 核心透传循环
    ///
    /// 三路 select:
    /// 1. VNC TCP → QoS Gate → WebSocket (图形数据下行)
    /// 2. WebSocket → VNC TCP (用户输入上行，无限速)
    /// 3. Session disconnect → 退出
    async fn proxy_loop(
        graphics_id: String,
        ws: WebSocketStream<TcpStream>,
        vnc: TcpStream,
        qos: GraphicsQosConfig,
        mut disconnect_rx: broadcast::Receiver<()>,
    ) {
        let (mut ws_tx, mut ws_rx) = ws.split();
        let (mut vnc_read, mut vnc_write) = vnc.into_split();

        // QoS 令牌桶
        let token_bucket = Arc::new(TokenBucket::new(qos.max_bandwidth_bps));

        // 独立心跳任务
        let heartbeat_state = Arc::new(ConnectionState::new());
        let heartbeat_state_clone = heartbeat_state.clone();

        // 下行：VNC → WebSocket (带 QoS)
        let downlink = async {
            let mut buf = vec![0u8; 65536]; // 64KB read buffer
            loop {
                let n = vnc_read.read(&mut buf).await?;
                if n == 0 { break; }

                // QoS 限速：等待令牌
                token_bucket.acquire(n as u64).await;

                // 发送到 WebSocket
                ws_tx.send(Message::Binary(buf[..n].to_vec())).await?;
                heartbeat_state.touch();
            }
            Ok::<(), GraphicsError>(())
        };

        // 上行：WebSocket → VNC (无限速，用户输入优先)
        let uplink = async {
            while let Some(msg) = ws_rx.next().await {
                match msg? {
                    Message::Binary(data) => {
                        vnc_write.write_all(&data).await?;
                        heartbeat_state_clone.touch();
                    }
                    Message::Ping(data) => {
                        ws_tx.send(Message::Pong(data)).await?;
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            Ok::<(), GraphicsError>(())
        };

        // 三路并发
        tokio::select! {
            r = downlink => {
                if let Err(e) = r {
                    warn!("[GraphicsBridge {}] downlink error: {}", graphics_id, e);
                }
            }
            r = uplink => {
                if let Err(e) = r {
                    warn!("[GraphicsBridge {}] uplink error: {}", graphics_id, e);
                }
            }
            _ = disconnect_rx.recv() => {
                info!("[GraphicsBridge {}] session disconnected, closing", graphics_id);
            }
        }
    }
}
```

### 2.4 GraphicsSession 注册表（registry.rs）

```rust
/// 图形会话信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicsSessionInfo {
    /// 图形会话 ID
    pub id: String,
    /// 关联的 SSH Session ID（如有）
    pub session_id: Option<String>,
    /// 关联的 Connection ID（如有）
    pub connection_id: Option<String>,
    /// 图形协议
    pub protocol: GraphicsProtocol,
    /// VNC 目标地址
    pub vnc_target: String,
    /// WebSocket 端口（供前端连接）
    pub ws_port: u16,
    /// WebSocket Token
    pub ws_token: String,
    /// 会话状态
    pub status: GraphicsSessionStatus,
    /// 当前质量级别
    pub quality_level: QualityLevel,
    /// 创建时间
    pub created_at: Instant,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphicsProtocol {
    Vnc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphicsSessionStatus {
    Starting,
    Active,
    Reconnecting,
    Paused,      // QoS 紧急暂停
    Disconnected,
    Error,
}

/// 图形会话注册表
pub struct GraphicsRegistry {
    sessions: RwLock<HashMap<String, GraphicsSessionHandle>>,
}

struct GraphicsSessionHandle {
    info: GraphicsSessionInfo,
    /// 控制通道：发送停止/质量调整命令
    control_tx: mpsc::Sender<GraphicsCommand>,
}

pub enum GraphicsCommand {
    Stop,
    SetQuality(QualityLevel),
    EmergencyPause,
    Resume,
}
```

### 2.5 WSL 图形服务管理（wsl.rs）

```rust
//! WSL 图形服务管理
//!
//! 职责：
//! 1. 探测 WSL 发行版中可用的 VNC 服务
//! 2. 在 WSL 内启动 VNC server
//! 3. 返回 VNC 连接信息供 GraphicsBridge 使用

/// 探测 WSL 中可用的 VNC 服务
pub async fn detect_vnc_server(distro: &str) -> Result<VncServerInfo, GraphicsError> {
    // 优先级：wayvnc (Wayland) > x11vnc (X11) > tigervnc
    let checks = [
        ("wayvnc", "wayvnc"),
        ("x11vnc", "x11vnc"),
        ("tigervnc", "Xtigervnc"),
    ];

    for (name, binary) in &checks {
        let output = Command::new("wsl.exe")
            .args(["-d", distro, "--", "which", binary])
            .output()
            .await?;

        if output.status.success() {
            return Ok(VncServerInfo {
                server_type: name.to_string(),
                binary_path: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            });
        }
    }

    Err(GraphicsError::NoVncServer(distro.to_string()))
}

/// 在 WSL 中启动 VNC server
///
/// 返回 VNC 监听端口。
///
/// ⚠️ **施工陷阱：WSL 端口冲突**
///
/// 绝对不要硬编码 5900/5901 端口！Windows 上以下程序会占用 VNC 常用端口：
/// - WSLg 内置 XServer（监听 :0 = 6000 端口族）
/// - RealVNC / TightVNC / TigerVNC 桌面客户端
/// - Windows Remote Desktop 的 VNC 兼容层
/// - 开发者自己运行的 VNC server
///
/// 解决方案：始终使用 `find_free_port()` 让 OS 分配随机高端口（49152-65535 动态端口范围）。
/// VNC server 通过 `-rfbport` 参数绑定到该端口，而非默认 5900+display。
pub async fn start_vnc_server(
    distro: &str,
    server_info: &VncServerInfo,
) -> Result<u16, GraphicsError> {
    let port = find_free_port().await?; // ← 关键：随机高端口，避免冲突

    match server_info.server_type.as_str() {
        "x11vnc" => {
            // x11vnc: X11 环境下的 VNC server
            // -display :0  连接到默认 X display
            // -rfbport N   指定 RFB 端口
            // -nopw         无密码（本地 WSL 安全）
            // -shared       允许多连接
            // -forever      保持运行
            // -bg           后台运行
            Command::new("wsl.exe")
                .args([
                    "-d", distro, "--",
                    "x11vnc",
                    "-display", ":0",
                    "-rfbport", &port.to_string(),
                    "-nopw", "-shared", "-forever", "-bg",
                ])
                .spawn()?;
        }
        "wayvnc" => {
            // wayvnc: Wayland 环境下的 VNC server
            Command::new("wsl.exe")
                .args([
                    "-d", distro, "--",
                    "wayvnc",
                    "--output", "*",
                    "0.0.0.0", &port.to_string(),
                ])
                .spawn()?;
        }
        _ => return Err(GraphicsError::UnsupportedVncServer(
            server_info.server_type.clone()
        )),
    }

    // 等待 VNC server 就绪（轮询连接）
    // 注意：不能只 connect 一次就认为就绪，某些 VNC server 启动后会先 accept
    // 再 reset，需要 retry + 验证 RFB 版本握手首 12 字节 "RFB 003.0xx\n"
    wait_for_vnc_ready(&format!("localhost:{}", port), Duration::from_secs(10)).await?;

    Ok(port)
}

/// 在 localhost 寻找可用端口
///
/// 绑定 port 0 让 OS 分配，立即关闭后返回端口号。
/// 存在极小概率的 TOCTOU 竞态（端口可能在返回后被其他进程抢占），
/// 但对于 VNC server 启动场景可接受——启动失败会被 wait_for_vnc_ready 捕获。
async fn find_free_port() -> Result<u16, GraphicsError> {
    let listener = TcpListener::bind("localhost:0").await
        .map_err(|e| GraphicsError::WsBindFailed(format!("find_free_port: {}", e)))?;
    let port = listener.local_addr()
        .map_err(|e| GraphicsError::WsBindFailed(format!("get port: {}", e)))?.port();
    drop(listener); // 立即释放，让 VNC server 绑定
    Ok(port)
}

/// 探测 WSLg 是否可用（Windows 11 内置 Wayland compositor）
pub async fn detect_wslg(distro: &str) -> bool {
    let output = Command::new("wsl.exe")
        .args(["-d", distro, "--", "test", "-S", "/mnt/wslg/.X11-unix/X0"])
        .output()
        .await;

    matches!(output, Ok(o) if o.status.success())
}
```

### 2.6 Tauri 命令（commands/graphics.rs）

```rust
/// 启动 WSL 图形会话
///
/// 流程：
/// 1. 探测 WSL 发行版的 VNC 服务
/// 2. 在 WSL 内启动 VNC server
/// 3. 创建 GraphicsBridge (WebSocket → VNC TCP 代理)
/// 4. 返回 WebSocket 连接信息给前端
#[tauri::command]
pub async fn graphics_start_wsl(
    distro: String,
    graphics_state: State<'_, Arc<GraphicsState>>,
) -> Result<GraphicsSessionInfo, String> { ... }

/// 启动远程图形会话
///
/// 流程：
/// 1. 复用已有 SSH 连接的 HandleController
/// 2. 创建 LocalForward 隧道 (localhost:0 → remote:vnc_port)
/// 3. 创建 GraphicsBridge (WebSocket → 隧道端口)
/// 4. 返回 WebSocket 连接信息给前端
#[tauri::command]
pub async fn graphics_start_remote(
    connection_id: String,
    remote_vnc_port: u16,
    graphics_state: State<'_, Arc<GraphicsState>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
    session_registry: State<'_, Arc<SessionRegistry>>,
) -> Result<GraphicsSessionInfo, String> { ... }

/// 停止图形会话
#[tauri::command]
pub async fn graphics_stop(
    graphics_id: String,
    graphics_state: State<'_, Arc<GraphicsState>>,
) -> Result<(), String> { ... }

/// 列出活跃图形会话
#[tauri::command]
pub async fn graphics_list(
    graphics_state: State<'_, Arc<GraphicsState>>,
) -> Result<Vec<GraphicsSessionInfo>, String> { ... }

/// 手动设置图形质量级别
#[tauri::command]
pub async fn graphics_set_quality(
    graphics_id: String,
    quality: QualityLevel,
    graphics_state: State<'_, Arc<GraphicsState>>,
) -> Result<(), String> { ... }
```

---

## 3. 前端架构

### 3.1 类型扩展（types/index.ts）

```typescript
// === Graphics Forwarding Types ===

export type GraphicsProtocol = 'vnc';

export type GraphicsSessionStatus =
  | 'starting'
  | 'active'
  | 'reconnecting'
  | 'paused'
  | 'disconnected'
  | 'error';

export type GraphicsQualityLevel =
  | 'full'
  | 'reduced'
  | 'low'
  | 'minimal'
  | 'paused';

export type GraphicsSource = 'wsl' | 'remote';

export interface GraphicsSessionInfo {
  id: string;
  sessionId?: string;
  connectionId?: string;
  protocol: GraphicsProtocol;
  source: GraphicsSource;
  vncTarget: string;
  wsPort: number;
  wsToken: string;
  status: GraphicsSessionStatus;
  qualityLevel: GraphicsQualityLevel;
}

// TabType 扩展
export type TabType =
  | 'terminal' | 'sftp' | 'forwards' | 'settings'
  | 'connection_monitor' | 'connection_pool' | 'topology'
  | 'local_terminal' | 'ide' | 'file_manager'
  | 'graphics';  // 新增

// PaneTerminalType 扩展（如果图形需要在分屏中）
export type PaneTerminalType = 'terminal' | 'local_terminal' | 'graphics';
```

### 3.2 API 层扩展（lib/api.ts）

```typescript
// === Graphics Forwarding API ===

graphicsStartWsl: async (distro: string): Promise<GraphicsSessionInfo> => {
  return invoke('graphics_start_wsl', { distro });
},

graphicsStartRemote: async (
  connectionId: string, 
  remoteVncPort: number
): Promise<GraphicsSessionInfo> => {
  return invoke('graphics_start_remote', { connectionId, remoteVncPort });
},

graphicsStop: async (graphicsId: string): Promise<void> => {
  return invoke('graphics_stop', { graphicsId });
},

graphicsList: async (): Promise<GraphicsSessionInfo[]> => {
  return invoke('graphics_list');
},

graphicsSetQuality: async (
  graphicsId: string, 
  quality: GraphicsQualityLevel
): Promise<void> => {
  return invoke('graphics_set_quality', { graphicsId, quality });
},
```

### 3.3 GraphicsView 组件

```
src/components/graphics/
├── GraphicsView.tsx          # 主容器（noVNC 嵌入 + 状态管理）
├── GraphicsToolbar.tsx       # 工具栏（全屏/质量/截图/剪贴板）
├── GraphicsStatusOverlay.tsx # 状态叠加层（连接中/重连中/暂停）
└── useGraphicsSession.ts    # 图形会话 hook
```

**GraphicsView.tsx 要点**：

```tsx
export const GraphicsView: React.FC<GraphicsViewProps> = ({
  sessionId,
  connectionId,
  graphicsSession,
}) => {
  const { t } = useTranslation();
  const canvasRef = useRef<HTMLDivElement>(null);
  const rfbRef = useRef<RFB | null>(null);  // noVNC RFB 实例

  // Key-Driven Reset: connectionId 变化时自动销毁重建
  // 在父组件中: <GraphicsView key={`graphics-${sessionId}-${connectionId}`} />

  useEffect(() => {
    if (!canvasRef.current || !graphicsSession) return;

    // 构建 WebSocket URL（带 token 认证）
    const wsUrl = `ws://localhost:${graphicsSession.wsPort}`;

    // 创建 noVNC RFB 连接
    const rfb = new RFB(canvasRef.current, wsUrl, {
      credentials: { password: graphicsSession.wsToken },
      wsProtocols: ['binary'],
    });

    // 配置
    rfb.viewOnly = false;
    rfb.scaleViewport = true;
    rfb.resizeSession = true;
    rfb.clipViewport = false;
    rfb.showDotCursor = true;

    // 事件监听
    rfb.addEventListener('connect', () => {
      info('[GraphicsView] VNC connected');
    });

    rfb.addEventListener('disconnect', (e) => {
      warn('[GraphicsView] VNC disconnected:', e.detail);
      // 不触发 Session 状态变更！只更新图形状态
    });

    rfbRef.current = rfb;

    // 清理（Key-Driven Reset 时自动触发）
    return () => {
      rfb.disconnect();
      rfbRef.current = null;
    };
  }, [graphicsSession]);

  return (
    <div className="relative h-full w-full bg-black">
      {/* noVNC Canvas 容器 */}
      <div ref={canvasRef} className="h-full w-full" />

      {/* 状态叠加层 */}
      {graphicsSession?.status !== 'active' && (
        <GraphicsStatusOverlay status={graphicsSession?.status} />
      )}

      {/* 工具栏 */}
      <GraphicsToolbar
        graphicsId={graphicsSession?.id}
        qualityLevel={graphicsSession?.qualityLevel}
        onSetQuality={handleSetQuality}
        onFullscreen={handleFullscreen}
        onScreenshot={handleScreenshot}
      />
    </div>
  );
};
```

### 3.4 TerminalPane 路由扩展

```tsx
// src/components/terminal/TerminalPane.tsx
// 新增 'graphics' 分支

{pane.terminalType === 'graphics' ? (
  <GraphicsView
    key={`graphics-${pane.sessionId}-${getSession(pane.sessionId)?.connectionId ?? ''}`}
    sessionId={pane.sessionId}
    connectionId={getSession(pane.sessionId)?.connectionId}
    graphicsSession={graphicsSession}
  />
) : pane.terminalType === 'terminal' ? (
  <TerminalView
    key={`${pane.sessionId}-${getSession(pane.sessionId)?.ws_url ?? ''}`}
    sessionId={pane.sessionId}
    paneId={pane.id}
    tabId={tabId}
    onFocus={handleFocus}
  />
) : (
  <LocalTerminalView ... />
)}
```

### 3.5 noVNC 集成

**安装**：

```bash
npm install @novnc/novnc
```

**⚠️ 施工陷阱：noVNC WebSocket 子协议握手**

noVNC 在创建 WebSocket 连接时会发送 `Sec-WebSocket-Protocol: binary` 头。如果 `GraphicsBridge` 的 `accept_async()` 没有正确回应这个子协议，浏览器会**静默拒绝连接**——不报错，WebSocket 直接 close，前端只看到 `onclose` 事件。

这是最容易踩的坑：`tokio-tungstenite` 默认的 `accept_async()` 不处理子协议协商。必须使用 `accept_hdr_async()` 并在回调中回写子协议头：

```rust
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::http;

/// ⚠️ 必须使用 accept_hdr_async 而非 accept_async
/// noVNC 发送 Sec-WebSocket-Protocol: binary
/// 不回应此头 → 浏览器静默断开连接
async fn accept_with_subprotocol(
    stream: TcpStream,
) -> Result<WebSocketStream<TcpStream>, GraphicsError> {
    let ws = accept_hdr_async(stream, |req: &Request, mut resp: Response| {
        // 检查客户端请求的子协议
        if let Some(protocols) = req.headers().get("Sec-WebSocket-Protocol") {
            let protocols_str = protocols.to_str().unwrap_or("");
            // noVNC 请求 "binary"，我们回应确认
            if protocols_str.contains("binary") {
                resp.headers_mut().insert(
                    "Sec-WebSocket-Protocol",
                    http::HeaderValue::from_static("binary"),
                );
            }
        }
        Ok(resp)
    })
    .await
    .map_err(|e| GraphicsError::WsBindFailed(format!("WS handshake: {}", e)))?;

    Ok(ws)
}
```

**验证方法**：在浏览器 DevTools → Network 面板中检查 WebSocket 连接的 `Sec-WebSocket-Protocol` 响应头，必须为 `binary`。如果缺失，noVNC 会立即断开且不产生任何错误日志。

**noVNC 认证适配**：

noVNC 的 `RFB` 构造函数支持 `credentials.password` 参数，但我们的 GraphicsBridge 使用一次性 Token 认证。三种适配方案：

```rust
// 方案 A（推荐）: URL Query String 传 Token
//   前端: ws://localhost:PORT?token=XXX
//   GraphicsBridge: 从 accept_hdr_async 回调的 Request URI 中提取 token
//   优势: noVNC 不感知认证层，RFB 握手不受干扰
//   在 accept_hdr_async 回调中一并解析验证，与子协议处理在同一个闭包

// 方案 B: WebSocket 首帧 Token（与 WsBridge 一致）
//   ⚠️ 问题: noVNC 会立即开始 RFB 握手（发送 "RFB 003.008\n"），
//   token 帧和 RFB 帧会混在一起，需要在 GraphicsBridge 中缓存
//   第一帧并区分 token vs RFB 数据。容易出 bug，不推荐。

// 方案 C: VNC RFB 认证（让 noVNC 原生处理）
//   GraphicsBridge 不做 WebSocket 层认证
//   依赖 VNC server 的 RFB SecurityType 认证
//   适用于 VNC server 设置了密码的场景
```

**推荐方案 A**：Token 放在 URL query string 中，在 `accept_hdr_async` 回调里一并验证，与 `Sec-WebSocket-Protocol` 处理在同一个闭包中完成，无需额外帧解析逻辑。前端构造 URL 时拼接 `?token=${graphicsSession.wsToken}`。

### 3.6 剪贴板双向同步

**目标**：实现 Windows/macOS 剪贴板 ↔ VNC 远程/WSL 剪贴板的双向同步，让图形桌面内的复制粘贴"透明穿越"。

**VNC RFB 剪贴板协议**：

RFB 协议原生支持剪贴板同步，通过两种消息类型：

| 方向 | RFB 消息 | Type 字节 | 说明 |
|------|---------|-----------|------|
| Client → Server | `ClientCutText` | `0x06` | 客户端剪贴板内容发送到 VNC server |
| Server → Client | `ServerCutText` | `0x03` | VNC server 剪贴板内容发送到客户端 |

**架构**：

```
┌──────────────┐     RFB: ServerCutText      ┌──────────────────┐
│ VNC Server   │ ──────────────────────────▶ │ noVNC (前端)      │
│ (WSL/Remote) │                              │                  │
│              │ ◀────────────────────────── │ clipboardPaste() │
└──────────────┘     RFB: ClientCutText      └────────┬─────────┘
                                                       │
                                              Tauri clipboard API
                                                       │
                                              ┌────────▼─────────┐
                                              │ OS Clipboard      │
                                              │ (Win/macOS/Linux) │
                                              └──────────────────┘
```

**实现方式（Phase 2）**：

由于 GraphicsBridge 是**透传模式**（不解析 RFB 内容），剪贴板同步**在前端 noVNC 层完成**，不需要修改 Rust 代码：

```tsx
// GraphicsView.tsx 中的剪贴板同步
useEffect(() => {
  if (!rfbRef.current) return;

  const rfb = rfbRef.current;
  const lastClipboardText = { current: '' };

  // Server → Client：VNC server 剪贴板变化
  rfb.addEventListener('clipboard', (e: CustomEvent) => {
    const text = e.detail.text;
    if (text) {
      // 写入 OS 剪贴板（通过 Tauri API）
      navigator.clipboard.writeText(text).catch(err => {
        warn('[Clipboard] Failed to write to OS clipboard:', err);
      });
    }
  });

  // Client → Server：监听 OS 剪贴板变化，推送到 VNC
  const clipboardInterval = setInterval(async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (text && text !== lastClipboardText.current) {
        lastClipboardText.current = text;
        rfb.clipboardPasteFrom(text);  // noVNC API: 发送 ClientCutText
      }
    } catch {
      // 权限被拒绝时静默忽略
    }
  }, 500); // 500ms 轮询（Clipboard API 无 change event）

  return () => clearInterval(clipboardInterval);
}, [rfbRef.current]);
```

**注意事项**：

1. **权限问题**：`navigator.clipboard.readText()` 需要页面处于焦点状态，Tauri WebView 中通常自动满足
2. **二进制剪贴板**（图片）：RFB 的 `ClientCutText` 只支持 Latin-1 文本。图片复制需要 Extended Clipboard（RFB 3.8 扩展），noVNC 部分支持，留到 Phase 3
3. **Tauri clipboard plugin**：`tauri-plugin-clipboard-manager` 提供更可靠的跨平台剪贴板访问，Phase 3 可替换 `navigator.clipboard` API
4. **安全**：剪贴板内容可能包含密码等敏感信息，**禁止在日志中记录剪贴板内容**

---

## 4. 数据流详解

### 4.1 WSL 图形会话

```
用户操作: 右键 WSL 会话 → "打开图形桌面"
     │
     ▼
[前端] api.graphicsStartWsl("Ubuntu")
     │
     ▼
[Tauri IPC] → graphics_start_wsl 命令
     │
     ├─ 1. detect_vnc_server("Ubuntu")        → 发现 x11vnc
     ├─ 2. detect_wslg("Ubuntu")              → WSLg 可用
     ├─ 3. start_vnc_server("Ubuntu", x11vnc)  → 启动在 port 5901
     ├─ 4. GraphicsBridge::start("localhost:5901", qos_config, None)
     │      ├─ VNC TCP connect → localhost:5901 ✓
     │      ├─ WS listener bind → localhost:49152
     │      └─ 返回 (graphics_id, 49152, "token_abc")
     ├─ 5. GraphicsRegistry.register(session_info)
     └─ 6. 返回 GraphicsSessionInfo 给前端
     │
     ▼
[前端] 创建 graphics Tab → 挂载 GraphicsView
     │
     ├─ new RFB(canvas, "ws://localhost:49152")
     ├─ 发送 token 认证帧 "token_abc"
     ├─ VNC RFB 握手（through GraphicsBridge 透传）
     └─ Canvas 开始渲染 Linux 桌面
```

### 4.2 远程 SSH 图形会话

```
用户操作: 右键 SSH 会话 → "远程图形桌面" → 输入 VNC 端口 5900
     │
     ▼
[前端] api.graphicsStartRemote(connectionId, 5900)
     │
     ▼
[Tauri IPC] → graphics_start_remote 命令
     │
     ├─ 1. State Gating: 检查 connectionState === 'active'
     ├─ 2. 获取 HandleController (from SessionRegistry)
     ├─ 3. ForwardingManager.start_local_forward(
     │        LocalForward { local_addr: "localhost:0", remote_host: "localhost", remote_port: 5900 }
     │      )
     │      → 隧道绑定到 localhost:38294
     ├─ 4. GraphicsBridge::start("localhost:38294", qos_config, disconnect_rx)
     │      ├─ VNC TCP connect → localhost:38294 (SSH 隧道)
     │      ├─ WS listener bind → localhost:49153
     │      └─ 返回 (graphics_id, 49153, "token_def")
     ├─ 5. GraphicsRegistry.register(session_info)
     └─ 6. 返回 GraphicsSessionInfo 给前端
     │
     ▼
[前端] 挂载 GraphicsView (同 WSL 流程)
```

### 4.3 QoS 降级流程

```
正常运行 (QualityLevel::Full)
     │
     ├─ [CongestionDetector] egress_queue > 70%
     │      └─ quality_tx.send(Degrade) → QualityLevel::Reduced
     │           └─ inject_set_encodings(Tight, quality=5) → VNC TCP
     │
     ├─ [CongestionDetector] 1s 内丢 12 帧
     │      └─ quality_tx.send(Degrade) → QualityLevel::Low
     │           └─ inject_set_encodings(Tight, quality=2)
     │           └─ downlink 增加 sleep(66ms) 限制 15FPS
     │
     ├─ [SshHealthMonitor] RTT 从 50ms 飙到 120ms (> 2x baseline)
     │      └─ quality_tx.send(EmergencyPause) → QualityLevel::Paused
     │           └─ 停止发送图形帧，VNC TCP 连接保活
     │           └─ 前端显示 "图形已暂停 - 网络拥塞"
     │
     └─ [SshHealthMonitor] RTT 恢复到 60ms (< 1.5x baseline)
            └─ quality_tx.send(Resume) → QualityLevel::Minimal → Low → ...
                 └─ 逐级恢复，每级等 5s 稳定后再升
```

---

## 5. 生命周期与不变量

### 5.1 实体依赖关系（扩展 SYSTEM_INVARIANTS §1.1）

```
Session (SSH 连接生命周期)
  ├─ Channel (shell, PTY)
  │   └─ WsBridge (终端 WebSocket)
  ├─ Channel (direct-tcpip)
  │   └─ Forward (端口转发)
  │       └─ GraphicsBridge (图形 WebSocket) ← 新增：Forward 的消费者
  ├─ SFTP Channel
  └─ WebShell

LocalTerminalSession (本地 PTY)
  └─ GraphicsBridge (WSL 图形) ← 新增：独立于 SSH Session
```

**不变量扩展**：

| # | 不变量 | 说明 |
|---|--------|------|
| G1 | **GraphicsBridge 不持有 Session 强引用** | 通过 `disconnect_rx` broadcast 单向通知 |
| G2 | **GraphicsBridge 断开不影响 Session** | 只标记图形 `Disconnected`，不触发 `connection:update` |
| G3 | **Session 断开必须关闭 GraphicsBridge** | `disconnect_rx` 触发 → 退出 proxy_loop → 清理资源 |
| G4 | **GraphicsBridge 断开不影响 Forward** | 即使 VNC 挂了，SSH 隧道仍然存活 |
| G5 | **QoS 紧急暂停不关闭连接** | VNC TCP 连接保活，仅停止 WebSocket 发送 |
| G6 | **GraphicsBridge 心跳独立于 WsBridge** | 两套独立计时器 + ConnectionState |
| G7 | **WSL 图形不依赖 SSH Session** | WSL VNC 走 localhost，无 SSH 连接 |
| G8 | **远程图形依赖 SSH Session** | 通过 LocalForward 隧道，Session 断开 → Forward 停止 → GraphicsBridge 断开 |

### 5.2 资源清理顺序

```
Session 关闭
  1. 发送 disconnect broadcast
  2. GraphicsBridge 收到 disconnect_rx → 退出 proxy_loop
  3. GraphicsBridge 关闭 WebSocket → 前端 noVNC disconnect
  4. GraphicsBridge 关闭 VNC TCP 连接
  5. Forward 停止 → 本地隧道端口释放
  6. 其他 Channel 清理（与现有相同）

App 退出
  1. GraphicsRegistry.close_all()
     ├─ 每个 GraphicsBridge: control_tx.send(Stop)
     ├─ 等待 proxy_loop 退出（最长 5s）
     └─ WSL VNC server 进程清理
  2. BridgeManager.close_all() (现有)
  3. ForwardingRegistry.stop_all() (现有)
  4. SessionRegistry.disconnect_all() (现有)
  5. LocalTerminalRegistry.close_all() (现有)
```

### 5.3 Key-Driven Reset 规则

| 组件 | Key 格式 | 何时重建 |
|------|---------|---------|
| `GraphicsView` (远程) | `graphics-{sessionId}-{connectionId}` | SSH 重连生成新 connectionId |
| `GraphicsView` (WSL) | `graphics-wsl-{graphicsId}` | 手动重启图形会话 |

---

## 6. 安全约束

### 6.1 GraphicsBridge Token 认证

- 复用现有 `generate_token()` 机制：CSPRNG 32 bytes + timestamp 8 bytes → Base64
- Token 有效期：300 秒（与 WsBridge 一致）
- 常量时间比较（`subtle::ConstantTimeEq`）
- 一次性使用：认证成功后 Token 失效

### 6.2 VNC 安全

- WSL 场景：VNC server 以 `-nopw` 启动（本机环回，无外部暴露）
- 远程场景：VNC 流量全程在 SSH 隧道内（encrypted）
- GraphicsBridge WebSocket 绑定 `localhost:0`（不对外暴露）
- **禁止**将 GraphicsBridge WebSocket 绑定到 `0.0.0.0`

### 6.3 WSL 进程管理

- VNC server 进程必须在图形会话关闭时终止
- 使用 SIGTERM 优雅关闭，5s 超时后 SIGKILL
- 记录 VNC server PID，app 退出时清理孤儿进程

---

## 7. 错误处理

### 7.1 错误分类（扩展 SYSTEM_INVARIANTS §3.1）

**可恢复（仅重连图形通道）**：
- VNC TCP 连接超时
- GraphicsBridge WebSocket 断开
- VNC server 临时无响应

**不可恢复（关闭图形会话）**：
- VNC server 进程退出
- WSL 发行版未安装 VNC 服务
- SSH 隧道关闭（远程场景）
- 用户主动关闭

**不影响图形（忽略）**：
- 单帧解码失败（noVNC 内部处理）
- QoS 帧丢弃（正常降级行为）

### 7.2 错误传播路径

```
底层错误 (std::io::Error / tokio::net::TcpStream error)
  → GraphicsError (graphics/error.rs)
  → Result<T, String> (Tauri command)
  → 前端 Toast / GraphicsStatusOverlay
```

```rust
#[derive(Debug, thiserror::Error)]
pub enum GraphicsError {
    #[error("VNC connection failed: {0}")]
    VncConnectionFailed(String),

    #[error("No VNC server found in WSL distro: {0}")]
    NoVncServer(String),

    #[error("VNC server type not supported: {0}")]
    UnsupportedVncServer(String),

    #[error("WSL not available or distro not found: {0}")]
    WslNotAvailable(String),

    #[error("GraphicsBridge WebSocket bind failed: {0}")]
    WsBindFailed(String),

    #[error("Session not active (state gating): {0}")]
    SessionNotActive(String),

    #[error("Graphics session not found: {0}")]
    SessionNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

---

## 8. i18n 键

新增命名空间 `graphics`，添加到所有 11 种语言：

```json
{
  "graphics": {
    "title": "图形桌面",
    "actions": {
      "start_wsl": "打开 WSL 图形桌面",
      "start_remote": "远程图形桌面",
      "stop": "关闭图形会话",
      "fullscreen": "全屏",
      "screenshot": "截图",
      "set_quality": "画质设置"
    },
    "status": {
      "starting": "正在启动图形服务...",
      "active": "图形连接正常",
      "reconnecting": "图形连接中断，正在重连...",
      "paused": "图形已暂停 (网络拥塞)",
      "disconnected": "图形已断开",
      "error": "图形连接错误"
    },
    "quality": {
      "full": "原始画质",
      "reduced": "标准画质",
      "low": "低画质",
      "minimal": "最低画质",
      "paused": "暂停传输",
      "auto": "自动调节"
    },
    "errors": {
      "no_vnc_server": "WSL 中未安装 VNC 服务。请运行: sudo apt install x11vnc",
      "wsl_not_available": "未检测到 WSL。请确认已安装 WSL2。",
      "vnc_connection_failed": "无法连接到 VNC 服务",
      "session_not_active": "SSH 连接不可用，请先连接",
      "port_conflict": "VNC 端口分配失败，请关闭其他 VNC 客户端后重试",
      "ws_handshake_failed": "图形通道握手失败"
    },
    "clipboard": {
      "synced": "剪贴板已同步",
      "sync_failed": "剪贴板同步失败",
      "permission_denied": "剪贴板权限被拒绝"
    },
    "tooltip": {
      "qos_indicator": "当前带宽: {{bandwidth}}, 丢帧率: {{dropRate}}%"
    }
  }
}
```

---

## 9. 实施计划

### Phase 1: MVP（~2-3 周）

- [ ] `graphics/` 模块骨架 + Feature Gate 六处守卫
- [ ] `GraphicsBridge` 核心：WebSocket ↔ TCP 透传（无 QoS）
- [ ] `wsl.rs`：VNC 探测 + 启动（x11vnc）
- [ ] Tauri 命令：`graphics_start_wsl`, `graphics_stop`, `graphics_list`
- [ ] 前端：`GraphicsView.tsx` + noVNC 集成
- [ ] 类型扩展 + API 层 + Tab 路由
- [ ] 基本 i18n（英文 + 中文）

**MVP 验收标准**：在 Windows + WSL Ubuntu 中，通过 OxideTerm 看到并操作 `xterm` 或 `firefox` 的 GUI 窗口。

### Phase 2: 安全护栏 + 远程支持（~2 周）

- [ ] QoS 令牌桶限速器
- [ ] 拥塞检测器 + `QualityLevel` 状态机
- [ ] 独立心跳 + 图形通道自动重连
- [ ] SSH 心跳 RTT 监控 → 紧急降级
- [ ] `graphics_start_remote`：SSH 隧道 + GraphicsBridge
- [ ] `GraphicsToolbar`：质量手动调节 / 全屏 / 截图
- [ ] 剪贴板文本双向同步（noVNC `clipboard` 事件 + `clipboardPasteFrom`）
- [ ] 完整 i18n（11 种语言）

### Phase 3: 优化与深度集成（后续迭代）

- [ ] 自研 WebGL 渲染器替换 noVNC（减少 JS overhead）
- [ ] 剪贴板图片同步（RFB Extended Clipboard + `tauri-plugin-clipboard-manager`）
- [ ] 多显示器支持
- [ ] 音频转发（PulseAudio over SSH tunnel）
- [ ] SSH X11 原生转发（`HandleCommand::RequestX11Forward`）
- [ ] WSLg 深度集成（无缝窗口模式，单个 GUI 应用嵌入 OxideTerm Tab）

---

## 10. 检查清单（提交前必须验证）

在修改 Graphics Forwarding 相关代码前，确认：

- [ ] Feature Gate：`cargo build` 和 `cargo build --no-default-features` 都通过
- [ ] 不修改 Wire Protocol 帧格式（`MessageType 0x00-0x03` 不变）
- [ ] GraphicsBridge 不持有 Session 强引用
- [ ] GraphicsBridge 断开不触发 `connection:update` 事件
- [ ] Session 断开正确传播到 GraphicsBridge（通过 `disconnect_rx`）
- [ ] QoS 限速不影响终端 WsBridge
- [ ] 心跳独立：GraphicsBridge 心跳超时不影响 Session 状态
- [ ] WebSocket 绑定 `localhost:0`（不暴露到外部网络）
- [ ] VNC server 进程在会话关闭时被清理
- [ ] WSL VNC 端口使用 `find_free_port()` 随机分配，不硬编码 5900
- [ ] Token 一次性使用 + 常量时间比较
- [ ] Token 通过 URL query string 传递（方案 A），`accept_hdr_async` 中验证
- [ ] 远程场景做 State Gating 检查
- [ ] `accept_hdr_async` 正确回应 `Sec-WebSocket-Protocol: binary`（否则 noVNC 静默断开）
- [ ] GraphicsView 使用 Key-Driven Reset（`key={...connectionId}`)
- [ ] 剪贴板同步不在日志中记录内容
- [ ] 新 UI 文本全部使用 i18n
- [ ] `npm run i18n:check` 通过

---

*文档版本: v0.1.0 | 最后更新: 2026-02-06*
