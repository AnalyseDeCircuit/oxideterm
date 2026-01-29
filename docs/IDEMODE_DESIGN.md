# OxideTerm 轻量 IDE 模式设计方案 v2

> 基于现有架构审计后的修订版本

## 1. 目标与定位

### 1.1 核心目标

将 OxideTerm 从「SSH 终端 + SFTP 文件管理器」升级为「轻量级远程开发环境」，提供：

- **项目级文件浏览**：以项目根目录为中心的文件树
- **多标签编辑器**：同时编辑多个远程文件（基于 CodeMirror 6）
- **集成终端**：编辑器与终端分屏协作
- **Git 感知**（可选）：显示文件状态和分支信息

### 1.2 非目标（明确边界）

- ❌ 不做 LSP/语言服务器集成（保持轻量）
- ❌ 不做远程调试器
- ❌ 不做插件系统
- ❌ 不做 Git 操作（仅展示状态）

### 1.3 与现有功能的关系

```
┌─────────────────────────────────────────────────────────┐
│                    OxideTerm 标签系统                     │
├──────────┬──────────┬──────────┬──────────┬─────────────┤
│ terminal │   sftp   │ forwards │ topology │    ide ←新增 │
└──────────┴──────────┴──────────┴──────────┴─────────────┘
                              ↑
            IDE 模式复用现有 SFTP 和终端基础设施
```

---

## 2. 现有基础设施分析

### 2.1 可直接复用 ✅

| 组件 | 位置 | 复用方式 |
|------|------|----------|
| **CodeMirror 6 编辑器** | `components/editor/RemoteFileEditor.tsx` | 抽取核心逻辑为 `useCodeMirrorEditor` hook |
| 文件树渲染 | `components/sftp/SFTPView.tsx` | 抽取 FileList 组件 |
| 分屏系统 | `types/index.ts` → `PaneNode` | 直接使用 |
| SFTP API | `lib/api.ts` → `sftp_*` | 直接调用 |
| 终端组件 | `components/terminal/Terminal.tsx` | 直接嵌入 |

