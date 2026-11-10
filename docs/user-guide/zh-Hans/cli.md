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
oxideterm connections edit prod --totp-credential "Shared MFA" --dry-run
oxideterm connections edit prod --clear-totp-credential --dry-run
oxideterm connections rename prod production --yes
oxideterm connections validate --strict
oxideterm connections export --format raw-safe --json
```

`connections open` 会按完整名称或 ID 查找已保存的 SSH 连接，再通过原生应用既有的保存连接流程打开。凭据、代理设置和其他连接选项仍由 GUI 负责读取。

`connections create` 和 `connections edit` 支持通过 `--totp-credential` 指定已有验证码凭据的名称或 ID。名称匹配不区分大小写；遇到重名时使用 ID。JSON 规格使用 `totpCredential`，`proxyChain` 中的跳板也可指定；使用 `clearTotpCredential: true` 可移除目标连接的关联。验证码密钥仍保存在受保护的存储中。

密码或密钥口令输入优先使用 `--password-stdin`、`--password-env`、`--passphrase-stdin` 或 `--passphrase-env`。不要把凭据值直接写进 shell 参数。

`connections batch-create` 依据一份 OpenSSH 风格模板和每台主机一行的数据一次创建多条连接。

模板：最多一个 `Host` 块，`HostName` 必须写 `{host}` 或省略。占位符 `{host}`、`{profile}`、`{user}`、`{port}`、`{name}`、`{group}` 会在 `User`、`IdentityFile`、`CertificateFile`、`ProxyCommand` 中替换；`Port` 取字面数字。以 `#` 开头的注释行会被丢弃，其他不受支持的指令会按行号报错。

数据：`[user[:auth]@]host[:port][?name=…&group=…&profile=…&tags=a,b]`，每行一个连接，`#` 开头为注释。每行都需要用户名，来自行内或模板。当该行真正用到的模板值包含 `{profile}`（模板用户名、其 `ProxyCommand`，或该行没有自身凭据时用到的模板身份文件）时，这一行必须提供 `profile=`。冒号后的 `auth` 含 `/`、`\` 或以 `~` 开头时视为私钥路径，其余视为password；`key=` 用于强制指定密钥路径，`pass=` 用于强制指定看起来像路径的password。行内凭据优先于模板 `IdentityFile`；没有行内凭据的行使用模板身份文件；模板没有身份文件时，才由共享的 `--password-env`/`--password-stdin`/`--key-path` 提供认证，模板带 `IdentityFile` 或 `CertificateFile` 时这些参数会被拒绝。空槽（`user:@host`）是错误，一个批次最多 2,000 行。

输入：`--spec <路径>` 指向含 `template`、`data` 与可选 `duplicate` 策略的 JSON 文件，不能与 `--template-file`/`--data-file` 同时使用。`--dry-run` 列出将新增、跳过、拒绝的行而不写入；`--json` 输出 JSON 报告。写入需要 `--yes`，且只要有任何行无效就拒绝写入，批次不会部分落盘。

重复：只有账号与凭据都相同（`user@host:port` 加同一个密钥或密码）的行才算同一条连接，这类重复行始终跳过；显示名已存在且 `user@host:port` 相同时，默认跳过策略下完全一致的行报告为 `batchUnchanged`，有差异的行报告为 `batchUpdate`（新增的 `group=`/`tags=` 落到原连接上）；同名但目标不同时默认跳过，传 `--duplicate rename` 才自动改名。模板包含 `ProxyCommand` 时，需要在应用设置中开启 ProxyCommand 授权。

```sh
oxideterm connections batch-create --template-file ./template.conf --data-file ./hosts.txt --dry-run
oxideterm connections batch-create --template-file ./template.conf --data-file ./hosts.txt --password-env OX_SSH_PW --yes
```

数据文件或 spec 文件中的行内凭据是磁盘上的明文：共享密码优先使用 `--password-stdin`/`--password-env`，并保护或删除携带凭据的文件。

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
oxideterm cloud-sync sync --dry-run --json
oxideterm cloud-sync sync --yes --json
oxideterm cloud-sync secrets status --json
```

运行 CLI 同步前先关闭桌面应用，包括试运行：命令需要独占配置。`sync --dry-run` 从远端后端准备计划，不应用也不发布；`preview` 和 `diff` 查看缓存状态。检查计划后再使用 `--yes`。

当前因果同步会合并独立变更。保留的 `push`、`pull` 和 `apply --from remote` 命令使用同一合并协调器，名称不表示仅上传或仅下载。`pull` 和 `apply` 只接受 merge 策略。逐项选择远端冲突候选应使用桌面预览；`resolve --strategy local-wins` 为冲突选择本地候选，应用前先检查试运行结果。skip、rename、replace 等归档导入策略用于 `.oxide` 导入。

本地应用与远端发布具有独立结果。本地变更应用后若发布失败，应保留配置并重试同步。恢复步骤参见[冲突与中断恢复](cloud-sync-and-backups.md#冲突与中断恢复)。

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
