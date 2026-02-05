# OxideTerm 使用指南 (v1.4.0)

> 从入门到精通，全面掌握 OxideTerm 的核心功能和最佳实践。
>
> **v1.4.0 新特性**：自动重连更可靠、SFTP 路径记忆、端口转发自动恢复。

## 📋 目录

1. [快速开始](#快速开始)
2. [基础操作](#基础操作)
3. [连接管理](#连接管理)
4. [终端功能](#终端功能)
5. [SFTP 文件管理](#sftp-文件管理)
6. [端口转发](#端口转发)
7. [高级功能](#高级功能)
8. [快捷键速查](#快捷键速查)
9. [最佳实践](#最佳实践)
10. [故障排查](#故障排查)

---

## 🚀 快速开始

### 首次运行

1. **启动 OxideTerm**
   - macOS: 应用程序文件夹
   - Windows: 开始菜单或桌面快捷方式
   - Linux: `oxideterm` 命令

2. **创建第一个连接**
   - 点击左侧边栏 **"New Connection"** 按钮
   - 填写服务器信息：
     - Name: `My Server`
     - Host: `example.com`
     - Port: `22`
     - Username: `admin`
     - Authentication: 选择密码或密钥

3. **连接到服务器**
   - 双击保存的连接
   - 输入密码（如果使用密码认证）
   - 终端窗口会自动打开

---

## 🎮 基础操作

### 创建终端

#### 本地终端

快速开启本地终端（无需 SSH）：

- **快捷键**: `Ctrl+T` (Windows/Linux) 或 `⌘T` (macOS) - 默认 Shell
- **Shell 选择器**: `Ctrl+Shift+T` (Windows/Linux) 或 `⌘+Shift+T` (macOS)
- **侧边栏**: Connections → Local Terminal
- **用途**: 本地开发、测试、脚本执行

#### SSH 终端

连接到远程服务器：

1. **方法 1**: 双击已保存的连接
2. **方法 2**: 右键菜单 → "Connect"
3. **方法 3**: 双击连接组展开/折叠

### 标签管理

> **注意**: 标签页切换快捷键目前正在开发中。

### 会话树导航

OxideTerm 使用树形结构组织会话：

```
┌── 📁 Production
│   ├── 🖥️  Web Server
│   │   └── 🟢 Active Session
│   └── 🖥️  Database Server
├── 📁 Development
│   └── 🖥️  Dev Machine
└── 💻 Local Terminal
```

**操作**：
- 点击展开/折叠分组
- 右键显示上下文菜单
- 拖拽排序（开发中）

---

## 🔌 连接管理

### 保存连接

#### 1. 基本配置

```
Name: Production Server
Host: prod.example.com
Port: 22
Username: admin
Group: Production
```

#### 2. 认证方式

**密码认证**：
- 选择 "Password"
- 输入密码（存储在系统钥匙串，安全加密）

**密钥认证**：
- 选择 "SSH Key"
- 选择私钥文件（例如：`~/.ssh/id_rsa`）
- 如有密码，输入 Passphrase

**SSH Agent**：
- 选择 "SSH Agent"
- 确保 SSH Agent 正在运行并加载了密钥

**证书认证**：
- 选择 "Certificate"
- 选择私钥和证书文件

### ProxyJump (跳板机)

#### 单跳配置

```
项目配置：
  Name: Internal Server
  Host: db.internal
  Port: 22
  Username: dbadmin
  
  Proxy Chain:
    ├── Host: bastion.example.com
    ├── Port: 22
    ├── Username: admin
    └── Auth: SSH Agent
```

**等价 SSH 命令**：
```bash
ssh -J admin@bastion.example.com dbadmin@db.internal
```

#### 多跳配置

```
项目配置：
  Name: HPC Compute Node
  Host: node123.cluster
  
  Proxy Chain:
    ├── Hop 1: login.university.edu (port 22)
    ├── Hop 2: gateway.cluster (port 22)
    └── Target: node123.cluster
```

**等价 SSH 命令**：
```bash
ssh -J student@login.university.edu,admin@gateway.cluster researcher@node123.cluster
```

### 连接分组

**创建分组**：
- 右键侧边栏 → New Group
- 输入组名（例如：`Production`, `Staging`, `Development`）

**分组管理**：
- 拖拽连接到分组
- 折叠/展开分组
- 组内连接自动继承标签颜色

---

## 🖥️ 终端功能

### 搜索（Ctrl+F / ⌘F）

#### Visible Buffer 搜索

搜索当前可见的终端输出：

1. 按 `Ctrl+Shift+F` (Win) / `Ctrl+F` (Lin) 或 `⌘F`
2. 输入搜索词
3. 匹配项实时高亮
4. 使用 `Enter` / `Shift+Enter` 导航

**选项**：
- **Aa**: 大小写敏感
- **.***: 正则表达式
- **Word**: 整词匹配

#### Deep History 搜索

搜索完整会话历史（最多 100,000 行）：

1. 切换到 "Deep History" 标签
2. 输入搜索词
3. 按 `Enter` 执行搜索
4. 点击结果跳转到对应位置

**示例查询**：
```
查找错误日志：^(ERROR|FATAL):
查找 IP 地址：\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b
查找 Git 命令：^git (commit|push|pull)
```

详见：[TERMINAL_SEARCH.md](./TERMINAL_SEARCH.md)

### 复制与粘贴

| 操作 | Windows/Linux | macOS |
|------|---------------|-------|
| **复制** | `Ctrl+Shift+C` 或选中自动复制 | `⌘+C` 或选中自动复制 |
| **粘贴** | `Ctrl+Shift+V` 或右键 | `⌘+V` 或右键 |

### 清屏

| 方法 | 快捷键/命令 |
|------|------------|
| **完全清屏** | `Ctrl+L` 或 `⌘+K` |
| **Shell 清屏** | `clear` 命令 |
| **重置终端** | 右键 → Reset Terminal |

### 滚动

| 操作 | 方法 |
|------|------|
| **向上滚动** | 鼠标滚轮向上 或 `Page Up` |
| **向下滚动** | 鼠标滚轮向下 或 `Page Down` |
| **滚动到顶部** | `Home` |
| **滚动到底部** | `End` |

### 字体与主题

#### 设置字体

1. 打开设置（侧边栏配置图标）
2. 切换到 **Terminal** 标签
3. 选择字体（推荐：JetBrains Mono, Fira Code）
4. 调整字体大小（8-32）

#### 切换主题

OxideTerm 内置多个终端主题：

- Catppuccin Mocha（默认）
- Dracula
- Gruvbox Dark
- One Dark
- Solarized Dark/Light
- Tokyo Night

**切换方法**：
设置 → Terminal → Theme

---

## 📁 SFTP 文件管理

### 打开 SFTP 浏览器

#### 方法 1：从终端会话

右键终端标签 → Open SFTP

#### 方法 2：从连接列表

右键连接 → SFTP Browser

### v1.4.0 新特性

#### State Gating（状态门禁）

当连接不稳定时，SFTP 会自动显示等待遮罩，防止无效操作：

```
┌─────────────────────────────────────┐
│          Waiting for connection...  │
│          Current state: reconnecting│
└─────────────────────────────────────┘
```

连接恢复后，SFTP 自动激活。

#### Path Memory（路径记忆）

重连后自动恢复之前的工作目录，无需手动导航。

### 文件操作

#### 上传文件

1. **拖拽上传**: 将本地文件拖到右侧（远程）面板
2. **工具栏上传**: 点击 "Upload" 按钮选择文件
3. **快捷键**: `ArrowRight` (选中本地文件时)

#### 下载文件

1. **拖拽下载**: 将远程文件拖到左侧（本地）面板
2. **右键菜单**: 右键文件 → Download
3. **快捷键**: `ArrowLeft` (选中远程文件时)

#### 其他操作

| 操作 | 方法 |
|------|------|
| **新建文件夹** | 右键 → New Folder |
| **重命名** | 右键 → Rename 或 `F2` |
| **删除** | 右键 → Delete 或 `Delete` / `Backspace` |
| **刷新** | 工具栏刷新按钮 |
| **全选** | `Ctrl+A` / `⌘+A` |

### 传输队列

查看和管理文件传输：

1. 点击底部 "Transfers" 面板
2. 查看进度条和速度
3. 点击 ❌ 取消传输
4. 传输完成后自动移除

**传输状态**：
- 🟢 Transferring: 正在传输
- ✅ Completed: 完成
- ❌ Failed: 失败
- ⏸️ Paused: 暂停（计划中）

详见：[SFTP.md](./SFTP.md)

---

## 🔀 端口转发

### Local Forward (-L)

**用途**: 将远程端口映射到本地

**示例**: 访问远程数据库

```
配置：
  Local Address: 127.0.0.1:3306
  Remote Host: localhost
  Remote Port: 3306
  
效果：
  本地访问 localhost:3306 → 转发到远程 localhost:3306
```

**应用场景**：
- 访问远程数据库（MySQL, PostgreSQL）
- 访问远程 Web 服务
- 绕过防火墙访问内网服务

### Remote Forward (-R)

**用途**: 将本地端口映射到远程

**示例**: 让远程服务器访问本地 Web 服务

```
配置：
  Remote Address: 0.0.0.0:8080
  Local Host: localhost
  Local Port: 3000
  
效果：
  远程访问 8080 → 转发到本地 3000
```

**应用场景**：
- Webhook 调试
- 远程演示本地应用
- 反向代理

### Dynamic Forward (-D, SOCKS5)

**用途**: 创建 SOCKS5 代理

**示例**: 使用 SSH 作为代理

```
配置：
  Local Address: 127.0.0.1:1080
  
效果：
  所有通过 localhost:1080 的流量都会经过 SSH 隧道
```

**配置浏览器**：
1. 打开浏览器代理设置
2. 设置 SOCKS5 代理: `localhost:1080`
3. 所有浏览流量通过 SSH 加密

### v1.4.0 Link Resilience

端口转发规则会被持久化，重连后**自动恢复**：

```
网络断开 → 显示警告 → 自动重连 → 规则自动恢复 → 继续使用
```

无需手动重新配置。

详见：[PORT_FORWARDING.md](./PORT_FORWARDING.md)

---

## 🚀 高级功能

### 自动重连 (v1.4.0 增强)

**功能**: 网络断开时自动恢复连接

**v1.4.0 改进**：
- 重连成功后自动恢复 SFTP 路径
- 端口转发规则自动重建
- 终端无缝恢复输入

**行为**：
```
正常使用 → 网络断开 → 显示 Input Lock → 自动重连 → 恢复使用
```

**断线行为**：
- **Terminal**: 输入锁定，显示 Overlay，保留历史
- **SFTP**: 显示等待遮罩，重连后恢复路径
- **Port Forward**: 暂停，重连后自动恢复

详见：[CONNECTION_POOL.md](./CONNECTION_POOL.md)

### 连接池

**功能**: 多个会话共享同一 SSH 连接

**示例**：
```
同一服务器 (prod.example.com:22):
  ├── Terminal 1 (Shell)
  ├── Terminal 2 (Logs)
  ├── SFTP Browser
  └── Port Forward (-L 3306)
  
实际 SSH 连接数: 1
```

**优势**：
- ✅ 减少认证次数
- ✅ 降低服务器负载
- ✅ 节省网络资源
- ✅ 统一重连管理

### AI 内联助手

**功能**: 直接在终端中与 AI 对话

**使用方法**：
1. 按 `Ctrl+Shift+I` (Win) / `Ctrl+I` (Lin) 或 `⌘I`
2. 输入问题或选中文本
3. AI 返回建议
4. 选择 Insert / Execute / Copy

**示例场景**：
```
场景 1: 错误诊断
  选中错误输出 → Ctrl+I → "如何修复？"
  
场景 2: 命令生成
  Ctrl+I → "查找大于100MB的文件"
  AI: find . -type f -size +100M
  
场景 3: 日志分析
  自动捕获可见缓冲区 → AI 分析问题
```

详见：[AI_INLINE_CHAT.md](./AI_INLINE_CHAT.md)

### 网络拓扑

**功能**: 自动构建服务器拓扑图，计算最优路径

**示例**：
```
网络拓扑：
  本地 → VPN Gateway → Internal Network → Database Server
  
自动路由：
  任意连接到 Database Server 都会自动经过 VPN Gateway
```

**v1.4.0 级联故障处理**：
当跳板机断开时，所有下游连接自动标记为断开，并尝试级联重连。

详见：[NETWORK_TOPOLOGY.md](./NETWORK_TOPOLOGY.md)

### .oxide 文件导出

**功能**: 加密导出连接配置，在设备间同步

#### 导出连接

**步骤**：
1. 右键连接 → Export to .oxide
2. 选择要导出的连接（支持多选）
3. **[v1.4.1 新增]** 查看导出概览：
   - 🔒 密码认证连接数
   - 🔑 密钥认证连接数
   - 🤖 SSH Agent 认证连接数
   - 📦 可选：勾选 **"Embed Private Keys"**（内嵌私钥文件）
4. 设置强密码（至少 12 字符，包含大小写/数字/特殊字符）
5. 保存 `.oxide` 文件

**[v1.4.1] 私钥内嵌功能**：

勾选 "Embed Private Keys" 选项后：
- ✅ **完全可移植**：无需手动复制 `~/.ssh/` 目录
- ✅ **跨设备迁移**：从 macOS 导出，在 Windows 导入时自动处理路径
- ✅ **智能检测**：自动检测缺失的密钥文件并警告
- ✅ **大小预览**：实时显示内嵌密钥后文件增加的大小
- ⚠️ **注意**：文件会包含私钥原始内容，但全程 ChaCha20 加密保护

**导出进度阶段（v1.4.1）**：
1. 🔍 读取密钥文件...（如启用 embed_keys）
2. 🔐 Argon2id 加密中...
3. 💾 写入文件...
4. ✅ 完成！

#### 导入连接

**步骤**：
1. File → Import .oxide File
2. 选择 `.oxide` 文件
3. 输入密码
4. **[v1.4.1]** 预览导入内容：
   - 显示所有连接列表
   - 标记重名冲突（将自动重命名）
   - 显示内嵌密钥信息
5. 确认导入

**导入行为**：
- 重名连接自动追加后缀（如 `Server (2)`）
- 内嵌的私钥提取到 `~/.ssh/imported/`，权限自动设为 `600`
- 密码和私钥口令安全存储到系统钥匙串

**安全性**：
- ✅ ChaCha20-Poly1305 AEAD 加密
- ✅ Argon2id 密钥派生（256MB 内存，4 次迭代）
- ✅ SHA-256 完整性校验
- ✅ 支持云存储同步（Dropbox, Google Drive）

详见：[SERIALIZATION.md](./SERIALIZATION.md)

---

## ⌨️ 快捷键速查

### 全局快捷键

| 操作 | Windows/Linux | macOS |
|------|---------------|-------|
| **新建本地终端** | `Ctrl+T` | `⌘T` |
| **打开 Shell 选择器** | `Ctrl+Shift+T` | `⌘+Shift+T` |
| **关闭窗口** | `Alt+F4` | `⌘+Q` |

### 终端快捷键

| 操作 | Windows/Linux | macOS |
|------|---------------|-------|
| **复制** | `Ctrl+Shift+C` | `⌘+C` |
| **粘贴** | `Ctrl+Shift+V` | `⌘+V` |
| **搜索** | `Ctrl+Shift+F` (Win) / `Ctrl+F` (Lin) | `⌘+F` |
| **清屏** | `Ctrl+L` | `⌘+K` |
| **AI 助手** | `Ctrl+Shift+I` (Win) / `Ctrl+I` (Lin) | `⌘+I` |

### 标签管理

目前标签页仅支持点击切换。快捷键支持正在开发中。

### SFTP 快捷键

| 操作 | Windows/Linux | macOS |
|------|---------------|-------|
| **上传** | `ArrowRight` | `ArrowRight` |
| **下载** | `ArrowLeft` | `ArrowLeft` |
| **删除** | `Delete` / `Backspace` | `Delete` / `Backspace` |
| **重命名** | `F2` | `F2` |
| **全选** | `Ctrl+A` | `⌘+A` |

---

## 💡 最佳实践

### 1. 组织连接

**按环境分组**：
```
📁 Production
📁 Staging
📁 Development
📁 Personal
```

**使用有意义的名称**：
- ✅ `Prod-Web-Server-01`
- ❌ `server1`

### 2. 安全管理

**使用密钥认证**：
- 生成专用密钥对：`ssh-keygen -t ed25519 -C "oxideterm"`
- 设置密钥加密（Passphrase）
- 避免使用密码认证

**定期轮换密钥**：
- 每 6-12 个月更新密钥
- 使用不同密钥访问不同环境

**保护 .oxide 文件**：
- 使用强密码（推荐 20+ 字符）
- 存储在加密云盘（iCloud, OneDrive Vault）
- 定期备份

### 3. 性能优化

**减少连接数**：
- 利用连接池复用
- 同一服务器的多个终端共享连接

**使用本地终端**：
- 本地开发优先使用本地终端
- 避免不必要的 SSH 连接

**清理空闲连接**：
- 关闭不用的终端
- 设置合理的空闲超时（默认 30 分钟）

### 4. 工作流优化

**使用会话树**：
- 按项目组织连接
- 折叠不常用的分组

**快捷键组合**：
- `Ctrl+Shift+I` → AI 快速帮助
- `Ctrl+F` → 搜索日志

**利用 AI 助手**：
- 错误诊断：选中错误 → `Ctrl+I`
- 命令生成：`Ctrl+I` → 描述需求
- 日志分析：自动捕获上下文

---

## 🐛 故障排查

### 连接问题

#### 无法连接到服务器

**可能原因**：
- 网络不可达
- 防火墙阻止
- SSH 服务未运行
- 认证失败

**解决步骤**：
1. 检查网络：`ping example.com`
2. 检查端口：`telnet example.com 22`
3. 验证认证：检查密码/密钥
4. 查看日志：开发者工具 → Console

#### 连接断开频繁

**可能原因**：
- 网络不稳定
- NAT 超时
- 服务器超时设置

**解决方案**：
1. 启用 keep-alive（默认已启用）
2. 检查网络质量
3. 联系服务器管理员调整超时

### v1.4.0 特有问题

#### SFTP 显示 "Waiting for connection"

**原因**: State Gating 机制检测到连接未就绪

**解决方案**：
1. 等待自动重连完成
2. 检查会话树中的连接状态
3. 如果长时间卡住，尝试手动重连

#### 重连后 SFTP 路径丢失

**原因**: Path Memory 未正常工作

**解决方案**：
1. 确保使用最新版本
2. 检查浏览器控制台是否有错误
3. 尝试手动导航

### 性能问题

#### 终端输入延迟高

**可能原因**：
- 网络延迟
- 服务器负载高
- WebGL 渲染问题

**解决方案**：
1. 检查网络延迟：`ping example.com`
2. 切换渲染器：设置 → Terminal → Renderer → Canvas
3. 减小字体大小

#### SFTP 传输速度慢

**可能原因**：
- 网络带宽限制
- 大量小文件
- 压缩开销

**解决方案**：
1. 使用有线网络
2. 压缩后传输大量小文件
3. 分批传输

### 显示问题

#### 颜色显示异常

**原因**: 主题不兼容

**解决方案**：
1. 切换主题：设置 → Terminal → Theme
2. 检查 `$TERM` 环境变量
3. 设置 `TERM=xterm-256color`

---

## 📚 相关文档

- [ARCHITECTURE.md](./ARCHITECTURE.md) - 系统架构设计 (v1.4.0 Strong Sync)
- [CONNECTION_POOL.md](./CONNECTION_POOL.md) - 连接池与自动重连
- [NETWORK_TOPOLOGY.md](./NETWORK_TOPOLOGY.md) - 拓扑路由与 ProxyJump
- [TERMINAL_SEARCH.md](./TERMINAL_SEARCH.md) - 终端搜索功能
- [LOCAL_TERMINAL.md](./LOCAL_TERMINAL.md) - 本地终端指南
- [AI_INLINE_CHAT.md](./AI_INLINE_CHAT.md) - AI 助手使用
- [PORT_FORWARDING.md](./PORT_FORWARDING.md) - 端口转发配置
- [SFTP.md](./SFTP.md) - SFTP 文件传输
- [SERIALIZATION.md](./SERIALIZATION.md) - .oxide 文件格式
- [SYSTEM_INVARIANTS.md](./SYSTEM_INVARIANTS.md) - 系统不变量

---

## 🙋 获取帮助

### 社区支持

- **GitHub Issues**: https://github.com/AnalyseDeCircuit/oxideterm/issues
- **Discussions**: https://github.com/AnalyseDeCircuit/oxideterm/discussions

### 贡献

欢迎贡献代码、文档或报告问题：
- Fork 项目并提交 PR
- 报告 Bug 或功能请求

---

*文档版本: v1.4.0 (Strong Sync + Key-Driven Reset) | 最后更新: 2026-02-04*
