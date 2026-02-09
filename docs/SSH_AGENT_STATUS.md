# SSH Agent 认证功能状态

## 实现概览

本文档记录 SSH Agent 认证功能的当前状态和未来计划。

## ✅ 已完成部分

### 1. 完整的类型系统支持

**后端 (Rust)**：
- ✅ `AuthMethod::Agent` 枚举变体（`src-tauri/src/ssh/config.rs`）
- ✅ `SavedAuth::Agent` 持久化支持（`src-tauri/src/config/types.rs`）
- ✅ `EncryptedAuth::Agent` .oxide 文件支持（`src-tauri/src/oxide_file/format.rs`）
- ✅ 导入导出逻辑完整处理 Agent 类型

**前端 (TypeScript)**：
- ✅ `ConnectRequest.auth_type` 包含 `'agent'`
- ✅ `ConnectionInfo.auth_type` 包含 `'agent'`
- ✅ `ProxyHopConfig.auth_type` 包含 `'agent'`
- ✅ `SaveConnectionRequest.auth_type` 包含 `'agent'`

### 2. 完整的 UI 支持

**三个对话框已更新**：
- ✅ `NewConnectionModal.tsx` - 新建连接支持 Agent 选项
- ✅ `EditConnectionModal.tsx` - 编辑连接支持 Agent 选项
- ✅ `AddJumpServerDialog.tsx` - 跳板机支持 Agent 选项

**UI 特性**：
- ✅ Agent 选项卡/单选按钮
- ✅ 友好的提示信息（中文）
- ✅ 一致的用户体验

### 3. 持久化与导入导出

- ✅ Agent 配置可以保存到本地数据库
- ✅ Agent 配置可以导出到 .oxide 文件
- ✅ .oxide 文件中的 Agent 配置可以导入
- ✅ 不需要 keychain 存储（Agent 本身不存储密码）

### 4. 跨平台检测

- ✅ Unix/Linux/macOS: 检测 `SSH_AUTH_SOCK` 环境变量
- ✅ Windows: 支持 `\\.\pipe\openssh-ssh-agent` 命名管道
- ✅ `is_agent_available()` 函数提供平台检测

### 5. 错误处理

- ✅ 清晰的错误信息
- ✅ 平台特定的帮助提示
- ✅ 建议用户使用密钥文件替代方案

## ✅ 核心认证流程（已完成）

### 实现方案

**位置**: `src-tauri/src/ssh/agent.rs`

**技术方案**: 通过 `AgentSigner` 包装器实现 russh `Signer` trait，绕过 `PrivateKey` 限制。

```rust
// AgentSigner 实现了 russh::keys::key::Signer trait
// 使用 authenticate_publickey_with() 而非 authenticate_publickey()
// AgentSigner 内部通过 Agent IPC 完成挑战-响应签名
```

**认证流程**:
1. `SshAgentClient::connect()` — 连接系统 SSH Agent (Unix socket / Windows named pipe)
2. `SshAgentClient::authenticate()` — 获取公钥列表，逐一尝试认证
3. 对每个公钥，使用 `handle.authenticate_publickey_with(user, AgentSigner)` 完成挑战签名
4. `AgentSigner` 内部调用 `agent.sign_request()` 完成签名

**关键集成点**:
- 直连: `connection_registry.rs` — `AuthMethod::Agent` 分支
- 跳板机: `proxy.rs` + `client.rs` — 代理链中每一跳均支持 Agent
- 重连: 使用相同 `AuthMethod`，重连时重新走 Agent 认证

## ✅ 验收标准（全部通过）

- [x] 可以在 UI 中选择 SSH Agent 认证
- [x] Agent 配置可以保存和加载
- [x] Agent 配置可以导出到 .oxide 文件
- [x] .oxide 文件中的 Agent 配置可以导入
- [x] **实际使用 SSH Agent 连接服务器**
- [x] **跳板机支持 Agent 认证**
- [x] **Agent 连接可以正常重连**（重连复用 AuthMethod::Agent）
- [x] Agent 不可用时显示清晰错误信息
- [x] 三大平台（Windows/macOS/Linux）的 Agent 检测
- [x] 前端 `isAgentAvailable()` API + UI 可用性指示器

## 🔄 未来计划

- [ ] Agent 转发 (Agent Forwarding) 功能
- [ ] 跨平台集成测试扩展
- [ ] Windows Named Pipe 连通性预检测（当前直接返回 `true`）

## 📚 参考资料

- [RFC 4251 - SSH Protocol Architecture](https://tools.ietf.org/html/rfc4251)
- [RFC 4252 - SSH Authentication Protocol](https://tools.ietf.org/html/rfc4252)
- [SSH Agent Protocol (PROTOCOL.agent)](https://github.com/openssh/openssh-portable/blob/master/PROTOCOL.agent)
- [russh Documentation](https://docs.rs/russh/)
- [russh-keys Documentation](https://docs.rs/russh-keys/)

## 📝 开发者注意事项

如果你想参与 Agent 认证的完整实现，请查看：

1. **核心文件**: `src-tauri/src/ssh/agent.rs` — `SshAgentClient` + `AgentSigner` 实现
2. **集成文件**: `src-tauri/src/ssh/connection_registry.rs` — `AuthMethod::Agent` 分支
3. **跳板机**: `src-tauri/src/ssh/proxy.rs` + `src-tauri/src/ssh/client.rs`
4. **前端检测**: `src-tauri/src/commands/connect_v2.rs` — `is_ssh_agent_available` 命令

## 更新日志

- **2026-01-14**: 完成类型系统、UI、持久化和导入导出支持
- **2026-02-07**: 完成核心认证流程（AgentSigner + authenticate_publickey_with），跳板机支持，前端可用性指示器
