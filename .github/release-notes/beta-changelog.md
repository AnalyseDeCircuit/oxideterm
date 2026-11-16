# OxideTerm Native Beta Changelog

Keep the newest beta release entry first. The native package workflow extracts
the section whose heading matches the release version, so older beta entries can
remain in this file.

## 2.3.4-beta.1

### 中文

这是 2.3.4 的首个 Beta，以稳定版 `v2.3.3` 为基线，重点验证终端排版、SSH 首屏和 macOS 全屏切换。

#### 终端与工作区

- 终端新增字距设置，行高改为基于所选字体的原生度量计算，设置预览随之更新。升级时会换算已保存的行高，尽量保持原来的行距。
- 对齐中英文和提示符图标的基线，改善 Powerline 分隔符、盲文点阵、下划线和空心光标的像素对齐。在 macOS 上，浅色不透明背景上的深色文字边缘更清晰，选区不应用这项调整。
- SSH 首屏直接使用所选配色，打开 SSH 终端不再自动展开侧栏。macOS 切换全屏或更改显示缩放时，不再用旧尺寸画面绘制新窗口（PR [#801](https://github.com/AnalyseDeCircuit/oxideterm/pull/801)）。
- 在首页和设置等页面，主机工具图标降低亮度。没有活动 SSH 连接时，提示会标明操作针对本机；OxideSens 关闭时，提示会指向启用说明。
- 11 个语言版 README 更新了演示截图，并把首页放在截图章节首位。

#### 👥 贡献者

- 感谢 @BilityLeu 修复终端启动时的界面问题和 macOS 切换全屏时的画面拉伸（PR [#801](https://github.com/AnalyseDeCircuit/oxideterm/pull/801)）。

#### 升级与验证

试用前建议备份稳定版设置，以便回退。试用时请重点检查自定义字体的字距和行高、SSH 连接后的首屏配色，以及 macOS 切换全屏和显示缩放时的画面。

### English

The first 2.3.4 beta builds on stable `v2.3.3` and focuses on terminal typography, the first SSH frame, and macOS fullscreen transitions.

#### Terminal and workspace

- Added a terminal character spacing control and based line height on the selected font’s native metrics. The settings preview reflects both controls. Saved line heights are converted on upgrade to keep row spacing close to what it was.
- Aligned Latin and CJK text and prompt icons on a shared baseline, and improved pixel alignment for Powerline separators, Braille dots, underlines, and outline cursors. On macOS, dark text has clearer edges on opaque light backgrounds without affecting selected text.
- Applied the chosen palette to the first SSH frame and kept the sidebar collapsed when opening an SSH terminal. Prevented stretched frames during macOS fullscreen and display scaling changes (PR [#801](https://github.com/AnalyseDeCircuit/oxideterm/pull/801)).
- Dimmed host tools on screens such as Home and Settings. Tooltips identify the local machine when there is no active SSH connection and point to setup instructions when OxideSens is off.
- Refreshed the demo screenshots in all 11 READMEs and put the welcome screen first in the gallery.

#### 👥 Contributors

- Thanks to @BilityLeu for fixing terminal startup glitches and stretched frames during macOS fullscreen transitions (PR [#801](https://github.com/AnalyseDeCircuit/oxideterm/pull/801)).

#### Upgrading and testing

Back up your stable settings before trying the beta so you can roll back. Check character spacing and line height with a custom font, the first frame after connecting over SSH, and fullscreen and display scaling changes on macOS.
