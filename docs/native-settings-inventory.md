# Native Settings Inventory

This inventory is the Phase 1 source map for native settings. Tauri backend
`settings.json` is not exhaustive; native settings are the union of
`settingsStore.ts`, visible settings UI controls, Tauri app-settings commands,
and frontend-local user preferences.

## Primary Schema Sources

| Source | Role | Native handling |
|---|---|---|
| `/Users/dominical/Documents/OxideTerm-main/src/store/settingsStore.ts` | Main semantic model, defaults, normalization, actions | Mirrored by `oxideterm-settings::PersistedSettings` |
| `/Users/dominical/Documents/OxideTerm-main/src-tauri/src/commands/app_settings.rs` | JSON envelope, validation, import/export sections | Mirrored by `SettingsStore` envelope and sanitization |
| `/Users/dominical/Documents/OxideTerm-main/src/components/settings/SettingsView.tsx` | Settings shell ordering and navigation | Phase 2 UI source |
| `/Users/dominical/Documents/OxideTerm-main/src/components/settings/tabs/*.tsx` | Visible controls | Every visible setting gets a persisted native field |
| `/Users/dominical/Documents/OxideTerm-main/src/locales/*/settings*.json` | Labels only | Not treated as schema |

## Persisted Settings Fields

| Native field | Tauri path | Default | Runtime status |
|---|---|---|---|
| `general.language` | `settings.general.language` / `app_lang` | `zh-CN` | Applied at native startup |
| `general.updateChannel` | `settings.general.updateChannel` | `beta` | Persisted |
| `terminal.theme` | `settings.terminal.theme` | `default` | Applied at native startup |
| `terminal.fontFamily` | `settings.terminal.fontFamily` | `jetbrains` | Persisted, terminal construction support |
| `terminal.customFontFamily` | `settings.terminal.customFontFamily` | empty | Persisted, terminal construction support |
| `terminal.fontSize` | `settings.terminal.fontSize` | `14` | Persisted, terminal construction support |
| `terminal.lineHeight` | `settings.terminal.lineHeight` | `1.2` | Persisted, terminal construction support |
| `terminal.cursorStyle` | `settings.terminal.cursorStyle` | `block` | Persisted |
| `terminal.cursorBlink` | `settings.terminal.cursorBlink` | `true` | Persisted, terminal construction support |
| `terminal.scrollback` | `settings.terminal.scrollback` | `1000` | Persisted |
| `terminal.renderer` | `settings.terminal.renderer` | `auto` or `canvas` on Windows | Persisted |
| `terminal.terminalEncoding` | `settings.terminal.terminalEncoding` | `utf-8` | Persisted |
| `terminal.adaptiveRenderer` | `settings.terminal.adaptiveRenderer` | `auto` | Persisted |
| `terminal.showFpsOverlay` | `settings.terminal.showFpsOverlay` | `false` | Persisted |
| `terminal.pasteProtection` | `settings.terminal.pasteProtection` | `true` | Persisted |
| `terminal.smartCopy` | `settings.terminal.smartCopy` | `true` | Persisted |
| `terminal.osc52Clipboard` | `settings.terminal.osc52Clipboard` | `true` | Persisted |
| `terminal.copyOnSelect` | `settings.terminal.copyOnSelect` | `false` | Persisted, terminal construction support |
| `terminal.middleClickPaste` | `settings.terminal.middleClickPaste` | `false` | Persisted |
| `terminal.selectionRequiresShift` | `settings.terminal.selectionRequiresShift` | `false` | Persisted |
| `terminal.autosuggest.localShellHistory` | `settings.terminal.autosuggest.localShellHistory` | `true` | Persisted |
| `terminal.commandBar.*` | `settings.terminal.commandBar` | Tauri defaults | Persisted |
| `terminal.commandMarks.*` | `settings.terminal.commandMarks` | Tauri defaults | Persisted |
| `terminal.background*` | `settings.terminal.background*` | Tauri defaults | Persisted |
| `terminal.highlightRules` | `settings.terminal.highlightRules` | `[]` | Persisted |
| `terminal.inBandTransfer.*` | `settings.terminal.inBandTransfer` | Tauri defaults | Persisted |
| `buffer.maxLines` | `settings.buffer.maxLines` | `8000` | Persisted |
| `appearance.*` | `settings.appearance` | Tauri defaults | Persisted |
| `connectionDefaults.username` | `settings.connectionDefaults.username` | `root` | Used by native new-connection form defaults later |
| `connectionDefaults.port` | `settings.connectionDefaults.port` | `22` | Used by native new-connection form defaults later |
| `treeUI.*` | `settings.treeUI` / legacy tree keys | empty | Persisted |
| `sidebarUI.*` | `settings.sidebarUI` / legacy UI state | Tauri defaults | Applied at native startup |
| `ai.*` | `settings.ai` | Tauri defaults | Persisted |
| `localTerminal.*` | `settings.localTerminal` | Tauri defaults | Persisted |
| `sftp.*` | `settings.sftp` | Tauri defaults | Persisted |
| `ide.*` | `settings.ide` | Tauri defaults | Persisted |
| `reconnect.*` | `settings.reconnect` | Tauri defaults | Persisted |
| `connectionPool.idleTimeoutSecs` | `settings.connectionPool.idleTimeoutSecs` | `1800` | Applied to native SSH registry at startup |
| `experimental.*` | `settings.experimental` | Tauri defaults | Persisted |
| `onboardingCompleted` | `settings.onboardingCompleted` | `false` | Persisted |
| `commandPaletteMru` | `settings.commandPaletteMru` | `[]` | Persisted |

## Frontend-Local User Settings Migrated Into Native

| localStorage key | Source file | Native field | Behavior |
|---|---|---|---|
| `oxide-settings-v2` | `settingsStore.ts` | root settings | Primary legacy snapshot |
| `oxide-settings` | `settingsStore.ts` | root settings | Legacy fallback snapshot |
| `oxide-ui-state` | `appStore.ts` | `sidebarUI.*` | Migrated when present |
| `oxide-tree-expanded` | `settingsStore.ts` legacy list | `treeUI.expandedIds` | Migrated when present |
| `oxide-focused-node` | `settingsStore.ts` legacy focus | `treeUI.focusedNodeId` | Migrated when present |
| `app_lang` | `i18n.ts` / `settingsStore.ts` | `general.language` | Fallback when settings snapshot has no language |
| `oxideterm_keybindings` | `keybindingStore.ts` | `keybindings.overrides` | Migrated as user keybinding overrides |
| `oxide-custom-themes` | `themes.ts` | `customThemes` | Migrated as custom theme registry |
| `oxide-launcher-enabled` | `launcherStore.ts` | `launcher.enabled` | Migrated as launcher opt-in setting |
| `oxideterm:agent-roles` | `agentRolesStore.ts` | `agentRoles` | Migrated as user role/pipeline settings |
| `oxideterm.saveConnection` | `NewConnectionModal.tsx` | `newConnection.saveConnection` | Migrated as new-connection default |

## Operational Metadata Not Migrated As Settings

| localStorage key | Reason |
|---|---|
| `oxideterm:lastExportTimestamp` | Export bookkeeping, not a user setting |
| Plugin-scoped localStorage keys | Plugin-owned storage; must remain under plugin storage manager semantics |

