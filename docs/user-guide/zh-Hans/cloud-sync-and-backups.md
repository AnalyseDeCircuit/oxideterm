# 云同步与备份

## 云同步状态

云同步用于把选定的本地状态和已配置的远端 backend 对齐。修改前先检查：

```sh
oxideterm cloud-sync status --json
oxideterm cloud-sync preview --json
oxideterm cloud-sync diff --dirty-only --format table
```

能用 backend 专属 configure 命令时，优先使用它：

```sh
oxideterm cloud-sync backend webdav configure \
  --endpoint https://example.invalid/sync \
  --namespace personal \
  --dry-run
```

## Push、Pull、Apply、Resolve

写操作应先看计划：

```sh
oxideterm cloud-sync push --dry-run --json
oxideterm cloud-sync pull --dry-run --json
oxideterm cloud-sync apply --from remote --strategy merge --dry-run
oxideterm cloud-sync resolve --strategy local-wins --dry-run
```

只有 JSON plan 的方向符合预期后，才加 `--yes`。

## 云同步 Secrets

cloud-sync secret 应通过 stdin 或环境变量写入：

```sh
oxideterm cloud-sync secrets status --json
printf '%s' "$SYNC_TOKEN" | oxideterm cloud-sync secrets set token --stdin
oxideterm cloud-sync secrets clear token
```

状态输出应该只包含 hint，不包含 secret 值。

## 备份

批量导入、sync apply 或高风险设置修改前先创建备份：

```sh
oxideterm backup preview --json
oxideterm backup create --output ./oxideterm-backup.json --json
oxideterm backup inspect ./oxideterm-backup.json --summary
oxideterm backup verify ./oxideterm-backup.json
```

restore 默认先走 dry-run。确认恢复计划后再真执行：

```sh
oxideterm backup restore ./oxideterm-backup.json --section settings --dry-run --json
oxideterm backup diff ./oxideterm-backup.json --section connections --json
```

## 支持 Bundle

提交 issue 或排障时使用脱敏 report bundle：

```sh
oxideterm report --bundle ./oxideterm-report.json --json
```

分享前先检查文件内容。bundle 设计上只包含路径、数量、warning、revision 和 secret hint，而不是 secret 值。
