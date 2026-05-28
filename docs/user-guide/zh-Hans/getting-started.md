# 快速开始

## 安装形态

Native 包会包含桌面应用、独立 `oxideterm` CLI、图标和远端 agent 二进制。CLI 按照与应用相同的 target 编译，并放在应用资源目录中。

macOS 打包产物包含 `.dmg`、`.app.zip` 和 portable 压缩包。Linux 与 Windows portable 包会使用各自平台合适的压缩格式。

## 首次检查

先用 CLI 查看路径并运行只读健康检查：

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

默认情况下，OxideTerm CLI 读取与桌面应用相同的配置文件。使用 `paths` 查看当前生效路径：

```sh
oxideterm paths --json
```

脚本、CI 或迁移场景可以指定另一个配置根目录：

```sh
oxideterm --config-dir ./fixtures/profile-a paths
OXIDETERM_CONFIG_DIR=./fixtures/profile-a oxideterm doctor --strict
```

如果一个配置根目录下需要多个隔离 profile，可以使用命名 profile：

```sh
oxideterm --config-dir ./fixtures --profile staging paths
```

profile 数据会存放在所选配置目录的 `profiles/<name>` 下面。

## 安全写入流程

多数写命令默认是 dry-run。先查看计划，再在确认符合预期后重复执行并加上 `--yes`：

```sh
oxideterm settings set terminal.fontSize 14 --dry-run --json
oxideterm settings set terminal.fontSize 14 --yes
```

批量导入、cloud-sync apply、`.oxide` 导入等高风险操作前，应该先创建备份，并用 restore/dry-run 检查回滚路径。
