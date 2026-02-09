# OxideTerm 字体双轨制

OxideTerm 使用 **"双轨制"** 字体系统，提供开箱即用的保底方案与完全自由的自定义能力。

## 双轨制架构

### 预设轨道：内置保底，开箱即用

在设置下拉菜单中提供预设字体选项：

| 字体 | 说明 | 图标支持 |
|------|------|----------|
| **JetBrains Mono NF (Subset) ✓** | 内置 woff2，绝对不乱码 | ✅ Nerd Font |
| **MesloLGM NF (Subset) ✓** | 内置 woff2，Apple Menlo 风格 | ✅ Nerd Font |
| **Maple Mono NF CN (Subset) ✓** | 内置 woff2，CJK 优化，圆润风格 | ✅ Nerd Font + 中文 |
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

## 🎯 CJK 智能 Fallback 策略

**核心理念**: 拉丁各异，中文统一

所有字体都自动 fallback 到 **Maple Mono NF CN** 的 CJK 部分：

```
用户选择 "JetBrains Mono NF (Subset)"
    ↓
浏览器渲染时：
  - 拉丁字母 (A-Z, 0-9) → JetBrains Mono (保持原有风格)
  - 中日韩字符 (中文/日文/韩文) → Maple Mono NF CN (统一 CJK)
  - Nerd Font 图标 → JetBrains Mono NF
```

**优势**：
- 🎨 拉丁字母保持各字体独特风格（JetBrains 的锐利、Meslo 的圆润等）
- 🇨🇳 中日韩字符全部使用 Maple Mono 的优秀 CJK 字形
- 📦 即使选择没有 CJK 的字体（如 Consolas），中文也能正常显示

## 字体加载策略

字体栈示例（JetBrains Mono）：

```css
font-family: 
  "JetBrainsMono Nerd Font",     /* 系统 NF */
  "JetBrainsMono Nerd Font Mono",/* 系统 NF Mono */
  "JetBrains Mono NF (Subset)",           /* 内置 woff2 */
  "JetBrains Mono",              /* 系统原版 */
  "Maple Mono NF CN (Subset)",            /* CJK fallback */
  monospace;                     /* 最终兜底 */
```

## 内置字体文件

| 文件夹 | 格式 | 大小 | 许可证 | 说明 |
|--------|------|------|--------|------|
| `JetBrainsMono/` | WOFF2 | ~4.0 MB | OFL | Hinted |
| `Meslo/` | WOFF2 | ~4.7 MB | Apache 2.0 | Hinted |
| `MapleMono/` | WOFF2 | ~25 MB | OFL | **Unhinted** (高分屏优化) |

**总计 ~34 MB**（WOFF2 压缩格式）

## ⚡ 性能优化：CJK 字体懒加载

Maple Mono NF CN (~25MB) 使用**懒加载策略**，避免阻塞应用启动：

### CSS 层面
所有 `@font-face` 声明都包含 `font-display: swap;`：
- 终端立即使用系统等宽字体渲染
- CJK 字体加载完成后自动切换，无需刷新

### 组件层面
使用 `document.fonts.load()` API 实现按需预加载：

```typescript
// src/lib/fontLoader.ts
import { ensureCJKFallback, onFontLoaded } from './lib/fontLoader';

// 在终端初始化时触发 CJK 预加载（非阻塞）
ensureCJKFallback();

// 监听字体加载完成，刷新终端布局
onFontLoaded('Maple Mono NF CN (Subset)', () => {
  terminal.refresh(0, terminal.rows - 1);
  fitAddon.fit();
});
```

### 加载流程
```
App 启动
    ↓
JetBrains/Meslo 立即加载 (小文件，~4MB)
终端立即可用 (系统等宽字体渲染)
    ↓
Maple Mono NF CN 后台加载 (~25MB)
    ↓
CJK 字体加载完成
终端自动刷新，中文字符切换到 Maple Mono
```

> **注意**: Maple Mono NF CN 使用 **Unhinted** 版本，在 Retina/HiDPI 高分屏上显示更平滑自然。Hinted 版本针对低分屏优化，在高分屏上反而显得过度锐化。

---

Last updated: 2025-02-04
