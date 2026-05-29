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
oxideterm connections create --name prod --host example.internal --user deploy --port 22 --dry-run
oxideterm connections rename prod production --yes
oxideterm connections validate --strict
oxideterm connections export --format raw-safe --json
```

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
