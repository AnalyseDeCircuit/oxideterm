# AI 内联终端助手

> 直接在终端中与 AI 对话，获取命令建议、错误诊断和代码解释。

## 🎯 功能概览

AI 内联助手为您的终端体验带来智能增强：

- **🔍 上下文感知**：自动捕获选中文本作为对话上下文
- **🌐 操作系统感知**：自动检测本地 OS，并在 SSH 终端中注入远端环境（检测中 / 失败 / 成功三态）
- **💬 流式响应**：实时显示 AI 生成的内容，无需等待
- **🔒 隐私优先**：所有请求直接从本地发起，API Key 存储在系统钥匙串
- **🌐 OpenAI 兼容**：支持 OpenAI、Ollama、DeepSeek、OneAPI 等任意兼容端点
- **📋 非破坏性输出**：AI 建议仅作预览，您可选择插入、执行或复制
- **🎨 VS Code 风格界面**：全新设计的浮动面板，跟随光标定位

---

## 🚀 快速开始

### 1. 启用 AI 功能

首次使用需要在设置中启用 AI 功能：

1. 打开设置
2. 切换到 **AI** 标签页
3. 启用 **Enable AI Capabilities** 开关
4. 阅读并确认隐私声明

### 2. 配置 API 端点

配置您的 AI 服务提供商：

```
Base URL: https://api.openai.com/v1  (默认)
Model:    gpt-4o-mini                 (推荐)
API Key:  sk-...                      (存储在系统钥匙串)
```

**支持的服务提供商**：
- **OpenAI** - `https://api.openai.com/v1`
- **Ollama** - `http://localhost:11434`
- **DeepSeek** - `https://api.deepseek.com/v1`
- **OneAPI** - 您的自定义网关地址

### 3. 开始使用

在任意终端窗口中：

1. 按下 **`Ctrl+Shift+I`** (Windows) 或 **`⌘I`** (macOS/Linux)
2. 在弹出的浮动面板中输入您的问题
3. AI 会根据当前上下文（环境 + 选区）生成响应

---

## 🎨 VS Code 风格界面

内联 AI 面板采用全新的 VS Code 风格设计：

### 视觉特性
- **无背景遮罩**：面板直接浮动在终端上方
- **阴影边框**：细腻的阴影效果提供层次感
- **固定宽度**：520px 宽度，紧凑高效
- **光标定位**：面板跟随终端光标位置显示
- **模型选择**：面板顶部可切换 Provider/Model

### 交互快捷键
| 快捷键 | 功能 |
|--------|------|
| `Enter` | 发送问题；有结果时执行命令 |
| `Tab` | 将 AI 建议插入终端 |
| `Esc` | 关闭面板 |

### 智能定位
面板会根据光标位置智能调整：
- 优先显示在光标下方
- 空间不足时自动切换到上方
- 水平方向自动适应屏幕边界

---

## 💡 使用场景

### 场景 1：命令错误诊断

```bash
$ git push origin mainn
fatal: 'mainn' does not match any known branch
```

**操作**：
1. 选中错误输出
2. 按 `Ctrl+Shift+I` / `⌘I`
3. 输入："这个错误是什么意思？"

**AI 响应**：
> "您尝试推送到名为 'mainn' 的分支，但该分支不存在。正确的分支名可能是 'main'。请使用：`git push origin main`"

**一键执行**：点击"Execute"按钮直接执行建议的命令

---

### 场景 2：命令生成

**操作**：
1. 按 `Ctrl+Shift+I` / `⌘I`
2. 输入："查找所有大于 100MB 的文件"

**AI 响应**：
```bash
find . -type f -size +100M -exec ls -lh {} \; | awk '{ print $9 ": " $5 }'
```

**一键插入**：点击"Insert"按钮将命令插入到终端（但不执行）

---

### 场景 3：日志分析

```bash
$ npm run build
...
ERROR in ./src/index.js 15:12
Module not found: Error: Can't resolve 'react-dom/client'
```

