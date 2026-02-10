# OxideTerm WSLg 图形回传 (WSL Graphics)

> **✅ 已实现** — 内置组件，非插件
>
> 后端: `src-tauri/src/graphics/` (mod.rs, wsl.rs, bridge.rs, commands.rs)
> 前端: `src/components/graphics/GraphicsView.tsx` (内置 Tab 组件)
> Feature gate: `wsl-graphics` (Cargo.toml default features)

> **版本**: v0.3.0
> **日期**: 2026-02
> **状态**: 已实现
> **前置依赖**: SYSTEM_INVARIANTS.md v1.4.0

---

## 0. 范围与目标

### 0.1 目标

在 OxideTerm 中嵌入显示 Windows WSL 的 GUI 应用/桌面，实现"终端 + 图形"统一工作区。

### 0.2 范围限定

| 在范围内 | 不在范围内 |
|----------|------------|
| Windows + WSLg 本地图形回传 | 远程 SSH 图形转发 |
| VNC → noVNC 渲染 | RDP / Wayland 原生协议 |
| 单 WSL 发行版单会话 | 多显示器 / 多桌面 |
| 文本剪贴板（WSLg 内置） | 图片剪贴板同步 |
| 基本工具栏（全屏/重连） | 音频转发 |

### 0.3 交付形态

**双层架构**：

| 层 | 形态 | 说明 |
|----|------|------|
| 后端 | Feature-gated Rust 模块 | `#[cfg(all(feature = "wsl-graphics", target_os = "windows"))]`，提供 5 个 Tauri 命令 |
| 前端 | 内置 React 组件 | `GraphicsView.tsx`，通过 `invoke()` 调用后端，注册为 Tab 视图 |

---

## 1. 为什么原方案 90% 不需要

原 v0.1.0 设计了 QoS / 自适应质量 / 独立心跳 / SSH 隧道 / 剪贴板同步等机制。缩减到 WSLg-only 后，绝大多数可以删除：

| 原设计模块 | 行数 | 是否保留 | 原因 |
|-----------|------|---------|------|
| QoS 令牌桶限速 | ~120 | ❌ | localhost 回环带宽 > 10 GB/s，无需限速 |
| 自适应质量 5 级状态机 | ~100 | ❌ | localhost 无拥塞，始终全质量 |
| 拥塞检测器 + SSH 心跳保护 | ~80 | ❌ | 无 SSH 连接 |
| 独立心跳系统 | ~60 | ❌ | noVNC 内置 WebSocket 重连 |
| 帧丢弃策略 | ~40 | ❌ | localhost 无丢帧 |
| 剪贴板双向同步 | ~80 | ❌ | WSLg 已内置 Windows ↔ WSL 剪贴板同步 |
| SSH LocalForward 隧道 | ~60 | ❌ | 无远程场景 |
| GraphicsRegistry (复杂) | ~100 | 简化为 HashMap | 单机单会话，无需 control channel |
| GraphicsBridge (复杂) | ~200 | 简化为 ~80 行 | 纯透传，无 QoS/心跳/帧解析 |
| **总计削减** | **~840** | | **最终目标 ~400 行 Rust + ~200 行 Plugin JS** |

---

## 2. 架构

```
Windows Host
┌─────────────────────────────────────────────────────────────┐
│ OxideTerm (Tauri)                                           │
│                                                             │
│  Rust Backend [feature: wsl-graphics]                       │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ commands.rs: 5 个 Tauri 命令 (list/start/stop/reconnect/ls)│    │
│  │ wsl.rs: WSL 发行版检测 + Xtigervnc + 桌面启动 + 会话清理  │    │
│  │ bridge.rs: WebSocket ↔ VNC TCP 透传代理 (支持重连)      │    │
│  └──────────────────────┬──────────────────────────────┘    │
│                       │ ws://127.0.0.1:{port}?token=xxx     │
│  Built-in Component  │                                     │
│  ┌────────────────────▼────────────────────────────────┐    │
│  │ GraphicsView.tsx (内置 Tab 组件)                      │    │
│  │ ┌─────────────────────┐ ┌─────────────────────────┐ │    │
│  │ │ noVNC (RFB Canvas)  │ │ Toolbar (全屏/重连/停止) │ │    │
│  │ └─────────────────────┘ └─────────────────────────┘ │    │
│  └──────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
        │ Xtigervnc on :10+ (standalone X server)
        │ Desktop via bootstrap script (D-Bus + XDG)
        ▼
┌─────────────────────┐
│ WSL (Ubuntu)        │
│  Xtigervnc :10      │
│  └─ xfce4-session   │
│     (D-Bus session) │
└─────────────────────┘
```

