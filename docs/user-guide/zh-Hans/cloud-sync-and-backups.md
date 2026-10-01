# 云同步与备份

日常同步状态、手动同步、冲突查看和恢复检查，优先使用桌面应用里的云同步和备份页面。确认可见应用状态后，再用 CLI 伴侣工具做自动化、CI 或支持包。

## 云同步状态

打开云同步页面，查看同步是否已配置、上次运行时间，以及本地或远端状态是否需要处理。改变同步方向前，先在应用里查看状态和警告。

点击同步可合并两端的独立修改；同一项内容发生冲突时，在预览中选择要保留的版本。

## 配置同步

从云同步设置页面配置后端。后端名称、命名空间和端点应该易于识别，但不要把 token 或密码写进标签。

凭据应通过凭据字段或应用的凭据存储流程输入。状态页面应显示提示、已配置标记或缺少凭据警告，不应显示原始凭据值。

## 备份

### 更改同步密码

在云同步设置中输入新密码并保存，确认后会创建新的同步空间。上传并校验成功后，应用切换到新空间；原空间和数据会保留。其他设备需要填写新的命名空间和密码。

如果上传失败，重新提交同一新密码可以继续上次的变更。

修改后端等其他设置时，先保存这些设置，再更改密码。CLI 可使用 `oxideterm cloud-sync change-password --yes`，由密码管理器通过标准输入传入新密码。

### `.oxide` 文件

新导出的 `.oxide` 文件会加密连接名称、数量等元数据，输入密码后才能预览内容。旧版文件仍可导入，新版文件需要使用支持新版格式的客户端读取。包含完整证书和私钥的档案会导入托管密钥存储，并保留证书认证方式和私钥口令。

### 创建备份

高影响操作前先创建备份：

- 批量导入连接。
- 导入 `.oxide` 包。
- 执行云同步应用或冲突解决。
- 迁移插件状态。
- 修改会影响终端、SSH、提权凭据、AI 或同步行为的设置。

使用应用里的备份或恢复页面查看将要改变的内容。重要恢复应先检查计划，只恢复最小必要部分，然后重新打开受影响页面并确认结果。

## 支持包

需要共享诊断信息时使用支持包。发送前先检查生成文件。它应包含路径、计数、警告、修订信息和凭据提示，而不是凭据值，提权凭据也一样。

## CLI 伴侣工具

脚本化同步、恢复计划、CI 检查或支持包使用 CLI 伴侣工具：

```sh
oxideterm cloud-sync status --json
oxideterm cloud-sync preview --json
oxideterm cloud-sync diff --dirty-only --format table
oxideterm backup preview --json
oxideterm backup create --output ./oxideterm-backup.json --json
oxideterm report --bundle ./oxideterm-report.json --json
```

CLI 写入先执行预演；只有计划符合预期后才确认：

```sh
oxideterm cloud-sync push --dry-run --json
oxideterm cloud-sync apply --from remote --strategy merge --dry-run
oxideterm backup restore ./oxideterm-backup.json --section settings --dry-run --json
```
