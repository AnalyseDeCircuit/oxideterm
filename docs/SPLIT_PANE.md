# Terminal Split Pane 分屏功能施工文档

> 版本：v1.0  
> 创建日期：2026-01-24  
> 状态：🚧 施工中

## 1. 目标概述

实现终端多窗格分屏功能，支持：
- 水平/垂直分割
- 递归嵌套布局（类似 VS Code）
- 独立聚焦追踪
- AI 上下文正确抓取当前活动 Pane 的缓冲区

## 2. 架构设计

### 2.1 数据模型：递归布局树 (Layout Tree)

```typescript
// 叶子节点：实际的终端 Pane
interface PaneLeaf {
  type: 'leaf';
  id: string;           // paneId (UUID)
  sessionId: string;    // 关联的 session
  terminalType: 'terminal' | 'local_terminal';
}

// 分支节点：容器
interface PaneGroup {
  type: 'group';
  id: string;
  direction: 'horizontal' | 'vertical';
  children: PaneNode[];
  sizes?: number[];     // 各子节点占比 (0-100)
}

type PaneNode = PaneLeaf | PaneGroup;

// Tab 扩展
interface Tab {
  id: string;
  type: TabType;
  // 新增分屏支持
  rootPane?: PaneNode;      // 布局树根节点
  activePaneId?: string;    // 当前聚焦的 Pane
  // 向后兼容
  sessionId?: string;       // 单 pane 时直接使用
  title: string;
  icon?: string;
}
```

### 2.2 Terminal Registry 升级

**改动前（sessionId 为 key）：**
```typescript
Map<sessionId, TerminalEntry>
```

**改动后（paneId 为 key）：**
```typescript
Map<paneId, TerminalEntry>

// 新增全局 active pane 追踪
let activePaneId: string | null = null;

// API 变更
registerTerminalBuffer(paneId, tabId, getter)
getTerminalBuffer(paneId, tabId)  
setActivePaneId(paneId)
getActivePaneId() → string | null
getActiveTerminalBuffer(tabId) → string | null  // 便捷方法
```

### 2.3 组件结构

```
SplitTerminalContainer.tsx        // 递归渲染布局树
├── PanelGroup                    // react-resizable-panels
│   ├── Panel                     // 叶子节点 → TerminalPane
│   │   └── TerminalPane.tsx      // 包装层，处理 focus 边框
│   │       └── TerminalView.tsx  // 原有组件
│   ├── PanelResizeHandle         // 拖拽条
│   └── Panel                     // 递归 → SplitTerminalContainer
│       └── ...
```

## 3. 施工阶段

### Phase 1: Registry 升级 & Focus 追踪 ✅ 已完成

**目标**：在不改变 UI 的情况下，完成底层数据结构升级

| 任务 | 文件 | 状态 |
|------|------|------|
| 1.1 升级 terminalRegistry.ts | `src/lib/terminalRegistry.ts` | ✅ |
| 1.2 扩展 Tab 类型定义 | `src/types/index.ts` | ✅ |
| 1.3 添加 Pane 管理 actions | `src/store/appStore.ts` | ✅ |
| 1.4 统一终端注册 (SSH + Local) | `TerminalView.tsx`, `LocalTerminalView.tsx` | ✅ |
| 1.5 更新 AI 上下文获取 | `src/components/ai/ChatInput.tsx` | ✅ |
| 1.6 添加 i18n 翻译键 | `src/locales/*/terminal.json` (11 languages) | ✅ |

### Phase 2: UI 层实现 ✅ 已完成

**目标**：实现可视化分屏界面

| 任务 | 文件 | 状态 |
|------|------|------|
| 2.1 安装 react-resizable-panels | `package.json` | ✅ |
| 2.2 创建 TerminalPane 包装组件 | `src/components/terminal/TerminalPane.tsx` | ✅ |
| 2.3 创建 SplitTerminalContainer | `src/components/terminal/SplitTerminalContainer.tsx` | ✅ |
| 2.4 集成到 AppLayout | `src/components/layout/AppLayout.tsx` | ✅ |
| 2.5 添加分屏按钮 UI | `src/components/terminal/SplitPaneToolbar.tsx` | ✅ |

**注意**：SSH 终端分屏暂未实现（需要复制会话逻辑），本地终端分屏已可用。

### Phase 3: 交互优化 ✅ 已完成

**目标**：提升用户体验

| 任务 | 文件 | 状态 |
|------|------|------|
| 3.1 键盘快捷键支持 | `src/hooks/useSplitPaneShortcuts.ts` | ✅ |
| 3.2 Resize 防抖优化 | `SplitTerminalContainer.tsx` | ✅ |
| 3.3 聚焦视觉反馈（Oxide Orange） | `src/styles.css` | ✅ (已在 TerminalPane.tsx 中实现) |
| 3.4 Pane 关闭逻辑 | `appStore.ts` | ✅ |

