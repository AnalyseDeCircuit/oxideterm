# Windows 终端支持改进方案

> 施工文档 v1.0 | 2026-02-04  
> **状态：✅ 已完成**

## 一、问题现状分析

### 1.1 当前实现概览

OxideTerm 使用 `portable-pty` v0.8 通过 Windows ConPTY 提供本地终端支持。当前已实现：

| 功能 | 文件位置 | 状态 |
|------|----------|------|
| Shell 扫描 | `src-tauri/src/local/shell.rs` | ✅ 完整 |
| PowerShell/pwsh 检测 | `shell.rs:280-340` | ✅ 完整 |
| WSL 发行版枚举 | `shell.rs:365-443` | ✅ 完整 |
| Git Bash 支持 | `shell.rs:345-360` | ✅ 完整 |
| Oh My Posh 设置 UI | `SettingsView.tsx:165-185` | ✅ 完整 |
| UTF-8 环境变量 | `pty.rs:136-168` | ⚠️ 不完整 |

### 1.2 核心问题

#### 问题 1：PowerShell UTF-8 编码不生效

**现状**：仅设置 `CHCP=65001` 环境变量
```rust
// pty.rs:145
cmd.env("CHCP", "65001");
```

**问题**：
- `CHCP` 作为环境变量无效，必须作为命令执行
- PowerShell 的 `[Console]::OutputEncoding` 未设置
- 导致中文、Emoji、Nerd Font 图标显示乱码

**正确做法**：
```powershell
# 必须在 PowerShell 启动时执行
chcp 65001 > $null
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
```

#### 问题 2：Oh My Posh 未初始化

**现状**：仅设置 `POSH_THEME` 环境变量
```rust
// pty.rs:160-163
if let Some(theme_path) = &config.oh_my_posh_theme {
    cmd.env("POSH_THEME", theme_path);
}
```

**问题**：
- Oh My Posh 需要显式初始化才能工作
- 仅设置环境变量不会激活提示符渲染

**正确做法**：
```powershell
# PowerShell 初始化
oh-my-posh init pwsh | Invoke-Expression

# 或带主题
oh-my-posh init pwsh --config $env:POSH_THEME | Invoke-Expression
```

#### 问题 3：WSL 环境变量传递不完整

**现状**：
```rust
// pty.rs:150-153
cmd.env("WSL_UTF8", "1");
cmd.env("WSLENV", "TERM:COLORTERM");
```

**问题**：
- `TERM_PROGRAM` 等重要变量未传递
- 影响 WSL 内应用的终端检测

#### 问题 4：Windows Terminal Shell Integration 缺失

**问题**：
- 不支持 Windows Terminal 的 OSC 序列（标题、CWD 追踪）
- 不支持 Shell Integration 标记（命令开始/结束）

---

## 二、改进方案设计

### 2.1 架构设计

```
┌─────────────────────────────────────────────────────────────┐
│                    PtyConfig (pty.rs)                        │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ + oh_my_posh_enabled: bool                              │ │
│  │ + oh_my_posh_theme: Option<String>                      │ │
│  │ + windows_utf8_init: bool  [NEW]                        │ │
│  │ + shell_integration: bool  [NEW]                        │ │
│  └─────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│              Windows Init Script Generator                   │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ generate_powershell_init_script()                       │ │
│  │ generate_cmd_init_script()                              │ │
│  │ generate_wsl_init_script()                              │ │
│  └─────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                  PTY Spawn with -Command                     │
│  pwsh.exe -NoLogo -NoExit -Command "<init_script>"          │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 PowerShell 初始化脚本

**方案**：通过 `-Command` 参数注入初始化脚本

```rust
// 生成 PowerShell 初始化脚本
fn generate_powershell_init(config: &PtyConfig) -> String {
    let mut script = String::new();
    
    // 1. UTF-8 编码设置
    script.push_str(r#"
        chcp 65001 > $null;
        [Console]::InputEncoding = [System.Text.Encoding]::UTF8;
        [Console]::OutputEncoding = [System.Text.Encoding]::UTF8;
        $OutputEncoding = [System.Text.Encoding]::UTF8;
    "#);
    
    // 2. Oh My Posh 初始化（如果启用）
    if config.oh_my_posh_enabled {
        if let Some(theme) = &config.oh_my_posh_theme {
            script.push_str(&format!(
                r#"if (Get-Command oh-my-posh -ErrorAction SilentlyContinue) {{
                    oh-my-posh init pwsh --config '{}' | Invoke-Expression
                }};"#,
                theme
            ));
        } else {
            script.push_str(r#"
                if (Get-Command oh-my-posh -ErrorAction SilentlyContinue) {
                    oh-my-posh init pwsh | Invoke-Expression
                };
            "#);
        }
    }
    
    // 3. 清屏（可选，提供干净的起始状态）
    script.push_str("Clear-Host;");
    
    script
}
```

### 2.3 Shell 参数修改

**修改 `get_shell_args()` 函数**：

```rust
// shell.rs - 修改 PowerShell 参数生成
"pwsh" | "powershell" => {
    let mut args = vec![
        "-NoLogo".to_string(),
        "-NoExit".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
    ];
    
    if !load_profile {
        args.push("-NoProfile".to_string());
    }
    
    // 添加初始化命令（UTF-8 + OMP）
    // 这部分将在 pty.rs 中处理
    
    args
}
```

### 2.4 WSL 环境变量增强

```rust
// pty.rs - 增强 WSL 环境变量
if config.shell.id.starts_with("wsl") {
    cmd.env("WSL_UTF8", "1");
    // 扩展 WSLENV 传递更多变量
    cmd.env("WSLENV", "TERM:COLORTERM:TERM_PROGRAM:TERM_PROGRAM_VERSION");
    
    // 传递终端信息
    cmd.env("TERM_PROGRAM", "OxideTerm");
    cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
}
```

### 2.5 设置项扩展

**新增设置项**（settingsStore.ts）：

```typescript
interface LocalTerminalSettings {
  // 现有
  ohMyPoshEnabled: boolean;
  ohMyPoshTheme: string | null;
  
