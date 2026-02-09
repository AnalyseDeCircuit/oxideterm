# OxideTerm 序列化架构 (v1.4.0)

> 本文档描述了 OxideTerm 的数据序列化策略、技术选型，以及与 **Strong Consistency Sync** 架构的集成方式。

## 概述

OxideTerm 使用两种序列化格式：

| 格式 | 库 | 用途 |
|------|-----|------|
| **MessagePack** | `rmp-serde` | 二进制持久化（redb 嵌入式数据库、.oxide 加密负载、滚动缓冲区） |
| **JSON** | `serde_json` | 人类可读配置（~/.oxideterm/connections.json、.oxide 明文元数据） |

## 序列化架构图

```
┌─────────────────────────────────────────────────────────────┐
│                    OxideTerm 序列化架构                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              MessagePack (rmp-serde)                 │   │
│  │                                                      │   │
│  │  应用场景:                                           │   │
│  │  • redb 嵌入式数据库 (会话恢复、端口转发规则)        │   │
│  │  • SFTP 传输进度持久化                               │   │
│  │  • .oxide 文件加密负载 (仅配置数据)                  │   │
│  │  • Terminal scroll_buffer 序列化 (100,000 行)       │   │
│  │                                                      │   │
│  │  特性支持:                                           │   │
│  │  ✓ 二进制紧凑格式 (高效存储)                         │   │
│  │  ✓ #[serde(tag = "type")] 内部标签枚举              │   │
│  │  ✓ chrono::DateTime<Utc> 原生支持                   │   │
│  │  ✓ Option<T> / Vec<T> 完全兼容                      │   │
│  │  ✓ 跨语言兼容 (未来可支持其他语言客户端)             │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                  JSON (serde_json)                   │   │
│  │                                                      │   │
│  │  应用场景:                                           │   │
│  │  • ~/.oxideterm/connections.json (用户配置)         │   │
│  │  • .oxide 文件 metadata 段 (明文可读)                │   │
│  │                                                      │   │
│  │  选择原因:                                           │   │
│  │  ✓ 人类可编辑 (调试友好)                             │   │
│  │  ✓ 无需解密即可查看 .oxide 文件信息                 │   │
│  │  ✓ 版本控制友好 (Git diff 可读)                     │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## v1.4.0 架构集成：Strong Sync 与序列化

在 v1.4.0 的 **Strong Consistency Sync** 架构下，序列化层与前端状态管理紧密协作。

### 数据流向

```mermaid
flowchart LR
    subgraph Backend ["后端 (Rust)"]
        DB[(redb 数据库)]
        MP[MessagePack 编解码]
        REG[ConnectionRegistry]
    end

    subgraph Frontend ["前端 (React)"]
        APP[AppStore (Fact)]
        TREE[SessionTreeStore (Logic)]
    end

    DB <-->|序列化/反序列化| MP
    MP <--> REG
    REG -->|connection:update 事件| APP
    APP -->|refreshConnections()| REG
    TREE -->|Intent| REG
```

### 关键约束

| 操作 | 序列化格式 | Strong Sync 行为 |
|------|-----------|------------------|
| 保存连接配置 | JSON | 触发 `refreshConnections()` |
| 会话恢复 | MessagePack | 恢复后触发 `connection:update` |
| 端口转发规则持久化 | MessagePack | 重连后自动恢复，触发同步 |
| 路径记忆 (SFTP) | 内存 Map | Key-Driven 重建时从 Map 恢复 |

---

## MessagePack 序列化组件

### 1. `src/state/session.rs` - 会话恢复持久化

**用途**: 应用重启后恢复会话（不是"导出"功能）

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSession {
    pub id: String,
    pub config: SessionConfig,        // 包含 AuthMethod (tag枚举)
    pub created_at: DateTime<Utc>,
    pub order: usize,
    pub version: u32,
    pub terminal_buffer: Option<Vec<u8>>,  // 可选的终端缓冲区
    pub buffer_config: BufferConfig,
}
```

**存储位置**: redb 嵌入式数据库 (`~/.oxideterm/state.redb`)  
**特殊类型**: `AuthMethod`(内部标签枚举), `DateTime<Utc>`, `Option<Vec<u8>>`

**重要说明**:  
- **会话恢复** ≠ **导出功能**
- `PersistedSession` 仅在本地使用，用于应用重启后恢复会话树
- 不会被导出到 `.oxide` 文件（`.oxide` 只导出连接配置）
- **v1.4.0**: 恢复后必须触发 `connection:update` 事件，确保前端 Store 同步

---