> **技术栈说明：** 编辑器基于 [CodeMirror 6](https://codemirror.net/)，已集成以下功能：
> - 20+ 语言语法高亮（通过 `@codemirror/lang-*` 包）
> - 主题：`@codemirror/theme-one-dark`
> - 快捷键：`Mod-s` 保存、标准编辑快捷键
> - 功能：行号、括号匹配、代码折叠、自动补全、搜索

### 2.2 需要扩展 🔧

| 功能 | 现状 | 扩展方式 |
|------|------|----------|
| TabType | 无 `ide` 类型 | 添加枚举值 |
| 会话管理 | 单 SFTP 会话 | IDE 模式独立管理多会话 |
| 文件缓存 | 无 | 新增 IndexedDB 缓存层 |

### 2.3 需要新建 🆕

| 组件 | 用途 |
|------|------|
| `ideStore.ts` | IDE 状态管理 |
| `IdeWorkspace.tsx` | IDE 主布局容器 |
| `IdeEditorTabs.tsx` | 多标签管理 |
| `useFileCache.ts` | 文件内容缓存 hook |

---

## 3. 架构设计

### 3.1 整体架构

```
┌────────────────────────────────────────────────────────────┐
│                      IdeWorkspace                          │
├────────────┬───────────────────────────────────────────────┤
│            │                                               │
│  IdeTree   │              IdeEditorArea                    │
│  (左侧)    │  ┌─────────────────────────────────────────┐  │
│            │  │ IdeEditorTabs                           │  │
│ ┌────────┐ │  │ [file1.ts] [file2.rs] [config.json]    │  │
│ │ 📁 src │ │  ├─────────────────────────────────────────┤  │
│ │  📄 a  │ │  │                                         │  │
│ │  📄 b  │ │  │        CodeMirror Editor                │  │
│ │ 📁 lib │ │  │                                         │  │
│ └────────┘ │  └─────────────────────────────────────────┘  │
│            ├───────────────────────────────────────────────┤
│            │              IdeTerminal                      │
│            │  ┌─────────────────────────────────────────┐  │
│            │  │ $ npm run build                         │  │
│            │  │ > Building...                           │  │
│            │  └─────────────────────────────────────────┘  │
└────────────┴───────────────────────────────────────────────┘
```

### 3.2 状态管理设计

```typescript
// src/store/ideStore.ts
import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';

interface IdeTab {
  id: string;
  path: string;           // 远程文件路径
  name: string;           // 文件名
  language: string;       // 语言类型
  content: string | null; // null = 未加载
  originalContent: string | null; // 用于 diff
  isDirty: boolean;
  isLoading: boolean;
  cursor?: { line: number; col: number };
  serverMtime?: number;   // 远程修改时间
}

interface IdeProject {
  rootPath: string;
  name: string;
  isGitRepo: boolean;
  gitBranch?: string;
}

interface IdeState {
  // 会话关联
  connectionId: string | null;  // SSH 连接 ID（复用连接池）
  sftpSessionId: string | null; // SFTP 会话 ID
  terminalSessionId: string | null; // 终端会话 ID
  
  // 项目状态
  project: IdeProject | null;
  
  // 编辑器状态
  tabs: IdeTab[];
  activeTabId: string | null;
  
  // 布局状态
  treeWidth: number;        // 文件树宽度
  terminalHeight: number;   // 终端高度
  terminalVisible: boolean;
  
  // Actions
  openProject: (connectionId: string, rootPath: string) => Promise<void>;
  closeProject: () => void;
  openFile: (path: string) => Promise<void>;
  closeTab: (tabId: string) => Promise<boolean>; // 返回 false 表示用户取消
  saveFile: (tabId: string) => Promise<void>;
  saveAllFiles: () => Promise<void>;
  setActiveTab: (tabId: string) => void;
  updateTabContent: (tabId: string, content: string) => void;
}
```

### 3.3 文件缓存策略

```typescript
// src/hooks/useFileCache.ts
interface CachedFile {
  path: string;
  content: string;
  mtime: number;
  cachedAt: number;
}

// 缓存策略：
// 1. 内存中最多保留 MAX_MEMORY_TABS = 10 个完整内容
// 2. 超出后，未修改的文件内容移至 IndexedDB
// 3. 已修改的文件永远保留在内存中
// 4. 重新激活标签时从 IndexedDB 恢复

const CACHE_DB_NAME = 'oxideterm-ide-cache';
const CACHE_STORE_NAME = 'files';
const MAX_MEMORY_TABS = 10;
const CACHE_TTL_MS = 24 * 60 * 60 * 1000; // 24 小时
```

---

## 4. 组件详细设计

### 4.1 目录结构

```
src/components/ide/
├── IdeWorkspace.tsx       # 主容器，管理布局
├── IdeTree.tsx            # 文件树（复用 SFTPView 逻辑）
├── IdeEditorArea.tsx      # 编辑器区域容器
├── IdeEditorTabs.tsx      # 标签栏
├── IdeEditor.tsx          # 单个编辑器实例
├── IdeTerminal.tsx        # 集成终端面板
├── IdeStatusBar.tsx       # 底部状态栏
├── IdeUnsavedGuard.tsx    # 未保存文件拦截器
├── dialogs/
│   ├── IdeOpenProjectDialog.tsx  # 打开项目对话框
│   └── IdeSaveConfirmDialog.tsx  # 保存确认对话框
└── hooks/
    ├── useIdeSession.ts   # 管理 IDE 相关会话
    ├── useFileCache.ts    # 文件缓存
    └── useGitStatus.ts    # Git 状态（可选）
```

### 4.2 IdeWorkspace 组件

```tsx
// src/components/ide/IdeWorkspace.tsx
interface IdeWorkspaceProps {
  connectionId: string;
  rootPath: string;
}

export function IdeWorkspace({ connectionId, rootPath }: IdeWorkspaceProps) {
  const { 
    project, 
    treeWidth, 
    terminalVisible, 
    terminalHeight 
  } = useIdeStore();
  
  return (
    <div className="flex h-full">
      {/* 文件树 - 可调整宽度 */}
      <Resizable
        width={treeWidth}
        minWidth={200}
        maxWidth={500}
        onResize={setTreeWidth}
      >
        <IdeTree />
      </Resizable>
      
      {/* 主编辑区 */}
      <div className="flex-1 flex flex-col">
        <IdeEditorArea />
        
        {/* 终端面板 - 可调整高度 */}
        {terminalVisible && (
          <Resizable
            height={terminalHeight}
            minHeight={100}
            maxHeight={400}
            direction="vertical"
            onResize={setTerminalHeight}
          >
            <IdeTerminal />
          </Resizable>
        )}
      </div>
      
      {/* 未保存文件拦截器 */}
      <IdeUnsavedGuard />
    </div>
  );
}
```

### 4.3 IdeEditorTabs 组件

```tsx
// src/components/ide/IdeEditorTabs.tsx
export function IdeEditorTabs() {
  const { tabs, activeTabId, setActiveTab, closeTab } = useIdeStore();
  const { t } = useTranslation();
  
  const handleClose = async (tabId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const tab = tabs.find(t => t.id === tabId);
    
    if (tab?.isDirty) {
      // 显示保存确认对话框
      const result = await showSaveConfirmDialog(tab.name);
      if (result === 'cancel') return;
      if (result === 'save') await saveFile(tabId);
    }
    
    closeTab(tabId);
  };
  
  return (
    <div className="flex items-center bg-zinc-900 border-b border-zinc-800 overflow-x-auto">
      {tabs.map(tab => (
        <div
          key={tab.id}
          onClick={() => setActiveTab(tab.id)}
          className={cn(
            "flex items-center gap-2 px-3 py-2 border-r border-zinc-800 cursor-pointer",
            "hover:bg-zinc-800 transition-colors",
            activeTabId === tab.id && "bg-zinc-800"
          )}
        >
          {/* 文件图标 */}
          <FileIcon language={tab.language} />
          
          {/* 文件名 */}
          <span className="text-sm truncate max-w-[120px]">
            {tab.name}
          </span>
          
          {/* 修改指示器 */}
          {tab.isDirty && (
            <span className="w-2 h-2 rounded-full bg-blue-500" />
          )}
          
          {/* 关闭按钮 */}
          <button
            onClick={(e) => handleClose(tab.id, e)}
            className="p-0.5 hover:bg-zinc-700 rounded"
          >
            <X className="w-3 h-3" />
          </button>
        </div>
      ))}
    </div>
  );
}
```

### 4.4 文件冲突处理

```typescript
// 保存时的冲突检测
async function saveFileWithConflictCheck(
  sessionId: string,
  path: string,
  content: string,
  expectedMtime: number | undefined
): Promise<SaveResult> {
  // 1. 先获取当前远程文件状态
  const stat = await api.sftpStat(sessionId, path);
  
  // 2. 检查是否有冲突
  if (expectedMtime && stat.modified !== expectedMtime) {
    return {
      status: 'conflict',
      localMtime: expectedMtime,
      remoteMtime: stat.modified,
    };
  }
  
  // 3. 无冲突，执行保存
  await api.sftpWriteContent(sessionId, path, content);
  const newStat = await api.sftpStat(sessionId, path);
  
  return {
    status: 'success',
    newMtime: newStat.modified,
  };
}

// 冲突解决策略
type ConflictResolution = 
  | 'overwrite'      // 覆盖远程
  | 'reload'         // 放弃本地，重新加载
  | 'save_as'        // 另存为
  | 'merge';         // 显示 diff（未来功能）
```

---

## 5. 后端 API 扩展

### 5.1 新增 Tauri 命令

```rust
// src-tauri/src/commands/ide.rs

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

use crate::sftp::session::{SftpRegistry, FileType};
use crate::sftp::types::PreviewContent;
use crate::sftp::error::SftpError;

/// 项目信息
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub root_path: String,
    pub name: String,
    pub is_git_repo: bool,
    pub git_branch: Option<String>,
    pub file_count: u32,  // 预估文件数
}

/// 打开项目（获取基本信息）
/// 
/// 注意：sftp_registry.get() 返回 Arc<Mutex<SftpSession>>，需要 lock().await
#[tauri::command]
pub async fn ide_open_project(
    session_id: String,
    path: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<ProjectInfo, String> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| format!("SFTP session not found: {}", session_id))?;
    
    let sftp = sftp.lock().await;
    
    // 验证目录存在
    let info = sftp.stat(&path).await
        .map_err(|e| format!("Path not found: {}", e))?;
    
    if info.file_type != FileType::Directory {
        return Err("Path is not a directory".to_string());
    }
    
    // 检查是否是 Git 仓库
    let git_path = format!("{}/.git", path);
    let is_git_repo = sftp.stat(&git_path).await.is_ok();
    
    // 获取 Git 分支（如果是 Git 仓库）
    let git_branch = if is_git_repo {
        get_git_branch(&sftp, &path).await.ok()
    } else {
        None
    };
    
    // 项目名称（目录名）
    let name = path.rsplit('/').next()
        .unwrap_or("project")
        .to_string();
    
    Ok(ProjectInfo {
        root_path: path,
        name,
        is_git_repo,
        git_branch,
        file_count: 0, // 延迟计算
    })
}

/// 获取 Git 分支名
/// 
/// 注意：这里使用 sftp.preview() 读取 .git/HEAD 文件内容，
/// 因为现有 API 没有提供直接读取小文件的方法。
/// 或者也可以新增一个 sftp.read_text_file() 方法。
async fn get_git_branch(
    sftp: &tokio::sync::MutexGuard<'_, crate::sftp::session::SftpSession>,
    project_path: &str
) -> Result<String, String> {
    let head_path = format!("{}/.git/HEAD", project_path);
    
    // 使用 preview 读取内容
    let preview = sftp.preview(&head_path).await
        .map_err(|e| e.to_string())?;
    
    let content = match preview {
        PreviewContent::Text { data, .. } => data,
        _ => return Err("HEAD is not a text file".to_string()),
    };
    
    // 解析 ref: refs/heads/main
    if let Some(branch) = content.strip_prefix("ref: refs/heads/") {
        Ok(branch.trim().to_string())
    } else {
        // Detached HEAD - 返回短 hash
        Ok(content.chars().take(7).collect())
    }
}

/// 批量获取文件状态（用于 Git 状态显示）
#[tauri::command]
pub async fn ide_batch_stat(
    session_id: String,
    paths: Vec<String>,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<Vec<Option<FileStatInfo>>, String> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| format!("SFTP session not found: {}", session_id))?;
    
    let sftp = sftp.lock().await;
    
    let mut results = Vec::with_capacity(paths.len());
    for path in paths {
        let stat = sftp.stat(&path).await.ok().map(|info| FileStatInfo {
            size: info.size,
            mtime: info.modified as u64,
            is_dir: info.file_type == FileType::Directory,
        });
        results.push(stat);
    }
    
    Ok(results)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatInfo {
    pub size: u64,
    pub mtime: u64,
    pub is_dir: bool,
}

/// 项目内搜索（安全版本）
/// 
/// ⚠️ 重要：这个功能需要通过 SSH 执行 grep 命令，而不是 SFTP。
/// 需要使用终端会话来执行远程命令。这是 Phase 4 的功能，
/// 暂时返回空结果，待终端 API 完善后再实现。
#[tauri::command]
pub async fn ide_search_in_project(
    _session_id: String,
    _project_path: String,
    query: String,
    max_results: u32,
    // 注意：这里需要 SSH 会话而不是 SFTP
    // ssh_registry: State<'_, Arc<SshRegistry>>,
) -> Result<SearchResults, String> {
    // 安全检查：限制最大结果数
    let _max_results = max_results.min(500);
    
    // 安全检查：验证 query 不包含危险字符
    if query.contains(|c: char| c == '\0' || c == '\n' || c == '\r') {
        return Err("Invalid search query".to_string());
    }
    
    // TODO: Phase 4 实现
    // 需要：
    // 1. 获取 SSH 会话（不是 SFTP）
    // 2. 执行 grep -r --include='*.{rs,ts,tsx,js,jsx,py,...}' -n -l "query" project_path
    // 3. 解析输出并返回结果
    
    Ok(SearchResults {
        matches: vec![],
        truncated: false,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub matches: Vec<SearchMatch>,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub preview: String,
}
```

### 5.2 安全约束

```rust
// 文件大小限制
const MAX_EDITABLE_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB

// 二进制文件检测（复用现有 preview 逻辑）
fn is_likely_binary(content: &[u8]) -> bool {
    // 检查前 8KB 是否包含 NULL 字节
    let check_len = content.len().min(8192);
    content[..check_len].contains(&0)
}

/// 检查文件是否可编辑
/// 
/// 注意：这里复用 sftp.preview() 的逻辑，因为现有 SftpSession 没有 read_file_range 方法。
/// preview() 已经实现了文件类型检测、大小检查、二进制检测等功能。
#[tauri::command]
pub async fn ide_check_file(
    session_id: String,
    path: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<FileCheckResult, String> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;
    
    let sftp = sftp.lock().await;
    
    // 先获取文件信息
    let info = sftp.stat(&path).await
        .map_err(|e| format!("File not found: {}", e))?;
    
    if info.file_type == FileType::Directory {
        return Ok(FileCheckResult::NotEditable { 
            reason: "Is a directory".to_string() 
        });
    }
    
    if info.size > MAX_EDITABLE_FILE_SIZE {
        return Ok(FileCheckResult::TooLarge { 
            size: info.size,
            limit: MAX_EDITABLE_FILE_SIZE,
        });
    }
    
    // 使用现有 preview 逻辑检测文件类型
    // preview 返回 Text/Hex/Image 等，我们只接受 Text
    let preview = sftp.preview(&path).await
        .map_err(|e| e.to_string())?;
    
    match preview {
        PreviewContent::Text { .. } => Ok(FileCheckResult::Editable {
            size: info.size,
            mtime: info.modified as u64,
        }),
        PreviewContent::TooLarge { size, max_size, .. } => Ok(FileCheckResult::TooLarge {
            size,
            limit: max_size,
        }),
        PreviewContent::Hex { .. } => Ok(FileCheckResult::Binary),
        _ => Ok(FileCheckResult::NotEditable {
            reason: "Unsupported file type".to_string(),
        }),
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileCheckResult {
    Editable { size: u64, mtime: u64 },
    TooLarge { size: u64, limit: u64 },
    Binary,
    NotEditable { reason: String },
}
```

---

## 6. 分阶段实施计划（详细步骤版）

> ⚠️ 本节为逐步操作指南，每个任务都包含具体的文件修改位置和代码示例

---

### Phase 1: 基础框架（2 周）

**目标：** IDE 标签可用，能打开项目并浏览文件

---

#### 任务 1.1: 扩展 TabType（0.5d）

**文件：** `src/types/index.ts`

**位置：** 第 323 行

**操作：** 找到 `TabType` 定义，添加 `'ide'`

```typescript
// 修改前（第 323 行）
export type TabType = 'terminal' | 'sftp' | 'forwards' | 'settings' | 'connection_monitor' | 'connection_pool' | 'topology' | 'local_terminal';

// 修改后
export type TabType = 'terminal' | 'sftp' | 'forwards' | 'settings' | 'connection_monitor' | 'connection_pool' | 'topology' | 'local_terminal' | 'ide';
```

**验证：** `pnpm tsc --noEmit` 无类型错误

---

#### 任务 1.2: 创建 ideStore.ts（1d）

**文件：** `src/store/ideStore.ts`（新建）

**完整内容：**

```typescript
// src/store/ideStore.ts
import { create } from 'zustand';
import { subscribeWithSelector, persist } from 'zustand/middleware';
import { api } from '../lib/api';

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

export interface IdeTab {
  id: string;
  path: string;           // 远程文件完整路径
  name: string;           // 文件名（显示用）
  language: string;       // CodeMirror 语言标识
  content: string | null; // null = 尚未加载
  originalContent: string | null; // 打开时的原始内容（用于 diff/dirty 检测）
  isDirty: boolean;
  isLoading: boolean;
  cursor?: { line: number; col: number };
  serverMtime?: number;   // 服务器端文件修改时间（Unix timestamp 秒）
  lastAccessTime: number; // 最后访问时间（用于 LRU 驱逐）
}

export interface IdeProject {
  rootPath: string;
  name: string;
  isGitRepo: boolean;
  gitBranch?: string;
}

interface IdeState {
  // ─── 会话关联 ───
  connectionId: string | null;    // SSH 连接 ID（复用连接池）
  sftpSessionId: string | null;   // SFTP 会话 ID
  terminalSessionId: string | null; // 终端会话 ID（可选）
  
  // ─── 项目状态 ───
  project: IdeProject | null;
  
  // ─── 编辑器状态 ───
  tabs: IdeTab[];
  activeTabId: string | null;
  
  // ─── 布局状态 ───
  treeWidth: number;
  terminalHeight: number;
  terminalVisible: boolean;
  
  // ─── 文件树状态 ───
  expandedPaths: Set<string>;  // 展开的目录路径
}

interface IdeActions {
  // 项目操作
  openProject: (connectionId: string, sftpSessionId: string, rootPath: string) => Promise<void>;
  closeProject: () => void;
  
  // 文件操作
  openFile: (path: string) => Promise<void>;
  closeTab: (tabId: string) => Promise<boolean>;
  closeAllTabs: () => Promise<boolean>;
  saveFile: (tabId: string) => Promise<void>;
  saveAllFiles: () => Promise<void>;
  
  // 标签操作
  setActiveTab: (tabId: string) => void;
  updateTabContent: (tabId: string, content: string) => void;
  updateTabCursor: (tabId: string, line: number, col: number) => void;
  
  // 布局操作
  setTreeWidth: (width: number) => void;
  setTerminalHeight: (height: number) => void;
  toggleTerminal: () => void;
  
  // 文件树操作
  togglePath: (path: string) => void;
  
  // 终端操作
  setTerminalSession: (sessionId: string | null) => void;
  
  // 内部方法
  _findTabByPath: (path: string) => IdeTab | undefined;
}

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

const MAX_OPEN_TABS = 20;
const WARN_TAB_COUNT = 15;

// ═══════════════════════════════════════════════════════════════════════════
// Store
// ═══════════════════════════════════════════════════════════════════════════

export const useIdeStore = create<IdeState & IdeActions>()(
  subscribeWithSelector(
    persist(
      (set, get) => ({
        // ─── Initial State ───
        connectionId: null,
        sftpSessionId: null,
        terminalSessionId: null,
        project: null,
        tabs: [],
        activeTabId: null,
        treeWidth: 280,
        terminalHeight: 200,
        terminalVisible: false,
        expandedPaths: new Set<string>(),

        // ─── Project Actions ───
        openProject: async (connectionId, sftpSessionId, rootPath) => {
          // 调用后端获取项目信息
          const projectInfo = await api.ideOpenProject(sftpSessionId, rootPath);
          
          set({
            connectionId,
            sftpSessionId,
            project: {
              rootPath: projectInfo.rootPath,
              name: projectInfo.name,
              isGitRepo: projectInfo.isGitRepo,
              gitBranch: projectInfo.gitBranch ?? undefined,
            },
            tabs: [],
            activeTabId: null,
            expandedPaths: new Set([rootPath]), // 默认展开根目录
          });
        },

        closeProject: () => {
          const { tabs } = get();
          const hasDirty = tabs.some(t => t.isDirty);
          
          if (hasDirty) {
            // 调用方需要先处理未保存文件
            console.warn('closeProject called with dirty tabs');
          }
          
          set({
            connectionId: null,
            sftpSessionId: null,
            terminalSessionId: null,
            project: null,
            tabs: [],
            activeTabId: null,
            expandedPaths: new Set(),
          });
        },

        // ─── File Actions ───
        openFile: async (path) => {
          const { tabs, sftpSessionId, _findTabByPath } = get();
          
          if (!sftpSessionId) {
            throw new Error('No SFTP session');
          }
          
          // 检查是否已打开
          const existingTab = _findTabByPath(path);
          if (existingTab) {
            set({ activeTabId: existingTab.id });
            return;
          }
          
          // 检查标签数量限制
          if (tabs.length >= MAX_OPEN_TABS) {
            throw new Error(`Maximum ${MAX_OPEN_TABS} tabs allowed`);
          }
          
          // 创建新标签（loading 状态）
          const tabId = crypto.randomUUID();
          const fileName = path.split('/').pop() || path;
          const extension = fileName.includes('.') ? fileName.split('.').pop() || '' : '';
          
          const newTab: IdeTab = {
            id: tabId,
            path,
            name: fileName,
            language: extensionToLanguage(extension),
            content: null,
            originalContent: null,
            isDirty: false,
            isLoading: true,
            lastAccessTime: Date.now(),
          };
          
          set(state => ({
            tabs: [...state.tabs, newTab],
            activeTabId: tabId,
          }));
          
          try {
            // 使用 preview API 加载文件内容
            const preview = await api.sftpPreview(sftpSessionId, path);
            
            if ('Text' in preview) {
              const stat = await api.sftpStat(sftpSessionId, path);
              
              set(state => ({
                tabs: state.tabs.map(t => 
                  t.id === tabId 
                    ? {
                        ...t,
                        content: preview.Text.data,
                        originalContent: preview.Text.data,
                        language: preview.Text.language || extensionToLanguage(extension),
                        isLoading: false,
                        serverMtime: stat.modified ?? undefined,
                      }
                    : t
                ),
              }));
            } else {
              // 非文本文件，关闭标签并报错
              set(state => ({
                tabs: state.tabs.filter(t => t.id !== tabId),
                activeTabId: state.tabs.length > 1 ? state.tabs[0].id : null,
              }));
              throw new Error('Cannot edit non-text file');
            }
          } catch (error) {
            // 加载失败，移除标签
            set(state => ({
              tabs: state.tabs.filter(t => t.id !== tabId),
              activeTabId: state.tabs.length > 1 ? state.tabs[0].id : null,
            }));
            throw error;
          }
        },

        closeTab: async (tabId) => {
          const { tabs, activeTabId } = get();
          const tab = tabs.find(t => t.id === tabId);
          
          if (!tab) return true;
          
          // 如果有未保存更改，调用方需要先确认
          if (tab.isDirty) {
            return false; // 返回 false 表示需要用户确认
          }
          
          const newTabs = tabs.filter(t => t.id !== tabId);
          const newActiveId = activeTabId === tabId
            ? (newTabs.length > 0 ? newTabs[newTabs.length - 1].id : null)
            : activeTabId;
          
          set({
            tabs: newTabs,
            activeTabId: newActiveId,
          });
          
          return true;
        },

        closeAllTabs: async () => {
          const { tabs } = get();
          const hasDirty = tabs.some(t => t.isDirty);
          
          if (hasDirty) {
            return false; // 需要用户确认
          }
          
          set({ tabs: [], activeTabId: null });
          return true;
        },

        saveFile: async (tabId) => {
          const { tabs, sftpSessionId } = get();
          const tab = tabs.find(t => t.id === tabId);
          
          if (!tab || !sftpSessionId || tab.content === null) {
            throw new Error('Cannot save: invalid state');
          }
          
          // 检查冲突
          const stat = await api.sftpStat(sftpSessionId, tab.path);
          if (tab.serverMtime && stat.modified && stat.modified !== tab.serverMtime) {
            throw new Error('CONFLICT'); // 调用方需要处理冲突
          }
          
          // 保存文件
          const result = await api.sftpWriteContent(sftpSessionId, tab.path, tab.content);
          
          set(state => ({
            tabs: state.tabs.map(t =>
              t.id === tabId
                ? {
                    ...t,
                    isDirty: false,
                    originalContent: t.content,
                    serverMtime: result.mtime ?? undefined,
                  }
                : t
            ),
          }));
        },

        saveAllFiles: async () => {
          const { tabs, saveFile } = get();
          const dirtyTabs = tabs.filter(t => t.isDirty);
          
          for (const tab of dirtyTabs) {
            await saveFile(tab.id);
          }
        },

        // ─── Tab Actions ───
        setActiveTab: (tabId) => {
          set(state => ({
            activeTabId: tabId,
            tabs: state.tabs.map(t =>
              t.id === tabId
                ? { ...t, lastAccessTime: Date.now() }
                : t
            ),
          }));
        },

        updateTabContent: (tabId, content) => {
          set(state => ({
            tabs: state.tabs.map(t =>
              t.id === tabId
                ? {
                    ...t,
                    content,
                    isDirty: content !== t.originalContent,
                  }
                : t
            ),
          }));
        },

        updateTabCursor: (tabId, line, col) => {
          set(state => ({
            tabs: state.tabs.map(t =>
              t.id === tabId
                ? { ...t, cursor: { line, col } }
                : t
            ),
          }));
        },

        // ─── Layout Actions ───
        setTreeWidth: (width) => set({ treeWidth: width }),
        setTerminalHeight: (height) => set({ terminalHeight: height }),
        toggleTerminal: () => set(state => ({ terminalVisible: !state.terminalVisible })),

        // ─── File Tree Actions ───
        togglePath: (path) => {
          set(state => {
            const newSet = new Set(state.expandedPaths);
            if (newSet.has(path)) {
              newSet.delete(path);
            } else {
              newSet.add(path);
            }
            return { expandedPaths: newSet };
          });
        },

        // ─── Terminal Actions ───
        setTerminalSession: (sessionId) => set({ terminalSessionId: sessionId }),

        // ─── Internal ───
        _findTabByPath: (path) => {
          return get().tabs.find(t => t.path === path);
        },
      }),
      {
        name: 'oxideterm-ide',
        // 只持久化布局设置，不持久化项目/标签状态
        partialize: (state) => ({
          treeWidth: state.treeWidth,
          terminalHeight: state.terminalHeight,
        }),
      }
    )
  )
);

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

function extensionToLanguage(ext: string): string {
  const map: Record<string, string> = {
    ts: 'typescript',
    tsx: 'typescript',
    js: 'javascript',
    jsx: 'javascript',
    rs: 'rust',
    py: 'python',
    go: 'go',
    java: 'java',
    c: 'c',
    cpp: 'cpp',
    h: 'c',
    hpp: 'cpp',
    cs: 'csharp',
    rb: 'ruby',
    php: 'php',
    swift: 'swift',
    kt: 'kotlin',
    scala: 'scala',
    json: 'json',
    yaml: 'yaml',
    yml: 'yaml',
    toml: 'toml',
    xml: 'xml',
    html: 'html',
    css: 'css',
    scss: 'css',
    less: 'css',
    md: 'markdown',
    sql: 'sql',
    sh: 'shell',
    bash: 'shell',
    zsh: 'shell',
    dockerfile: 'dockerfile',
  };
  return map[ext.toLowerCase()] || 'plaintext';
}

// Selector hooks for performance
export const useIdeProject = () => useIdeStore(state => state.project);
export const useIdeTabs = () => useIdeStore(state => state.tabs);
export const useIdeActiveTab = () => useIdeStore(state => 
  state.tabs.find(t => t.id === state.activeTabId)
);
export const useIdeDirtyCount = () => useIdeStore(state => 
  state.tabs.filter(t => t.isDirty).length
);
```

**验证：** `pnpm tsc --noEmit` 无类型错误

---

#### 任务 1.3: 添加 API 函数（0.5d）

**文件：** `src/lib/api.ts`

**位置：** 在文件末尾（约第 1100 行前）添加

```typescript
  // ═══════════════════════════════════════════════════════════════════════════
  // IDE Mode Commands
  // ═══════════════════════════════════════════════════════════════════════════

  ideOpenProject: async (sessionId: string, path: string): Promise<{
    rootPath: string;
    name: string;
    isGitRepo: boolean;
    gitBranch: string | null;
    fileCount: number;
  }> => {
    if (USE_MOCK) return { rootPath: path, name: 'mock', isGitRepo: false, gitBranch: null, fileCount: 0 };
    return invoke('ide_open_project', { sessionId, path });
  },

  ideCheckFile: async (sessionId: string, path: string): Promise<
    | { type: 'editable'; size: number; mtime: number }
    | { type: 'too_large'; size: number; limit: number }
    | { type: 'binary' }
    | { type: 'not_editable'; reason: string }
  > => {
    if (USE_MOCK) return { type: 'editable', size: 100, mtime: Date.now() / 1000 };
    return invoke('ide_check_file', { sessionId, path });
  },

  ideBatchStat: async (sessionId: string, paths: string[]): Promise<Array<{
    size: number;
    mtime: number;
    isDir: boolean;
  } | null>> => {
    if (USE_MOCK) return paths.map(() => null);
    return invoke('ide_batch_stat', { sessionId, paths });
  },
```

**验证：** `pnpm tsc --noEmit` 无类型错误

---

#### 任务 1.4: 创建后端 IDE 模块（1d）

##### Step 1: 创建 ide.rs 文件

**文件：** `src-tauri/src/commands/ide.rs`（新建）

```rust
//! IDE Mode Commands
//!
//! Commands for the lightweight IDE mode feature.

use serde::Serialize;
use std::sync::Arc;
use tauri::State;

use crate::sftp::error::SftpError;
use crate::sftp::session::SftpRegistry;
use crate::sftp::types::{FileType, PreviewContent};

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub root_path: String,
    pub name: String,
    pub is_git_repo: bool,
    pub git_branch: Option<String>,
    pub file_count: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatInfo {
    pub size: u64,
    pub mtime: u64,
    pub is_dir: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileCheckResult {
    Editable { size: u64, mtime: u64 },
    TooLarge { size: u64, limit: u64 },
    Binary,
    NotEditable { reason: String },
}

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

const MAX_EDITABLE_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB

// ═══════════════════════════════════════════════════════════════════════════
// Commands
// ═══════════════════════════════════════════════════════════════════════════

/// Open a project directory and return basic info
#[tauri::command]
pub async fn ide_open_project(
    session_id: String,
    path: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<ProjectInfo, String> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| format!("SFTP session not found: {}", session_id))?;

    let sftp = sftp.lock().await;

    // Verify directory exists
    let info = sftp
        .stat(&path)
        .await
        .map_err(|e| format!("Path not found: {}", e))?;

    if info.file_type != FileType::Directory {
        return Err("Path is not a directory".to_string());
    }

    // Check if it's a Git repository
    let git_path = format!("{}/.git", path);
    let is_git_repo = sftp.stat(&git_path).await.is_ok();

    // Get Git branch if applicable
    let git_branch = if is_git_repo {
        get_git_branch_inner(&sftp, &path).await.ok()
    } else {
        None
    };

    // Extract project name from path
    let name = path
        .rsplit('/')
        .next()
        .unwrap_or("project")
        .to_string();

    Ok(ProjectInfo {
        root_path: path,
        name,
        is_git_repo,
        git_branch,
        file_count: 0, // Defer counting
    })
}

/// Check if a file is editable
#[tauri::command]
pub async fn ide_check_file(
    session_id: String,
    path: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<FileCheckResult, String> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| format!("SFTP session not found: {}", session_id))?;

    let sftp = sftp.lock().await;

    // Get file info
    let info = sftp
        .stat(&path)
        .await
        .map_err(|e| format!("File not found: {}", e))?;

    if info.file_type == FileType::Directory {
        return Ok(FileCheckResult::NotEditable {
            reason: "Is a directory".to_string(),
        });
    }

    if info.size > MAX_EDITABLE_FILE_SIZE {
        return Ok(FileCheckResult::TooLarge {
            size: info.size,
            limit: MAX_EDITABLE_FILE_SIZE,
        });
    }

    // Use preview to detect file type
    let preview = sftp.preview(&path).await.map_err(|e| e.to_string())?;

    match preview {
        PreviewContent::Text { .. } => Ok(FileCheckResult::Editable {
            size: info.size,
            mtime: info.modified as u64,
        }),
        PreviewContent::TooLarge { size, max_size, .. } => Ok(FileCheckResult::TooLarge {
            size,
            limit: max_size,
        }),
        PreviewContent::Hex { .. } => Ok(FileCheckResult::Binary),
        _ => Ok(FileCheckResult::NotEditable {
            reason: "Unsupported file type".to_string(),
        }),
    }
}

/// Batch stat multiple paths
#[tauri::command]
pub async fn ide_batch_stat(
    session_id: String,
    paths: Vec<String>,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<Vec<Option<FileStatInfo>>, String> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| format!("SFTP session not found: {}", session_id))?;

    let sftp = sftp.lock().await;

    let mut results = Vec::with_capacity(paths.len());
    for path in paths {
        let stat = sftp.stat(&path).await.ok().map(|info| FileStatInfo {
            size: info.size,
            mtime: info.modified as u64,
            is_dir: info.file_type == FileType::Directory,
        });
        results.push(stat);
    }

    Ok(results)
}

// ═══════════════════════════════════════════════════════════════════════════
// Internal Helpers
// ═══════════════════════════════════════════════════════════════════════════

async fn get_git_branch_inner(
    sftp: &tokio::sync::MutexGuard<'_, crate::sftp::session::SftpSession>,
    project_path: &str,
) -> Result<String, String> {
    let head_path = format!("{}/.git/HEAD", project_path);

    // Use preview to read the file
    let preview = sftp.preview(&head_path).await.map_err(|e| e.to_string())?;

    let content = match preview {
        PreviewContent::Text { data, .. } => data,
        _ => return Err("HEAD is not a text file".to_string()),
    };

    // Parse: ref: refs/heads/main
    if let Some(branch) = content.strip_prefix("ref: refs/heads/") {
        Ok(branch.trim().to_string())
    } else {
        // Detached HEAD - return short hash
        Ok(content.chars().take(7).collect())
    }
}
```

##### Step 2: 在 mod.rs 中注册模块

**文件：** `src-tauri/src/commands/mod.rs`

**位置：** 在第 15 行左右（其他 mod 声明之后）添加

```rust
// 修改前
pub mod scroll;
pub mod session_tree;
pub mod sftp;
pub mod ssh;

// 修改后
pub mod ide;  // ← 添加这行
pub mod scroll;
pub mod session_tree;
pub mod sftp;
pub mod ssh;
```

**位置：** 在第 30 行左右（其他 pub use 之后）添加

```rust
// 修改前
pub use scroll::*;
pub use session_tree::*;
pub use sftp::*;
pub use ssh::*;

// 修改后
pub use ide::*;  // ← 添加这行
pub use scroll::*;
pub use session_tree::*;
pub use sftp::*;
pub use ssh::*;
```

##### Step 3: 在 lib.rs 中注册命令

**文件：** `src-tauri/src/lib.rs`

**位置：** 在 `#[cfg(feature = "local-terminal")]` 块内（约第 440 行），在 SFTP commands 注释之前添加

```rust
            // IDE Mode commands
            commands::ide_open_project,
            commands::ide_check_file,
            commands::ide_batch_stat,
```

**位置：** 在 `#[cfg(not(feature = "local-terminal"))]` 块内（约第 580 行），同样位置添加相同内容

**验证：** `cd src-tauri && cargo check`

---

#### 任务 1.5: 创建 IdeWorkspace 组件（1d）

**文件：** `src/components/ide/IdeWorkspace.tsx`（新建）

**先创建目录结构：**
```bash
mkdir -p src/components/ide/dialogs
mkdir -p src/components/ide/hooks
```

**完整内容：**

```tsx
// src/components/ide/IdeWorkspace.tsx
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2 } from 'lucide-react';
import { useIdeStore, useIdeProject, useIdeTabs } from '../../store/ideStore';
import { IdeTree } from './IdeTree';
import { IdeEditorArea } from './IdeEditorArea';
import { IdeTerminal } from './IdeTerminal';
import { IdeStatusBar } from './IdeStatusBar';
import { cn } from '../../lib/utils';

interface IdeWorkspaceProps {
  connectionId: string;
  sftpSessionId: string;
  rootPath: string;
}

export function IdeWorkspace({ connectionId, sftpSessionId, rootPath }: IdeWorkspaceProps) {
  const { t } = useTranslation();
  const project = useIdeProject();
  const tabs = useIdeTabs();
  const { 
    openProject, 
    treeWidth, 
    terminalVisible, 
    terminalHeight,
    setTreeWidth,
    setTerminalHeight,
  } = useIdeStore();
  
  // 初始化项目
  useEffect(() => {
    if (!project || project.rootPath !== rootPath) {
      openProject(connectionId, sftpSessionId, rootPath).catch(console.error);
    }
  }, [connectionId, sftpSessionId, rootPath, project, openProject]);
  
  // 加载中状态
  if (!project) {
    return (
      <div className="flex items-center justify-center h-full bg-zinc-900">
        <Loader2 className="w-8 h-8 animate-spin text-orange-500" />
        <span className="ml-3 text-zinc-400">{t('ide.loading_project')}</span>
      </div>
    );
  }
  
  return (
    <div className="flex flex-col h-full bg-zinc-900">
      {/* 主工作区 */}
      <div className="flex flex-1 overflow-hidden">
        {/* 文件树（左侧） */}
        <div 
          className="flex-shrink-0 border-r border-zinc-800 overflow-hidden"
          style={{ width: treeWidth }}
        >
          <IdeTree />
        </div>
        
        {/* 可拖拽分隔线 */}
        <div
          className="w-1 bg-zinc-800 hover:bg-orange-500/50 cursor-col-resize transition-colors"
          onMouseDown={(e) => {
            e.preventDefault();
            const startX = e.clientX;
            const startWidth = treeWidth;
            
            const onMouseMove = (e: MouseEvent) => {
              const delta = e.clientX - startX;
              const newWidth = Math.max(200, Math.min(500, startWidth + delta));
              setTreeWidth(newWidth);
            };
            
            const onMouseUp = () => {
              document.removeEventListener('mousemove', onMouseMove);
              document.removeEventListener('mouseup', onMouseUp);
            };
            
            document.addEventListener('mousemove', onMouseMove);
            document.addEventListener('mouseup', onMouseUp);
          }}
        />
        
        {/* 编辑器区域（右侧） */}
        <div className="flex-1 flex flex-col overflow-hidden">
          <IdeEditorArea />
          
          {/* 终端面板（底部） */}
          {terminalVisible && (
            <>
              {/* 可拖拽分隔线 */}
              <div
                className="h-1 bg-zinc-800 hover:bg-orange-500/50 cursor-row-resize transition-colors"
                onMouseDown={(e) => {
                  e.preventDefault();
                  const startY = e.clientY;
                  const startHeight = terminalHeight;
                  
                  const onMouseMove = (e: MouseEvent) => {
                    const delta = startY - e.clientY;
                    const newHeight = Math.max(100, Math.min(400, startHeight + delta));
                    setTerminalHeight(newHeight);
                  };
                  
                  const onMouseUp = () => {
                    document.removeEventListener('mousemove', onMouseMove);
                    document.removeEventListener('mouseup', onMouseUp);
                  };
                  
                  document.addEventListener('mousemove', onMouseMove);
                  document.addEventListener('mouseup', onMouseUp);
                }}
              />
              <div style={{ height: terminalHeight }}>
                <IdeTerminal />
              </div>
            </>
          )}
        </div>
      </div>
      
      {/* 状态栏 */}
      <IdeStatusBar />
    </div>
  );
}
```

---

#### 任务 1.6: 创建占位组件（0.5d）

以下为 Phase 1 的占位组件，后续 Phase 会完善：

**文件：** `src/components/ide/IdeTree.tsx`

```tsx
// src/components/ide/IdeTree.tsx
import { useTranslation } from 'react-i18next';
import { Folder } from 'lucide-react';
import { useIdeProject } from '../../store/ideStore';

export function IdeTree() {
  const { t } = useTranslation();
  const project = useIdeProject();
  
  if (!project) {
    return <div className="p-4 text-zinc-500">{t('ide.no_project')}</div>;
  }
  
  return (
    <div className="h-full flex flex-col bg-zinc-900">
      {/* 项目标题 */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-zinc-800">
        <Folder className="w-4 h-4 text-orange-500" />
        <span className="text-sm font-medium truncate">{project.name}</span>
        {project.isGitRepo && project.gitBranch && (
          <span className="text-xs text-zinc-500 ml-auto">{project.gitBranch}</span>
        )}
      </div>
      
      {/* 文件列表（Phase 1 占位） */}
      <div className="flex-1 p-4 text-zinc-500 text-sm">
        {t('ide.file_tree_placeholder')}
      </div>
    </div>
  );
}
```

**文件：** `src/components/ide/IdeEditorArea.tsx`

```tsx
// src/components/ide/IdeEditorArea.tsx
import { useTranslation } from 'react-i18next';
import { Code2 } from 'lucide-react';
import { useIdeTabs, useIdeActiveTab } from '../../store/ideStore';

export function IdeEditorArea() {
  const { t } = useTranslation();
  const tabs = useIdeTabs();
  const activeTab = useIdeActiveTab();
  
  if (tabs.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center text-zinc-500">
        <Code2 className="w-16 h-16 mb-4 opacity-20" />
        <p>{t('ide.no_open_files')}</p>
        <p className="text-sm mt-1">{t('ide.click_to_open')}</p>
      </div>
    );
  }
  
  return (
    <div className="flex-1 flex flex-col">
      {/* 标签栏（Phase 2 实现） */}
      <div className="h-9 bg-zinc-900 border-b border-zinc-800 flex items-center px-2 text-sm text-zinc-400">
        {tabs.map(tab => (
          <span key={tab.id} className="px-2">{tab.name}</span>
        ))}
      </div>
      
      {/* 编辑器（Phase 2 实现） */}
      <div className="flex-1 bg-zinc-950 p-4 text-zinc-500">
        {activeTab ? `Editing: ${activeTab.path}` : 'No file selected'}
      </div>
    </div>
  );
}
```

**文件：** `src/components/ide/IdeTerminal.tsx`

```tsx
// src/components/ide/IdeTerminal.tsx
import { useTranslation } from 'react-i18next';
import { Terminal } from 'lucide-react';

export function IdeTerminal() {
  const { t } = useTranslation();
  
  return (
    <div className="h-full bg-zinc-950 flex items-center justify-center text-zinc-500">
      <Terminal className="w-8 h-8 mr-2 opacity-20" />
      <span>{t('ide.terminal_placeholder')}</span>
    </div>
  );
}
```

**文件：** `src/components/ide/IdeStatusBar.tsx`

```tsx
// src/components/ide/IdeStatusBar.tsx
import { useIdeProject, useIdeActiveTab, useIdeDirtyCount } from '../../store/ideStore';
import { GitBranch } from 'lucide-react';

export function IdeStatusBar() {
  const project = useIdeProject();
  const activeTab = useIdeActiveTab();
  const dirtyCount = useIdeDirtyCount();
  
  return (
    <div className="h-6 bg-zinc-800 border-t border-zinc-700 flex items-center px-3 text-xs text-zinc-400">
      {/* Git 分支 */}
      {project?.isGitRepo && project.gitBranch && (
        <div className="flex items-center gap-1 mr-4">
          <GitBranch className="w-3 h-3" />
          <span>{project.gitBranch}</span>
        </div>
      )}
      
      {/* 光标位置 */}
      {activeTab?.cursor && (
        <span className="mr-4">
          Ln {activeTab.cursor.line}, Col {activeTab.cursor.col}
        </span>
      )}
      
      {/* 语言 */}
      {activeTab && (
        <span className="mr-4">{activeTab.language}</span>
      )}
      
      {/* 未保存文件数 */}
      {dirtyCount > 0 && (
        <span className="ml-auto text-orange-500">
          {dirtyCount} unsaved
        </span>
      )}
    </div>
  );
}
```

**文件：** `src/components/ide/index.ts`（导出文件）

```typescript
// src/components/ide/index.ts
export { IdeWorkspace } from './IdeWorkspace';
export { IdeTree } from './IdeTree';
export { IdeEditorArea } from './IdeEditorArea';
export { IdeTerminal } from './IdeTerminal';
export { IdeStatusBar } from './IdeStatusBar';
```

---

#### 任务 1.7: 添加 i18n 键值（0.5d）

**文件：** 所有 `src/locales/*/common.json` 文件

**添加以下键值（以 en 为例）：**

```json
{
  "ide": {
    "loading_project": "Loading project...",
    "no_project": "No project opened",
    "file_tree_placeholder": "File tree will appear here",
    "no_open_files": "No open files",
    "click_to_open": "Double-click a file in the tree to open",
    "terminal_placeholder": "Terminal (Phase 3)",
    "open_project": "Open Project",
    "close_project": "Close Project",
    "select_folder": "Select a folder as project root",
    "unsaved_changes": "The following files have unsaved changes:",
    "save_all": "Save All",
    "discard_all": "Discard All",
    "file_conflict": "File Conflict",
    "file_conflict_desc": "The remote file has been modified. Choose how to proceed:",
    "conflict_overwrite": "Overwrite Remote",
    "conflict_reload": "Reload File",
    "conflict_save_as": "Save As",
    "file_too_large": "File Too Large",
    "file_too_large_desc": "File size {{size}} exceeds limit {{limit}}",
    "file_binary": "Cannot edit binary file",
    "terminal_toggle": "Toggle Terminal",
    "git_branch": "Branch: {{branch}}",
    "search_placeholder": "Search files..."
  }
}
```

**中文版本 `src/locales/zh-CN/common.json`：**

```json
{
  "ide": {
    "loading_project": "正在加载项目...",
    "no_project": "未打开项目",
    "file_tree_placeholder": "文件树将显示在这里",
    "no_open_files": "没有打开的文件",
    "click_to_open": "双击文件树中的文件以打开",
    "terminal_placeholder": "终端（第三阶段）",
    "open_project": "打开项目",
    "close_project": "关闭项目",
    "select_folder": "选择文件夹作为项目根目录",
    "unsaved_changes": "以下文件有未保存的更改：",
    "save_all": "全部保存",
    "discard_all": "全部放弃",
    "file_conflict": "文件冲突",
    "file_conflict_desc": "远程文件已被修改，请选择处理方式：",
    "conflict_overwrite": "覆盖远程",
    "conflict_reload": "重新加载",
    "conflict_save_as": "另存为",
    "file_too_large": "文件过大",
    "file_too_large_desc": "文件大小 {{size}} 超过限制 {{limit}}",
    "file_binary": "无法编辑二进制文件",
    "terminal_toggle": "切换终端",
    "git_branch": "分支：{{branch}}",
    "search_placeholder": "搜索文件..."
  }
}
```

---

#### Phase 1 验证清单

- [ ] `pnpm tsc --noEmit` 无错误
- [ ] `cd src-tauri && cargo check` 无错误
- [ ] `pnpm dev` 可以启动
- [ ] 在 appStore 中添加 createIdeTab action（待实现）
- [ ] 从侧边栏可以新建 IDE 标签（待实现入口）

---

### Phase 2: 编辑器核心功能（3 周）

**目标：** 完整的多标签编辑器体验，包括文件打开、编辑、保存、冲突检测

---

#### 任务 2.1: 实现 IdeTree 文件树（2d）

**文件：** `src/components/ide/IdeTree.tsx`（替换 Phase 1 占位）

**完整内容：**

```tsx
// src/components/ide/IdeTree.tsx
import { useState, useEffect, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  Folder, 
  FolderOpen, 
  File, 
  ChevronRight, 
  ChevronDown,
  RefreshCw,
  Loader2,
  AlertCircle,
} from 'lucide-react';
import { api } from '../../lib/api';
import { useIdeStore, useIdeProject } from '../../store/ideStore';
import { cn } from '../../lib/utils';
import { FileInfo } from '../../types';

// 文件图标映射
const FILE_ICONS: Record<string, string> = {
  ts: '📘', tsx: '📘', js: '📙', jsx: '📙',
  rs: '🦀', py: '🐍', go: '🔵', java: '☕',
  json: '📋', yaml: '📋', yml: '📋', toml: '📋',
  md: '📝', txt: '📄', html: '🌐', css: '🎨',
  sh: '📜', bash: '📜', zsh: '📜',
  dockerfile: '🐳', gitignore: '🙈',
};

function getFileIcon(name: string): string {
  const ext = name.includes('.') ? name.split('.').pop()?.toLowerCase() || '' : '';
  const lowerName = name.toLowerCase();
  
  // 特殊文件名
  if (lowerName === 'dockerfile') return '🐳';
  if (lowerName === '.gitignore') return '🙈';
  if (lowerName === 'cargo.toml') return '📦';
  if (lowerName === 'package.json') return '📦';
  
  return FILE_ICONS[ext] || '📄';
}

interface TreeNodeProps {
  path: string;
  name: string;
  isDir: boolean;
  depth: number;
  sftpSessionId: string;
}

function TreeNode({ path, name, isDir, depth, sftpSessionId }: TreeNodeProps) {
  const { expandedPaths, togglePath, openFile } = useIdeStore();
  const [children, setChildren] = useState<FileInfo[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  const isExpanded = expandedPaths.has(path);
  
  // 加载子目录
  const loadChildren = useCallback(async () => {
    if (!isDir || children !== null) return;
    
    setLoading(true);
    setError(null);
    
    try {
      const items = await api.sftpListDir(sftpSessionId, path);
      // 排序：目录在前，按名称排序
      const sorted = items.sort((a, b) => {
        if (a.file_type === 'Directory' && b.file_type !== 'Directory') return -1;
        if (a.file_type !== 'Directory' && b.file_type === 'Directory') return 1;
        return a.name.localeCompare(b.name);
      });
      setChildren(sorted);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [isDir, children, sftpSessionId, path]);
  
  // 展开时加载
  useEffect(() => {
    if (isExpanded && children === null && !loading) {
      loadChildren();
    }
  }, [isExpanded, children, loading, loadChildren]);
  
  const handleClick = useCallback(() => {
    if (isDir) {
      togglePath(path);
    }
  }, [isDir, path, togglePath]);
  
  const handleDoubleClick = useCallback(() => {
    if (!isDir) {
      openFile(path).catch(console.error);
    }
  }, [isDir, path, openFile]);
  
  const paddingLeft = 12 + depth * 16;
  
  return (
    <div>
      <div
        className={cn(
          'flex items-center py-1 cursor-pointer hover:bg-zinc-800/50 transition-colors',
          'text-sm text-zinc-300'
        )}
        style={{ paddingLeft }}
        onClick={handleClick}
        onDoubleClick={handleDoubleClick}
      >
        {/* 展开/折叠图标 */}
        {isDir && (
          <span className="w-4 h-4 mr-1 flex items-center justify-center text-zinc-500">
            {loading ? (
              <Loader2 className="w-3 h-3 animate-spin" />
            ) : isExpanded ? (
              <ChevronDown className="w-3 h-3" />
            ) : (
              <ChevronRight className="w-3 h-3" />
            )}
          </span>
        )}
        {!isDir && <span className="w-4 h-4 mr-1" />}
        
        {/* 文件/文件夹图标 */}
        <span className="mr-2 text-xs">
          {isDir ? (
            isExpanded ? '📂' : '📁'
          ) : (
            getFileIcon(name)
          )}
        </span>
        
        {/* 文件名 */}
        <span className="truncate">{name}</span>
      </div>
      
      {/* 子节点 */}
      {isDir && isExpanded && children && (
        <div>
          {children.map(child => (
            <TreeNode
              key={child.path}
              path={child.path}
              name={child.name}
              isDir={child.file_type === 'Directory'}
              depth={depth + 1}
              sftpSessionId={sftpSessionId}
            />
          ))}
        </div>
      )}
      
      {/* 错误状态 */}
      {isDir && isExpanded && error && (
        <div 
          className="flex items-center gap-2 py-1 text-xs text-red-400"
          style={{ paddingLeft: paddingLeft + 20 }}
        >
          <AlertCircle className="w-3 h-3" />
          <span>{error}</span>
        </div>
      )}
    </div>
  );
}

export function IdeTree() {
  const { t } = useTranslation();
  const project = useIdeProject();
  const { sftpSessionId, expandedPaths } = useIdeStore();
  const [rootChildren, setRootChildren] = useState<FileInfo[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  // 加载根目录
  const loadRoot = useCallback(async () => {
    if (!sftpSessionId || !project) return;
    
    setLoading(true);
    setError(null);
    
    try {
      const items = await api.sftpListDir(sftpSessionId, project.rootPath);
      const sorted = items.sort((a, b) => {
        if (a.file_type === 'Directory' && b.file_type !== 'Directory') return -1;
        if (a.file_type !== 'Directory' && b.file_type === 'Directory') return 1;
        return a.name.localeCompare(b.name);
      });
      setRootChildren(sorted);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [sftpSessionId, project]);
  
  useEffect(() => {
    loadRoot();
  }, [loadRoot]);
  
  if (!project || !sftpSessionId) {
    return <div className="p-4 text-zinc-500">{t('ide.no_project')}</div>;
  }
  
  return (
    <div className="h-full flex flex-col bg-zinc-900">
      {/* 项目标题 */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-zinc-800">
        <Folder className="w-4 h-4 text-orange-500" />
        <span className="text-sm font-medium truncate flex-1">{project.name}</span>
        {project.isGitRepo && project.gitBranch && (
          <span className="text-xs text-zinc-500">{project.gitBranch}</span>
        )}
        <button
          onClick={loadRoot}
          className="p-1 hover:bg-zinc-800 rounded transition-colors"
          title={t('ide.refresh')}
        >
          <RefreshCw className={cn('w-3 h-3 text-zinc-500', loading && 'animate-spin')} />
        </button>
      </div>
      
      {/* 文件列表 */}
      <div className="flex-1 overflow-auto">
        {loading && !rootChildren && (
          <div className="flex items-center justify-center p-4">
            <Loader2 className="w-5 h-5 animate-spin text-zinc-500" />
          </div>
        )}
        
        {error && (
          <div className="p-4 text-red-400 text-sm">
            <AlertCircle className="w-4 h-4 inline mr-2" />
            {error}
          </div>
        )}
        
        {rootChildren && rootChildren.map(item => (
          <TreeNode
            key={item.path}
            path={item.path}
            name={item.name}
            isDir={item.file_type === 'Directory'}
            depth={0}
            sftpSessionId={sftpSessionId}
          />
        ))}
      </div>
    </div>
  );
}
```

**验证：** 能够浏览项目文件结构，点击目录可展开/折叠

---

#### 任务 2.2: 抽取 useCodeMirrorEditor Hook（1d）

**文件：** `src/components/ide/hooks/useCodeMirrorEditor.ts`（新建）

**完整内容：**

```typescript
// src/components/ide/hooks/useCodeMirrorEditor.ts
import { useRef, useEffect, useCallback } from 'react';
import { EditorView, keymap, lineNumbers, highlightActiveLineGutter } from '@codemirror/view';
import { EditorState, Extension, Compartment } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { indentOnInput, bracketMatching, foldGutter, foldKeymap } from '@codemirror/language';
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
import { autocompletion, completionKeymap } from '@codemirror/autocomplete';
import { oneDark } from '@codemirror/theme-one-dark';
import { loadLanguage } from '../../../lib/codemirror/languageLoader';

export interface UseCodeMirrorEditorOptions {
  /** 初始内容 */
  initialContent: string;
  /** CodeMirror 语言标识（如 'typescript', 'rust', 'python'） */
  language: string | null;
  /** 内容变化回调 */
  onContentChange: (content: string) => void;
  /** 光标位置变化回调 */
  onCursorChange?: (line: number, col: number) => void;
  /** 保存快捷键回调 */
  onSave: () => void;
  /** 是否只读 */
  readOnly?: boolean;
}

export interface UseCodeMirrorEditorResult {
  /** 绑定到容器 div 的 ref */
  containerRef: React.RefObject<HTMLDivElement>;
  /** 外部更新编辑器内容 */
  setContent: (content: string) => void;
  /** 获取当前内容 */
  getContent: () => string;
  /** 聚焦编辑器 */
  focus: () => void;
  /** 获取 EditorView 实例（高级用法） */
  getView: () => EditorView | null;
}

// Oxide 主题覆盖（与 RemoteFileEditor 保持一致）
const oxideTheme = EditorView.theme({
  '&': { 
    height: '100%', 
    fontSize: '13px',
    backgroundColor: 'transparent',
  },
  '.cm-scroller': { 
    fontFamily: '"JetBrains Mono", "Fira Code", "Consolas", monospace',
    overflow: 'auto',
    lineHeight: '1.5',
  },
  '.cm-content': {
    caretColor: '#f97316',
  },
  '.cm-gutters': { 
    backgroundColor: 'rgb(39 39 42 / 0.5)',
    borderRight: '1px solid rgb(63 63 70 / 0.5)',
    color: 'rgb(113 113 122)',
  },
  '.cm-activeLineGutter': { 
    backgroundColor: 'rgb(234 88 12 / 0.1)',
    color: 'rgb(251 146 60)',
  },
  '.cm-activeLine': { 
    backgroundColor: 'rgb(234 88 12 / 0.05)',
  },
  '&.cm-focused .cm-cursor': { 
    borderLeftColor: '#f97316',
    borderLeftWidth: '2px',
  },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': {
    backgroundColor: 'rgb(234 88 12 / 0.3)',
  },
  '.cm-searchMatch': {
    backgroundColor: 'rgb(234 179 8 / 0.3)',
    outline: '1px solid rgb(234 179 8 / 0.5)',
  },
  '.cm-searchMatch.cm-searchMatch-selected': {
    backgroundColor: 'rgb(234 179 8 / 0.5)',
  },
});

export function useCodeMirrorEditor(options: UseCodeMirrorEditorOptions): UseCodeMirrorEditorResult {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const contentRef = useRef(options.initialContent);
  const languageCompartment = useRef(new Compartment());
  
  // 保存回调的 ref，避免重新创建编辑器
  const callbacksRef = useRef({
    onContentChange: options.onContentChange,
    onCursorChange: options.onCursorChange,
    onSave: options.onSave,
  });
  
  // 更新回调 ref
  useEffect(() => {
    callbacksRef.current = {
      onContentChange: options.onContentChange,
      onCursorChange: options.onCursorChange,
      onSave: options.onSave,
    };
  }, [options.onContentChange, options.onCursorChange, options.onSave]);
  
  // 初始化编辑器
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    
    let view: EditorView | null = null;
    let mounted = true;
    
    const init = async () => {
      // 加载语言支持（异步）
      const langSupport = await loadLanguage(options.language);
      if (!mounted) return;
      
      const extensions: Extension[] = [
        // 基础功能
        lineNumbers(),
        highlightActiveLineGutter(),
        history(),
        foldGutter(),
        indentOnInput(),
        bracketMatching(),
        autocompletion(),
        highlightSelectionMatches(),
        
        // 主题
        oneDark,
        oxideTheme,
        
        // 语言（使用 Compartment 以便后续切换）
        languageCompartment.current.of(langSupport ? [langSupport] : []),
        
        // 快捷键
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          ...foldKeymap,
          ...searchKeymap,
          ...completionKeymap,
          indentWithTab,
          { 
            key: 'Mod-s', 
            run: () => { 
              callbacksRef.current.onSave(); 
              return true; 
            },
            preventDefault: true,
          },
        ]),
        
        // 更新监听器
        EditorView.updateListener.of((update) => {
          // 内容变化
          if (update.docChanged) {
            const content = update.state.doc.toString();
            contentRef.current = content;
            callbacksRef.current.onContentChange(content);
          }
          
          // 光标位置（仅在有回调时处理）
          if (callbacksRef.current.onCursorChange && (update.selectionSet || update.docChanged)) {
            const pos = update.state.selection.main.head;
            const line = update.state.doc.lineAt(pos);
            callbacksRef.current.onCursorChange(line.number, pos - line.from + 1);
          }
        }),
        
        // 只读模式
        ...(options.readOnly ? [EditorState.readOnly.of(true)] : []),
      ];
      
      // 创建编辑器
      const state = EditorState.create({
        doc: options.initialContent,
        extensions,
      });
      
      container.innerHTML = '';
      view = new EditorView({ state, parent: container });
      viewRef.current = view;
    };
    
    init();
    
    return () => {
      mounted = false;
      view?.destroy();
      viewRef.current = null;
    };
  // 仅在 initialContent 或 language 变化时重建
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [options.initialContent, options.language, options.readOnly]);
  
  // 外部更新内容
  const setContent = useCallback((content: string) => {
    const view = viewRef.current;
    if (view && content !== contentRef.current) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: content },
      });
      contentRef.current = content;
    }
  }, []);
  
  // 获取当前内容
  const getContent = useCallback(() => contentRef.current, []);
  
  // 聚焦编辑器
  const focus = useCallback(() => {
    viewRef.current?.focus();
  }, []);
  
  // 获取 EditorView
  const getView = useCallback(() => viewRef.current, []);
  
  return { 
    containerRef: containerRef as React.RefObject<HTMLDivElement>, 
    setContent, 
    getContent,
    focus,
    getView,
  };
}
```

**验证：** TypeScript 编译通过

---

#### 任务 2.3: 实现 IdeEditor 组件（1d）

**文件：** `src/components/ide/IdeEditor.tsx`（新建）

```tsx
// src/components/ide/IdeEditor.tsx
import { useCallback, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2 } from 'lucide-react';
import { useIdeStore, IdeTab } from '../../store/ideStore';
import { useCodeMirrorEditor } from './hooks/useCodeMirrorEditor';
import { cn } from '../../lib/utils';

interface IdeEditorProps {
  tab: IdeTab;
}

export function IdeEditor({ tab }: IdeEditorProps) {
  const { t } = useTranslation();
  const { updateTabContent, updateTabCursor, saveFile } = useIdeStore();
  
  const handleContentChange = useCallback((content: string) => {
    updateTabContent(tab.id, content);
  }, [tab.id, updateTabContent]);
  
  const handleCursorChange = useCallback((line: number, col: number) => {
    updateTabCursor(tab.id, line, col);
  }, [tab.id, updateTabCursor]);
  
  const handleSave = useCallback(async () => {
    try {
      await saveFile(tab.id);
    } catch (e) {
      // 错误由 saveFile 内部处理
      console.error('Save failed:', e);
    }
  }, [tab.id, saveFile]);
  
  const { containerRef, focus } = useCodeMirrorEditor({
    initialContent: tab.content ?? '',
    language: tab.language,
    onContentChange: handleContentChange,
    onCursorChange: handleCursorChange,
    onSave: handleSave,
    readOnly: tab.isLoading,
  });
  
  // 标签激活时聚焦编辑器
  useEffect(() => {
    // 延迟聚焦，确保 DOM 已渲染
    const timer = setTimeout(() => focus(), 50);
    return () => clearTimeout(timer);
  }, [focus]);
  
  // 加载中状态
  if (tab.isLoading) {
    return (
      <div className="flex-1 flex items-center justify-center bg-zinc-950">
        <Loader2 className="w-6 h-6 animate-spin text-orange-500" />
        <span className="ml-2 text-zinc-400">{t('ide.loading_file')}</span>
      </div>
    );
  }
  
  // 内容未加载
  if (tab.content === null) {
    return (
      <div className="flex-1 flex items-center justify-center bg-zinc-950 text-zinc-500">
        {t('ide.file_not_loaded')}
      </div>
    );
  }
  
  return (
    <div 
      ref={containerRef} 
      className={cn(
        'flex-1 overflow-hidden',
        'bg-zinc-950',
        // 未保存时显示微弱的橙色边框
        tab.isDirty && 'ring-1 ring-orange-500/20'
      )}
    />
  );
}
```

---

#### 任务 2.4: 实现 IdeEditorTabs 组件（1d）

**文件：** `src/components/ide/IdeEditorTabs.tsx`（新建）

```tsx
// src/components/ide/IdeEditorTabs.tsx
import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { X, Circle, Loader2 } from 'lucide-react';
import { useIdeTabs, useIdeStore, IdeTab } from '../../store/ideStore';
import { cn } from '../../lib/utils';
import { IdeSaveConfirmDialog } from './dialogs/IdeSaveConfirmDialog';

// 文件图标（同 IdeTree）
const FILE_ICONS: Record<string, string> = {
  typescript: '📘', javascript: '📙', rust: '🦀', python: '🐍',
  go: '🔵', java: '☕', json: '📋', yaml: '📋', markdown: '📝',
  html: '🌐', css: '🎨', shell: '📜', plaintext: '📄',
};

interface TabItemProps {
  tab: IdeTab;
  isActive: boolean;
  onActivate: () => void;
  onClose: () => void;
}

function TabItem({ tab, isActive, onActivate, onClose }: TabItemProps) {
  const handleClose = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    onClose();
  }, [onClose]);
  
  const icon = FILE_ICONS[tab.language] || '📄';
  
  return (
    <div
      onClick={onActivate}
      className={cn(
        'group flex items-center gap-2 px-3 py-1.5 border-r border-zinc-800',
        'cursor-pointer transition-colors min-w-0',
        isActive 
          ? 'bg-zinc-800 text-zinc-100' 
          : 'bg-zinc-900 text-zinc-400 hover:bg-zinc-800/50 hover:text-zinc-300'
      )}
    >
      {/* 文件图标 */}
      <span className="text-xs flex-shrink-0">{icon}</span>
      
      {/* 文件名 */}
      <span className="text-sm truncate max-w-[120px]">
        {tab.name}
      </span>
      
      {/* 状态指示器 */}
      <div className="w-4 h-4 flex items-center justify-center flex-shrink-0">
        {tab.isLoading ? (
          <Loader2 className="w-3 h-3 animate-spin text-zinc-500" />
        ) : tab.isDirty ? (
          <Circle className="w-2 h-2 fill-blue-500 text-blue-500" />
        ) : null}
      </div>
      
      {/* 关闭按钮 */}
      <button
        onClick={handleClose}
        className={cn(
          'p-0.5 rounded transition-colors flex-shrink-0',
          'opacity-0 group-hover:opacity-100',
          'hover:bg-zinc-700 text-zinc-500 hover:text-zinc-300'
        )}
      >
        <X className="w-3 h-3" />
      </button>
    </div>
  );
}

export function IdeEditorTabs() {
  const { t } = useTranslation();
  const tabs = useIdeTabs();
  const { activeTabId, setActiveTab, closeTab, saveFile } = useIdeStore();
  
  // 关闭确认对话框状态
  const [confirmDialog, setConfirmDialog] = useState<{
    open: boolean;
    tab: IdeTab | null;
  }>({ open: false, tab: null });
  
  const handleCloseTab = useCallback(async (tab: IdeTab) => {
    if (tab.isDirty) {
      // 显示确认对话框
      setConfirmDialog({ open: true, tab });
    } else {
      await closeTab(tab.id);
    }
  }, [closeTab]);
  
  const handleConfirmSave = useCallback(async () => {
    const tab = confirmDialog.tab;
    if (!tab) return;
    
    try {
      await saveFile(tab.id);
      await closeTab(tab.id);
    } catch (e) {
      console.error('Save before close failed:', e);
      // 保存失败，不关闭
    }
    setConfirmDialog({ open: false, tab: null });
  }, [confirmDialog.tab, saveFile, closeTab]);
  
  const handleConfirmDiscard = useCallback(async () => {
    const tab = confirmDialog.tab;
    if (!tab) return;
    
    // 强制关闭（忽略 dirty 状态）
    useIdeStore.setState(state => ({
      tabs: state.tabs.filter(t => t.id !== tab.id),
      activeTabId: state.activeTabId === tab.id
        ? (state.tabs.length > 1 ? state.tabs[state.tabs.length - 2].id : null)
        : state.activeTabId,
    }));
    setConfirmDialog({ open: false, tab: null });
  }, [confirmDialog.tab]);
  
  const handleConfirmCancel = useCallback(() => {
    setConfirmDialog({ open: false, tab: null });
  }, []);
  
  if (tabs.length === 0) {
    return null;
  }
  
  return (
    <>
      <div className="flex items-center bg-zinc-900 border-b border-zinc-800 overflow-x-auto">
        {tabs.map(tab => (
          <TabItem
            key={tab.id}
            tab={tab}
            isActive={tab.id === activeTabId}
            onActivate={() => setActiveTab(tab.id)}
            onClose={() => handleCloseTab(tab)}
          />
        ))}
      </div>
      
      {/* 保存确认对话框 */}
      <IdeSaveConfirmDialog
        open={confirmDialog.open}
        fileName={confirmDialog.tab?.name || ''}
        onSave={handleConfirmSave}
        onDiscard={handleConfirmDiscard}
        onCancel={handleConfirmCancel}
      />
    </>
  );
}
```

---

#### 任务 2.5: 创建保存确认对话框（0.5d）

**文件：** `src/components/ide/dialogs/IdeSaveConfirmDialog.tsx`（新建）

```tsx
// src/components/ide/dialogs/IdeSaveConfirmDialog.tsx
import { useTranslation } from 'react-i18next';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '../../ui/alert-dialog';
import { Button } from '../../ui/button';

interface IdeSaveConfirmDialogProps {
  open: boolean;
  fileName: string;
  onSave: () => void;
  onDiscard: () => void;
  onCancel: () => void;
}

export function IdeSaveConfirmDialog({
  open,
  fileName,
  onSave,
  onDiscard,
  onCancel,
}: IdeSaveConfirmDialogProps) {
  const { t } = useTranslation();
  
  return (
    <AlertDialog open={open} onOpenChange={(o) => !o && onCancel()}>
      <AlertDialogContent className="bg-zinc-900 border-zinc-800">
        <AlertDialogHeader>
          <AlertDialogTitle className="text-zinc-100">
            {t('ide.unsaved_changes_title')}
          </AlertDialogTitle>
          <AlertDialogDescription className="text-zinc-400">
            {t('ide.unsaved_changes_desc', { fileName })}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel asChild>
            <Button variant="ghost" onClick={onCancel}>
              {t('common.cancel')}
            </Button>
          </AlertDialogCancel>
          <Button variant="destructive" onClick={onDiscard}>
            {t('ide.discard')}
          </Button>
          <AlertDialogAction asChild>
            <Button variant="default" onClick={onSave}>
              {t('ide.save')}
            </Button>
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
```

---

#### 任务 2.6: 更新 IdeEditorArea（0.5d）

**文件：** `src/components/ide/IdeEditorArea.tsx`（替换 Phase 1 占位）

```tsx
// src/components/ide/IdeEditorArea.tsx
import { useTranslation } from 'react-i18next';
import { Code2 } from 'lucide-react';
import { useIdeTabs, useIdeActiveTab } from '../../store/ideStore';
import { IdeEditorTabs } from './IdeEditorTabs';
import { IdeEditor } from './IdeEditor';

export function IdeEditorArea() {
  const { t } = useTranslation();
  const tabs = useIdeTabs();
  const activeTab = useIdeActiveTab();
  
  // 无标签状态
  if (tabs.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center text-zinc-500 bg-zinc-950">
        <Code2 className="w-16 h-16 mb-4 opacity-20" />
        <p className="text-lg">{t('ide.no_open_files')}</p>
        <p className="text-sm mt-1 text-zinc-600">{t('ide.click_to_open')}</p>
        <div className="mt-6 text-xs text-zinc-600 space-y-1">
          <p>💡 {t('ide.tip_double_click')}</p>
          <p>💡 {t('ide.tip_save_shortcut')}</p>
        </div>
      </div>
    );
  }
  
  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      {/* 标签栏 */}
      <IdeEditorTabs />
      
      {/* 编辑器 */}
      <div className="flex-1 overflow-hidden">
        {activeTab && <IdeEditor key={activeTab.id} tab={activeTab} />}
      </div>
    </div>
  );
}
```

---

#### 任务 2.7: 实现文件冲突检测和处理（1d）

**文件：** `src/components/ide/dialogs/IdeConflictDialog.tsx`（新建）

```tsx
// src/components/ide/dialogs/IdeConflictDialog.tsx
import { useTranslation } from 'react-i18next';
import { AlertTriangle } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../ui/dialog';
import { Button } from '../../ui/button';

export type ConflictResolution = 'overwrite' | 'reload' | 'cancel';

interface IdeConflictDialogProps {
  open: boolean;
  fileName: string;
  localTime: Date | null;
  remoteTime: Date | null;
  onResolve: (resolution: ConflictResolution) => void;
}

export function IdeConflictDialog({
  open,
  fileName,
  localTime,
  remoteTime,
  onResolve,
}: IdeConflictDialogProps) {
  const { t } = useTranslation();
  
  const formatTime = (date: Date | null) => {
    if (!date) return '-';
    return date.toLocaleString();
  };
  
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onResolve('cancel')}>
      <DialogContent className="bg-zinc-900 border-zinc-800 max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-yellow-500">
            <AlertTriangle className="w-5 h-5" />
            {t('ide.file_conflict')}
          </DialogTitle>
          <DialogDescription className="text-zinc-400">
            {t('ide.file_conflict_desc')}
          </DialogDescription>
        </DialogHeader>
        
        <div className="py-4 space-y-3 text-sm">
          <div className="flex justify-between">
            <span className="text-zinc-500">{t('ide.file_name')}:</span>
            <span className="text-zinc-300 font-mono">{fileName}</span>
          </div>
          <div className="flex justify-between">
            <span className="text-zinc-500">{t('ide.your_version')}:</span>
            <span className="text-zinc-300">{formatTime(localTime)}</span>
          </div>
          <div className="flex justify-between">
            <span className="text-zinc-500">{t('ide.remote_version')}:</span>
            <span className="text-zinc-300">{formatTime(remoteTime)}</span>
          </div>
        </div>
        
        <DialogFooter className="flex-col sm:flex-row gap-2">
          <Button
            variant="ghost"
            onClick={() => onResolve('cancel')}
          >
            {t('common.cancel')}
          </Button>
          <Button
            variant="outline"
            onClick={() => onResolve('reload')}
          >
            {t('ide.conflict_reload')}
          </Button>
          <Button
            variant="default"
            onClick={() => onResolve('overwrite')}
          >
            {t('ide.conflict_overwrite')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

**更新 ideStore.ts 中的 saveFile 方法以支持冲突处理：**

在 `src/store/ideStore.ts` 中，修改 `saveFile` action：

```typescript
// 在 IdeState 接口中添加
interface IdeState {
  // ... 现有字段 ...
  
  // 冲突状态
  conflictState: {
    tabId: string;
    localMtime: number;
    remoteMtime: number;
  } | null;
}

interface IdeActions {
  // ... 现有 actions ...
  
  resolveConflict: (resolution: 'overwrite' | 'reload') => Promise<void>;
  clearConflict: () => void;
}

// 修改 saveFile 实现
saveFile: async (tabId) => {
  const { tabs, sftpSessionId, conflictState } = get();
  const tab = tabs.find(t => t.id === tabId);
  
  if (!tab || !sftpSessionId || tab.content === null) {
    throw new Error('Cannot save: invalid state');
  }
  
  // 检查冲突
  const stat = await api.sftpStat(sftpSessionId, tab.path);
  if (tab.serverMtime && stat.modified && stat.modified !== tab.serverMtime) {
    // 设置冲突状态，由 UI 层处理
    set({
      conflictState: {
        tabId,
        localMtime: tab.serverMtime,
        remoteMtime: stat.modified,
      }
    });
    throw new Error('CONFLICT');
  }
  
  // 保存文件
  const result = await api.sftpWriteContent(sftpSessionId, tab.path, tab.content);
  
  set(state => ({
    tabs: state.tabs.map(t =>
      t.id === tabId
        ? {
            ...t,
            isDirty: false,
            originalContent: t.content,
            serverMtime: result.mtime ?? undefined,
          }
        : t
    ),
    conflictState: null,
  }));
},

resolveConflict: async (resolution) => {
  const { conflictState, tabs, sftpSessionId } = get();
  if (!conflictState || !sftpSessionId) return;
  
  const tab = tabs.find(t => t.id === conflictState.tabId);
  if (!tab || tab.content === null) return;
  
  if (resolution === 'overwrite') {
    // 强制保存（忽略冲突）
    const result = await api.sftpWriteContent(sftpSessionId, tab.path, tab.content);
    
    set(state => ({
      tabs: state.tabs.map(t =>
        t.id === conflictState.tabId
          ? {
              ...t,
              isDirty: false,
              originalContent: t.content,
              serverMtime: result.mtime ?? undefined,
            }
          : t
      ),
      conflictState: null,
    }));
  } else if (resolution === 'reload') {
    // 重新加载远程内容
    const preview = await api.sftpPreview(sftpSessionId, tab.path);
    
    if ('Text' in preview) {
      const stat = await api.sftpStat(sftpSessionId, tab.path);
      
      set(state => ({
        tabs: state.tabs.map(t =>
          t.id === conflictState.tabId
            ? {
                ...t,
                content: preview.Text.data,
                originalContent: preview.Text.data,
                isDirty: false,
                serverMtime: stat.modified ?? undefined,
              }
            : t
        ),
        conflictState: null,
      }));
    }
  }
},

clearConflict: () => {
  set({ conflictState: null });
},
```

---

#### 任务 2.8: 在 appStore 中添加 IDE 标签创建（0.5d）

**文件：** `src/store/appStore.ts`

**位置：** 在 `createTab` 函数中（约第 613 行），在 `local_terminal` case 之后添加 `ide` case：

```typescript
// 在 createTab 函数中，local_terminal case 之后添加：

    // Handle IDE mode tabs
    if (type === 'ide') {
      if (!sessionId) return;

      // For IDE, sessionId is actually the SFTP session ID
      const newTab: Tab = {
        id: crypto.randomUUID(),
        type: 'ide',
        sessionId,  // SFTP session ID
        title: i18n.t('tabs.ide'),
        icon: '💻'
      };

      set((state) => ({
        tabs: [...state.tabs, newTab],
        activeTabId: newTab.id
      }));
      return;
    }
```

---

#### 任务 2.9: 添加 Phase 2 i18n 键值（0.5d）

**追加到各语言文件的 `ide` 对象中：**

```json
{
  "ide": {
    "loading_file": "Loading file...",
    "file_not_loaded": "File content not loaded",
    "unsaved_changes_title": "Unsaved Changes",
    "unsaved_changes_desc": "\"{{fileName}}\" has unsaved changes. What would you like to do?",
    "discard": "Don't Save",
    "save": "Save",
    "tip_double_click": "Double-click a file to open it",
    "tip_save_shortcut": "Press Cmd/Ctrl+S to save",
    "file_name": "File",
    "your_version": "Your version",
    "remote_version": "Remote version",
    "refresh": "Refresh"
  }
}
```

**中文：**

```json
{
  "ide": {
    "loading_file": "正在加载文件...",
    "file_not_loaded": "文件内容未加载",
    "unsaved_changes_title": "未保存的更改",
    "unsaved_changes_desc": "\"{{fileName}}\" 有未保存的更改。您想要怎么做？",
    "discard": "不保存",
    "save": "保存",
    "tip_double_click": "双击文件以打开",
    "tip_save_shortcut": "按 Cmd/Ctrl+S 保存",
    "file_name": "文件",
    "your_version": "您的版本",
    "remote_version": "远程版本",
    "refresh": "刷新"
  }
}
```

---

#### Phase 2 验证清单

- [ ] 文件树可以展开/折叠目录
- [ ] 双击文件可以打开
- [ ] 编辑器可以编辑内容
- [ ] `Cmd/Ctrl+S` 可以保存
- [ ] 标签栏显示正确，可以切换/关闭
- [ ] 未保存文件有蓝色圆点指示
- [ ] 关闭未保存文件会弹出确认框
- [ ] 保存时冲突会弹出冲突对话框
- [ ] `pnpm tsc --noEmit` 无错误

---

### Phase 3: 终端集成（1.5 周）

**目标：** IDE 模式内嵌终端，支持自动 CD 到项目目录

---

#### 任务 3.1: 创建 IDE 终端会话管理 Hook（1d）

**文件：** `src/components/ide/hooks/useIdeTerminal.ts`（新建）

```typescript
// src/components/ide/hooks/useIdeTerminal.ts
import { useState, useCallback, useEffect } from 'react';
import { api } from '../../../lib/api';
import { useIdeStore } from '../../../store/ideStore';
import { useAppStore } from '../../../store/appStore';

interface UseIdeTerminalResult {
  /** 终端会话 ID（用于 TerminalView） */
  terminalSessionId: string | null;
  /** WebSocket token */
  wsToken: string | null;
  /** 是否正在创建 */
  isCreating: boolean;
  /** 创建错误 */
  error: string | null;
  /** 创建终端会话 */
  createTerminal: () => Promise<void>;
  /** 关闭终端会话 */
  closeTerminal: () => Promise<void>;
}

export function useIdeTerminal(): UseIdeTerminalResult {
  const { connectionId, terminalSessionId, project, setTerminalSession } = useIdeStore();
  const [wsToken, setWsToken] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  // 创建终端会话
  const createTerminal = useCallback(async () => {
    if (!connectionId || terminalSessionId) return;
    
    setIsCreating(true);
    setError(null);
    
    try {
      // 使用现有 SSH 连接创建终端
      const response = await api.createTerminal({
        connectionId,
        cols: 120,
        rows: 30,
      });
      
      // 更新 appStore 的 sessions（用于 TerminalView）
      useAppStore.setState(state => {
        const newSessions = new Map(state.sessions);
        newSessions.set(response.sessionId, response.session);
        return { sessions: newSessions };
      });
      
      // 更新 ideStore
      setTerminalSession(response.sessionId);
      setWsToken(response.wsToken);
      
      // 自动 CD 到项目目录
      if (project?.rootPath) {
        // 等待终端连接建立
        setTimeout(async () => {
          try {
            // 发送 cd 命令（通过 WebSocket，不在此处实现）
            // 这里只是设置初始工作目录的标记
            console.log(`IDE Terminal: should cd to ${project.rootPath}`);
          } catch (e) {
            console.error('Auto CD failed:', e);
          }
        }, 500);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsCreating(false);
    }
  }, [connectionId, terminalSessionId, project, setTerminalSession]);
  
  // 关闭终端会话
  const closeTerminal = useCallback(async () => {
    if (!terminalSessionId) return;
    
    try {
      await api.closeTerminal(terminalSessionId);
      
      // 从 appStore 移除
      useAppStore.setState(state => {
        const newSessions = new Map(state.sessions);
        newSessions.delete(terminalSessionId);
        return { sessions: newSessions };
      });
      
      setTerminalSession(null);
      setWsToken(null);
    } catch (e) {
      console.error('Close terminal failed:', e);
    }
  }, [terminalSessionId, setTerminalSession]);
  
  return {
    terminalSessionId,
    wsToken,
    isCreating,
    error,
    createTerminal,
    closeTerminal,
  };
}
```

---

#### 任务 3.2: 实现 IdeTerminal 组件（2d）

**文件：** `src/components/ide/IdeTerminal.tsx`（替换 Phase 1 占位）

```tsx
// src/components/ide/IdeTerminal.tsx
import { useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Terminal, X, Loader2, RefreshCw, AlertCircle } from 'lucide-react';
import { useIdeStore } from '../../store/ideStore';
import { useIdeTerminal } from './hooks/useIdeTerminal';
import { TerminalView } from '../terminal/TerminalView';
import { cn } from '../../lib/utils';
import { Button } from '../ui/button';

export function IdeTerminal() {
  const { t } = useTranslation();
  const { terminalVisible, toggleTerminal, project } = useIdeStore();
  const {
    terminalSessionId,
    isCreating,
    error,
    createTerminal,
    closeTerminal,
  } = useIdeTerminal();
  
  // 首次打开时自动创建终端
  useEffect(() => {
    if (terminalVisible && !terminalSessionId && !isCreating && !error) {
      createTerminal();
    }
  }, [terminalVisible, terminalSessionId, isCreating, error, createTerminal]);
  
  // 关闭终端面板
  const handleClose = useCallback(async () => {
    await closeTerminal();
    toggleTerminal();
  }, [closeTerminal, toggleTerminal]);
  
  // 重试创建
  const handleRetry = useCallback(() => {
    createTerminal();
  }, [createTerminal]);
  
  return (
    <div className="h-full flex flex-col bg-zinc-950">
      {/* 标题栏 */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-zinc-900 border-b border-zinc-800">
        <div className="flex items-center gap-2">
          <Terminal className="w-4 h-4 text-orange-500" />
          <span className="text-sm text-zinc-300">{t('ide.terminal')}</span>
          {project?.rootPath && (
            <span className="text-xs text-zinc-500 truncate max-w-[200px]">
              {project.rootPath}
            </span>
          )}
        </div>
        <div className="flex items-center gap-1">
          {error && (
            <Button
              variant="ghost"
              size="sm"
              onClick={handleRetry}
              className="h-6 px-2"
            >
              <RefreshCw className="w-3 h-3 mr-1" />
              {t('common.retry')}
            </Button>
          )}
          <button
            onClick={handleClose}
            className="p-1 hover:bg-zinc-800 rounded transition-colors"
          >
            <X className="w-4 h-4 text-zinc-500 hover:text-zinc-300" />
          </button>
        </div>
      </div>
      
      {/* 终端内容 */}
      <div className="flex-1 overflow-hidden">
        {isCreating && (
          <div className="flex items-center justify-center h-full">
            <Loader2 className="w-6 h-6 animate-spin text-orange-500" />
            <span className="ml-2 text-zinc-400">{t('ide.creating_terminal')}</span>
          </div>
        )}
        
        {error && !isCreating && (
          <div className="flex flex-col items-center justify-center h-full text-red-400">
            <AlertCircle className="w-8 h-8 mb-2" />
            <p className="text-sm">{t('ide.terminal_error')}</p>
            <p className="text-xs text-zinc-500 mt-1">{error}</p>
          </div>
        )}
        
        {terminalSessionId && !isCreating && (
          <TerminalView
            sessionId={terminalSessionId}
            isActive={terminalVisible}
            // IDE 模式不需要 paneId/tabId，因为只有一个终端
          />
        )}
      </div>
    </div>
  );
}
```

---

#### 任务 3.3: 添加自动 CD 功能（1d）

在终端连接建立后，自动发送 `cd` 命令。需要修改 `TerminalView.tsx` 或在连接时直接设置工作目录。

**方案 A（推荐）：在创建终端时设置工作目录**

需要修改后端 `create_terminal` 命令，添加可选的 `initial_cwd` 参数：

**文件：** `src-tauri/src/commands/ssh.rs`

找到 `create_terminal` 函数，添加参数：

```rust
#[tauri::command]
pub async fn create_terminal(
    connection_id: String,
    cols: Option<u32>,
    rows: Option<u32>,
    initial_cwd: Option<String>,  // ← 新增
    // ... 其他参数
) -> Result<CreateTerminalResponse, String> {
    // ... 现有代码 ...
    
    // 在创建 PTY 后，如果有 initial_cwd，发送 cd 命令
    if let Some(cwd) = initial_cwd {
        // 发送 cd 命令（需要在终端完全初始化后）
        // 这里有多种实现方式，一种是在 shell 初始化后发送
        // channel.write_all(format!("cd '{}' && clear\n", cwd).as_bytes()).await?;
    }
    
    // ...
}
```

**方案 B：前端发送 CD 命令**

在 `useIdeTerminal.ts` 中，终端连接建立后通过 WebSocket 发送命令：

```typescript
// 在 createTerminal 成功后
// 等待 WebSocket 连接建立，然后发送 CD 命令
if (project?.rootPath) {
  // 监听终端就绪事件
  const handleTerminalReady = () => {
    // 使用 terminalRegistry 或直接通过 WebSocket 发送
    // 这需要访问 WebSocket 实例
  };
}
```

**注意：** 方案 A 更可靠，但需要后端修改。方案 B 可能有时序问题。

---

#### 任务 3.4: 添加终端快捷键（0.5d）

**文件：** `src/components/ide/IdeWorkspace.tsx`

添加全局快捷键处理：

```tsx
// 在 IdeWorkspace 组件中添加
import { useEffect } from 'react';

// 在组件内部
useEffect(() => {
  const handleKeyDown = (e: KeyboardEvent) => {
    // Ctrl+` 切换终端
    if (e.ctrlKey && e.key === '`') {
      e.preventDefault();
      toggleTerminal();
    }
  };
  
  window.addEventListener('keydown', handleKeyDown);
  return () => window.removeEventListener('keydown', handleKeyDown);
}, [toggleTerminal]);
```

---

#### 任务 3.5: 添加 Phase 3 i18n 键值（0.5d）

```json
{
  "ide": {
    "terminal": "Terminal",
    "creating_terminal": "Creating terminal...",
    "terminal_error": "Failed to create terminal",
    "terminal_shortcut": "Toggle Terminal (Ctrl+`)"
  }
}
```

**中文：**

```json
{
  "ide": {
    "terminal": "终端",
    "creating_terminal": "正在创建终端...",
    "terminal_error": "创建终端失败",
    "terminal_shortcut": "切换终端 (Ctrl+`)"
  }
}
```

---

#### Phase 3 验证清单

- [ ] 点击终端区域可以打开/关闭终端
- [ ] `Ctrl+\`` 快捷键可以切换终端
- [ ] 终端可以正常输入命令
- [ ] 终端自动 CD 到项目目录（如实现）
- [ ] 关闭终端面板会断开终端会话
- [ ] 重新打开终端可以创建新会话

---

### Phase 4: Git 状态与搜索（2 周）

**目标：** 文件树显示 Git 状态，支持项目内文件搜索

---

#### 任务 4.1: 实现 Git 状态 Hook（2d）

**文件：** `src/components/ide/hooks/useGitStatus.ts`（新建）

```typescript
// src/components/ide/hooks/useGitStatus.ts
import { useState, useEffect, useCallback, useRef } from 'react';
import { api } from '../../../lib/api';
import { useIdeStore } from '../../../store/ideStore';

export type GitFileStatus = 
  | 'modified'    // M - 已修改
  | 'added'       // A - 新增
  | 'deleted'     // D - 已删除
  | 'renamed'     // R - 重命名
  | 'untracked'   // ? - 未跟踪
  | 'ignored'     // ! - 忽略
  | 'conflict';   // U - 冲突

export interface GitStatus {
  branch: string;
  ahead: number;
  behind: number;
  files: Map<string, GitFileStatus>;
}

interface UseGitStatusResult {
  status: GitStatus | null;
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

// 解析 git status --porcelain=v1 输出
function parseGitStatus(output: string): Map<string, GitFileStatus> {
  const files = new Map<string, GitFileStatus>();
  
  for (const line of output.split('\n')) {
    if (!line.trim()) continue;
    
    const status = line.substring(0, 2);
    const path = line.substring(3);
    
    // 第一个字符是 staged 状态，第二个是 unstaged 状态
    const indexStatus = status[0];
    const workStatus = status[1];
    
    let fileStatus: GitFileStatus = 'modified';
    
    if (status === '??') {
      fileStatus = 'untracked';
    } else if (status === '!!') {
      fileStatus = 'ignored';
    } else if (indexStatus === 'A' || workStatus === 'A') {
      fileStatus = 'added';
    } else if (indexStatus === 'D' || workStatus === 'D') {
      fileStatus = 'deleted';
    } else if (indexStatus === 'R' || workStatus === 'R') {
      fileStatus = 'renamed';
    } else if (indexStatus === 'U' || workStatus === 'U') {
      fileStatus = 'conflict';
    } else if (indexStatus === 'M' || workStatus === 'M') {
      fileStatus = 'modified';
    }
    
    files.set(path, fileStatus);
  }
  
  return files;
}

export function useGitStatus(): UseGitStatusResult {
  const { project, sftpSessionId, terminalSessionId } = useIdeStore();
  const [status, setStatus] = useState<GitStatus | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refreshIntervalRef = useRef<number | null>(null);
  
  const refresh = useCallback(async () => {
    if (!project?.isGitRepo || !terminalSessionId) {
      setStatus(null);
      return;
    }
    
    setIsLoading(true);
    setError(null);
    
    try {
      // TODO: 需要实现通过 SSH 执行命令并获取输出的 API
      // 暂时使用 mock 数据
      
      // 实际实现需要：
      // 1. 执行 git status --porcelain=v1 --branch
      // 2. 解析输出
      // const output = await api.sshExec(terminalSessionId, 
      //   `cd '${project.rootPath}' && git status --porcelain=v1 --branch`
      // );
      
      // Mock 实现
      setStatus({
        branch: project.gitBranch || 'main',
        ahead: 0,
        behind: 0,
        files: new Map(),
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsLoading(false);
    }
  }, [project, terminalSessionId]);
  
  // 初始加载
  useEffect(() => {
    if (project?.isGitRepo) {
      refresh();
    }
  }, [project?.isGitRepo, refresh]);
  
  // 定期刷新（每 30 秒）
  useEffect(() => {
    if (project?.isGitRepo) {
      refreshIntervalRef.current = window.setInterval(refresh, 30000);
      return () => {
        if (refreshIntervalRef.current) {
          clearInterval(refreshIntervalRef.current);
        }
      };
    }
  }, [project?.isGitRepo, refresh]);
  
  return { status, isLoading, error, refresh };
}
```

---

#### 任务 4.2: 在文件树中显示 Git 状态（1d）

修改 `IdeTree.tsx`，为文件添加 Git 状态颜色：

```tsx
// 在 TreeNode 组件中添加 Git 状态支持
interface TreeNodeProps {
  // ... 现有 props
  gitStatus?: GitFileStatus;
}

// 状态颜色映射
const GIT_STATUS_COLORS: Record<GitFileStatus, string> = {
  modified: 'text-yellow-500',
  added: 'text-green-500',
  deleted: 'text-red-500',
  renamed: 'text-blue-500',
  untracked: 'text-zinc-500',
  ignored: 'text-zinc-600',
  conflict: 'text-red-600',
};

function TreeNode({ path, name, isDir, depth, sftpSessionId, gitStatus }: TreeNodeProps) {
  // ... 现有代码
  
  const textColorClass = gitStatus ? GIT_STATUS_COLORS[gitStatus] : 'text-zinc-300';
  
  return (
    <div>
      <div
        className={cn(
          'flex items-center py-1 cursor-pointer hover:bg-zinc-800/50 transition-colors',
          'text-sm',
          textColorClass  // 使用 Git 状态颜色
        )}
        // ...
      >
        {/* 文件名 */}
        <span className="truncate">{name}</span>
        
        {/* Git 状态指示器 */}
        {gitStatus && gitStatus !== 'ignored' && (
          <span className="ml-auto mr-2 text-xs opacity-70">
            {gitStatus === 'modified' && 'M'}
            {gitStatus === 'added' && 'A'}
            {gitStatus === 'deleted' && 'D'}
            {gitStatus === 'renamed' && 'R'}
            {gitStatus === 'untracked' && 'U'}
            {gitStatus === 'conflict' && '!'}
          </span>
        )}
      </div>
      {/* ... */}
    </div>
  );
}
```

---

#### 任务 4.3: 实现文件搜索面板（3d）

**文件：** `src/components/ide/IdeSearchPanel.tsx`（新建）

```tsx
// src/components/ide/IdeSearchPanel.tsx
import { useState, useCallback, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Search, X, Loader2, File, ChevronRight } from 'lucide-react';
import { api } from '../../lib/api';
import { useIdeStore } from '../../store/ideStore';
import { cn } from '../../lib/utils';
import { Input } from '../ui/input';

interface SearchMatch {
  path: string;
  line: number;
  column: number;
  preview: string;
  matchStart: number;
  matchEnd: number;
}

interface SearchResult {
  path: string;
  matches: SearchMatch[];
}

interface IdeSearchPanelProps {
  open: boolean;
  onClose: () => void;
}

export function IdeSearchPanel({ open, onClose }: IdeSearchPanelProps) {
  const { t } = useTranslation();
  const { sftpSessionId, project, openFile } = useIdeStore();
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const inputRef = useRef<HTMLInputElement>(null);
  const debounceRef = useRef<number | null>(null);
  
  // 聚焦输入框
  useEffect(() => {
    if (open) {
      inputRef.current?.focus();
    }
  }, [open]);
  
  // 执行搜索
  const doSearch = useCallback(async (searchQuery: string) => {
    if (!searchQuery.trim() || !sftpSessionId || !project) {
      setResults([]);
      return;
    }
    
    setIsSearching(true);
    setError(null);
    
    try {
      // TODO: 调用后端搜索 API
      // const response = await api.ideSearchInProject(
      //   sftpSessionId, 
      //   project.rootPath, 
      //   searchQuery,
      //   100
      // );
      
      // Mock 实现
      setResults([]);
      
      // 展开所有结果
      // setExpandedPaths(new Set(response.matches.map(r => r.path)));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsSearching(false);
    }
  }, [sftpSessionId, project]);
  
  // 防抖搜索
  const handleQueryChange = useCallback((value: string) => {
    setQuery(value);
    
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }
    
    debounceRef.current = window.setTimeout(() => {
      doSearch(value);
    }, 300);
  }, [doSearch]);
  
  // 跳转到搜索结果
  const handleMatchClick = useCallback((match: SearchMatch) => {
    openFile(match.path).then(() => {
      // TODO: 跳转到指定行
      // 需要通过 ideStore 传递目标行号，然后在 IdeEditor 中处理
    });
  }, [openFile]);
  
  // 切换文件展开
  const togglePath = useCallback((path: string) => {
    setExpandedPaths(prev => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);
  
  if (!open) return null;
  
  return (
    <div className="w-80 h-full flex flex-col bg-zinc-900 border-r border-zinc-800">
      {/* 标题栏 */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800">
        <div className="flex items-center gap-2">
          <Search className="w-4 h-4 text-orange-500" />
          <span className="text-sm font-medium">{t('ide.search')}</span>
        </div>
        <button
          onClick={onClose}
          className="p-1 hover:bg-zinc-800 rounded transition-colors"
        >
          <X className="w-4 h-4 text-zinc-500" />
        </button>
      </div>
      
      {/* 搜索输入 */}
      <div className="p-2 border-b border-zinc-800">
        <div className="relative">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500" />
          <Input
            ref={inputRef}
            value={query}
            onChange={(e) => handleQueryChange(e.target.value)}
            placeholder={t('ide.search_placeholder')}
            className="pl-8 bg-zinc-800 border-zinc-700"
          />
          {isSearching && (
            <Loader2 className="absolute right-2 top-1/2 -translate-y-1/2 w-4 h-4 animate-spin text-zinc-500" />
          )}
        </div>
      </div>
      
      {/* 搜索结果 */}
      <div className="flex-1 overflow-auto">
        {error && (
          <div className="p-4 text-red-400 text-sm">{error}</div>
        )}
        
        {!query && (
          <div className="p-4 text-zinc-500 text-sm text-center">
            {t('ide.search_hint')}
          </div>
        )}
        
        {query && results.length === 0 && !isSearching && (
          <div className="p-4 text-zinc-500 text-sm text-center">
            {t('ide.no_results')}
          </div>
        )}
        
        {results.map(result => (
          <div key={result.path} className="border-b border-zinc-800/50">
            <div
              className="flex items-center gap-2 px-3 py-1.5 hover:bg-zinc-800/50 cursor-pointer"
              onClick={() => togglePath(result.path)}
            >
              <ChevronRight 
                className={cn(
                  'w-3 h-3 text-zinc-500 transition-transform',
                  expandedPaths.has(result.path) && 'rotate-90'
                )}
              />
              <File className="w-4 h-4 text-zinc-500" />
              <span className="text-sm truncate flex-1">
                {result.path.split('/').pop()}
              </span>
              <span className="text-xs text-zinc-600">
                {result.matches.length}
              </span>
            </div>
            
            {expandedPaths.has(result.path) && (
              <div className="pl-6">
                {result.matches.map((match, idx) => (
                  <div
                    key={idx}
                    className="flex items-center gap-2 px-3 py-1 hover:bg-zinc-800/30 cursor-pointer text-sm"
                    onClick={() => handleMatchClick(match)}
                  >
                    <span className="text-zinc-600 w-8 text-right">
                      {match.line}
                    </span>
                    <span className="truncate text-zinc-400">
                      {match.preview.substring(0, match.matchStart)}
                      <span className="text-yellow-500 font-medium">
                        {match.preview.substring(match.matchStart, match.matchEnd)}
                      </span>
                      {match.preview.substring(match.matchEnd)}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
```

---

#### 任务 4.4: 后端搜索命令（如需要）（1d）

**注意：** Phase 4 的搜索功能需要通过 SSH 执行 `grep` 命令。如果现有后端没有执行远程命令的 API，需要添加。

**文件：** `src-tauri/src/commands/ide.rs`

```rust
/// 项目内搜索（通过 SSH 执行 grep）
/// 
/// 注意：这需要一个活动的 SSH 会话（不是 SFTP）
/// 可以考虑复用终端会话或创建临时 exec channel
#[tauri::command]
pub async fn ide_search_in_project(
    connection_id: String,
    project_path: String,
    query: String,
    max_results: u32,
    ssh_pool: State<'_, Arc<SshConnectionPool>>,
) -> Result<SearchResults, String> {
    // 安全检查
    let max_results = max_results.min(500);
    
    if query.contains(|c: char| c == '\0' || c == '\'' || c == '"' || c == '`') {
        return Err("Invalid search query".to_string());
    }
    
    // 获取 SSH 连接
    let conn = ssh_pool
        .get(&connection_id)
        .ok_or_else(|| format!("SSH connection not found: {}", connection_id))?;
    
    // 构建 grep 命令
    // 使用 -r (递归) -n (行号) -I (忽略二进制) --include (文件类型)
    let cmd = format!(
        r#"grep -rn -I --include='*.{{rs,ts,tsx,js,jsx,py,go,java,c,cpp,h,hpp,json,yaml,yml,toml,md,txt,sh}}' -m {} -- '{}' '{}' 2>/dev/null | head -n {}"#,
        max_results,
        query.replace("'", "'\\''"),  // 转义单引号
        project_path,
        max_results
    );
    
    // 执行命令
    let output = conn.exec(&cmd).await
        .map_err(|e| format!("Search failed: {}", e))?;
    
    // 解析 grep 输出
    // 格式: /path/to/file:123:matching line content
    let mut matches = Vec::new();
    
    for line in output.lines() {
        if matches.len() >= max_results as usize {
            break;
        }
        
        // 解析: path:line:content
        let parts: Vec<&str> = line.splitn(3, ':').collect();
        if parts.len() >= 3 {
            if let Ok(line_num) = parts[1].parse::<u32>() {
                matches.push(SearchMatch {
                    path: parts[0].to_string(),
                    line: line_num,
                    column: 0,  // grep 不提供列号
                    preview: parts[2].chars().take(200).collect(),
                });
            }
        }
    }
    
    Ok(SearchResults {
        matches,
        truncated: matches.len() >= max_results as usize,
    })
}
```

---

#### 任务 4.5: 添加搜索快捷键和 UI 入口（0.5d）

在 `IdeWorkspace.tsx` 中添加：

```tsx
// 状态
const [searchOpen, setSearchOpen] = useState(false);

// 快捷键
useEffect(() => {
  const handleKeyDown = (e: KeyboardEvent) => {
    // Cmd/Ctrl+Shift+F 打开搜索
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === 'f') {
      e.preventDefault();
      setSearchOpen(prev => !prev);
    }
  };
  
  window.addEventListener('keydown', handleKeyDown);
  return () => window.removeEventListener('keydown', handleKeyDown);
}, []);

