# CLI 伴侣工具

`oxideterm` CLI 用于无界面检查、自动化、CI 校验、迁移和恢复。它不应该打印凭据值；涉及凭据的命令只输出提示或状态。

## 全局选项

```sh
oxideterm --config-dir <path> <command>
oxideterm --profile <name> <command>
OXIDETERM_CONFIG_DIR=<path> oxideterm <command>
```

脚本使用 `--json` 或 `--format json`。CI 中如果警告也应该失败，使用 `doctor --strict` 或命令自己的 `--strict`。

多数写命令共享同一组安全选项：

- `--dry-run`：只显示计划，不写入。
- `--yes`：确认真实写入。
- `--json` 或 `--format json`：输出机器可读结果。

## 诊断

```sh
oxideterm paths --json
oxideterm diagnose --json
oxideterm doctor --strict
oxideterm report --json
```

准备问题报告或支持信息时使用 `report --bundle <path>`。分享前应先检查支持包内容。

## 设置

```sh
oxideterm settings validate --strict
oxideterm settings sections --json
oxideterm settings get ai.providers --json
oxideterm settings set terminal.fontSize 14 --dry-run
oxideterm settings export --section appearance --json
oxideterm settings diff ./settings-snapshot.json --section appearance
```

`set` 和 `unset` 只修改已经存在的 JSON path。真实写入需要显式加 `--yes`。

## 连接

```sh
oxideterm connections list
oxideterm connections search prod --json
oxideterm connections open prod
oxideterm connections create --name prod --host example.internal --user deploy --port 22 --dry-run
oxideterm connections rename prod production --yes
oxideterm connections validate --strict
oxideterm connections export --format raw-safe --json
```

`connections open` 会按完整名称或 ID 查找已保存的 SSH 连接，再通过原生应用既有的保存连接流程打开。凭据、代理设置和其他连接选项仍由 GUI 负责读取。

密码或密钥口令输入优先使用 `--password-stdin`、`--password-env`、`--passphrase-stdin` 或 `--passphrase-env`。不要把凭据值直接写进 shell 参数。

## 备份与恢复

```sh
oxideterm backup create --output ./oxideterm-backup.json --json
oxideterm backup inspect ./oxideterm-backup.json --summary
oxideterm backup restore ./oxideterm-backup.json --section settings --dry-run --json
```

恢复命令应先用 `--dry-run` 检查计划，再用 `--yes` 确认真执行。

## 云同步

```sh
oxideterm cloud-sync status --json
oxideterm cloud-sync diff --dirty-only --format table
oxideterm cloud-sync backend webdav configure --endpoint https://example.invalid/sync --dry-run
oxideterm cloud-sync push --dry-run --json
oxideterm cloud-sync pull --dry-run --json
oxideterm cloud-sync apply --from remote --strategy merge --dry-run
oxideterm cloud-sync secrets status --json
```

凭据命令只能输出提示或状态。写入凭据时使用标准输入或环境变量。

## 外部 MCP stdio bridge

外部客户端只支持 stdio MCP 时，先在 OxideTerm 的“设置 → 网络与代理 → 外部 MCP 控制”创建客户端并复制只显示一次的凭据，再把客户端命令配置为：

```sh
OXIDETERM_MCP_TOKEN='<客户端凭据>' oxideterm mcp bridge
```

CC Switch 等接受单个 MCP 服务对象的客户端可以使用：

```json
{
  "command": "oxideterm",
  "args": ["mcp", "bridge"],
  "env": {
    "OXIDETERM_MCP_TOKEN": "<客户端凭据>"
  }
}
```

需要顶层 `mcpServers` 的客户端则把该对象放到 `mcpServers.oxideterm` 下。

bridge 会从当前配置目录发现正在运行的回环端点。使用命名 profile 时，客户端命令也要带相同的 `--profile`；自定义配置目录则带 `--config-dir`。只有自动发现不可用时才设置 `OXIDETERM_MCP_ENDPOINT` 或 `--endpoint`，且它必须是 `http://localhost.../mcp`、`127.0.0.1` 或 `::1`。凭据只能放在外部客户端的秘密环境设置中，不要放进命令参数、日志或项目文件。

## Batch Plans

batch plan 可以把多个变更合并成一次可审查操作：

```sh
oxideterm batch apply ./plan.json --dry-run
oxideterm batch apply ./plan.json --yes --json
```

当设置、连接快照和云同步配置需要一起审查时，使用批处理模式。

## Shell Completion

```sh
oxideterm completion zsh > ~/.zfunc/_oxideterm
oxideterm completion path zsh
oxideterm completion install zsh
```

只有在确定要覆盖已有 completion 文件时才给 `completion install` 加 `--force`。
