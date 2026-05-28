# 连接与端口转发

## 保存连接

保存连接用于复用 SSH 配置，包括 name、host、user、port、group、tags、color、认证方式和可选连接后命令。

列出和查看连接：

```sh
oxideterm connections list
oxideterm connections show prod --json
oxideterm connections search prod
```

用直接参数创建连接：

```sh
oxideterm connections create \
  --name prod \
  --host example.internal \
  --user deploy \
  --port 22 \
  --group production \
  --auth agent \
  --dry-run
```

检查计划后，再重复执行并加 `--yes`。

## 分组

分组用于让连接列表更好读：

```sh
oxideterm connections groups
oxideterm connections group add production --yes
oxideterm connections group rename production prod --yes
```

分组只用于人工导航，不要用来存 secret。

## 校验与导出

导入、CI 检查或生成支持报告前，先跑校验：

```sh
oxideterm connections validate --strict
oxideterm connections export --format raw-safe --json
```

`raw-safe` 输出适合审查和自动化，不包含凭据值。

## 端口转发

转发规则可以独立于 `.oxide` 包管理：

```sh
oxideterm forwards list
oxideterm forwards create \
  --type local \
  --bind-port 8080 \
  --target-host localhost \
  --target-port 80 \
  --connection prod \
  --dry-run
oxideterm forwards validate --json
```

转发类型：

- `local`：本地端口转到远端目标。
- `remote`：远端端口转到本地目标。
- `dynamic`：SOCKS 风格动态转发。

只有当规则应该随所属连接自动启动时，才使用 `--auto-start`。
