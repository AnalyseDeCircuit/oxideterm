# OxideTerm 使用指南

OxideTerm Native 是基于 Rust/GPUI 的 SSH 工作区，包含终端、SFTP、端口转发、主机工具、图形/VNC 会话、设置、云同步、插件、AI 辅助和独立 CLI 伴侣工具。桌面应用是产品主体；CLI 用于自动化和诊断。

## 文档

- [快速开始](./getting-started.md)：首次启动、本地终端检查、保存连接设置和配置路径。
- [应用指南](./app.md)：应用布局、标签页、会话、SFTP、IDE、转发、主机工具、图形/VNC、AI、Agent Skills、高级命令发送器、设置、插件和云同步。
- [架构](./architecture.md)：桌面应用组织方式，包括节点、终端、主机工具、modem 传输、图形/VNC、SFTP、IDE、AI、插件、同步、安全和 CLI 边界。
- [桌面工作流](./desktop.md)：混合页面分栏、粘贴前编辑、终端文件传输、审计与会话录制、主机工具和桌面导航。
- [CLI 伴侣工具](./cli.md)：诊断、设置、连接、备份、云同步和自动化的常用命令。
- [连接与端口转发](./connections-and-forwards.md)：保存 SSH 配置、Kerberos、动态验证码、Mosh、连接导入、串口和端口转发。
- [云同步与备份](./cloud-sync-and-backups.md)：首次配置、第二台设备接入、冲突、中断恢复、备份和恢复计划。
- [便携 `.oxide` 包](./portable-oxide.md)：加密导入导出、便携文件夹设置、凭据迁移和数据目录备份。
- [插件与凭据](./plugins-and-secrets.md)：插件管理器、插件设置和不泄露凭据的自动化流程。
- [Native 插件开发](./plugin-development.md)：面向 Native 应用的清单、进程和 WASM 插件开发。
- [Agent Skills 参考](../../agent-skills.md)：发现、渐进加载、资源限制和 OxideSens 安全边界。
- [排障](./troubleshooting.md)：应用内检查、终端辅助、连接恢复、主机工具、图形/VNC、同步恢复、诊断和问题报告。
