# 本地终端 - 原生 Shell 集成

> 无需 SSH 连接，直接在本地机器上运行终端会话，支持多 Shell 和跨平台。

## 🎯 核心功能

本地终端允许您在 OxideTerm 中直接访问本地机器的 Shell，就像使用 iTerm2、Windows Terminal 或 GNOME Terminal 一样。

**与 SSH 终端的对比**：

| 特性 | 本地终端 | SSH 终端 |
|------|---------|---------|
| **连接方式** | 本地 PTY | SSH 协议 |
| **延迟** | 0ms | 取决于网络 |
| **认证** | 无需 | 需要密码/密钥 |
| **使用场景** | 本地开发、脚本执行 | 远程服务器管理 |
| **支持 Shell** | 所有本地 Shell | 远程服务器 Shell |

---

## 🚀 快速开始

### 创建本地终端

#### 方法 1：快捷键

**Windows/Linux**: `Ctrl+Shift+N`  
**macOS**: `⌘+Shift+N`

#### 方法 2：侧边栏

1. 展开左侧边栏
2. 切换到 **Connections** 标签
3. 点击 **Local Terminal** 按钮

#### 方法 3：菜单

顶部菜单 → File → New Local Terminal

---

## 🎨 Shell 选择

OxideTerm 会自动扫描您系统上可用的 Shell，并允许您选择使用哪一个。

### Windows 支持的 Shell

| Shell | 路径 | 优先级 |
|-------|------|--------|
| **PowerShell 7+** | `C:\Program Files\PowerShell\7\pwsh.exe` | ⭐⭐⭐⭐⭐ |
| **PowerShell 5.1** | `C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe` | ⭐⭐⭐⭐ |
| **Git Bash** | `C:\Program Files\Git\bin\bash.exe` | ⭐⭐⭐⭐ |
| **WSL** | `C:\Windows\System32\wsl.exe` | ⭐⭐⭐⭐⭐ |
| **Command Prompt** | `C:\Windows\System32\cmd.exe` | ⭐⭐⭐ |

**推荐**：PowerShell 7 或 WSL（Ubuntu）

### 🪟 Windows 终端增强功能 (v1.4.0+)

OxideTerm v1.4.0 引入了多项 Windows 终端增强功能：

#### 1. 自动 UTF-8 编码初始化

启用 **Oh My Posh** 后，PowerShell 会自动执行以下初始化：

```powershell
# 自动注入的初始化脚本
[Console]::InputEncoding = [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
```

**效果**：
- ✅ 中文、日文、韩文正确显示
- ✅ Emoji 正确渲染（🎉 🚀 ✅）
- ✅ Nerd Font 图标正确显示（  ）

#### 2. Oh My Posh 自动初始化

启用后，OxideTerm 会自动执行 Oh My Posh 初始化：

```powershell
# 自动注入（如果检测到 oh-my-posh 命令）
oh-my-posh init pwsh --config 'C:\Users\你的用户名\.poshthemes\主题.omp.json' | Invoke-Expression
```