// 在布局中添加搜索面板
{searchOpen && (
  <IdeSearchPanel 
    open={searchOpen} 
    onClose={() => setSearchOpen(false)} 
  />
)}
```

---

#### 任务 4.6: 添加 Phase 4 i18n 键值（0.5d）

```json
{
  "ide": {
    "search": "Search",
    "search_hint": "Type to search in project files",
    "no_results": "No results found",
    "search_error": "Search failed"
  }
}
```

**中文：**

```json
{
  "ide": {
    "search": "搜索",
    "search_hint": "输入以在项目文件中搜索",
    "no_results": "未找到结果",
    "search_error": "搜索失败"
  }
}
```

---

#### Phase 4 验证清单

- [ ] 文件树显示 Git 状态颜色（如果实现）
- [ ] `Cmd/Ctrl+Shift+F` 打开搜索面板
- [ ] 搜索可以找到文件内容
- [ ] 点击搜索结果可以打开文件
- [ ] 搜索面板可以关闭

---

## 7. 关键技术决策

### 7.1 为什么不用现有 SFTPView？

| 方面 | SFTPView | IdeTree |
|------|----------|---------|
| 布局 | 双面板（本地+远程） | 单面板（仅远程） |
| 操作 | 传输为主 | 编辑为主 |
| 状态 | 无缓存 | 带缓存的状态管理 |
| 图标 | 统一文件图标 | 按语言显示图标 |

**结论：** 复用 FileList 渲染逻辑，但创建新容器组件。

### 7.2 会话管理策略

```
IDE 标签创建时：
1. 复用现有 SSH 连接（通过 connectionId）
2. 创建独立的 SFTP 会话（用于文件操作）
3. 按需创建终端会话（用户打开终端时）