**键盘快捷键：**
| 快捷键 (Mac) | 快捷键 (Win/Linux) | 功能 |
|-------------|-------------------|------|
| Cmd+Shift+E | Ctrl+Shift+E | 水平分屏 |
| Cmd+Shift+D | Ctrl+Shift+D | 垂直分屏 |
| Cmd+Shift+W | Ctrl+Shift+W | 关闭当前面板 |
| Cmd+Option+←/→/↑/↓ | Ctrl+Alt+Arrow | 在面板间导航 |

## 4. 关键设计决策

### 4.1 统一前端 Buffer 注册

**决策**：SSH 终端也使用前端 Buffer Getter 注册到 Registry

**理由**：
1. AI 上下文逻辑统一，只需知道 `paneId`
2. 支持离线上下文：SSH 断开但 Buffer 还在时，AI 仍能分析
3. 简化双轨制带来的复杂性

**实现**：在 `TerminalView.tsx` 的 xterm 初始化后，调用 `registerTerminalBuffer(paneId, tabId, () => getBufferContent())`

### 4.2 聚焦视觉反馈

**设计**：活动 Pane 顶部显示 2px 的 Oxide Orange (#F97316) 边框

```css
.terminal-pane.active {
  border-top: 2px solid #F97316;
}

.terminal-pane:not(.active) {
  border-top: 2px solid transparent;
}
```

### 4.3 最大分屏限制

- 单 Tab 最多 **4 个 Pane**
- 支持任意嵌套方向组合

### 4.4 键盘快捷键规划

| 快捷键 (Mac) | 快捷键 (Win/Linux) | 功能 |
|-------------|-------------------|------|
| `Cmd+Shift+D` | `Ctrl+Shift+D` | 垂直分割当前 Pane |
| `Cmd+Shift+E` | `Ctrl+Shift+E` | 水平分割当前 Pane |
| `Cmd+Option+←/→/↑/↓` | `Ctrl+Alt+Arrow` | 切换 Pane 聚焦 |
| `Cmd+Shift+W` | `Ctrl+Shift+W` | 关闭当前 Pane（最后一个时关闭 Tab） |

## 5. 风险与缓解

| 风险 | 缓解措施 |
|------|----------|
| xterm.js resize 卡顿 | ResizeObserver + 16ms 防抖 |
| 布局状态丢失 | 持久化到 localStorage |
| 复杂嵌套难以调试 | 添加 DEV 模式布局可视化 |
| 内存泄漏（多实例） | 严格的 cleanup 逻辑 |

## 6. 测试清单

- [ ] 单 Pane Tab 向后兼容
- [ ] 垂直分割创建新 Pane
- [ ] 水平分割创建新 Pane
- [ ] 嵌套分割（左大右上下）
- [ ] Pane 聚焦切换更新 activePaneId
- [ ] AI 上下文抓取正确 Pane
- [ ] Resize 拖拽流畅无卡顿
- [ ] 关闭 Pane 正确更新布局树
- [ ] 关闭最后 Pane 关闭整个 Tab
- [ ] SSH 和 Local 终端均可分屏

## 7. 跨分屏视野 (Cross-Pane Vision)

> 新增于 v1.1

### 7.1 功能概述

AI 聊天支持同时获取所有分屏的终端内容，而非仅获取当前活动 Pane。这对于调试场景非常有用：

- 左屏显示错误日志，右屏显示代码
- 上屏运行服务器，下屏执行 curl 测试
- AI 可以综合分析所有屏幕内容

### 7.2 使用方式

1. 在 AI 聊天输入框启用 "包含上下文"
2. 当 Tab 有多个分屏时，会出现 "所有分屏" 按钮
3. 点击启用后，AI 将获取所有 Pane 的缓冲区

### 7.3 技术实现

**Registry 新增 API:**
```typescript
// 获取所有 Pane 的上下文（数组形式）
gatherAllPaneContexts(tabId: string, maxCharsPerPane?: number): GatheredPaneContext[]

// 获取合并后的上下文字符串（带分隔标记）
getCombinedPaneContext(tabId: string, maxCharsPerPane?: number, separator?: string): string
```

**输出格式:**
```
=== PANE 1 (terminal) [ACTIVE] ===
... terminal buffer ...

=== PANE 2 (local_terminal) ===
... terminal buffer ...
```

### 7.4 性能考虑

- 每个 Pane 的缓冲区默认截取 `contextMaxChars / 4` 字符
- 最多支持 4 个分屏，总上下文不会超过设置限制
- 只在用户明确启用时才获取全部上下文

## 8. 参考资源

- [react-resizable-panels](https://github.com/bvaughn/react-resizable-panels)
- [VS Code workbench layout](https://github.com/microsoft/vscode/tree/main/src/vs/workbench/browser/layout)
- [xterm.js fit addon](https://github.com/xtermjs/xterm.js/tree/master/addons/addon-fit)
