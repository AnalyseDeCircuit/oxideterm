# 远程环境探测器 (Remote Environment Detector)

> **目标**：SSH 连接建立后自动探测远程主机的 OS / 架构 / Shell 类型，注入 AI 上下文，让 Inline Panel 和 Sidebar Chat 给出正确的平台命令。

---

## 1. 问题

| 面板 | 当前做法 | 缺陷 |
|------|---------|------|
| Inline Panel | `navigator.platform` → 本地 OS | SSH 到 CentOS 时 prompt 说 `Current OS: macOS` |
| Sidebar Chat | `guessRemoteOS()` 靠 hostname 模式匹配 | `192.168.1.100` 返回 `null`，多数服务器无法识别 |

## 2. 架构概览

```
SSH 连接 Active
     │
     ▼  (tokio::spawn, ~1s)
┌────────────────────┐
│  env_detector.rs   │  ← 新模块，仿照 profiler.rs
│  open_shell()      │
│  Phase A: 判平台   │  if [ -n "$PSModulePath" ]; ...
│  Phase B: 分支探测 │  Linux: uname + /etc/os-release
│                    │  Windows: PSVersionTable + ver
│  close_shell()     │
└────────┬───────────┘
         │ RemoteEnvInfo
         ▼
  ConnectionEntry.remote_env  ← 缓存
         │
         ├──→ Tauri Event "env:detected"
         │         ↓
         │    appStore.connections[id].remoteEnv
         │
         ├──→ sidebarContextProvider.ts
         │    formatSystemPromptSegment()
         │    "Remote OS: Linux (Ubuntu 22.04)"
         │
         └──→ AiInlinePanel.tsx
              "Remote: Linux x86_64 /bin/bash"
```

## 3. 数据结构

### Rust (`session/env_detector.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEnvInfo {
    /// "Linux" | "macOS" | "Windows" | "FreeBSD" | "Unknown"
    /// 特殊值: "Windows_MinGW" (Git Bash), "Windows_WSL" (WSL 环境)
    pub os_type: String,
    
    /// PRETTY_NAME from /etc/os-release, ver output, sw_vers, etc.
    pub os_version: Option<String>,
    
    /// uname -r
    pub kernel: Option<String>,
    
    /// uname -m or PROCESSOR_ARCHITECTURE
    pub arch: Option<String>,
    
    /// $SHELL or PowerShell version
    pub shell: Option<String>,
    
    /// chrono::Utc::now().timestamp()
    pub detected_at: i64,
}
```

### TypeScript (`types/index.ts`)

```typescript
export type RemoteEnvInfo = {
  osType: string;
  osVersion?: string;
  kernel?: string;
  arch?: string;
  shell?: string;
  detectedAt: number;
};