IDE 标签关闭时：
1. 提示保存未保存文件
2. 关闭 SFTP 会话
3. 关闭终端会话（如果存在）
4. 不关闭 SSH 连接（可能被其他标签使用）
```

### 7.3 内存管理策略

```typescript
// 标签数量限制
const MAX_OPEN_TABS = 20;  // 硬性限制
const WARN_TAB_COUNT = 15; // 超过时显示警告

// 内存中保留的编辑器数量
const MAX_MEMORY_EDITORS = 10;

// 驱逐策略：LRU + 优先保留 dirty 标签
function selectTabsToEvict(tabs: IdeTab[], count: number): string[] {
  return tabs
    .filter(t => !t.isDirty && t.id !== activeTabId)
    .sort((a, b) => a.lastAccessTime - b.lastAccessTime)
    .slice(0, count)
    .map(t => t.id);
}
```

---

## 8. 快捷键设计

| 快捷键 | 功能 | 范围 |
|--------|------|------|
| `Cmd/Ctrl + S` | 保存当前文件 | 编辑器 |
| `Cmd/Ctrl + W` | 关闭当前标签 | 全局 |
| `Cmd/Ctrl + Shift + S` | 保存所有文件 | 全局 |
| `Cmd/Ctrl + P` | 快速打开文件 | 全局 |
| `Cmd/Ctrl + Shift + P` | 命令面板 | 全局 |
| `Cmd/Ctrl + B` | 切换侧边栏 | 全局 |
| `` Ctrl + ` `` | 切换终端 | 全局 |
| `Alt + Left/Right` | 切换标签 | 全局 |
| `Cmd/Ctrl + 1-9` | 跳转到第 N 个标签 | 全局 |

