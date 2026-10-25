# Workspace Navigation

Use the top bar to choose which tabs to see, the left sidebar to manage connections, and the right sidebar for tools alongside your work.

## Choose a Tab Scope

The searchable selector in the center of the top bar offers:

| Scope | Tabs shown |
| --- | --- |
| All tabs | Every tab in this window; the default view. |
| Global pages | Settings, Session Manager, plugins, cloud sync, and other pages without a connection. |
| Local terminals | Local shell tabs. |
| A connection | That connection's terminal, SFTP, IDE, forwarding, and connection-bound plugin pages. |

Connections with the same address remain distinct when their account, saved identity, or route differs. A scope does not connect to a host or change the target of a command.

Switching scopes remembers the last selected tab for each scope during this window's lifetime. If it has closed, another remaining tab in that scope is selected. An empty scope offers **All tabs** rather than opening a different host. These selections are not synchronized or restored after restarting.

Filtering affects presentation only. Hidden terminals, transfers, forwarding, and background tasks continue running. A mixed split tab appears in every scope represented by its panes; switching scopes keeps the entire layout intact.

## Open and Organize Pages

Opening a page explicitly reveals its scope. For example, opening Settings switches a connection-filtered window to global pages; opening a connection's IDE reveals that connection. When already viewing all tabs, the window stays on all tabs. Background updates do not switch the scope.

Tab navigation, numeric shortcuts, and bulk-close actions use the visible tabs. Dragging changes the visible tabs' relative order without rearranging hidden tabs. Titles retain their full text; when they do not fit, scroll the tab strip horizontally. Closing or moving a page to another window updates the corresponding window's selection.

## Top Bar and Sidebars

- **Command palette:** use the search icon to find an action without remembering its shortcut.
- **Zen mode:** hide the surrounding navigation to concentrate on the current page. Use the configured shortcut to leave it.
- **Background tasks:** inspect the summary when background work is present.
- **Appearance:** the sun or moon shows the current light or dark mode. Configure both schemes in Settings before switching between them.
- **Right sidebar:** OxideSens and individual Host Tools open beside the current workspace. Tool groups separate frequently changing status from on-demand inspection. **More** appears only when the available height cannot fit every tool.
- **App lock:** when configured and enabled in Settings, its shortcut sits at the bottom of the right sidebar. Locking protects the workspace while connections continue running.

The top bar remains available in full screen and hides in Zen mode. On Windows and Linux, workspace actions leave room for the window controls at the right edge. Shortcut hints follow your configured bindings.

Host Tools operate on the connection selected in their panel, and terminal actions use the active pane. In a mixed split, check the actual target rather than treating the tab scope as a command destination.

## Quick Commands

The quick-command bar has a searchable group selector and a horizontally scrollable command list. Select a group to keep its commands close at hand; this selection is remembered across restarts and is independent of filtering in command management. Empty groups can be selected. Deleting the selected group returns the bar to all commands.

Manage commands, ordering, group icons, and colors in command management. Group colors also identify their commands; reset a custom color to return to the theme default. Groups and their appearance travel with supported export and cloud-sync data. Hover a command to see its full name and a bounded preview before sending it. Check the active terminal and any broadcast destinations before execution.

See [Desktop Workflows](desktop.md) for split layouts and [Application Guide](app.md) for the advanced command sender.