### 2. `src/state/forwarding.rs` - 端口转发规则存储

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedForward {
    pub id: String,
    pub session_id: String,
    pub forward_type: ForwardType,   // Local/Remote/Dynamic
    pub rule: ForwardRule,
    pub created_at: DateTime<Utc>,
    pub auto_start: bool,
    pub version: u32,
}
```

**存储位置**: redb 嵌入式数据库  
**特殊类型**: `ForwardType`(枚举), `DateTime<Utc>`

**v1.4.0 Link Resilience**: 当连接重连成功后，后端自动从 redb 恢复 `auto_start=true` 的转发规则。

---

### 3. `src/session/scroll_buffer.rs` - 终端滚动缓冲区

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedBuffer {
    pub lines: Vec<TerminalLine>,     // 最多 100,000 行
    pub total_lines: u64,
    pub captured_at: DateTime<Utc>,
    pub max_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalLine {
    pub text: String,                  // ANSI codes stripped
    pub timestamp: u64,                // Unix milliseconds
}
```

**用途**: 会话恢复时的终端历史  
**特殊类型**: `Vec<TerminalLine>`, `DateTime<Utc>`

**序列化方式**:
```rust
// Save to bytes
let bytes: Vec<u8> = buffer.save_to_bytes().await?;

// Load from bytes
let buffer = ScrollBuffer::load_from_bytes(&bytes).await?;
```

---

### 4. `src/sftp/progress.rs` - 传输进度存储

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTransferProgress {
    pub transfer_id: String,
    pub transfer_type: TransferType,
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub status: TransferStatus,
    pub last_updated: DateTime<Utc>,
    pub session_id: String,
    pub error: Option<String>,
}
```

**存储位置**: redb 数据库  
**特殊类型**: `DateTime<Utc>`, `PathBuf`, `Option<String>`

---

### 5. `src/oxide_file/crypto.rs` - .oxide 加密负载

**重要**: `.oxide` 文件是**纯配置导出格式**，不包含：
- ❌ 会话数据（`PersistedSession`）
- ❌ 终端缓冲区（`SerializedBuffer`）
- ❌ 端口转发规则（`PersistedForward`）

包含内容：
- ✅ 连接配置（host, port, username, auth）
- ✅ ProxyJump 跳板机链路
- ✅ 连接选项（ConnectionOptions）
- ✅ **[v1.4.1+]** 可选的私钥文件内嵌（embed_keys 选项）

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptedPayload {
    pub version: u32,
    pub connections: Vec<EncryptedConnection>,  // 仅配置
    pub checksum: String,  // SHA-256 完整性校验
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedConnection {
    pub name: String,
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: EncryptedAuth,
    pub color: Option<String>,
    pub tags: Vec<String>,
    pub options: ConnectionOptions,
    pub proxy_chain: Vec<EncryptedProxyHop>,  // 跳板机链路
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EncryptedAuth {
    Password { password: String },
    Key { 
        key_path: String, 
        passphrase: Option<String>,
        embedded_key: Option<String>,  // v1.4.1+ base64 编码的内嵌私钥
    },
    Certificate { 
        key_path: String, 
        cert_path: String, 
        passphrase: Option<String>,
        embedded_key: Option<String>,  // v1.4.1+ base64 编码的内嵌私钥
    },
    Agent,
}

// v1.4.1+: embedded_key 为 Option<String>，存储 base64 编码的私钥内容
// 导入时解码并写入 ~/.ssh/imported/ 目录
```

**设计决策**:  
- ✅ `.oxide` = 配置迁移工具（设备间同步）
- ❌ 不是会话备份工具（不包含运行时状态）
- ✅ 密码直接内联在加密负载中（无需系统钥匙串）
- ✅ **[v1.4.1+]** 支持私钥内嵌，实现真正的可移植备份

**v1.4.1 新增功能：私钥内嵌（embed_keys）**

导出时可选择将私钥文件内容嵌入 .oxide 文件，优势：

- ✅ **完全可移植**：无需手动复制 `~/.ssh/` 目录
- ✅ **设备间迁移**：从 macOS 导出，在 Windows 导入，自动处理路径差异
- ✅ **备份完整性**：单一 .oxide 文件包含所有认证凭据
- ⚠️ **安全性**：文件大小会增加（每个密钥约 1-4KB），但全程加密保护

**导入行为**：
- 内嵌密钥会被提取到 `~/.ssh/imported/` 目录
- 文件权限自动设置为 `600`（仅所有者可读写）
- 路径会更新为新的导入位置
- 原始路径信息保留在元数据中

**Pre-flight 检查（v1.4.1+）**

