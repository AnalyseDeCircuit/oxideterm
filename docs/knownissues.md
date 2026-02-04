# OxideTerm 已知问题

## 输入法相关

### IME 选词导致的"搜索连击"场景

在本地终端搜索框（React 开发）中使用中文输入法。

**行为**：用户输入拼音后按下 Enter 键确认选词。

**预期**：搜索框确认当前选定的任意字符，并触发一次搜索，高亮第一个匹配项（例如 1/26）。

**现状**：触发了"连击"效应。索引瞬间从 1/26 跳到 2/26。

---

## Windows 终端相关

### Command Prompt (cmd.exe) 编码限制

**问题**：cmd.exe 的 UTF-8 支持有限，无法通过程序化方式完全解决。

**建议**：使用 PowerShell 7+ 或 WSL 替代 cmd.exe。

### Oh My Posh 需要手动安装

**问题**：OxideTerm 只负责初始化 Oh My Posh，但不会自动安装它。

**解决方案**：
```powershell
# 使用 winget 安装
winget install JanDeDobbeleer.OhMyPosh

# 或使用 scoop
scoop install oh-my-posh
```

### Nerd Font 图标显示为方块

**问题**：Oh My Posh 主题使用的 Nerd Font 图标显示为空白方块。

**解决方案**：
1. 下载 Nerd Font：https://www.nerdfonts.com/
2. 推荐字体：JetBrains Mono Nerd Font、Meslo Nerd Font
3. 在 OxideTerm 设置中选择已安装的 Nerd Font

### 旧版 Windows 10 ConPTY 问题

**问题**：Windows 10 1809 之前的版本 ConPTY 支持不完整。

**建议**：升级到 Windows 10 1903 或更高版本，推荐 Windows 11。