**数据流**：

```
WSL Xtigervnc :10 ─TCP─▶ Rust Bridge ─WebSocket─▶ noVNC (GraphicsView) ─Canvas─▶ 用户看到 GUI
                   (localhost)              (localhost)
```

**与现有系统零耦合**：
- 不修改 Wire Protocol（`MessageType 0x00-0x03` 不变）
- 不复用 WsBridge 代码路径
- 不影响终端数据平面
- 不依赖 SSH Session / ConnectionRegistry

---

## 3. 后端实现 (Rust)

### 3.1 模块结构

```
src-tauri/src/
├── graphics/                    # feature: wsl-graphics
│   ├── mod.rs                   # 类型定义 + 状态 + 条件编译 + stub 命令
│   ├── bridge.rs                # WebSocket ↔ VNC TCP 透传 (支持 reconnect)
│   ├── wsl.rs                   # WSL 检测 + Xtigervnc + D-Bus + 桌面启动脚本 + 会话清理
│   └── commands.rs              # 5 个 Tauri 命令 (list/start/stop/reconnect/ls)
```

### 3.2 Feature Gate

```toml
# src-tauri/Cargo.toml
[features]
default = ["local-terminal"]
local-terminal = ["dep:portable-pty"]
wsl-graphics = []  # 无新依赖（复用 tokio-tungstenite）
```

六处 `#[cfg]` 守卫（与 `local-terminal` 的 8 处同级）：

| # | 位置 | 守卫 |
|---|------|------|
| 1 | `lib.rs` 模块声明 | `#[cfg(all(feature = "wsl-graphics", target_os = "windows"))]` |
| 2 | `lib.rs` 状态初始化 | `WslGraphicsState::new()` |
| 3 | `lib.rs` 状态管理 | `builder.manage(graphics_state)` |
| 4 | `lib.rs` 命令注册 | 追加进 `invoke_handler`（见下方策略） |
| 5 | `lib.rs` 退出清理 | `graphics_state.shutdown().await` |
| 6 | `commands/mod.rs` | `pub mod graphics; pub use graphics::*;` |

#### 命令注册策略：避免分支爆炸

当前 `local-terminal` 已使用两套完整 `generate_handler!` 列表（with/without），约 250 行×2。
如果直接为 `wsl-graphics` 再加条件分支，会形成 2×2=4 个排列，约 1000 行重复代码。

**推荐方案**：将 wsl-graphics 的 4 个命令追加到现有两个分支的尾部（+4 行/分支），
用 `#[cfg(not(all(feature = "wsl-graphics", target_os = "windows")))]` 提供 stub 空实现，
使命令在非 Windows 平台编译时注册为无操作（返回 Err），避免分支翻倍：

```rust
// commands/graphics.rs (非 Windows 平台的 stub)
#[cfg(not(all(feature = "wsl-graphics", target_os = "windows")))]
pub mod graphics {
    #[tauri::command]
    pub async fn wsl_graphics_list_distros() -> Result<Vec<()>, String> {
        Err("WSL Graphics not available on this platform".into())
    }
    // ... 其余 3 个 stub
}
```

这样两个现有分支都无条件包含这 4 个命令名，无需新增 cfg 分支。

### 3.3 WebSocket ↔ TCP 代理 (bridge.rs)

