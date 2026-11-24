use super::bindings::*;
use std::borrow::Cow;

pub fn workspace_definitions() -> Vec<ActionDefinition> {
    let mut actions = vec![
        def(
            "app.newTerminal",
            ActionScope::Global,
            KeyCombo::cmd("t"),
            KeyCombo::ctrl("t"),
        ),
        def(
            "app.shellLauncher",
            ActionScope::Global,
            KeyCombo::cmd_shift("t"),
            KeyCombo::ctrl_shift("t"),
        ),
        def_with_terminal_behavior(
            "app.closeTab",
            ActionScope::Global,
            TerminalBehavior::Never,
            KeyCombo::cmd("w"),
            KeyCombo::ctrl("w"),
        ),
        def(
            "app.closeOtherTabs",
            ActionScope::Global,
            KeyCombo::cmd_shift("w"),
            KeyCombo::ctrl_shift("w"),
        ),
        def(
            "app.newConnection",
            ActionScope::Global,
            KeyCombo::cmd("n"),
            KeyCombo::ctrl("n"),
        ),
        def(
            "app.settings",
            ActionScope::Global,
            KeyCombo::cmd(","),
            KeyCombo::ctrl(","),
        ),
        def(
            "app.toggleSidebar",
            ActionScope::Global,
            KeyCombo::cmd("\\"),
            KeyCombo::ctrl("\\"),
        ),
        def(
            "app.commandPalette",
            ActionScope::Global,
            KeyCombo::cmd("k"),
            KeyCombo::ctrl("k"),
        ),
        def(
            "app.zenMode",
            ActionScope::Global,
            KeyCombo::cmd_shift("z"),
            KeyCombo::ctrl_shift("z"),
        ),
        def(
            "app.toggleFullscreen",
            ActionScope::Global,
            KeyCombo::cmd_ctrl("f"),
            KeyCombo::plain("f11"),
        ),
        def(
            "app.nextTab",
            ActionScope::Global,
            KeyCombo::cmd("}"),
            KeyCombo::ctrl("tab"),
        ),
        def(
            "app.prevTab",
            ActionScope::Global,
            KeyCombo::cmd("{"),
            KeyCombo::ctrl_shift("tab"),
        ),
        def(
            "app.navBack",
            ActionScope::Global,
            KeyCombo::cmd("["),
            KeyCombo {
                key: "arrowleft".to_string(),
                ctrl: false,
                shift: false,
                alt: true,
                meta: false,
            },
        ),
        def(
            "app.navForward",
            ActionScope::Global,
            KeyCombo::cmd("]"),
            KeyCombo {
                key: "arrowright".to_string(),
                ctrl: false,
                shift: false,
                alt: true,
                meta: false,
            },
        ),
    ];

    for index in 1..=9 {
        actions.push(def(
            format!("app.goToTab{index}"),
            ActionScope::Global,
            KeyCombo::cmd(index.to_string()),
            KeyCombo::ctrl(index.to_string()),
        ));
    }

    actions.extend([
        def(
            "app.fontIncrease",
            ActionScope::Global,
            KeyCombo::cmd("="),
            KeyCombo::ctrl("="),
        ),
        def(
            "app.fontDecrease",
            ActionScope::Global,
            KeyCombo::cmd("-"),
            KeyCombo::ctrl("-"),
        ),
        def(
            "app.fontReset",
            ActionScope::Global,
            KeyCombo::cmd("0"),
            KeyCombo::ctrl("0"),
        ),
        def(
            "app.showShortcuts",
            ActionScope::Global,
            KeyCombo::cmd("/"),
            KeyCombo::ctrl("/"),
        ),
        def(
            "terminal.search",
            ActionScope::Terminal,
            KeyCombo::cmd("f"),
            KeyCombo::ctrl_shift("f"),
        ),
        def(
            "terminal.copy",
            ActionScope::Terminal,
            KeyCombo::cmd("c"),
            KeyCombo::ctrl_shift("c"),
        ),
        def(
            "terminal.cut",
            ActionScope::Terminal,
            KeyCombo::cmd("x"),
            KeyCombo::ctrl_shift("x"),
        ),
        def(
            "terminal.paste",
            ActionScope::Terminal,
            KeyCombo::cmd("v"),
            KeyCombo::ctrl_shift("v"),
        ),
        def(
            "terminal.clearScreen",
            ActionScope::Terminal,
            // Shells own Ctrl+L on every platform and use it to clear and
            // redraw the prompt. Keep the host-only action on a shifted chord.
            KeyCombo::ctrl_shift("l"),
            KeyCombo::ctrl_shift("l"),
        ),
        def(
            "terminal.aiPanel",
            ActionScope::Terminal,
            KeyCombo::cmd("i"),
            KeyCombo::ctrl_shift("i"),
        ),
        def(
            "terminal.recording",
            ActionScope::Terminal,
            KeyCombo::cmd_shift("r"),
            KeyCombo::ctrl_shift("r"),
        ),
        def(
            "terminal.toggleFreeTypeMode",
            ActionScope::Terminal,
            KeyCombo::cmd_shift("f"),
            KeyCombo::ctrl_alt("f"),
        ),
        def_with_terminal_behavior(
            "terminal.closePanel",
            ActionScope::Terminal,
            TerminalBehavior::WhenPanelOpen,
            KeyCombo::plain("escape"),
            KeyCombo::plain("escape"),
        ),
        def(
            "split.horizontal",
            ActionScope::Split,
            KeyCombo::cmd_shift("e"),
            KeyCombo::ctrl_shift("e"),
        ),
        def(
            "split.vertical",
            ActionScope::Split,
            KeyCombo::cmd_shift("d"),
            KeyCombo::ctrl_shift("d"),
        ),
        def(
            "split.closePane",
            ActionScope::Split,
            KeyCombo::cmd_shift("w"),
            KeyCombo::ctrl_shift("w"),
        ),
        def(
            "split.navLeft",
            ActionScope::Split,
            KeyCombo::cmd_alt("arrowleft"),
            KeyCombo::ctrl_alt("arrowleft"),
        ),
        def(
            "split.navRight",
            ActionScope::Split,
            KeyCombo::cmd_alt("arrowright"),
            KeyCombo::ctrl_alt("arrowright"),
        ),
        def(
            "palette.eventLog",
            ActionScope::Palette,
            KeyCombo::cmd("j"),
            KeyCombo::ctrl("j"),
        ),
        def(
            "palette.aiSidebar",
            ActionScope::Palette,
            KeyCombo::cmd_shift("a"),
            KeyCombo::ctrl_shift("a"),
        ),
        // Keep Ctrl+B available as the tmux prefix on Windows and Linux.
        def(
            "palette.broadcast",
            ActionScope::Palette,
            KeyCombo::cmd("b"),
            KeyCombo::ctrl_shift("b"),
        ),
    ]);

    actions.extend([
        def(
            "editor.save",
            ActionScope::Editor,
            KeyCombo::cmd("s"),
            KeyCombo::ctrl("s"),
        ),
        def(
            "editor.copy",
            ActionScope::Editor,
            KeyCombo::cmd("c"),
            KeyCombo::ctrl("c"),
        ),
        def(
            "editor.cut",
            ActionScope::Editor,
            KeyCombo::cmd("x"),
            KeyCombo::ctrl("x"),
        ),
        def(
            "editor.paste",
            ActionScope::Editor,
            KeyCombo::cmd("v"),
            KeyCombo::ctrl("v"),
        ),
        def(
            "editor.selectAll",
            ActionScope::Editor,
            KeyCombo::cmd("a"),
            KeyCombo::ctrl("a"),
        ),
        def(
            "editor.undo",
            ActionScope::Editor,
            KeyCombo::cmd("z"),
            KeyCombo::ctrl("z"),
        ),
        def(
            "editor.redo",
            ActionScope::Editor,
            KeyCombo::cmd_shift("z"),
            KeyCombo::ctrl_shift("z"),
        ),
        def(
            "editor.addNextMatch",
            ActionScope::Editor,
            KeyCombo::cmd("d"),
            KeyCombo::ctrl("d"),
        ),
        def(
            "editor.find",
            ActionScope::Editor,
            KeyCombo::cmd("f"),
            KeyCombo::ctrl("f"),
        ),
        def(
            "sftp.selectAll",
            ActionScope::Sftp,
            KeyCombo::cmd("a"),
            KeyCombo::ctrl("a"),
        ),
        def(
            "sftp.editPath",
            ActionScope::Sftp,
            KeyCombo::cmd("l"),
            KeyCombo::ctrl("l"),
        ),
        def(
            "sftp.revealTerminalDirectory",
            ActionScope::Sftp,
            KeyCombo::cmd_shift("g"),
            KeyCombo::ctrl_shift("g"),
        ),
        def(
            "sftp.open",
            ActionScope::Sftp,
            KeyCombo::plain("enter"),
            KeyCombo::plain("enter"),
        ),
        def(
            "sftp.preview",
            ActionScope::Sftp,
            KeyCombo::plain("space"),
            KeyCombo::plain("space"),
        ),
        def(
            "sftp.upload",
            ActionScope::Sftp,
            KeyCombo::plain("arrowright"),
            KeyCombo::plain("arrowright"),
        ),
        def(
            "sftp.download",
            ActionScope::Sftp,
            KeyCombo::plain("arrowleft"),
            KeyCombo::plain("arrowleft"),
        ),
        def(
            "sftp.delete",
            ActionScope::Sftp,
            KeyCombo::plain("delete"),
            KeyCombo::plain("delete"),
        ),
        def(
            "sftp.rename",
            ActionScope::Sftp,
            KeyCombo::plain("f2"),
            KeyCombo::plain("f2"),
        ),
    ]);

    actions.push(def(
        "sftp.togglePreviewSource",
        ActionScope::Sftp,
        KeyCombo::plain("u"),
        KeyCombo::plain("u"),
    ));

    actions.extend([
        def(
            "fileManager.selectAll",
            ActionScope::FileManager,
            KeyCombo::cmd("a"),
            KeyCombo::ctrl("a"),
        ),
        def(
            "fileManager.copy",
            ActionScope::FileManager,
            KeyCombo::cmd("c"),
            KeyCombo::ctrl("c"),
        ),
        def(
            "fileManager.cut",
            ActionScope::FileManager,
            KeyCombo::cmd("x"),
            KeyCombo::ctrl("x"),
        ),
        def(
            "fileManager.paste",
            ActionScope::FileManager,
            KeyCombo::cmd("v"),
            KeyCombo::ctrl("v"),
        ),
        def(
            "fileManager.editPath",
            ActionScope::FileManager,
            KeyCombo::cmd("l"),
            KeyCombo::ctrl("l"),
        ),
        def(
            "fileManager.open",
            ActionScope::FileManager,
            KeyCombo::plain("enter"),
            KeyCombo::plain("enter"),
        ),
        def(
            "fileManager.preview",
            ActionScope::FileManager,
            KeyCombo::plain("space"),
            KeyCombo::plain("space"),
        ),
        def(
            "fileManager.delete",
            ActionScope::FileManager,
            KeyCombo::plain("delete"),
            KeyCombo::plain("delete"),
        ),
        def(
            "fileManager.deleteOrParent",
            ActionScope::FileManager,
            KeyCombo::plain("backspace"),
            KeyCombo::plain("backspace"),
        ),
        def(
            "fileManager.rename",
            ActionScope::FileManager,
            KeyCombo::plain("f2"),
            KeyCombo::plain("f2"),
        ),
        def(
            "preview.previous",
            ActionScope::Preview,
            KeyCombo::plain("arrowleft"),
            KeyCombo::plain("arrowleft"),
        ),
        def(
            "preview.next",
            ActionScope::Preview,
            KeyCombo::plain("arrowright"),
            KeyCombo::plain("arrowright"),
        ),
        def(
            "preview.metadata",
            ActionScope::Preview,
            KeyCombo::plain("i"),
            KeyCombo::plain("i"),
        ),
        def(
            "preview.source",
            ActionScope::Preview,
            KeyCombo::plain("u"),
            KeyCombo::plain("u"),
        ),
        def(
            "preview.zoomIn",
            ActionScope::Preview,
            KeyCombo::plain("="),
            KeyCombo::plain("="),
        ),
        def(
            "preview.zoomOut",
            ActionScope::Preview,
            KeyCombo::plain("-"),
            KeyCombo::plain("-"),
        ),
        def(
            "preview.resetZoom",
            ActionScope::Preview,
            KeyCombo::plain("0"),
            KeyCombo::plain("0"),
        ),
        def(
            "preview.rotate",
            ActionScope::Preview,
            KeyCombo::plain("r"),
            KeyCombo::plain("r"),
        ),
        def(
            "remoteDesktop.copy",
            ActionScope::RemoteDesktop,
            KeyCombo::cmd("c"),
            KeyCombo::ctrl("c"),
        ),
        def(
            "remoteDesktop.paste",
            ActionScope::RemoteDesktop,
            KeyCombo::cmd("v"),
            KeyCombo::ctrl("v"),
        ),
    ]);
    for (id, key) in [
        ("terminal.suggestPrevious", "arrowup"),
        ("terminal.suggestNext", "arrowdown"),
    ] {
        let combo = KeyCombo {
            alt: true,
            ..KeyCombo::plain(key)
        };
        actions.push(def(id, ActionScope::Terminal, combo.clone(), combo));
    }
    actions.extend([
        def(
            "terminal.wordBackward",
            ActionScope::Terminal,
            KeyCombo {
                alt: true,
                ..KeyCombo::plain("arrowleft")
            },
            KeyCombo {
                alt: true,
                ..KeyCombo::plain("b")
            },
        ),
        def(
            "terminal.wordForward",
            ActionScope::Terminal,
            KeyCombo {
                alt: true,
                ..KeyCombo::plain("arrowright")
            },
            KeyCombo {
                alt: true,
                ..KeyCombo::plain("f")
            },
        ),
        def(
            "terminal.scrollPageUp",
            ActionScope::Terminal,
            KeyCombo {
                shift: true,
                ..KeyCombo::plain("pageup")
            },
            KeyCombo {
                shift: true,
                ..KeyCombo::plain("pageup")
            },
        ),
        def(
            "terminal.scrollPageDown",
            ActionScope::Terminal,
            KeyCombo {
                shift: true,
                ..KeyCombo::plain("pagedown")
            },
            KeyCombo {
                shift: true,
                ..KeyCombo::plain("pagedown")
            },
        ),
        def(
            "terminal.scrollLineUp",
            ActionScope::Terminal,
            KeyCombo {
                shift: true,
                ..KeyCombo::plain("arrowup")
            },
            KeyCombo {
                shift: true,
                ..KeyCombo::plain("arrowup")
            },
        ),
        def(
            "terminal.scrollLineDown",
            ActionScope::Terminal,
            KeyCombo {
                shift: true,
                ..KeyCombo::plain("arrowdown")
            },
            KeyCombo {
                shift: true,
                ..KeyCombo::plain("arrowdown")
            },
        ),
        def(
            "terminal.scrollTop",
            ActionScope::Terminal,
            KeyCombo {
                shift: true,
                ..KeyCombo::plain("home")
            },
            KeyCombo {
                shift: true,
                ..KeyCombo::plain("home")
            },
        ),
        def(
            "terminal.scrollBottom",
            ActionScope::Terminal,
            KeyCombo {
                shift: true,
                ..KeyCombo::plain("end")
            },
            KeyCombo {
                shift: true,
                ..KeyCombo::plain("end")
            },
        ),
        def(
            "terminal.pasteAlternate",
            ActionScope::Terminal,
            KeyCombo {
                shift: true,
                ..KeyCombo::plain("insert")
            },
            KeyCombo {
                shift: true,
                ..KeyCombo::plain("insert")
            },
        ),
        def(
            "terminal.copyAlternate",
            ActionScope::Terminal,
            KeyCombo::ctrl("insert"),
            KeyCombo::ctrl("insert"),
        ),
        def(
            "terminal.terminateTask",
            ActionScope::Terminal,
            KeyCombo::cmd_shift("k"),
            KeyCombo::cmd_shift("k"),
        ),
        def(
            "terminal.killTask",
            ActionScope::Terminal,
            KeyCombo {
                alt: true,
                ..KeyCombo::cmd_shift("k")
            },
            KeyCombo {
                alt: true,
                ..KeyCombo::cmd_shift("k")
            },
        ),
    ]);
    actions.extend([
        def(
            "terminal.aiSubmit",
            ActionScope::AiPanel,
            KeyCombo::plain("enter"),
            KeyCombo::plain("enter"),
        ),
        def(
            "terminal.aiInsert",
            ActionScope::AiPanel,
            KeyCombo::plain("tab"),
            KeyCombo::plain("tab"),
        ),
    ]);
    actions
}

pub fn def(
    id: impl Into<Cow<'static, str>>,
    scope: ActionScope,
    mac: KeyCombo,
    other: KeyCombo,
) -> ActionDefinition {
    def_with_terminal_behavior(id, scope, TerminalBehavior::Always, mac, other)
}

fn def_with_terminal_behavior(
    id: impl Into<Cow<'static, str>>,
    scope: ActionScope,
    terminal_behavior: TerminalBehavior,
    mac: KeyCombo,
    other: KeyCombo,
) -> ActionDefinition {
    ActionDefinition {
        id: id.into(),
        label: None,
        scope,
        terminal_behavior,
        mac: normalize_combo(mac),
        other: normalize_combo(other),
    }
}