  // 新增
  windowsUtf8Init: boolean;      // 自动初始化 UTF-8 编码（默认 true）
  shellIntegration: boolean;     // 启用 Shell Integration 序列
}
```

---

## 三、详细实施计划

### Phase 1: PowerShell UTF-8 初始化 ✅ 优先级最高

**目标**：解决中文/Emoji/Nerd Font 乱码问题

**修改文件**：
1. `src-tauri/src/local/pty.rs`
   - 新增 `generate_powershell_init_script()` 函数
   - 修改 `PtyHandle::new()` 中的命令构建逻辑

**实施步骤**：
```rust
// pty.rs 新增函数
#[cfg(target_os = "windows")]
fn generate_powershell_init_script(config: &PtyConfig) -> Option<String> {
    // 仅对 PowerShell 生成初始化脚本
    if !matches!(config.shell.id.as_str(), "powershell" | "pwsh") {
        return None;
    }
    
    let mut parts = Vec::new();
    
    // UTF-8 编码初始化
    parts.push(
        "[Console]::InputEncoding = [Console]::OutputEncoding = \
         [System.Text.Encoding]::UTF8; \
         $OutputEncoding = [System.Text.Encoding]::UTF8"
    );
    
    // Oh My Posh 初始化
    if config.oh_my_posh_enabled {
        if let Some(theme) = &config.oh_my_posh_theme {
            if !theme.is_empty() {
                parts.push(&format!(
                    "if (Get-Command oh-my-posh -ErrorAction SilentlyContinue) {{ \
                     oh-my-posh init pwsh --config '{}' | Invoke-Expression }}", 
                    theme
                ));
            }
        } else {
            parts.push(
                "if (Get-Command oh-my-posh -ErrorAction SilentlyContinue) { \
                 oh-my-posh init pwsh | Invoke-Expression }"
            );
        }
    }
    
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}
```

**命令构建修改**：
```rust
// 在构建 PowerShell 命令时
#[cfg(target_os = "windows")]
{
    if let Some(init_script) = generate_powershell_init_script(&config) {
        // 使用 -Command 注入初始化脚本，然后保持交互
        cmd.arg("-Command");
        cmd.arg(&format!(
            "{}; Set-Location '{}'; $Host.UI.RawUI.WindowTitle = 'OxideTerm'",
            init_script,
            config.cwd.as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "$HOME".to_string())
        ));
    }
}
```

### Phase 2: Oh My Posh 完整集成

**目标**：启用 OMP 后自动初始化提示符

**依赖**：Phase 1 的初始化脚本机制

**额外工作**：
1. 检测 `oh-my-posh` 命令是否存在
2. 处理主题路径（支持 `~` 展开）
3. 添加错误处理和用户提示

### Phase 3: WSL 环境变量增强

**修改文件**：`src-tauri/src/local/pty.rs`

**修改内容**：
```rust
if config.shell.id.starts_with("wsl") {
    cmd.env("WSL_UTF8", "1");
    cmd.env("WSLENV", "TERM:COLORTERM:TERM_PROGRAM:TERM_PROGRAM_VERSION:POSH_THEME/p");
    cmd.env("TERM_PROGRAM", "OxideTerm");
    cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
    
    // 如果启用 OMP，传递主题路径（转换为 WSL 路径格式）
    if config.oh_my_posh_enabled {
        if let Some(theme) = &config.oh_my_posh_theme {
            // Windows 路径需要转换为 WSL 路径
            // C:\Users\... -> /mnt/c/Users/...
            cmd.env("POSH_THEME", convert_to_wsl_path(theme));
        }
    }
}
```

### Phase 4: 设置 UI 更新

**修改文件**：
- `src/store/settingsStore.ts` - 添加新设置项
- `src/components/settings/SettingsView.tsx` - 添加 UI 控件
- `src/locales/*/settings_view.json` - 添加翻译

**新增 UI 元素**：
- "自动初始化 UTF-8 编码" 开关（默认开启）
- Oh My Posh 检测状态指示

### Phase 5: 文档更新

**修改文件**：
- `docs/LOCAL_TERMINAL.md` - 更新 Windows 支持说明
- `docs/knownissues.md` - 记录已知限制

---

## 四、测试计划

### 4.1 测试矩阵

| Shell | UTF-8 中文 | Emoji | Nerd Font | Oh My Posh |
|-------|-----------|-------|-----------|------------|
| cmd.exe | ⬜ | ⬜ | ⬜ | N/A |
| PowerShell 5.1 | ⬜ | ⬜ | ⬜ | ⬜ |
| PowerShell 7+ (pwsh) | ⬜ | ⬜ | ⬜ | ⬜ |
| Git Bash | ⬜ | ⬜ | ⬜ | N/A |
| WSL (Ubuntu) | ⬜ | ⬜ | ⬜ | ⬜ |

### 4.2 测试命令

```powershell
# UTF-8 测试
echo "中文测试 日本語 한국어"
echo "Emoji: 🎉 🚀 ✅ ❌"

# Nerd Font 测试
echo " PowerShell |  Git |  Folder"

# Oh My Posh 测试
oh-my-posh --version
$env:POSH_THEME
```

### 4.3 验收标准

1. ✅ PowerShell 中文字符正确显示
2. ✅ Emoji 正确渲染
3. ✅ Nerd Font 图标正确显示（需要 Nerd Font 字体）
4. ✅ Oh My Posh 提示符正确渲染
5. ✅ WSL 内 `$TERM_PROGRAM` 显示 "OxideTerm"

---

## 五、已知限制

### 5.1 无法解决的问题

| 问题 | 原因 | 建议 |
|------|------|------|
| cmd.exe 编码支持差 | Windows 设计限制 | 建议使用 PowerShell |
| 旧版 Windows 10 ConPTY bug | 系统版本问题 | 建议更新 Windows |
| 某些 Nerd Font 图标显示为方块 | 字体不完整 | 使用完整 Nerd Font |

### 5.2 用户配置要求

1. **字体**：必须使用 Nerd Font 变体才能显示图标
2. **Oh My Posh**：需要用户自行安装 `oh-my-posh`
3. **PowerShell 7**：推荐使用 pwsh 而非 Windows PowerShell 5.1

---

## 六、回滚方案

如果改进导致问题，可以通过以下方式回滚：

1. **设置开关**：用户可禁用 "自动初始化 UTF-8 编码"
2. **代码回滚**：移除 `-Command` 参数注入
3. **环境变量**：设置 `OXIDETERM_SKIP_INIT=1` 跳过初始化

---

## 七、时间估算

| 阶段 | 预计时间 | 依赖 |
|------|----------|------|
| Phase 1: PowerShell UTF-8 | 30 分钟 | 无 |
| Phase 2: Oh My Posh | 20 分钟 | Phase 1 |
| Phase 3: WSL 增强 | 15 分钟 | 无 |
| Phase 4: 设置 UI | 30 分钟 | Phase 1-3 |
| Phase 5: 文档 | 20 分钟 | Phase 1-4 |
| **总计** | **~2 小时** | |

---

## 八、参考资料

- [Oh My Posh 文档](https://ohmyposh.dev/docs/installation/prompt)
- [PowerShell UTF-8 编码](https://docs.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_character_encoding)
- [WSLENV 文档](https://docs.microsoft.com/en-us/windows/wsl/interop#share-environment-variables)
- [Windows Terminal Shell Integration](https://docs.microsoft.com/en-us/windows/terminal/tutorials/shell-integration)

---

**文档状态**：✅ 已完成  
**作者**：GitHub Copilot  
**完成日期**：2026-02-04