极简透传，不解析 VNC RFB 协议内容：

```rust
use tokio::net::TcpStream;
use tokio_tungstenite::accept_hdr_async;
use futures_util::{SinkExt, StreamExt};

pub struct WslGraphicsBridge;

impl WslGraphicsBridge {
    /// 启动代理：绑定 localhost 随机端口，返回 (ws_port, token)
    pub async fn start(
        vnc_addr: &str,     // e.g. "localhost:59371"
    ) -> Result<(u16, String, JoinHandle<()>), GraphicsError> {
        let vnc_target = vnc_addr.to_string();
        let token = generate_token();
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let ws_port = listener.local_addr()?.port();

        let expected_token = token.clone();
        let handle = tokio::spawn(async move {
            // 接受单个 WebSocket 连接
            if let Ok((stream, _)) = listener.accept().await {
                if let Err(e) = Self::proxy(stream, &vnc_target, &expected_token).await {
                    tracing::warn!("Graphics proxy error: {}", e);
                }
            }
        });

        Ok((ws_port, token, handle))
    }

    async fn proxy(
        ws_stream: TcpStream,
        vnc_addr: &str,
        expected_token: &str,
    ) -> Result<(), GraphicsError> {
        // 1. WebSocket 握手（处理 noVNC 的 Sec-WebSocket-Protocol: binary）
        //    同时从 URL query string 验证 token
        let ws = accept_hdr_async(ws_stream, |req: &Request, mut resp: Response| {
            // 验证 token
            let uri = req.uri().to_string();
            let valid = uri.contains(&format!("token={}", expected_token));
            if !valid {
                *resp.status_mut() = StatusCode::FORBIDDEN;
                return Err(Response::from(resp));
            }
            // 回应 Sec-WebSocket-Protocol: binary（noVNC 必需，否则静默断开）
            if let Some(protocols) = req.headers().get("Sec-WebSocket-Protocol") {
                if protocols.to_str().unwrap_or("").contains("binary") {
                    resp.headers_mut().insert(
                        "Sec-WebSocket-Protocol",
                        "binary".parse().unwrap(),
                    );
                }
            }
            Ok(resp)
        }).await?;

        // 2. 连接 VNC TCP
        let vnc = TcpStream::connect(vnc_addr).await?;
        let (vnc_read, vnc_write) = tokio::io::split(vnc);
        let (mut ws_tx, mut ws_rx) = ws.split();

        // 3. 双向透传（两个 task，任一结束则全部退出）
        tokio::select! {
            // VNC → WebSocket
            r = async {
                let mut reader = BufReader::new(vnc_read);
                let mut buf = vec![0u8; 65536];
                loop {
                    let n = reader.read(&mut buf).await?;
                    if n == 0 { break; }
                    ws_tx.send(Message::Binary(buf[..n].to_vec().into())).await?;
                }
                Ok::<_, GraphicsError>(())
            } => { r }
            // WebSocket → VNC
            r = async {
                let mut writer = vnc_write;
                while let Some(msg) = ws_rx.next().await {
                    match msg? {
                        Message::Binary(data) => {
                            writer.write_all(&data).await?;
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
                Ok::<_, GraphicsError>(())
            } => { r }
        }
    }
}
```

**关键**：`accept_hdr_async` 而非 `accept_async` — noVNC 发送 `Sec-WebSocket-Protocol: binary`，不回应此头则浏览器**静默断开**无任何错误。这是最容易踩的坑。

### 3.4 WSL 管理 (wsl.rs)