导出前端新增智能体检功能，自动分析选中连接：

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportPreflightResult {
    pub total_connections: usize,
    pub connections_with_passwords: usize,
    pub connections_with_keys: usize,
    pub connections_with_agent: usize,
    pub missing_keys: Vec<(String, String)>,  // (connection_name, key_path)
    pub total_key_bytes: u64,
    pub can_export: bool,
}
```

**前端 UI 增强**：
- 📊 **导出概览面板**：显示密码/密钥/Agent 认证分布
- ⚠️ **缺失密钥警告**：实时检测无法访问的密钥文件
- 📦 **密钥大小预览**：显示内嵌后文件增加的大小
- 🔄 **进度阶段显示**：读取密钥 → 加密 → 写入，清晰反馈

---

## JSON 序列化组件

### 1. `src/config/storage.rs` - 用户配置文件

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub version: u32,
    pub connections: Vec<SavedConnection>,
    pub groups: Vec<String>,                  // 连接分组
}
```

**文件路径**: `~/.oxideterm/connections.json` (macOS/Linux) 或 `%APPDATA%\OxideTerm\connections.json` (Windows)

**保持 JSON 原因**:  
- 用户可能需要手动编辑配置
- 调试友好，出问题时可直接查看文件内容
- 版本控制友好（Git diff 可读）

**重要**: 密码不存储在此文件中，仅保存 `keychain_id` 引用！

```rust
// 示例：密码通过 keychain_id 引用
pub enum SavedAuth {
    Password {
        keychain_id: String,  // 例如: "oxideterm-a1b2c3d4-e5f6-..."
    },
    Key {
        key_path: String,
        has_passphrase: bool,
        passphrase_keychain_id: Option<String>,  // 也是引用
    },
    // ...
}
```

**v1.4.0 Strong Sync**: 任何对 `connections.json` 的写入操作完成后，后端必须 emit `connection:update` 事件，触发前端 `AppStore.refreshConnections()`。

---

### 2. `src/oxide_file/format.rs` - .oxide 文件元数据

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OxideMetadata {
    pub exported_at: DateTime<Utc>,
    pub exported_by: String,           // "OxideTerm v1.4.0"
    pub description: Option<String>,
    pub num_connections: usize,
    pub connection_names: Vec<String>,
}
```

**用途**: .oxide 文件的**明文头部**（不加密）  
**保持 JSON 原因**: 允许用户在不解密的情况下查看文件信息

**文件结构**:
```
.oxide File Layout:
┌─────────────────────────┐
│  Header (21 bytes)       │  ← Binary: Magic + Version + Lengths
├─────────────────────────┤
│  Salt (32 bytes)         │  ← Argon2id 盐值
├─────────────────────────┤
│  Nonce (12 bytes)        │  ← ChaCha20 nonce
├─────────────────────────┤
│  Metadata (JSON)         │  ← **明文 JSON**，查看文件信息
├─────────────────────────┤
│  Encrypted Data          │  ← **MessagePack 序列化** 后加密的连接配置
├─────────────────────────┤
│  Auth Tag (16 bytes)     │  ← ChaCha20-Poly1305 认证标签
└─────────────────────────┘
```

---

## 带标签的枚举类型

以下枚举使用 `#[serde(tag = "type")]` 内部标签格式，MessagePack 完全支持：

| 枚举 | 位置 | 变体 | 用途 |
|------|------|------|------|
| `AuthMethod` | `session/types.rs` | Password, KeyFile, Agent, Certificate, KeyboardInteractive | 会话运行时认证 |
| `EncryptedAuth` | `oxide_file/format.rs` | password, key, certificate, agent | .oxide 导出格式 |
| `SavedAuth` | `config/types.rs` | Password, Key, Certificate, Agent | 本地配置中的认证（keychain引用） |
| `ForwardType` | `forwarding/mod.rs` | Local, Remote, Dynamic | 端口转发类型 |
| `ConnectionState` | `state/types.rs` | Connecting, Active, Idle, LinkDown, Reconnecting, Disconnecting, Disconnected, Error(String) | **v1.4.0 新增**: 连接生命周期状态 |

**示例**: MessagePack 序列化的内部标签格式

```rust
// Rust 定义
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EncryptedAuth {
    Password { password: String },
    Key { key_path: String, passphrase: Option<String> },
}

// MessagePack 序列化后的逻辑结构 (Map):
{
  "type": "password",
  "password": "secret123"
}

{
  "type": "key",
  "key_path": "/home/user/.ssh/id_rsa",
  "passphrase": null
}
```

---

## 技术选型理由

### 为什么选择 MessagePack (rmp-serde)？

| 对比项 | bincode (废弃) | postcard | rmp-serde |
|--------|---------------|----------|-----------|
| 维护状态 | ⚠️ RUSTSEC-2025-0141 | ✅ 活跃 | ✅ 活跃 |
| `#[serde(tag)]` | ✅ 支持 | ❌ 不支持 | ✅ 支持 |
| `DateTime<Utc>` | ✅ 支持 | ❌ 需转换 | ✅ 支持 |
| `Option<T>` | ✅ 支持 | ⚠️ 受限 | ✅ 支持 |
| 序列化大小 | 中等 | 最小 | 中等 |
| 跨语言兼容 | ❌ Rust only | ❌ Rust only | ✅ 多语言 |