**操作**：
1. 按 `Ctrl+Shift+I` / `⌘I`
2. 输入："如何修复？"

**AI 响应**：
> "缺少 `react-dom` 依赖。请运行：`npm install react-dom`"

---

## 🎨 Overlay 界面

```
┌────────────────────────────────────────────────────────┐
│ AI Inline Chat                                    [×]  │
├────────────────────────────────────────────────────────┤
│  Model: Provider/Model                              │
│                                                        │
│  ┌──────────────────────────────────────────────────┐ │
│  │ 您的问题...                                       │ │
│  └──────────────────────────────────────────────────┘ │
│                                                        │
│  ┌──────────────────────────────────────────────────┐ │
│  │ AI Response:                                     │ │
│  │                                                  │ │
│  │ 根据错误信息，您需要...                          │ │
│  │                                                  │ │
│  │ ```bash                                          │ │
│  │ npm install react-dom                            │ │
│  │ ```                                              │ │
│  └──────────────────────────────────────────────────┘ │
│                                                        │
│  [Insert]  [Execute]  [Copy]  [Regenerate]           │
└────────────────────────────────────────────────────────┘
```

---

## ⚙️ 上下文策略

AI 助手支持智能上下文注入：

### 上下文组成

每次发送消息时，系统自动构建结构化提示：

```
[System Prompt]
You are a helpful terminal assistant...

Environment: Local/SSH + 远端环境（如已检测）
Selection Context: (如果有选中文本，打开面板时冻结)
[用户选中的文本]

[User Message]
用户的问题
```

默认系统提示要求：除非用户明确要求解释，否则只返回命令或代码本身。

### 1. Selection（选中文本）- 最高优先级

- **触发**：当您在终端中选中文本时
- **冻结时机**：面板打开时自动捕获并冻结选区
- **动态占位符**：有选区时显示 "分析选中的内容..."

**示例**：
```bash
# 选中这行错误
command not found: kubectl
```
AI 可以识别具体问题并提供精准建议。

### 2. 无选区模式

- **触发**：未选中文本时
- **占位符**：显示 "询问 AI..."
- **上下文**：仅包含环境信息（不注入可见缓冲区）

> 当前版本的内联面板不会自动发送可见缓冲区，仅使用选区（如果存在）+ 环境信息。

---

## 🔒 隐私与安全

### 数据传输

- ✅ **本地发起请求**：所有 API 调用直接从您的本地机器发起
- ✅ **无中转服务器**：OxideTerm 不运行任何中转代理
- ✅ **上下文可控**：仅发送选中的文本（如有）+ 环境信息

### API Key 存储

- ✅ **系统钥匙串**：API Key 存储在 OS 原生安全存储中（v1.6.0 起）
  - macOS: Keychain Services（`com.oxideterm.ai` 服务）
  - Windows: Credential Manager
  - Linux: Secret Service（libsecret / gnome-keyring）
  - 与 SSH 密码享有同等 OS 级别加密保护
- ✅ **自动迁移**：旧版本的 XOR vault 文件会在首次访问时自动迁移到系统钥匙串
- ❌ **绝不落盘**：API Key 不会写入配置文件或 localStorage
- ❌ **不进入日志**：API Key 不会出现在任何日志中

### 上下文限制

您可以配置上下文上限以控制成本：

- **最大字符数**：默认 8,000 字符（用于选中文本截断）
- **可见行数**：默认 120 行（供侧边栏聊天使用）

---

## 🎛️ 高级配置

### 设置位置

设置 → AI 标签页

### 可配置项

| 配置项 | 默认值 | 说明 |
|--------|--------|------|
| **Enable AI** | `false` | 全局开关，首次启用需确认 |
| **Base URL** | `https://api.openai.com/v1` | API 端点地址 |
| **Model** | `gpt-4o-mini` | 使用的模型 |
| **API Key** | (空) | 存储在系统钥匙串（`com.oxideterm.ai`） |
| **Max Characters** | `8000` | 选中上下文最大字符数（超出会截断） |
| **Visible Lines** | `120` | 供侧边栏聊天使用的可见行数上限（内联面板不使用） |

