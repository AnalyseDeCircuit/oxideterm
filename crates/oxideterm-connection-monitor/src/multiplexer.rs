use serde::Deserialize;

use crate::shell::{posix_shell_command, shell_quote, windows_encoded_command};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Multiplexer {
    Zellij,
    Herdr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MuxSession {
    pub name: String,
    pub running: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MuxKind {
    Workspace,
    Tab,
    Pane,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MuxNode {
    pub kind: MuxKind,
    pub id: String,
    pub label: String,
    pub children: Vec<MuxNode>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MuxAction {
    Focus,
    Close,
    CreateChild,
}

impl Multiplexer {
    pub fn name(self) -> &'static str {
        match self {
            Self::Herdr => "Herdr",
            Self::Zellij => "Zellij",
        }
    }

    pub fn list_command(self, windows: bool) -> String {
        let (exe, args) = match self {
            Self::Herdr => ("herdr", "session list --json"),
            Self::Zellij => ("zellij", "list-sessions --no-formatting"),
        };
        if windows {
            windows_encoded_command(&format!(
                "[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); if (!(Get-Command {exe} -CommandType Application -ErrorAction SilentlyContinue)) {{exit 127}}; & {exe} {args}; exit $LASTEXITCODE"
            ))
        } else {
            posix_shell_command(&format!(
                "command -v {exe} >/dev/null || exit 127; exec {exe} {args}"
            ))
        }
    }

    fn command(self, windows: bool, session: &str, args: &[&str], attach: bool) -> Option<String> {
        // Session names and object IDs are host data. Never let them introduce options or a second command.
        if std::iter::once(session)
            .chain(args.iter().copied())
            .any(|arg| arg.is_empty() || arg.chars().any(char::is_control))
            || session.starts_with('-')
        {
            return None;
        }
        let quote = |value: &str| {
            if windows {
                use base64::{Engine, engine::general_purpose::STANDARD};
                // PowerShell recognizes Unicode smart quotes as syntax too. Decode arguments as data.
                format!(
                    "([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{}')))",
                    STANDARD.encode(value)
                )
            } else {
                shell_quote(value)
            }
        };
        let program = match self {
            Self::Herdr => "herdr",
            Self::Zellij => "zellij",
        };
        let mut words = vec![program.to_owned()];
        if attach {
            if self == Self::Herdr {
                words.push("session".into());
            }
            words.push("attach".into());
        } else {
            words.push("--session".into());
        }
        words.push(quote(session));
        words.extend(args.iter().map(|arg| quote(arg)));
        let script = words.join(" ");
        Some(if windows {
            windows_encoded_command(&format!(
                "[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); & {script}; exit $LASTEXITCODE"
            ))
        } else {
            posix_shell_command(&format!("exec {script}"))
        })
    }

    pub fn attach(self, windows: bool, session: &str) -> Option<String> {
        self.command(windows, session, &[], true)
    }
    pub fn inspect(self, windows: bool, session: &str) -> Option<String> {
        self.command(
            windows,
            session,
            match self {
                Self::Herdr => &["api", "snapshot"],
                Self::Zellij => &["action", "list-panes", "--json", "--tab"],
            },
            false,
        )
    }
    pub fn action(
        self,
        windows: bool,
        session: &str,
        node: Option<&MuxNode>,
        action: MuxAction,
    ) -> Option<String> {
        use MuxAction::*;
        use MuxKind::*;
        let id = node.map(|n| n.id.as_str()).unwrap_or("");
        if id.starts_with('-') {
            return None;
        }
        let kind = node.map(|n| n.kind);
        let args: &[&str] = match (self, kind, action) {
            (Self::Herdr, None, CreateChild) => &["workspace", "create"],
            (Self::Herdr, Some(Workspace), CreateChild) => &["tab", "create", "--workspace", id],
            (Self::Herdr, Some(Pane), CreateChild) => {
                &["pane", "split", id, "--direction", "right"]
            }
            (Self::Herdr, Some(Workspace), Focus) => &["workspace", "focus", id],
            (Self::Herdr, Some(Tab), Focus) => &["tab", "focus", id],
            (Self::Herdr, Some(Workspace), Close) => &["workspace", "close", id],
            (Self::Herdr, Some(Tab), Close) => &["tab", "close", id],
            (Self::Herdr, Some(Pane), Close) => &["pane", "close", id],
            (Self::Zellij, None, CreateChild) => &["action", "new-tab"],
            (Self::Zellij, Some(Tab), CreateChild) => &["action", "new-pane", "--tab-id", id],
            (Self::Zellij, Some(Tab), Focus) => &["action", "go-to-tab-by-id", id],
            (Self::Zellij, Some(Pane), Focus) => &["action", "focus-pane-id", id],
            (Self::Zellij, Some(Tab), Close) => &["action", "close-tab-by-id", id],
            (Self::Zellij, Some(Pane), Close) => &["action", "close-pane", "--pane-id", id],
            _ => return None,
        };
        self.command(windows, session, args, false)
    }

    pub fn sessions(self, output: &str) -> Option<Vec<MuxSession>> {
        #[derive(Deserialize)]
        struct Sessions {
            sessions: Vec<Session>,
        }
        #[derive(Deserialize)]
        struct Session {
            name: String,
            running: bool,
        }
        match self {
            Self::Herdr => Some(
                serde_json::from_str::<Sessions>(output)
                    .ok()?
                    .sessions
                    .into_iter()
                    .map(|s| MuxSession {
                        name: s.name,
                        running: s.running,
                    })
                    .collect(),
            ),
            Self::Zellij => output
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| {
                    let (name, suffix) = line.trim_end().rsplit_once(" [Created ")?;
                    let (_, state) = suffix.split_once(" ago]")?;
                    Some(MuxSession {
                        name: name.into(),
                        running: match state.trim() {
                            "" | "(current)" => true,
                            "(EXITED - attach to resurrect)" => false,
                            _ => return None,
                        },
                    })
                })
                .collect(),
        }
    }

    pub fn tree(self, output: &str) -> Option<Vec<MuxNode>> {
        match self {
            Self::Herdr => herdr_tree(output),
            Self::Zellij => zellij_tree(output),
        }
    }
}

fn zellij_tree(output: &str) -> Option<Vec<MuxNode>> {
    #[derive(Deserialize)]
    struct Pane {
        id: u32,
        is_plugin: bool,
        title: String,
        tab_id: u64,
        tab_name: String,
    }
    let mut tabs: Vec<MuxNode> = Vec::new();
    for pane in serde_json::from_str::<Vec<Pane>>(output).ok()? {
        let tab_id = pane.tab_id.to_string();
        let index = tabs.iter().position(|t| t.id == tab_id).unwrap_or_else(|| {
            tabs.push(MuxNode {
                kind: MuxKind::Tab,
                id: tab_id,
                label: pane.tab_name,
                children: vec![],
            });
            tabs.len() - 1
        });
        let prefix = if pane.is_plugin { "plugin" } else { "terminal" };
        tabs[index].children.push(MuxNode {
            kind: MuxKind::Pane,
            id: format!("{prefix}_{}", pane.id),
            label: pane.title,
            children: vec![],
        });
    }
    Some(tabs)
}

fn herdr_tree(output: &str) -> Option<Vec<MuxNode>> {
    #[derive(Deserialize)]
    struct Envelope {
        result: ResultBody,
    }
    #[derive(Deserialize)]
    struct ResultBody {
        snapshot: Snapshot,
    }
    #[derive(Deserialize)]
    struct Snapshot {
        workspaces: Vec<Workspace>,
        tabs: Vec<Tab>,
        panes: Vec<Pane>,
    }
    #[derive(Deserialize)]
    struct Workspace {
        workspace_id: String,
        label: String,
    }
    #[derive(Deserialize)]
    struct Tab {
        workspace_id: String,
        tab_id: String,
        label: String,
    }
    #[derive(Deserialize)]
    struct Pane {
        workspace_id: String,
        tab_id: String,
        pane_id: String,
        label: Option<String>,
        title: Option<String>,
    }
    let snap = serde_json::from_str::<Envelope>(output)
        .ok()?
        .result
        .snapshot;
    let mut spaces: Vec<_> = snap
        .workspaces
        .into_iter()
        .map(|w| MuxNode {
            kind: MuxKind::Workspace,
            id: w.workspace_id,
            label: w.label,
            children: vec![],
        })
        .collect();
    for t in snap.tabs {
        let w = spaces.iter_mut().find(|w| w.id == t.workspace_id)?;
        w.children.push(MuxNode {
            kind: MuxKind::Tab,
            id: t.tab_id,
            label: t.label,
            children: vec![],
        });
    }
    for p in snap.panes {
        let w = spaces.iter_mut().find(|w| w.id == p.workspace_id)?;
        let t = w.children.iter_mut().find(|t| t.id == p.tab_id)?;
        t.children.push(MuxNode {
            kind: MuxKind::Pane,
            label: p.label.or(p.title).unwrap_or_else(|| p.pane_id.clone()),
            id: p.pane_id,
            children: vec![],
        });
    }
    Some(spaces)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zellij_preserves_session_names_and_distinguishes_resurrection() {
        let sessions=Multiplexer::Zellij.sessions("work box [Created 2h ago] (current)\nolder [Created 4days ago] (EXITED - attach to resurrect)\n").unwrap();
        assert_eq!(
            sessions,
            [
                MuxSession {
                    name: "work box".into(),
                    running: true
                },
                MuxSession {
                    name: "older".into(),
                    running: false
                }
            ]
        );
        assert!(
            Multiplexer::Zellij
                .attach(false, "--force-run-commands")
                .is_none()
        );
        let tree=zellij_tree(r#"[{"tab_id":4,"tab_name":"dev","id":1,"title":"editor","is_plugin":false},{"tab_id":4,"tab_name":"dev","id":1,"title":"menu","is_plugin":true}]"#).unwrap();
        assert_eq!(tree[0].id, "4");
        assert_eq!(
            tree[0]
                .children
                .iter()
                .map(|n| n.id.as_str())
                .collect::<Vec<_>>(),
            ["terminal_1", "plugin_1"]
        );
    }

    #[test]
    fn herdr_groups_by_parent_ids_even_when_labels_match() {
        let tree=herdr_tree(r#"{"result":{"snapshot":{"workspaces":[{"workspace_id":"a","label":"dev"},{"workspace_id":"b","label":"dev"}],"tabs":[{"workspace_id":"b","tab_id":"t","label":"main"}],"panes":[{"workspace_id":"b","tab_id":"t","pane_id":"p","title":"shell"}]}}}"#).unwrap();
        assert!(tree[0].children.is_empty());
        let pane = &tree[1].children[0].children[0];
        assert_eq!((pane.id.as_str(), pane.label.as_str()), ("p", "shell"));
        assert!(
            Multiplexer::Herdr
                .action(false, "work", Some(pane), MuxAction::Focus)
                .is_none()
        );
    }
}