**关键决策因素**:

1. **安全性**: bincode 存在已知安全漏洞 (RUSTSEC-2025-0141)，项目已废弃
2. **功能完整性**: postcard 不支持内部标签枚举，需要重构大量认证相关代码
3. **生态兼容**: rmp-serde 与 serde 生态完全兼容，零摩擦迁移
4. **跨语言潜力**: MessagePack 是通用格式，未来可支持其他语言客户端（例如：Python 脚本导入 .oxide 文件）

---

### 为什么配置文件保持 JSON？

1. **可编辑性**: 用户可能需要手动修改配置（例如：批量修改端口号）
2. **可调试性**: 出问题时可以直接查看文件内容
3. **版本控制友好**: Git diff 友好，便于跟踪配置变化
4. **人类可读**: `connections.json` 可以作为配置备份参考

**示例**: `connections.json` 文件片段

```json
{
  "version": 1,
  "connections": [
    {
      "id": "conn-123",
      "name": "Production Server",
      "host": "prod.example.com",
      "port": 22,
      "username": "admin",
      "auth": {
        "type": "password",
        "keychain_id": "oxideterm-a1b2c3d4-e5f6-7890-abcd"
      },
      "group": "Production",
      "options": {
        "jump_host": null,
        "local_forward": [],
        "remote_forward": [],
        "dynamic_forward": null
      }
    }
  ],
  "groups": ["Production", "Staging", "Development"]
}
```

---

## API 参考

### 序列化

```rust
// MessagePack (使用命名字段格式，支持默认值和可选字段)
let bytes: Vec<u8> = rmp_serde::to_vec_named(&data)?;

// JSON (人类可读配置)
let json: String = serde_json::to_string_pretty(&data)?;
```

### 反序列化

```rust
// MessagePack
let data: T = rmp_serde::from_slice(&bytes)?;

// JSON  
let data: T = serde_json::from_str(&json)?;
```

### 错误处理

```rust
// MessagePack 编码错误
rmp_serde::encode::Error

// MessagePack 解码错误
rmp_serde::decode::Error

// JSON 错误
serde_json::Error
```

> **注意**: 使用 `to_vec_named` 而非 `to_vec` 是为了支持带有 `#[serde(default)]` 或 `Option<T>` 字段的结构体。
> 命名字段格式确保反序列化时字段匹配基于名称而非位置，提供更好的向后兼容性。

---

## 数据持久化总览

| 数据类型 | 格式 | 存储位置 | 生命周期 | Strong Sync 行为 |
|---------|------|---------|---------|------------------|
| **连接配置** | JSON | `~/.oxideterm/connections.json` | 永久 | 写入后 emit `connection:update` |
| **密码/密钥口令** | 系统钥匙串 | macOS Keychain / Windows Credential / Linux libsecret | 永久 | N/A |
| **会话恢复数据** | MessagePack | `~/.oxideterm/state.redb` | 持久 | 恢复后 emit `connection:update` |
| **端口转发规则** | MessagePack | `~/.oxideterm/state.redb` | 持久 | 重连后自动恢复 (Link Resilience) |
| **终端缓冲区** | MessagePack | 内存 / `state.redb` | 临时 | N/A |
| **.oxide 导出文件** | MessagePack + JSON | 用户指定路径 | 临时 | 导入后触发 `refreshConnections()` |
| **路径记忆 (SFTP)** | 内存 Map | `PathMemoryMap` | 临时 | Key-Driven 重建时恢复 |

---

## 历史变更

| 版本 | 日期 | 变更 |
|------|------|------|
| **v1.4.0** | 2026-02-04 | **Strong Sync 集成**: 所有持久化操作与前端状态同步；新增 `ConnectionState` 枚举；移除对已废弃文档的引用 |
| v1.1.0 | 2026-01-19 | 澄清 `.oxide` 文件不包含会话数据；添加本地终端和滚动缓冲区说明 |
| v0.3.0 | 2026-01-15 | 从 bincode/postcard 迁移到 rmp-serde |
| v0.2.0 | - | 使用 bincode 进行二进制序列化 |
| v0.1.0 | - | 初始版本，全部使用 JSON |

---

## 相关文档

- [ARCHITECTURE.md](./ARCHITECTURE.md) - 整体架构设计 (v1.4.0 Strong Sync)
- [PORT_FORWARDING.md](./PORT_FORWARDING.md) - 端口转发与 Link Resilience
- [SFTP.md](./SFTP.md) - SFTP 传输与路径记忆
- [PROTOCOL.md](./PROTOCOL.md) - 前后端通信协议

---

*文档版本: v1.4.0 | 最后更新: 2026-02-04*