### 使用自托管 Ollama

```
Base URL: http://localhost:11434
Model:    llama3.2
API Key:  (留空，Ollama 不需要)
```

### 使用 DeepSeek

```
Base URL: https://api.deepseek.com/v1
Model:    deepseek-chat
API Key:  sk-... (从 DeepSeek 控制台获取)
```

---

## 🔧 操作按钮

AI 响应面板提供以下操作按钮：

### Insert（插入）

- 将 AI 建议的命令**插入**到终端输入框
- **不会自动执行**
- 您可以在执行前检查和修改命令

### Execute（执行）

- 直接在终端中**执行** AI 建议的命令
- ⚠️ 仅在您完全信任 AI 建议时使用
- 建议用于简单的只读命令（如 `ls`, `cat`）

### Regenerate（重试）

- 重新生成本次问题的响应

### Copy（复制）

- 将 AI 响应复制到剪贴板
- 适用于需要粘贴到其他应用的场景

---

## ❓ 常见问题

### Q: 支持哪些 AI 模型？

A: 任何兼容 OpenAI Chat Completions API 的模型，包括：
- OpenAI GPT 系列（gpt-4, gpt-3.5-turbo, gpt-4o-mini）
- Anthropic Claude（通过代理）
- 本地模型（Ollama, LM Studio）
- 国内厂商（DeepSeek, 通义千问等，通过 OneAPI）

---

### Q: API Key 是否安全？

A: 是的。API Key 存储在操作系统原生钥匙串中（v1.6.0 起）：
- macOS 使用 Keychain Services，Windows 使用 Credential Manager
- 由操作系统提供硬件级加密保护
- 不会写入任何配置文件或日志

OxideTerm 自身无法访问其他应用的 API Key，反之亦然。

---

### Q: 如何禁用 AI 功能？

A: 
1. 打开设置 → AI 标签页
2. 关闭 **Enable AI Capabilities** 开关
3. Overlay 快捷键将不再响应

---

### Q: 可以使用免费的本地 AI 模型吗？

A: 可以！推荐使用 Ollama：

1. 安装 Ollama：`brew install ollama` (macOS)
2. 拉取模型：`ollama pull llama3.2`
3. 配置 Base URL：`http://localhost:11434`
4. Model：`llama3.2`
5. API Key：留空

---

### Q: 为什么响应速度慢？

A: 可能的原因：
- **网络延迟**：OpenAI API 服务器可能较慢，考虑使用 Ollama 本地模型
- **模型选择**：`gpt-4` 比 `gpt-3.5-turbo` 慢，`gpt-4o-mini` 速度最快
- **上下文过大**：减少 `Max Characters` 配置值

---

### Q: 会发送我的终端历史吗？

A: **不会**。仅发送您主动选中的文本（如果有），并附带环境信息。不会发送完整滚动缓冲区。

---

## 🛣️ 未来计划

- [ ] **Command Context 抽取**：自动识别最后一条命令及其输出
- [ ] **精确 Tokenizer**：使用 tiktoken 进行精确 token 计数
- [ ] **多轮对话**：在 Overlay 中支持连续追问
- [ ] **代码高亮**：AI 返回的代码块语法高亮

---

## 📝 快捷键参考

| 快捷键 | 功能 |
|--------|------|
| `Ctrl+Shift+I` / `⌘I` | 打开 AI Inline Panel |
| `Esc` | 关闭 Overlay |
| `Enter` | 发送问题 / 执行命令 |
| `Tab` | 插入建议命令 |

---

## 🙏 致谢

AI 内联助手的设计灵感来自：
- [GitHub Copilot](https://github.com/features/copilot) - 代码补全
- [Cursor AI](https://cursor.sh/) - 编辑器内 AI 对话
- [Warp Terminal](https://www.warp.dev/) - AI 命令建议

---

*文档版本: v1.6.2 | 最后更新: 2026-02-08*
