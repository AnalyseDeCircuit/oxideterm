# 插件与 Secrets

## 插件

交互式安装、浏览、启用、禁用和设置优先使用桌面插件管理器。CLI 适合无头检查和脚本化修改：

```sh
oxideterm plugins list --json
oxideterm plugins enable demo.plugin --dry-run
oxideterm plugins disable demo.plugin --dry-run
```

CLI 管理的插件状态主要服务于 headless 工作流。如果桌面应用正在运行，依赖这些状态前应关闭或刷新相关视图。

## 插件设置

插件设置以序列化值存储：

```sh
oxideterm plugins settings list --json
oxideterm plugins settings get demo.plugin/theme --json
oxideterm plugins settings set demo.plugin/theme --value-json '"dark"' --dry-run
oxideterm plugins settings export --json
```

可重复初始化时使用 import/export，真实写入前先审查 JSON。

## 通用 Secrets

统一的 `secrets` 命令可以管理 AI provider keys、plugin secrets、cloud-sync secrets、connection secrets 和 portable secrets，并且不打印值：

```sh
oxideterm secrets status --scope ai --json
printf '%s' "$OPENAI_API_KEY" | oxideterm secrets set --scope ai --id builtin-openai --stdin
oxideterm secrets set --scope plugin --plugin-id demo.plugin --key token --env PLUGIN_TOKEN
oxideterm secrets clear --scope cloud-sync --key token
```

secret 规则：

- 优先使用 stdin 或环境变量。
- 不要把 secret 值作为命令参数传入。
- 不要把 secret 粘贴进支持报告。
- JSON 输出只应检查 hint/status，不应包含值。
