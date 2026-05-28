# 排障

## 先跑只读检查

修改文件前先运行：

```sh
oxideterm paths --json
oxideterm diagnose --json
oxideterm doctor --strict
oxideterm report --json
```

`doctor --strict` 会把 warning 当作失败，适合 CI 或迁移脚本。

## 常见恢复步骤

如果设置加载失败：

```sh
oxideterm settings validate --strict
oxideterm settings show --json
```

如果连接状态不对：

```sh
oxideterm connections validate --strict
oxideterm connections export --format raw-safe --json
```

如果云同步行为异常：

```sh
oxideterm cloud-sync status --json
oxideterm cloud-sync history --failed-only
oxideterm cloud-sync diff --dirty-only --format table
```

## 先备份

在覆盖文件修复状态前，先创建备份：

```sh
oxideterm backup create --output ./before-fix.json --json
```

然后用 dry-run 先检查针对性修复。

## Shell Completion

生成或安装 completion：

```sh
oxideterm completion zsh > ~/.zfunc/_oxideterm
oxideterm completion path zsh
oxideterm completion install zsh
```

只有确认要覆盖已有生成文件时，才加 `--force`。

## Bug Report

提交 issue 时优先附加脱敏 bundle，而不是原始配置文件：

```sh
oxideterm report --bundle ./oxideterm-report.json --json
```

分享前先检查 bundle。必要时移除私人主机名、用户名、路径或项目名。