```rust
/// WSL 发行版信息
pub struct WslDistro {
    pub name: String,
    pub is_default: bool,
    pub is_running: bool,
}

/// 列出 WSL 发行版
pub async fn list_distros() -> Result<Vec<WslDistro>, GraphicsError> {
    // wsl.exe --list --verbose → 解析输出
    // ⚠️ 注意 UTF-16LE BOM 编码（某些 Windows 版本）
}

/// 探测 WSL 中可用的 VNC 服务（优先级：wayvnc > x11vnc > tigervnc）
pub async fn detect_vnc(distro: &str) -> Result<String, GraphicsError> {
    let checks = ["wayvnc", "x11vnc", "Xtigervnc"];
    for binary in &checks {
        let output = Command::new("wsl.exe")
            .args(["-d", distro, "--", "which", binary])
            .output().await?;
        if output.status.success() {
            return Ok(binary.to_string());
        }
    }
    Err(GraphicsError::NoVncServer(distro.to_string()))
}

/// 在 WSL 中启动 VNC 服务器
///
/// ⚠️ 绝对不硬编码 5900/5901 端口！使用 find_free_port() 让 OS 分配。
pub async fn start_vnc(
    distro: &str,
    vnc_binary: &str,
) -> Result<(u16, Child), GraphicsError> {
    let port = find_free_port().await?;

    let child = match vnc_binary {
        "x11vnc" => {
            Command::new("wsl.exe")
                .args(["-d", distro, "--",
                    "x11vnc", "-display", ":0",
                    "-rfbport", &port.to_string(),
                    "-nopw", "-forever", "-shared"])
                .spawn()?
        }
        "wayvnc" => {
            Command::new("wsl.exe")
                .args(["-d", distro, "--",
                    "wayvnc", "--output=HEADLESS-1",
                    "0.0.0.0", &port.to_string()])
                .spawn()?
        }
        _ => return Err(GraphicsError::UnsupportedVnc(vnc_binary.to_string())),
    };

    // 等待 VNC 就绪（轮询 RFB 握手首 12 字节 "RFB 003.0xx\n"）
    wait_for_vnc_ready(&format!("localhost:{}", port), Duration::from_secs(10)).await?;

    Ok((port, child))
}

/// 查找可用端口（bind :0 → 读取分配的端口 → 释放）
async fn find_free_port() -> Result<u16, GraphicsError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}
```

### 3.5 Tauri 命令 (commands.rs)

仅暴露 4 个命令，插件通过 `ctx.api.invoke()` 调用：

```rust
/// 列出 WSL 发行版
#[tauri::command]
pub async fn wsl_graphics_list_distros() -> Result<Vec<WslDistro>, String>

/// 启动图形会话
/// 返回 { id, wsPort, wsToken, distro, vncServer }
#[tauri::command]
pub async fn wsl_graphics_start(
    state: State<'_, Arc<WslGraphicsState>>,
    distro: String,
) -> Result<WslGraphicsSession, String>

/// 停止图形会话
#[tauri::command]
pub async fn wsl_graphics_stop(
    state: State<'_, Arc<WslGraphicsState>>,
    session_id: String,
) -> Result<(), String>

/// 列出活跃图形会话
#[tauri::command]
pub async fn wsl_graphics_list_sessions(
    state: State<'_, Arc<WslGraphicsState>>,
) -> Result<Vec<WslGraphicsSession>, String>
```

### 3.6 全局状态

```rust
pub struct WslGraphicsState {
    /// 活跃会话：session_id → (VNC 子进程, Bridge JoinHandle, 会话信息)
    sessions: RwLock<HashMap<String, WslGraphicsHandle>>,
}

struct WslGraphicsHandle {
    info: WslGraphicsSession,
    vnc_child: Child,               // WSL 中的 VNC 进程
    bridge_handle: JoinHandle<()>,   // WebSocket↔TCP 代理 task
}

impl WslGraphicsState {
    /// App 退出时清理所有会话
    pub async fn shutdown(&self) {
        let mut sessions = self.sessions.write().await;
        for (_, handle) in sessions.drain() {
            handle.bridge_handle.abort();
            // SIGTERM → 5s 超时 → SIGKILL
            let _ = handle.vnc_child.kill(); // wsl.exe 子进程
        }
    }
}
```

---

## 4. 前端实现 (OxideTerm Plugin)

### 4.1 插件结构

