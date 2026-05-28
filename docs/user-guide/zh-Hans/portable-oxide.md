# Portable `.oxide` 包

`.oxide` 是加密的便携导入导出包，用于在机器或 profile 之间迁移 OxideTerm 数据。它可以包含连接、转发、应用设置、快捷命令、插件设置，以及可选的 portable secrets。

## 校验与预览

导入前一定先校验和预览：

```sh
oxideterm oxide validate ./profile.oxide
oxideterm oxide preview-import ./profile.oxide --password-stdin --json
oxideterm oxide diff ./profile.oxide --strategy merge --password-env OXIDE_PASSWORD
```

bundle 密码优先通过 stdin 或环境变量提供。不要把密码直接写进 shell history。

## 导入

选择冲突策略：

- `skip`：保留已有本地记录。
- `rename`：冲突记录用新名字导入。
- `replace`：替换本地记录。
- `merge`：合并兼容记录。

示例：

```sh
oxideterm oxide import ./profile.oxide \
  --strategy merge \
  --import-portable-secrets \
  --password-env OXIDE_PASSWORD \
  --dry-run
```

检查计划后，再加 `--yes`。

## 导出

导出选定 profile：

```sh
oxideterm oxide export ./profile.oxide \
  --connection prod \
  --forward web \
  --description "Production workspace" \
  --password-env OXIDE_PASSWORD \
  --json
```

只有目标机器需要加密 portable secrets 时，才使用 `--include-portable-secrets`。只有明确希望把私钥文件放进加密包时，才使用 `--embed-keys`。

## Portable Runtime

portable runtime keystore 用于保护 portable secrets：

```sh
oxideterm portable status --json
printf '%s' "$PORTABLE_PASSWORD" | oxideterm portable setup --password-stdin
oxideterm portable unlock --password-env OXIDETERM_PORTABLE_PASSWORD
```

只有明确要删除本地 portable keystore 时，才使用 `portable reset --yes`。
