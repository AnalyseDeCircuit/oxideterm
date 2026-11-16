# OxideTerm Native Beta Changelog

Keep the newest beta release entry first. The native package workflow extracts
the section whose heading matches the release version, so older beta entries can
remain in this file.

## 2.3.4-beta.1

### 中文

这是 2.3.4 的首个 Beta，以稳定版 `v2.3.3` 为基线，新增 XTerminal 的 SSH 连接导入，简化首次启动引导，并改进终端排版、滚动和配色。

#### 连接与首次启动

- 可从 XTerminal 导入 SSH 连接。导出文件中的密码和私钥口令写入受保护的凭据存储，不会出现在预览中。XTerminal 的导出不含私钥正文；要复制私钥，请同时选择原私钥文件。跳板链路和非 SSH 连接不会迁移。
- 缩短首次启动引导，精简功能介绍，把主题预览和字体等常用设置放在同一步。新配置默认使用浅色 Code Light 和深色 GitHub Dark。

#### 终端与工作区

- 终端新增字距设置，行高改为基于所选字体的原生度量计算，设置预览随之更新。升级时会换算已保存的行高，尽量保持原来的行距。
- 对齐中英文和提示符图标的基线，改善 Powerline 分隔符、盲文点阵、下划线和空心光标的像素对齐。在 macOS 上，浅色不透明背景上的深色文字边缘更清晰，选区不应用这项调整。
- SSH 首屏直接使用所选配色，打开 SSH 终端不再自动展开侧栏。macOS 切换全屏或更改显示缩放时，不再用旧尺寸画面绘制新窗口（PR [#801](https://github.com/AnalyseDeCircuit/oxideterm/pull/801)）。
- 终端滚动条不再占用内容宽度。滚动时保留搜索高亮，拖动滑块时可越过分屏边界，点击滑块以外的区域仍会交给终端（PR [#802](https://github.com/AnalyseDeCircuit/oxideterm/pull/802)）。
- 在首页和设置等页面，主机工具图标降低亮度。没有活动 SSH 连接时，提示会标明操作针对本机；OxideSens 关闭时，提示会指向启用说明。
- 11 个语言版 README 更新了演示截图，并把首页放在截图章节首位。

#### 插件兼容与会话安全

- 修复 Beta 版误判兼容插件的问题：插件仅声明稳定版范围时，按 Beta 的基础版本判断；明确包含预发行版本的要求仍按完整版本判断。
- Kitty 文件传输授权随终端会话结束而撤销。旧确认框不能授权新会话，重连后收到的新请求也不会被迟到的通知清除（PR [#803](https://github.com/AnalyseDeCircuit/oxideterm/pull/803)）。

#### 👥 贡献者

- 感谢 @BilityLeu 修复终端首屏和 macOS 全屏切换问题，并改进滚动条与搜索高亮（PR [#801](https://github.com/AnalyseDeCircuit/oxideterm/pull/801)、[#802](https://github.com/AnalyseDeCircuit/oxideterm/pull/802)）。
- 感谢 @m00nLi 修复 Kitty 文件传输授权在会话结束时未撤销的问题（PR [#803](https://github.com/AnalyseDeCircuit/oxideterm/pull/803)）。

#### 升级与验证

试用前建议备份稳定版设置，以便回退。请重点验证 XTerminal 密码和私钥导入、插件在 Beta 上的加载、自定义字体的字距和行高、SSH 首屏配色，以及 macOS 的全屏和显示缩放切换。

### English

The first 2.3.4 beta builds on stable `v2.3.3`, adds XTerminal SSH import, streamlines first-run setup, and improves terminal typography, scrolling, and colors.

#### Connections and first-run setup

- Added SSH import from XTerminal. Passwords and key passphrases included in the export go to protected credential storage and stay out of the preview. XTerminal exports do not contain private key data; select the original key file alongside the export to copy it. Jump chains and non-SSH connections are not imported.
- Shortened first-run setup, trimmed the feature tour, and put the theme preview and common controls such as terminal fonts in one step. New profiles default to Code Light and GitHub Dark.

#### Terminal and workspace

- Added a terminal character spacing control and based line height on the selected font’s native metrics. The settings preview reflects both controls. Saved line heights are converted on upgrade to keep row spacing close to what it was.
- Aligned Latin and CJK text and prompt icons on a shared baseline, and improved pixel alignment for Powerline separators, Braille dots, underlines, and outline cursors. On macOS, dark text has clearer edges on opaque light backgrounds without affecting selected text.
- Applied the chosen palette to the first SSH frame and kept the sidebar collapsed when opening an SSH terminal. Prevented stretched frames during macOS fullscreen and display scaling changes (PR [#801](https://github.com/AnalyseDeCircuit/oxideterm/pull/801)).
- Overlaid terminal scrollbars without taking up content width. Search highlights remain visible while scrolling, scrollbar drags can cross split-pane boundaries, and clicks outside the thumb go to the terminal (PR [#802](https://github.com/AnalyseDeCircuit/oxideterm/pull/802)).
- Dimmed host tools on screens such as Home and Settings. Tooltips identify the local machine when there is no active SSH connection and point to setup instructions when OxideSens is off.
- Refreshed the demo screenshots in all 11 READMEs and put the welcome screen first in the gallery.

#### Plugin compatibility and session security

- Fixed compatible plugins being rejected on beta builds. Stable-only version ranges are checked against the beta’s base version, while requirements that explicitly name prereleases still use the full version.
- Revoked Kitty file-transfer approval when the terminal session ends. An old dialog cannot approve a new session, and a late notification no longer clears a request from a reconnected session (PR [#803](https://github.com/AnalyseDeCircuit/oxideterm/pull/803)).

#### 👥 Contributors

- Thanks to @BilityLeu for the terminal startup and macOS fullscreen fixes and the scrollbar and search highlight improvements (PR [#801](https://github.com/AnalyseDeCircuit/oxideterm/pull/801), [#802](https://github.com/AnalyseDeCircuit/oxideterm/pull/802)).
- Thanks to @m00nLi for fixing Kitty file-transfer approval persisting after a session ended (PR [#803](https://github.com/AnalyseDeCircuit/oxideterm/pull/803)).

#### Upgrading and testing

Back up your stable settings before trying the beta so you can roll back. Check XTerminal password and key import, plugin loading on the beta, character spacing and line height with a custom font, the first SSH frame, and macOS fullscreen and display scaling transitions.