// 扩展已有类型
export type SshConnectionInfo = {
  // ...existing fields...
  remoteEnv?: RemoteEnvInfo | null;
};
```

## 4. 探测命令设计

### Phase A: 平台判别（原子化，单行安全）

```bash
echo '===DETECT==='; if [ -n "$PSModulePath" ]; then echo 'PLATFORM=windows'; else echo "PLATFORM=$(uname -s 2>/dev/null || echo unknown)"; fi; echo '===END==='
```

**为什么用 `$PSModulePath`**：所有 Windows 环境（PowerShell、cmd+pwsh、OpenSSH Server）都设置此变量，而 Unix 系统不会设置。比 `$PSVersionTable` 更可靠（后者在 cmd 中不可用）。

### Phase B-Unix:

```bash
echo '===ENV==='; uname -s 2>/dev/null; echo '===ARCH==='; uname -m 2>/dev/null; echo '===KERNEL==='; uname -r 2>/dev/null; echo '===SHELL==='; echo $SHELL 2>/dev/null; echo '===DISTRO==='; cat /etc/os-release 2>/dev/null | grep -E '^(PRETTY_NAME|ID)=' | head -2; echo '===END==='
```

### Phase B-Windows (PowerShell):

```powershell
echo '===ENV==='; [System.Environment]::OSVersion.VersionString; echo '===ARCH==='; $env:PROCESSOR_ARCHITECTURE; echo '===SHELL==='; "PowerShell $($PSVersionTable.PSVersion)"; echo '===END==='
```

### "伪装" Windows 识别规则

`uname -s` 返回值中包含以下模式时，识别为 Windows 变体环境：

| uname -s 输出 | 映射 |
|--------------|------|
| `MINGW64_NT-*` / `MINGW32_NT-*` | `Windows_MinGW` ("Git Bash") |
| `MSYS_NT-*` | `Windows_MSYS` |
| `CYGWIN_NT-*` | `Windows_Cygwin` |

AI Prompt 注入示例：`"Remote environment: Windows (MinGW/Git Bash) — paths use /c/Users format"`

## 5. 触发时机

在 `SshConnectionRegistry` 中，`ConnectionEntry` 创建并插入 `connections` DashMap 后：

1. **`connect()`** (直连) — line ~778
2. **`establish_tunneled_connection()`** (隧道连接) — line ~1032

```rust
// 在 self.connections.insert() 之后
self.spawn_env_detection(&connection_id, &entry, app_handle.clone());
```

新方法 `spawn_env_detection()`：
- Clone `entry.handle_controller`
- `tokio::spawn` 异步执行 `detect_remote_env()`
- 结果写入 `entry.remote_env`
- Emit `env:detected` 事件（payload 中包含 connectionId）

## 6. 超时与失败

| 阶段 | 超时 | 失败处理 |
|------|------|---------|
| Channel 打开 | 5s | `os_type = "Unknown"`, warn 日志 |
| Phase A 输出等待 | 3s | 同上 |
| Phase B 输出等待 | 5s | 只填 `os_type`，其余 `None` |
| 总探测 | 8s | 强制终止，标记 Unknown |

失败时仍然 emit 事件，payload `os_type = "Unknown"`，前端 AI prompt 注入：
```
- Remote OS: Unknown (detection failed, provide platform-agnostic commands when possible)
```

## 7. 异步竞态处理

用户可能在探测完成前就打开 AI 面板。

### 前端策略

`SshConnectionInfo.remoteEnv` 有三种状态：
- `undefined` — 探测尚未开始/进行中
- `null` — 无 SSH 连接（本地终端）
- `RemoteEnvInfo` — 探测完成

**AiInlinePanel** 和 **sidebarContextProvider** 注入逻辑：

```typescript
if (remoteEnv === undefined) {
  // 探测进行中
  prompt += '- Remote OS: [detecting...] (provide platform-agnostic commands)\n';
} else if (remoteEnv === null) {
  // 本地终端
  prompt += `- Terminal: Local (${localOS})\n`;
} else {
  // 探测完成
  prompt += `- Remote OS: ${remoteEnv.osType}\n`;
}
```

### Rust 侧

`ConnectionEntry.remote_env` 类型：`RwLock<Option<RemoteEnvInfo>>`
- 初始值 `None`（探测中）
- 探测完成写入 `Some(info)`
- `to_info()` 序列化时包含此字段

## 8. 前端改动清单

### `sidebarContextProvider.ts`

1. `EnvironmentSnapshot` 增加 `remoteEnv?: RemoteEnvInfo`（替代 `remoteOSHint: string | null`）
2. `gatherSidebarContext()` 从 `appStore.connections[id]` 读取 `remoteEnv`
3. `formatSystemPromptSegment()` 输出详细环境信息
4. 保留 `guessRemoteOS()` 作为 `remoteEnv === undefined` 时的 fallback
5. 探测中标注 `[detecting...]`

### `AiInlinePanel.tsx`

1. 新增 props: `sessionId?: string`
2. 通过 `sessionId` → `appStore.sessions[id].connectionId` → `appStore.connections[cid].remoteEnv` 获取环境
3. System prompt 从 `"Current OS: macOS"` 改为：
   - SSH + 已探测: `"Remote environment: Linux (Ubuntu 22.04), arch: x86_64, shell: /bin/bash"`
   - SSH + 探测中: `"Remote environment: detecting... (provide platform-agnostic commands)"`
   - SSH + 失败: `"Remote environment: unknown (provide platform-agnostic commands when possible)"`
   - 本地终端: `"Local OS: macOS"`

### `TerminalView.tsx` / `LocalTerminalView.tsx`

传入 `sessionId` prop 给 `AiInlinePanel`。

### `useConnectionEvents.ts`

新增监听 `env:detected` 事件，更新 `appStore.connections[id].remoteEnv`。

### `api.ts`

新增 `getRemoteEnv(connectionId: string): Promise<RemoteEnvInfo | null>`。

### `types/index.ts`

新增 `RemoteEnvInfo` 类型，扩展 `SshConnectionInfo`。

## 9. 文件清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `src-tauri/src/session/env_detector.rs` | **新建** | 核心探测逻辑 |
| `src-tauri/src/session/mod.rs` | 修改 | `pub mod env_detector;` |
| `src-tauri/src/ssh/connection_registry.rs` | 修改 | 新字段 + `spawn_env_detection()` + `to_info()` 扩展 |
| `src-tauri/src/commands/ssh.rs` | 修改 | 新增 `get_remote_env` 命令 |
| `src-tauri/src/lib.rs` | 修改 | 注册新命令 |
| `src/types/index.ts` | 修改 | `RemoteEnvInfo` + `SshConnectionInfo` 扩展 |
| `src/lib/api.ts` | 修改 | `getRemoteEnv()` wrapper |
| `src/lib/sidebarContextProvider.ts` | 修改 | 替换 `guessRemoteOS()`，增强 prompt |
| `src/components/terminal/AiInlinePanel.tsx` | 修改 | 新增 `sessionId` prop，OS 感知 |
| `src/components/terminal/TerminalView.tsx` | 修改 | 传 `sessionId` |
| `src/components/terminal/LocalTerminalView.tsx` | 修改 | 传标记 |
| `src/hooks/useConnectionEvents.ts` | 修改 | 监听 `env:detected` |

## 10. 测试矩阵

| 场景 | 预期 |
|------|------|
| SSH → Ubuntu 22.04 | `Linux / Ubuntu 22.04.3 LTS / x86_64 / /bin/bash` |
| SSH → macOS | `macOS / sw_vers 输出 / arm64 / /bin/zsh` |
| SSH → Windows (OpenSSH + PowerShell) | `Windows / Microsoft Windows NT 10.0 / AMD64 / PowerShell 7.x` |
| SSH → Windows (Git Bash) | `Windows_MinGW / MINGW64_NT... / x86_64 / /usr/bin/bash` |
| SSH → 路由器 (受限 shell) | `Unknown` + warn 日志 |
| SSH → 隧道连接 | 隧道目标的环境，非跳板机 |
| 本地终端 | 不触发探测，使用 `navigator.platform` |
| 连接后 0.1s 打开 AI | `[detecting...]` → 稍后自动更新为实际结果 |
| 探测超时 | `Unknown` + 标注 |

## 11. 安全考量

- 探测命令均为**只读操作**（`uname`, `cat`, `echo`），无副作用
- **不写入任何文件**到远程主机
- Shell channel 探测完即关闭，**不持久占用** `MaxSessions` 配额
- 探测结果**不包含敏感信息**（无密码、无密钥）
- 结果仅缓存在内存中，**不持久化到磁盘**
