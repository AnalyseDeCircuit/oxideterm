# Host Tools Dashboard

This example OxideTerm Native plugin demonstrates:

- a manifest-declared plugin tab;
- a left activity-bar icon backed by a sidebar panel;
- a standalone activity-bar refresh action with its own icon;
- versioned, host-rendered OxideTerm UI components, including cards, toolbars,
  selects, status pills, buttons, empty states, and responsive tables;
- rich baseline calls such as `sessions.getSummary` and `hostTools.getExtensions`;
- an explicitly approved `host_tools.custom.execute` monitor;
- bounded TSV output rendered as a native table.

## Install

Copy this complete directory to `<config-dir>/plugins/host-tools-dashboard`, then make the process entry executable on macOS or Linux:

```sh
chmod +x <config-dir>/plugins/host-tools-dashboard/bin/host-tools-dashboard.js
```

This example requires Node.js on `PATH`. Restart OxideTerm, then enable **Host Tools Dashboard** in the plugin manager and approve its requested capabilities. The process runtime also receives the standard `runtime.process.trusted` approval because it is a local executable.

## Use

1. Connect at least one SSH node.
2. Open the plugin's Activity icon in the left bar.
3. Select **Refresh**, or use the standalone Refresh icon in the left bar.
4. Use **Open full dashboard** to open the same data in a tab.

The tab, sidebar panel, and standalone action each use their manifest-declared
Lucide icon. The action registration references `refresh-dashboard`; its title,
icon, placement, and `dashboard.refresh` command cannot be replaced at runtime.

The runtime sends only component data. OxideTerm owns rendering, theme tokens,
density, focus, keyboard/IME behavior, accessibility semantics, and validation;
the plugin does not ship GPUI code, HTML, CSS, or arbitrary drawing callbacks.

By default, the plugin selects the first active connected node and assumes Linux. Set `nodeId` or `osType` in the plugin manager when you want an explicit node or a different remote operating system.

The commands in `plugin.json` are static package metadata and contain no credentials. The host reuses the existing routed node connection, limits execution to three seconds and 16 KiB, parses TSV before delivery, and does not return standard error or failed-command stdout.
