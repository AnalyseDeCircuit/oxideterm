# 快速开始

## 安装形态

Native 包会包含桌面应用、图标、远端 agent 二进制和独立 `oxideterm` CLI 伴侣工具。桌面应用是主要入口。

macOS 打包产物包含 `.dmg`、`.app.zip` 和便携压缩包。Linux 与 Windows 便携包会使用各自平台合适的压缩格式。

## 首次启动

先打开 OxideTerm 桌面应用。主窗口是带标签页的 SSH 工作区，左侧活动栏用于进入会话、文件、端口转发、插件、云同步、通知和设置。

建议从本地终端标签页开始：

1. 创建一个本地终端。
2. 运行简单命令，例如 `pwd` 或 `echo ok`。
3. 打开设置，确认终端字体、主题、shell 和键盘行为。
4. 添加一个保存的 SSH 连接。
5. 连接主机，并确认终端正常打开。

之后再试用你最常用的应用页面：SFTP/文件管理器、连接监控、端口转发、IDE 工作区、AI 侧边栏和插件管理器。

## 检查应用页面

本地终端和第一个 SSH 连接可用后，再检查你预计会常用的应用页面：

- 会话：保存连接和活跃 SSH 节点。
- 连接监控：连接健康、已失效节点和重连状态。
- 文件管理器或 SFTP：远端目录浏览和传输。
- IDE 工作区：远端项目目录和编辑器标签页。
- AI 侧边栏：当前工作区上下文和工具审批。
- 插件：已安装插件和插件设置。
- 云同步：同步状态和备份状态。

## CLI 伴侣工具诊断

只有在需要只读诊断路径和健康状态时，才使用 CLI 伴侣工具：

```sh
oxideterm paths
oxideterm doctor --strict
oxideterm report --json
```

开发环境可以通过 Cargo 运行同样的命令：

```sh
cargo run -p oxideterm-cli -- paths
cargo run -p oxideterm-cli -- doctor --strict
```

## 配置目录

桌面应用和 CLI 读取同一套配置文件。日常交互式修改优先使用桌面设置页面。需要诊断或脚本化时，可以用 `paths` 查看当前生效路径：

```sh
oxideterm paths --json
```

脚本、CI 或迁移场景可以让 CLI 指定另一个配置根目录：

```sh
oxideterm --config-dir ./fixtures/profile-a paths
OXIDETERM_CONFIG_DIR=./fixtures/profile-a oxideterm doctor --strict
```

如果一个配置根目录下需要多个隔离配置档，可以使用命名配置档：

```sh
oxideterm --config-dir ./fixtures --profile staging paths
```

配置档数据会存放在所选配置目录的 `profiles/<name>` 下面。

## 安全写入流程

日常使用时，通过设置、连接管理器、插件管理器或云同步页面完成普通配置变更。脚本化 CLI 写入时，先查看计划，再在确认符合预期后重复执行并加上 `--yes`：

```sh
oxideterm settings set terminal.fontSize 14 --dry-run --json
oxideterm settings set terminal.fontSize 14 --yes
```

批量导入、云同步应用、`.oxide` 导入等高风险操作前，应该先创建备份，并用恢复预演检查回滚路径。