```
~/.oxideterm/plugins/com.oxideterm.wsl-graphics/
├── plugin.json
├── index.js          # ESM bundle（含 noVNC RFB 类）
├── icon.svg
└── locales/
    ├── en.json
    └── zh-CN.json
```

### 4.2 plugin.json

```json
{
  "id": "com.oxideterm.wsl-graphics",
  "name": "WSL Graphics",
  "version": "1.0.0",
  "description": "View WSL GUI applications inside OxideTerm",
  "author": "OxideTerm",
  "main": "./index.js",
  "engines": { "oxideterm": ">=1.8.0" },
  "contributes": {
    "tabs": [{
      "id": "wsl-graphics",
      "title": "plugin.wsl_graphics.tab_title",
      "icon": "Monitor"
    }],
    "apiCommands": [
      "wsl_graphics_list_distros",
      "wsl_graphics_start",
      "wsl_graphics_stop",
      "wsl_graphics_list_sessions"
    ]
  }
}
```

### 4.3 核心组件 (index.js 内)

```javascript
// 使用宿主共享的 React
const { React, ReactDOM } = window.__OXIDE__;
const { useState, useEffect, useRef, useCallback } = React;

// noVNC 的 RFB 类打包进 bundle
import RFB from '@novnc/novnc/core/rfb.js';

/**
 * WSL Graphics Tab 主组件
 */
function GraphicsTab({ ctx }) {
  const canvasRef = useRef(null);
  const rfbRef = useRef(null);
  const [session, setSession] = useState(null);
  const [status, setStatus] = useState('idle'); // idle | starting | active | error
  const [distros, setDistros] = useState([]);
  const [error, setError] = useState(null);

  // 加载 WSL 发行版列表
  useEffect(() => {
    ctx.api.invoke('wsl_graphics_list_distros')
      .then(setDistros)
      .catch(e => setError(String(e)));
  }, []);

  // 启动图形会话
  const startSession = useCallback(async (distro) => {
    setStatus('starting');
    setError(null);
    try {
      const sess = await ctx.api.invoke('wsl_graphics_start', { distro });
      setSession(sess);
      setStatus('active');
    } catch (e) {
      setError(String(e));
      setStatus('error');
    }
  }, []);

  // 连接 noVNC
  useEffect(() => {
    if (!session || !canvasRef.current) return;

    const url = `ws://127.0.0.1:${session.wsPort}?token=${session.wsToken}`;
    const rfb = new RFB(canvasRef.current, url, {
      wsProtocols: ['binary'],
    });
    rfb.scaleViewport = true;
    rfb.resizeSession = true;
    rfbRef.current = rfb;

    rfb.addEventListener('disconnect', () => setStatus('disconnected'));
    rfb.addEventListener('connect', () => setStatus('active'));

    return () => {
      rfb.disconnect();
      rfbRef.current = null;
    };
  }, [session]);

  // 停止会话
  const stopSession = useCallback(async () => {
    if (session) {
      await ctx.api.invoke('wsl_graphics_stop', { sessionId: session.id });
      setSession(null);
      setStatus('idle');
    }
  }, [session]);

  // 渲染
  if (status === 'idle' || status === 'error') {
    return DistroSelector({ distros, onSelect: startSession, error });
  }

  return React.createElement('div', {
    style: { position: 'relative', width: '100%', height: '100%', background: '#000' }
  },
    React.createElement('div', { ref: canvasRef, style: { width: '100%', height: '100%' } }),
    Toolbar({ onFullscreen, onReconnect, onStop: stopSession, status }),
  );
}

// 插件入口
export function activate(ctx) {
  ctx.ui.registerTabView('wsl-graphics', (props) =>
    React.createElement(GraphicsTab, { ...props, ctx })
  );
}

export function deactivate() {
  // noVNC RFB 实例在组件卸载时已通过 useEffect cleanup 销毁
}
```

### 4.4 构建插件

```bash
# 使用 esbuild 打包（含 noVNC）
npx esbuild src/main.js --bundle --format=esm \
  --external:react --external:react-dom \
  --outfile=dist/index.js --minify

