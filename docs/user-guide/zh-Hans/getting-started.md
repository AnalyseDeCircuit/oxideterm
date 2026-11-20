# 快速开始

## 安装形态

Native 包会包含桌面应用、图标、Linux musl 远程文件/代码 agent 二进制和独立
`oxideterm` CLI 伴侣工具。远程 agent 通过 stdio JSON-RPC 提供结构化文件、搜索、
Git 和符号操作；它不是 SSH authentication agent，也不拥有终端、SFTP 或转发连接。
桌面应用是主要入口。

macOS 打包产物包含 `.dmg`、`.app.zip` 和便携压缩包。Linux 与 Windows 便携包会使用各自平台合适的压缩格式。

在 macOS 上更改登录时启动，请在系统设置的登录项中添加或移除 OxideTerm。

## 首次启动

左侧活动栏用于管理连接和打开全局页面，顶部栏用于筛选标签页，右侧可以打开 OxideSens 和主机工具。详见[工作区导航](workspace.md)。

1. 从[官方发布页](https://github.com/AnalyseDeCircuit/oxideterm/releases)下载匹配系统和处理器的安装包。macOS 打开 DMG，将 OxideTerm 拖入“应用程序”；Windows 运行安装程序；Linux 可选 AppImage 或对应发行版的软件包。如果希望应用和数据一起随身携带，见[便携目录](portable-oxide.md)。
2. 启动 OxideTerm，在欢迎页选择 **本地终端**，输入 `echo ok`，确认本机 Shell 已启动、键盘输入正常。打开 **设置**，检查终端字体、主题、Shell 和快捷键，再连接远程主机。
3. 选择 **新建连接 → SSH**，给配置起一个容易辨认的名字，填写管理员提供的服务器地址、端口和远程用户名。选择服务器接受的认证方式；使用私钥时，选私钥文件而不是对应的公钥。
4. 选择 **保存并连接**。首次连接时，先与管理员通过可信渠道提供的主机密钥指纹核对，再确认。私钥口令用于解锁本机私钥，服务器密码用于登录远程账户；保存连接配置本身不等于同意保存凭据。
5. 在远程终端运行 `hostname`，确认连到了预期主机。关闭终端标签，在 **会话管理器** 中找到刚才保存的配置并重新打开。新的终端可以复用已连接的节点。

要查看同一台服务器的文件，从该会话打开 SFTP 即可，无需另存一份主机配置。如果要停止所有工具共用的连接，才需要断开节点。详见[连接与端口转发](connections-and-forwards.md)。

## 检查应用页面

本地终端和第一个 SSH 连接可用后，再检查你预计会常用的应用页面：

- 会话：保存连接和活跃 SSH 节点。
- 连接监控与主机工具：连接健康、已失效节点、重连状态、主机资源和动作结果。
- 文件管理器：浏览本地目录。要浏览远端文件或传输文件，在已连接的节点上打开 SFTP。
- 终端辅助：右键菜单动作、命令栏动作、背景图片设置和 X/Y/ZMODEM 传输提示。
- 远程桌面：安装并启用 RDP 或 VNC 插件，再从新建连接或会话管理打开配置。Windows 的 WSL 图形是独立的内置功能。
- IDE 工作区：远端项目目录和编辑器标签页。
- AI 侧边栏：当前工作区上下文和工具审批。
- 插件：已安装插件和插件设置。
- 云同步：同步状态和备份状态。

## CLI 伴侣工具诊断

要检查配置路径和应用状态，又不改动设置，可以运行：

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

日常使用时，通过设置、连接管理器、提权凭据页面、插件管理器或云同步页面完成普通配置变更。脚本化 CLI 写入时，先查看计划，再在确认符合预期后重复执行并加上 `--yes`：

```sh
oxideterm settings set terminal.fontSize 14 --dry-run --json
oxideterm settings set terminal.fontSize 14 --yes
```

批量导入、云同步应用、提权凭据变更、`.oxide` 导入等高风险操作前，应该先创建备份，并用恢复预演检查回滚路径。
