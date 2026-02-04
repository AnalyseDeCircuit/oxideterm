# OxideTerm 字体双轨制

OxideTerm 使用 **"双轨制"** 字体系统，提供开箱即用的保底方案与完全自由的自定义能力。

## 双轨制架构

### 预设轨道：内置保底，开箱即用

在设置下拉菜单中提供预设字体选项：

| 字体 | 说明 | 图标支持 |
|------|------|----------|
| **JetBrains Mono NF ✓** | 内置 woff2，绝对不乱码 | ✅ Nerd Font |
| **MesloLGM NF ✓** | 内置 woff2，Apple Menlo 风格 | ✅ Nerd Font |
| Cascadia Code | Windows 系统字体（如有） | ⚠️ 需安装 NF 版 |
| Consolas | Windows 系统字体 | ❌ |
| Menlo | macOS 系统字体 | ❌ |

> ✓ 表示内置保底，即使系统没装也能正常显示

### 自定义轨道：无限自由，吃准本机

选择 **"自定义..."** 后，可输入任意字体栈：

```
'Sarasa Fixed SC', 'JetBrainsMono Nerd Font', monospace
```

支持：
- 任意系统已安装的字体
- 多字体优先级排列（按逗号分隔）
- 自动追加 `monospace` 兜底

## 字体加载策略

所有预设字体使用 **系统优先 → 内置兜底** 策略：

```
用户选择 "JetBrains Mono NF"
    ↓
浏览器依次尝试：
  1. "JetBrainsMono Nerd Font"      ← 系统安装的 NF
  2. "JetBrainsMono Nerd Font Mono" ← 系统安装的 NF Mono
  3. "JetBrains Mono NF"            ← 内置 woff2 (保底)
  4. "JetBrains Mono"               ← 系统原版
  5. monospace                      ← 最终兜底
```

## 内置字体文件

| 文件夹 | 格式 | 大小 | 许可证 |
|--------|------|------|--------|
| `JetBrainsMono/` | WOFF2 | ~4.0 MB | OFL |
| `Meslo/` | WOFF2 | ~4.7 MB | Apache 2.0 |

**总计 ~8.7 MB**（相比 TTF 格式减少 58%）

---

Last updated: 2025-02-04