---

## 9. 错误处理和边界情况

### 9.1 网络断开

```typescript
// 监听网络状态
useEffect(() => {
  const unsubscribe = useAppStore.subscribe(
    state => state.networkOnline,
    (online) => {
      if (!online) {
        // 标记所有标签为离线状态
        setTabsOffline();
        // 显示重连提示
        showOfflineBanner();
      } else {
        // 重新同步文件状态
        syncAllTabsWithRemote();
      }
    }
  );
  return unsubscribe;
}, []);
```

### 9.2 大文件处理

- 打开时检查文件大小（ide_check_file）
- 超过 10MB 显示警告，允许用户选择是否继续
- 超过 50MB 直接拒绝（防止浏览器崩溃）

### 9.3 并发编辑

- 同一文件不允许在多个标签中打开
- 尝试打开已打开的文件时，跳转到对应标签

---

## 10. 测试策略

### 10.1 单元测试

- ideStore 状态变更逻辑
- useFileCache 缓存策略
- 冲突检测逻辑

### 10.2 集成测试

- 打开项目 → 打开文件 → 编辑 → 保存 流程
- 多标签切换和关闭
- 网络断开和恢复

### 10.3 性能测试

- 100+ 文件的目录加载时间
- 10 个标签同时打开的内存占用
- 大文件（5MB）打开时间