**前提条件**：
1. 安装 Oh My Posh：`winget install JanDeDobbeleer.OhMyPosh`
2. 安装 Nerd Font 字体：[Nerd Fonts](https://www.nerdfonts.com/)
3. 在 OxideTerm 设置中选择 Nerd Font

#### 3. WSL 环境变量传递增强

WSL 发行版会自动接收以下环境变量：

| 变量 | 值 | 用途 |
|------|----|----|
| `TERM` | `xterm-256color` | 终端类型 |
| `COLORTERM` | `truecolor` | 真彩色支持 |
| `TERM_PROGRAM` | `OxideTerm` | 终端程序标识 |
| `TERM_PROGRAM_VERSION` | `1.4.0` | 版本号 |
| `POSH_THEME` | 用户配置路径 | Oh My Posh 主题（自动转换 Windows 路径） |

**配置方式**：设置 → 本地终端 → 启用 Oh My Posh

### macOS 支持的 Shell

| Shell | 路径 | 优先级 |
|-------|------|--------|
| **Zsh** | `/bin/zsh` | ⭐⭐⭐⭐⭐（默认） |
| **Bash** | `/bin/bash` | ⭐⭐⭐⭐ |
| **Fish** | `/usr/local/bin/fish` 或 `/opt/homebrew/bin/fish` | ⭐⭐⭐⭐⭐ |
| **Nushell** | `/usr/local/bin/nu` 或 `/opt/homebrew/bin/nu` | ⭐⭐⭐⭐ |

**推荐**：Zsh（系统默认） 或 Fish（现代 Shell）

### Linux 支持的 Shell

| Shell | 路径 | 优先级 |
|-------|------|--------|
| **Bash** | `/bin/bash` | ⭐⭐⭐⭐⭐（通用默认） |
| **Zsh** | `/usr/bin/zsh` | ⭐⭐⭐⭐⭐ |
| **Fish** | `/usr/bin/fish` | ⭐⭐⭐⭐⭐ |
| **Dash** | `/bin/dash` | ⭐⭐⭐ |

**推荐**：Bash（兼容性最好） 或 Zsh（功能丰富）

---

## ⚙️ 配置与设置

### 设置默认 Shell

1. 打开设置（`⌘,` / `Ctrl+,`）
2. 切换到 **Local Terminal** 标签
3. 在 "Default Shell" 下拉菜单中选择
4. 点击 "Save"

### Shell 扫描逻辑

OxideTerm 使用以下逻辑自动检测 Shell：

#### Windows
```
1. Command Prompt (cmd.exe) - 始终可用
2. PowerShell 5.1 - 检查系统目录
3. PowerShell 7+ (pwsh.exe) - 检查以下位置：
   - C:\Program Files\PowerShell\7\
   - C:\Program Files (x86)\PowerShell\7\
   - PATH 环境变量
4. Git Bash - 检查：
   - C:\Program Files\Git\bin\bash.exe
   - C:\Program Files (x86)\Git\bin\bash.exe
5. WSL - 检查 C:\Windows\System32\wsl.exe
```

#### macOS/Linux
```
1. 解析 /etc/shells 文件
2. 使用 `which` 命令检测常见 Shell：
   - bash, zsh, fish, dash, sh, tcsh, ksh
3. 检查常见安装路径：
   - /usr/local/bin/*
   - /opt/homebrew/bin/* (macOS Apple Silicon)
   - /usr/bin/*
   - /bin/*
```

### 自定义 Shell

如果您的 Shell 没有被自动检测到，可以手动添加：

**配置文件位置**：
```
~/.oxideterm/shells.json  (macOS/Linux)
%APPDATA%\OxideTerm\shells.json  (Windows)
```

**格式**：
```json
{
  "custom_shells": [
    {
      "name": "custom-shell",
      "label": "My Custom Shell",
      "path": "/usr/local/bin/my-shell",
      "args": ["--login"]
    }
  ]
}
```

---

## 🔧 高级功能

### 1. 设置工作目录

创建本地终端时指定 CWD（Current Working Directory）：

```typescript
// 通过 API
await invoke('create_local_terminal', {
  request: {
    shell_id: 'bash',
    cwd: '/Users/alice/projects/my-app'
  }
});
```

**用途**：
- 从项目管理器快速打开项目终端
- 自动化脚本启动

### 2. 环境变量

传递自定义环境变量：

```rust
// 后端实现
PtyConfig {
    shell: shell_info,
    cwd: Some(PathBuf::from("/path/to/dir")),
    env: vec![
        ("NODE_ENV".to_string(), "development".to_string()),
        ("DEBUG".to_string(), "true".to_string()),
    ],
    // ...
}
```

**注意**：环境变量会继承父进程（OxideTerm）的环境。

### 3. 多终端管理

本地终端完全独立，每个终端都是独立的 Shell 进程：

```
本地终端 1: PowerShell (C:\Users\alice)
本地终端 2: Git Bash (C:\projects\app)
本地终端 3: WSL Ubuntu (/home/alice)
```

**优势**：
- 不同 Shell 之间互不干扰
- 可以同时运行多个工作目录
- 每个终端独立的历史和状态

---

## 🏗️ 技术架构

### PTY 封装

本地终端使用 `portable-pty` 库，OxideTerm 对其进行了线程安全封装：

```rust
pub struct PtyHandle {
    master: StdMutex<Box<dyn MasterPty + Send>>,
    child: StdMutex<Box<dyn portable_pty::Child + Send + Sync>>,
    reader: Arc<StdMutex<Box<dyn Read + Send>>>,
    writer: Arc<StdMutex<Box<dyn Write + Send>>>,
}

// 手动实现 Sync
unsafe impl Sync for PtyHandle {}
```

**关键设计**：
- **独立读写句柄**：避免锁争用
- **Arc + Mutex**：允许跨任务共享
- **专用 I/O 线程**：使用 `spawn_blocking` 处理阻塞 I/O

### 数据流

```
┌─────────────────────────────────────────────────────────┐
│  Frontend (React)                                       │
│  ├── LocalTerminalView (xterm.js)                       │
│  └── Tauri IPC                                          │
└──────────────────┬──────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────┐
│  Backend (Rust)                                         │
│  ├── LocalTerminalSession                               │
│  │   ├── PtyHandle (Arc<> for thread safety)            │
│  │   ├── Write Pump (input_tx → PTY writer)             │
│  │   └── Read Pump (PTY reader → event_tx)              │
│  └── LocalTerminalRegistry                              │
└──────────────────┬──────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────┐
│  Native PTY                                             │
│  ├── Windows: ConPTY (conpty.dll)                       │
│  ├── macOS: BSD PTY (/dev/ptmx)                         │
│  └── Linux: Unix98 PTY (/dev/pts/*)                     │
└─────────────────────────────────────────────────────────┘
```

### 分屏生命周期管理 (v1.4.0)

> **重要约束**: 分屏中的每个 Pane 都拥有独立的 PTY 进程，关闭 Tab 时必须**递归清理所有 PTY**。

#### 问题背景

当本地终端 Tab 包含分屏布局时，`Tab.sessionId` 为 `undefined`（已迁移到 `rootPane` 模式），如果只关闭 `sessionId` 会导致：
- 后端 PTY 进程泄漏（孤儿进程）
- `LocalTerminalRegistry` 计数不回落
- 侧边栏显示的终端数量与实际不符

#### 解决方案：递归清理

```typescript
// appStore.ts - closeTab 实现
closeTab: async (tabId) => {
  const tab = get().tabs.find(t => t.id === tabId);
  
  // Phase 1: 收集分屏中所有终端 session
  let localTerminalIds: string[] = [];
  
  if (tab.rootPane) {
    // 递归收集所有 pane 的 sessionId
    const sessions = collectAllPaneSessions(tab.rootPane);
    localTerminalIds = sessions.localTerminalIds;
  } else if (tab.sessionId && tab.type === 'local_terminal') {
    localTerminalIds = [tab.sessionId];
  }
  
  // Phase 2: 并行关闭所有本地终端 PTY
  await Promise.all(
    localTerminalIds.map((sid) => api.localCloseTerminal(sid))
  );
  
  // Phase 3: Strong Sync - 刷新状态确保一致
  await useLocalTerminalStore.getState().refreshTerminals();
}
```

#### 辅助函数

```typescript
// 递归收集 paneTree 中所有 session
export function collectAllPaneSessions(node: PaneNode): {
  localTerminalIds: string[];
  sshTerminalIds: string[];
} {
  if (node.type === 'leaf') {
    if (node.terminalType === 'local_terminal') {
      return { localTerminalIds: [node.sessionId], sshTerminalIds: [] };
    } else {
      return { localTerminalIds: [], sshTerminalIds: [node.sessionId] };
    }
  }
  
  // Group node: 递归收集子节点
  const result = { localTerminalIds: [], sshTerminalIds: [] };
  for (const child of node.children) {
    const childResult = collectAllPaneSessions(child);
    result.localTerminalIds.push(...childResult.localTerminalIds);
    result.sshTerminalIds.push(...childResult.sshTerminalIds);
  }
  return result;
}
```

#### 一致性约束

| 约束 | 描述 |
|------|------|
| **递归清理** | 关闭 Tab 必须遍历 `rootPane` 关闭所有 PTY |
| **Strong Sync** | 清理后调用 `refreshTerminals()` 同步状态 |
| **无孤儿进程** | 任何情况下都不能留下未关闭的 PTY |
| **禁止 unmount 杀 PTY** | 组件 cleanup 不能关闭 PTY（StrictMode 会 double-mount） |

#### ⚠️ 重要：React StrictMode 兼容性

```typescript
// ❌ 错误：在 useEffect cleanup 中关闭 PTY
return () => {
  useLocalTerminalStore.getState().closeTerminal(sessionId); // 会被 StrictMode 触发！
};

// ✅ 正确：只清理前端资源，PTY 由 closeTab 管理
return () => {
  terminalRef.current?.dispose();
  console.debug(`[LocalTerminalView] Unmount cleanup (PTY kept alive)`);
};
```

**原因**：React StrictMode 在开发模式下会 `mount → unmount → mount` 组件，如果在 unmount 时关闭 PTY，会导致"秒退"。

### Feature Gate

本地终端功能通过 Cargo feature 控制：

```toml
[features]
default = ["local-terminal"]
local-terminal = ["dep:portable-pty"]
```

**用途**：
- 桌面端：完整支持
- 移动端：通过 `--no-default-features` 剥离，减小包体积

---

## 🎯 使用场景

### 场景 1：本地开发

```
项目：~/projects/my-app
终端 1: npm run dev  (开发服务器)
终端 2: npm test     (测试运行)
终端 3: git status   (版本控制)
```

### 场景 2：跨 Shell 工作流

```
Windows 环境：
终端 1: PowerShell   (系统管理)
终端 2: Git Bash     (Unix 工具)
终端 3: WSL Ubuntu   (Linux 环境)
```

### 场景 3：混合本地/远程

```
终端 1: 本地终端 (macOS Zsh)
        └── 编辑代码、运行测试

终端 2: SSH 终端 (生产服务器)
        └── 查看日志、重启服务

终端 3: 本地终端 (Git Bash)
        └── 提交代码、推送
```

---

## 🐛 故障排查

### Q: 某个 Shell 没有被检测到？

A: 可能的原因：
- **不在标准路径**：添加到 PATH 或使用自定义配置
- **权限问题**：确保 Shell 可执行权限
- **未安装**：确认 Shell 已正确安装

解决方案：
1. 检查 Shell 路径：`which zsh` (Unix) 或 `where pwsh` (Windows)
2. 手动添加到 `shells.json`
3. 重启 OxideTerm 刷新 Shell 列表

---

### Q: 本地终端无法启动？

A: 常见原因：
- **Shell 路径错误**：检查 Shell 是否存在
- **权限不足**：确保 OxideTerm 有权限执行 Shell
- **PTY 初始化失败**：查看日志详细错误

解决方案：
1. 打开开发者工具（`⌘+Option+I` / `Ctrl+Shift+I`）
2. 查看 Console 和 Backend 日志
3. 尝试使用其他 Shell

---

### Q: 输出乱码或显示问题？

A: 可能的原因：
- **字符编码问题**：Shell 输出非 UTF-8
- **终端类型不兼容**：某些程序依赖特定终端类型
- **字体缺失**：缺少 Nerd Font 图标字体

解决方案：
1. 检查 Shell 编码设置（`echo $LANG`）
2. 设置 `TERM=xterm-256color`
3. 安装 Nerd Font 字体

---

### Q: Windows PowerShell 启动慢？

A: PowerShell 启动时会加载配置文件（profile）

解决方案：
1. 优化 PowerShell profile：`$PROFILE`
2. 使用 `-NoProfile` 参数跳过
3. 切换到 PowerShell 7（启动更快）

---

### Q: WSL 终端无法连接？

A: 可能的原因：
- **WSL 未安装**：运行 `wsl --install`
- **WSL 版本问题**：确保 WSL 2
- **默认发行版未设置**：`wsl --set-default Ubuntu`

解决方案：
1. 检查 WSL 状态：`wsl --list --verbose`
2. 更新 WSL：`wsl --update`
3. 重启 WSL 服务

---

## 🔑 快捷键参考

| 操作 | Windows/Linux | macOS |
|------|---------------|-------|
| **新建本地终端** | `Ctrl+Shift+N` | `⌘+Shift+N` |
| **关闭终端** | `Ctrl+Shift+W` | `⌘+W` |
| **下一个标签** | `Ctrl+Tab` | `⌘+}` |
| **上一个标签** | `Ctrl+Shift+Tab` | `⌘+{` |
| **清屏** | `Ctrl+L` | `⌘+K` |

---

## 📊 性能特性

### 资源占用

| 指标 | 典型值 |
|------|--------|
| **内存占用** | ~10MB / 终端 |
| **CPU 占用** | ~0-1%（空闲时） |
| **启动时间** | < 100ms |

### I/O 性能

| 指标 | 数值 |
|------|------|
| **缓冲区大小** | 8KB（读取） |
| **延迟** | < 1ms（本地） |
| **吞吐量** | > 100MB/s |

---

## 🛠️ 高级配置

### 设置 Shell 参数

某些 Shell 需要特定参数才能正常工作：

```json
{
  "custom_shells": [
    {
      "name": "zsh",
      "label": "Zsh (Login)",
      "path": "/bin/zsh",
      "args": ["-l"]  // --login
    },
    {
      "name": "bash",
      "label": "Bash (Interactive)",
      "path": "/bin/bash",
      "args": ["-i"]  // --interactive
    }
  ]
}
```

### 设置初始化脚本

在 Shell 配置文件中添加 OxideTerm 特定设置：

**Zsh** (`~/.zshrc`):
```bash
if [[ "$TERM_PROGRAM" == "OxideTerm" ]]; then
    # OxideTerm 特定配置
    export PS1="%F{cyan}%~ %F{white}❯ "
fi
```

**Bash** (`~/.bashrc`):
```bash
if [[ "$TERM_PROGRAM" == "OxideTerm" ]]; then
    # OxideTerm 特定配置
    export PS1="\[\e[36m\]\w \[\e[0m\]❯ "
fi
```

**PowerShell** (`$PROFILE`):
```powershell
if ($env:TERM_PROGRAM -eq "OxideTerm") {
    # OxideTerm 特定配置
    function prompt {
        "$PWD> "
    }
}
```

---

## 🎓 与传统终端的对比

| 功能 | OxideTerm 本地终端 | iTerm2/Alacritty | Windows Terminal |
|------|-------------------|------------------|------------------|
| **跨平台** | ✅ macOS/Windows/Linux | ❌ macOS only | ❌ Windows only |
| **SSH 集成** | ✅ 无缝切换 | ❌ 需外部工具 | ⚠️ 有限支持  |
| **拓扑路由** | ✅ ProxyJump 自动计算 | ❌ | ❌ |
| **AI 助手** | ✅ 内置 | ❌ | ❌ |
| **连接池** | ✅ 自动复用 | ❌ | ❌ |

---

*文档版本: v1.4.0 (Strong Sync + 分屏生命周期管理) | 最后更新: 2026-02-04*
