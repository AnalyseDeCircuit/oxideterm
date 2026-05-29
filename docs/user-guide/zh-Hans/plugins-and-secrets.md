# 插件与凭据

日常插件工作使用桌面应用里的插件管理器：浏览、安装、启用、禁用、更新和配置插件。CLI 伴侣工具只用于无界面检查和脚本化修改。

## 插件管理器

从活动栏打开“插件”，查看已安装插件和可用操作。启用非本人编写的插件前，先检查插件身份、权限、设置，以及它会给应用添加哪些页面或能力。

典型流程：

1. 打开“插件”。
2. 选择一个插件。
3. 查看描述、版本、权限和设置。
4. 启用或禁用插件。
5. 如果插件改变菜单、工具、面板或设置，重新打开相关应用页面。

如果插件看起来已安装但没有生效，先检查插件管理器。桌面应用运行时，不要直接编辑插件状态文件。

## 插件设置

从插件管理器配置插件设置。非凭据设置使用清晰标签。凭据只放进明确设计为凭据的字段。

需要跨机器重复配置时，通过支持的应用或 CLI 流程导出设置，检查导出的 JSON，然后导入目标配置档。

## 凭据

凭据包括 AI 供应商密钥、插件令牌、云同步凭据、连接密码或密钥口令，以及便携包凭据。

凭据规则：

- 通过凭据字段或凭据存储流程输入凭据。
- 不要把凭据值写进插件名称、标签、备注或普通文本字段。
- 不要把凭据粘贴进 AI 提示词、支持包、问题报告或日志。
- 状态页面应该显示提示或已配置标记，而不是凭据值。

## CLI 伴侣工具

无界面插件检查或脚本化配置使用 CLI 伴侣工具：

```sh
oxideterm plugins list --json
oxideterm plugins enable demo.plugin --dry-run
oxideterm plugins disable demo.plugin --dry-run
oxideterm plugins settings export --json
```

CLI 写入凭据时，优先使用标准输入或环境变量：

```sh
oxideterm secrets status --scope ai --json
printf '%s' "$OPENAI_API_KEY" | oxideterm secrets set --scope ai --id builtin-openai --stdin
oxideterm secrets set --scope plugin --plugin-id demo.plugin --key token --env PLUGIN_TOKEN
oxideterm secrets clear --scope cloud-sync --key token
```