---

## 11. 风险评估

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| CodeMirror 内存泄漏 | 中 | 高 | 严格的清理逻辑，定期检查 |
| SFTP 会话断开 | 高 | 中 | 自动重连，本地缓存 |
| 文件保存冲突 | 中 | 高 | 冲突检测 + 用户确认 |
| 大项目性能 | 中 | 中 | 虚拟滚动 + 懒加载 |

---

## 12. 成功指标

| 指标 | 目标值 |
|------|--------|
| 打开项目响应时间 | < 2s |
| 打开文件响应时间 | < 500ms |
| 保存文件响应时间 | < 1s |
| 10 标签内存占用 | < 200MB |
| 无未保存文件丢失 | 100% |

---

## 附录 A: i18n 键值设计

```json
{
  "ide": {
    "open_project": "打开项目",
    "close_project": "关闭项目", 
    "select_folder": "选择文件夹作为项目根目录",
    "unsaved_changes": "以下文件有未保存的更改：",
    "save_all": "全部保存",
    "discard_all": "全部放弃",
    "file_conflict": "文件冲突",
    "file_conflict_desc": "远程文件已被修改，请选择处理方式：",
    "conflict_overwrite": "覆盖远程",
    "conflict_reload": "重新加载",
    "conflict_save_as": "另存为",
    "file_too_large": "文件过大",
    "file_too_large_desc": "文件大小 {{size}} 超过限制 {{limit}}",
    "file_binary": "无法编辑二进制文件",
    "terminal_toggle": "切换终端",
    "git_branch": "分支: {{branch}}",
    "search_placeholder": "搜索文件...",
    "no_open_files": "无打开的文件",
    "click_to_open": "双击文件树中的文件开始编辑"
  }
}
```