# noVNC (~150KB minified) 会被内联到 bundle 中
```

**注意**：`react` 和 `react-dom` 标记为 external，运行时从 `window.__OXIDE__` 获取。

---

## 5. 生命周期

### 5.1 启动流程

```
用户操作: 打开 WSL Graphics Tab
  │
  ├─ 1. 插件调用 wsl_graphics_list_distros → 显示发行版列表
  ├─ 2. 用户选择 "Ubuntu" → 插件调用 wsl_graphics_start("Ubuntu")
  │     │
  │     ├─ 后端: detect_vnc("Ubuntu") → 发现 x11vnc
  │     ├─ 后端: start_vnc("Ubuntu", "x11vnc") → port 59371 (随机)
  │     ├─ 后端: WslGraphicsBridge::start("localhost:59371")
  │     │        → ws_port 49832, token "abc..."
  │     └─ 返回 { id, wsPort: 49832, wsToken: "abc...", distro: "Ubuntu" }
  │
  ├─ 3. 插件创建 noVNC RFB → ws://127.0.0.1:49832?token=abc...
  ├─ 4. WebSocket 握手 → Token 验证 → VNC RFB 握手（透传）
  └─ 5. Canvas 开始渲染 WSL 桌面
```

### 5.2 关闭流程

```
用户操作: 关闭 Tab 或点击停止
  │
  ├─ 1. 插件 useEffect cleanup → rfb.disconnect()
  ├─ 2. 插件调用 wsl_graphics_stop(session_id)
  │     │
  │     ├─ 后端: bridge_handle.abort() → WebSocket 代理 task 终止
  │     ├─ 后端: vnc_child.kill() → SIGTERM VNC 进程
  │     └─ 后端: sessions.remove(session_id)
  └─ 3. 完成
