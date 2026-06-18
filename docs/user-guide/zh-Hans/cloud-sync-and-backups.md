# 云同步与备份

日常同步状态、手动同步、冲突查看和恢复检查，优先使用桌面应用里的云同步和备份页面。确认可见应用状态后，再用 CLI 伴侣工具做自动化、CI 或支持包。

## 云同步状态

打开云同步页面，查看同步是否已配置、上次运行时间，以及本地或远端状态是否需要处理。改变同步方向前，先在应用里查看状态和警告。

需要明确 push 或 pull 时，从应用里执行手动同步。如果出现冲突，基于可见状态解决，不要只凭文件名或时间戳猜。

## 配置同步

从云同步设置页面配置后端。后端名称、命名空间和端点应该易于识别，但不要把 token 或密码写进标签。

凭据应通过凭据字段或应用的凭据存储流程输入。状态页面应显示提示、已配置标记或缺少凭据警告，不应显示原始凭据值。

## 备份

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