---

**总预估时间：** 8-10 周（Phase 1-3 必做，Phase 4 可选）

**建议启动顺序：** Phase 1 → Phase 2 → Phase 3 → 用户反馈 → 决定是否做 Phase 4

---

## 附录 B: 架构审计发现与修正

> 本节记录设计方案审计中发现的问题及修正措施

### B.1 API 兼容性问题 ⚠️ 已修正

| 问题 | 原设计 | 实际情况 | 修正 |
|------|--------|----------|------|
| `sftp.read_file_range()` | 用于二进制检测 | 方法不存在 | 改用 `sftp.preview()` |
| `sftp.stat().is_dir()` | 布尔方法 | 返回 `FileInfo`，需检查 `file_type == FileType::Directory` | 已修正 |
| `sftp.read_file()` | 直接读取文件 | 不存在，需用 `preview()` | 已修正 |
| `SftpRegistry.get()` 返回类型 | 直接返回 `SftpSession` | 返回 `Arc<Mutex<SftpSession>>` | 已添加 `.lock().await` |

### B.2 现有 API 可复用清单

```rust
// 可直接复用的现有 SFTP API
sftp.stat(path)           // 获取文件信息 → FileInfo
sftp.list_dir(path)       // 列出目录内容 → Vec<FileInfo>
sftp.preview(path)        // 预览文件（自动检测类型）→ PreviewContent
sftp.write_content(path)  // 写入文件内容
sftp.mkdir(path)          // 创建目录
sftp.rename(old, new)     // 重命名/移动
sftp.delete(path)         // 删除文件
sftp.delete_recursive()   // 递归删除目录

// FileInfo 结构（来自 sftp/types.rs）
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub file_type: FileType,  // Directory | File | Symlink | Unknown
    pub size: u64,
    pub modified: i64,        // 注意是 i64，需转换
    pub permissions: String,
    pub owner: Option<String>,
    pub group: Option<String>,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
}
```