```

### 5.3 App 退出清理

```rust
// lib.rs 退出 hook
#[cfg(all(feature = "wsl-graphics", target_os = "windows"))]
graphics_state.shutdown().await;
// → 遍历所有活跃会话 → abort bridge → kill VNC 子进程
```

### 5.4 异常场景

| 场景 | 行为 |
|------|------|
| VNC 进程崩溃 | Bridge TCP read 返回 EOF → WebSocket close → noVNC `disconnect` 事件 → 插件显示重连按钮 |
| WSL 关闭 | 同上 |
| 用户快速开关 | `wsl_graphics_stop` 幂等，先 kill 旧进程再启新的 |
| VNC 未安装 | `detect_vnc` 返回 `NoVncServer` 错误 → 插件提示安装命令 |

---

## 6. 安全约束

1. **WebSocket 绑定 `127.0.0.1:0`**：不暴露到外部网络
2. **Token 一次性使用**：CSPRNG 32 bytes + Base64，常量时间比较
3. **Token 通过 URL query string 传递**：在 `accept_hdr_async` 中验证
4. **VNC `-nopw` 模式**：本机回环，无外部暴露
5. **VNC 端口随机分配**：`find_free_port()`，不硬编码 5900

---

## 7. i18n

插件自带 `locales/` 目录，通过 `ctx.i18n.t()` 使用：

```json
{
  "tab_title": "WSL Graphics",
  "select_distro": "Select WSL Distribution",
  "starting": "Starting VNC server...",
  "no_distros": "No WSL distributions found",
  "no_vnc": "No VNC server installed. Run: sudo apt install x11vnc",
  "reconnect": "Reconnect",
  "fullscreen": "Fullscreen",
  "stop": "Stop",
  "error": "Graphics Error"
}
```

---

## 8. 施工陷阱清单

| # | 陷阱 | 说明 |
|---|------|------|
| 1 | **noVNC 静默断开** | `accept_hdr_async` 必须回应 `Sec-WebSocket-Protocol: binary`，否则浏览器静默关闭 WebSocket，无任何错误 |
| 2 | **VNC 端口冲突** | 不硬编码 5900/5901，WSLg/RealVNC/TigerVNC 可能占用。始终 `find_free_port()` |
| 3 | **wsl.exe 输出编码** | `wsl --list --verbose` 在某些 Windows 版本输出 UTF-16LE 带 BOM，需要正确解码 |
| 4 | **WSLg DISPLAY 变量** | WSLg 下 `DISPLAY` 可能是 `:0` 或 `:0.0` 或通过 `WAYLAND_DISPLAY`，需适配 |
| 5 | **VNC 启动竞态** | `find_free_port()` 存在 TOCTOU，端口可能被抢。通过 `wait_for_vnc_ready()` 超时重试处理 |
| 6 | **React 实例共享** | 插件必须使用 `window.__OXIDE__.React`，不能 bundle 自己的 React（会导致 hooks 崩溃） |

---

## 9. 实施计划 (~1 周)

### Phase 1: 后端骨架 (2 天)

- [ ] `graphics/mod.rs` + Feature Gate 四处守卫
- [ ] `wsl.rs`: `list_distros()` + `detect_vnc()` + `start_vnc()`
- [ ] `bridge.rs`: WebSocket ↔ TCP 透传 + Token 验证 + 子协议处理
- [ ] `commands.rs`: 4 个 Tauri 命令
- [ ] `WslGraphicsState`: 会话注册 + `shutdown()` 清理

### Phase 2: 前端插件 (2 天)

- [ ] `plugin.json` 清单
- [ ] `GraphicsTab` 组件：发行版选择 → 启动 → noVNC 渲染
- [ ] `Toolbar`: 全屏 / 重连 / 停止
- [ ] 状态叠加层：启动中 / 断开 / 错误提示
- [ ] esbuild 打包脚本

### Phase 3: 联调 + i18n (1 天)

- [ ] Windows + WSL Ubuntu 端到端验证
- [ ] `x11vnc` + `wayvnc` 两种 VNC 服务器测试
- [ ] 英文 + 中文 i18n
- [ ] App 退出清理验证

### 验收标准

在 Windows 11 + WSL Ubuntu 中，通过 OxideTerm 的 WSL Graphics Tab 看到并操作 `xterm` 或 `firefox` 的 GUI 窗口。

---

## 10. 提交前检查清单

- [ ] `cargo build --features wsl-graphics` 通过（Windows）
- [ ] `cargo build` 通过（不含 wsl-graphics 的默认编译）
- [ ] `cargo build --no-default-features` 通过
- [ ] 不修改 Wire Protocol 帧格式（`0x00-0x03` 不变）
- [ ] 不复用 WsBridge 代码路径
- [ ] WebSocket 绑定 `127.0.0.1:0`
- [ ] VNC 端口使用 `find_free_port()` 随机分配
- [ ] Token 一次性 + 常量时间比较
- [ ] `accept_hdr_async` 正确回应 `Sec-WebSocket-Protocol: binary`
- [ ] VNC 子进程在会话关闭/App 退出时被清理
- [ ] 插件使用 `window.__OXIDE__.React`（不自带 React）
- [ ] 新 UI 文本使用 `ctx.i18n.t()`

---

## 附录：后续扩展（不在当前范围）

| 扩展方向 | 复杂度 | 说明 |
|----------|--------|------|
| 远程 SSH 图形 | 高 | 需复用 LocalForward 隧道 + State Gating + QoS |
| 图片剪贴板 | 中 | RFB Extended Clipboard + tauri-plugin-clipboard-manager |
| 音频转发 | 高 | PulseAudio over SSH tunnel |
| 自研 WebGL 渲染 | 高 | 替换 noVNC 的 Canvas 渲染，减少 JS 开销 |
| 多显示器 | 中 | 多个 VNC 连接 + Tab/Pane 管理 |

---

*文档版本: v0.2.0 | 最后更新: 2026-02-10*