### B.3 需要新增的后端功能

| 功能 | 必要性 | 说明 |
|------|--------|------|
| `sftp_read_text_file` | 高 | 专门读取文本文件内容（不走 preview 检测流程） |
| `ide_open_project` | 高 | Phase 1 必需 |
| `ide_check_file` | 高 | Phase 2 必需 |
| `ide_batch_stat` | 中 | 优化性能，可延迟 |
| `ide_search_in_project` | 低 | Phase 4 功能 |

### B.4 前端现有组件复用清单

```typescript
// 可直接复用
RemoteFileEditor.tsx    // CodeMirror 6 编辑器 → 抽取为 useCodeMirrorEditor hook
SFTPView.tsx            // FileList 渲染逻辑
api.ts                  // sftpStat, sftpListDir, sftpWriteContent
types/index.ts          // FileInfo, TabType, PaneNode

// 需要适配
TabType                 // 添加 'ide'
appStore.ts             // 添加 createIdeTab action
```

### B.5 RemoteFileEditor 复用策略（CodeMirror 6）

现有 `RemoteFileEditor.tsx` 是一个 **Dialog 组件**（模态框），IDE 模式需要**非模态的嵌入式编辑器**。

**现有 CodeMirror 6 配置（来自 RemoteFileEditor.tsx）：**

```typescript
// 已使用的 CodeMirror 6 包（可直接复用）
import { EditorView, keymap, lineNumbers, highlightActiveLineGutter } from '@codemirror/view';
import { EditorState, Extension } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { indentOnInput, bracketMatching, foldGutter, foldKeymap } from '@codemirror/language';
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
import { autocompletion, completionKeymap } from '@codemirror/autocomplete';
import { oneDark } from '@codemirror/theme-one-dark';

// 语言加载器（懒加载）
import { loadLanguage, normalizeLanguage } from '../../lib/codemirror/languageLoader';
```

**建议方案：**

```typescript
// 1. 抽取 CodeMirror 6 初始化逻辑为 hook
// src/components/ide/hooks/useCodeMirrorEditor.ts
import { useRef, useEffect, useCallback } from 'react';
import { EditorView, keymap, lineNumbers, highlightActiveLineGutter } from '@codemirror/view';
import { EditorState, Extension } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { indentOnInput, bracketMatching, foldGutter, foldKeymap } from '@codemirror/language';
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
import { autocompletion, completionKeymap } from '@codemirror/autocomplete';
import { oneDark } from '@codemirror/theme-one-dark';
import { loadLanguage } from '../../../lib/codemirror/languageLoader';

interface UseCodeMirrorEditorOptions {
  initialContent: string;
  language: string | null;
  onContentChange: (content: string) => void;
  onCursorChange?: (line: number, col: number) => void;
  onSave: () => void;
}

export function useCodeMirrorEditor(options: UseCodeMirrorEditorOptions) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const contentRef = useRef(options.initialContent);
  
  // 初始化编辑器
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    
    let view: EditorView | null = null;
    let mounted = true;
    
    const init = async () => {
      const langSupport = await loadLanguage(options.language);
      if (!mounted) return;
      
      const extensions: Extension[] = [
        lineNumbers(),
        highlightActiveLineGutter(),
        history(),
        foldGutter(),
        indentOnInput(),
        bracketMatching(),
        autocompletion(),
        highlightSelectionMatches(),
        oneDark,
        // Oxide 主题适配
        EditorView.theme({
          '&': { height: '100%', fontSize: '13px' },
          '.cm-scroller': { 
            fontFamily: '"JetBrains Mono", "Fira Code", monospace',
            overflow: 'auto',
          },
          '.cm-gutters': { 
            backgroundColor: 'rgb(39 39 42 / 0.5)',
            borderRight: '1px solid rgb(63 63 70 / 0.5)',
          },
          '.cm-activeLineGutter': { backgroundColor: 'rgb(234 88 12 / 0.1)' },
          '.cm-activeLine': { backgroundColor: 'rgb(234 88 12 / 0.05)' },
          '&.cm-focused .cm-cursor': { borderLeftColor: '#f97316' },
        }),
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          ...foldKeymap,
          ...searchKeymap,
          ...completionKeymap,
          indentWithTab,
          { key: 'Mod-s', run: () => { options.onSave(); return true; } },
        ]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            const content = update.state.doc.toString();
            contentRef.current = content;
            options.onContentChange(content);
          }
          if (options.onCursorChange) {
            const pos = update.state.selection.main.head;
            const line = update.state.doc.lineAt(pos);
            options.onCursorChange(line.number, pos - line.from + 1);
          }
        }),
      ];
      
      if (langSupport) extensions.push(langSupport);
      
      const state = EditorState.create({
        doc: options.initialContent,
        extensions,
      });
      
      container.innerHTML = '';
      view = new EditorView({ state, parent: container });
      viewRef.current = view;
    };
    
    init();
    
    return () => {
      mounted = false;
      view?.destroy();
      viewRef.current = null;
    };
  }, [options.language]); // 语言变化时重新初始化
  
  // 外部更新内容
  const setContent = useCallback((content: string) => {
    const view = viewRef.current;
    if (view && content !== contentRef.current) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: content },
      });
    }
  }, []);
  
  return { containerRef, setContent, getContent: () => contentRef.current };
}

// 2. IdeEditor 组件使用 hook
function IdeEditor({ tab }: { tab: IdeTab }) {
  const { containerRef } = useCodeMirrorEditor({
    initialContent: tab.content ?? '',
    language: tab.language,
    onContentChange: (c) => useIdeStore.getState().updateTabContent(tab.id, c),
    onCursorChange: (line, col) => useIdeStore.getState().updateTabCursor(tab.id, line, col),
    onSave: () => useIdeStore.getState().saveFile(tab.id),
  });
  
  return <div ref={containerRef} className="h-full" />;
}

// 3. 保留 RemoteFileEditor 作为 SFTP 模式的模态编辑器
```

### B.6 潜在风险清单

| 风险 | 概率 | 影响 | 状态 |
|------|------|------|------|
| `SftpSession` API 假设错误 | 已发生 | 高 | ✅ 已修正 |
| `FileInfo` 字段类型不匹配 | 中 | 中 | ✅ 已标注 |
| 搜索功能需要 SSH 会话 | 已确认 | 低 | ✅ 已标注为 Phase 4 |
| CodeMirror 多实例内存 | 中 | 高 | 📝 需测试 |
| IndexedDB 配额限制 | 低 | 中 | 📝 需添加清理策略 |

### B.7 类型安全检查清单

实施时需确保以下类型正确：

```typescript
// types/index.ts 需要添加
export type TabType = 'terminal' | 'sftp' | 'forwards' | 'settings' | 
  'connection_monitor' | 'connection_pool' | 'topology' | 'local_terminal' | 
  'ide';  // ← 新增

// FileInfo.modified 是 i64，前端需要处理
interface FileInfo {
  modified: number;  // Unix timestamp (秒)，注意后端是 i64
}

// IdeTab.serverMtime 应与 FileInfo.modified 类型一致
interface IdeTab {
  serverMtime?: number;  // 同样是 Unix timestamp (秒)
}
```

---

## 附录 C: 实施前置条件清单

在开始 Phase 1 之前，建议完成以下准备工作：

- [ ] 确认 `sftp_read_text_file` 是否需要新增（或直接使用 `preview`）
- [ ] 在 `types/index.ts` 中添加 `'ide'` 到 `TabType`
- [ ] 创建 `src/components/ide/` 目录结构
- [ ] 创建 `src/store/ideStore.ts` 骨架
- [ ] 抽取 `useCodeMirrorEditor` hook
- [ ] 在 `src-tauri/src/commands/` 创建 `ide.rs` 模块
- [ ] 在 `lib.rs` 注册新命令

---

*文档版本: v2.2 (完整实施指南)*
*最后更新: 2026-01-30*