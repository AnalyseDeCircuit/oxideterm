use crate::transport::*;
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    path::{Component, Path},
    rc::Rc,
};
use zeroize::{Zeroize, Zeroizing};

pub const LOCAL: &str = "/home/demo";
pub const REMOTE: &str = "/srv/oxideterm";
pub type Files = Rc<RefCell<DemoFiles>>;

#[derive(Clone)]
pub struct Entry {
    pub data: Option<Vec<u8>>,
    pub revision: u64,
}

pub struct DemoFiles {
    pub entries: BTreeMap<String, Entry>,
    revision: u64,
}
impl DemoFiles {
    pub fn seeded() -> Files {
        let mut fs = Self {
            entries: BTreeMap::new(),
            revision: 0,
        };
        for dir in ["/", "/home", LOCAL, "/srv", REMOTE] {
            fs.put(dir, None);
        }
        fs.put("/home/demo/notes.txt", Some(b"OxideTerm\n".to_vec()));
        fs.put(
            "/srv/oxideterm/README.md",
            Some(b"# OxideTerm\n\nHello from the remote workspace.\n".to_vec()),
        );
        Rc::new(RefCell::new(fs))
    }
    pub fn path(cwd: &str, input: &str) -> String {
        let joined = Path::new(cwd).join(input);
        let mut parts = Vec::new();
        for part in joined.components() {
            match part {
                Component::ParentDir => {
                    parts.pop();
                }
                Component::Normal(name) => parts.push(name.to_string_lossy()),
                _ => {}
            }
        }
        format!("/{}", parts.join("/"))
    }
    pub fn list(&self, path: &str) -> Result<Vec<(&str, &Entry)>, String> {
        if !self.entries.get(path).is_some_and(|e| e.data.is_none()) {
            return Err("ENOTDIR".into());
        }
        Ok(self
            .entries
            .iter()
            .filter_map(|(p, e)| {
                (Path::new(p).parent() == Some(Path::new(path))).then_some((p.as_str(), e))
            })
            .collect())
    }
    pub fn put(&mut self, path: &str, data: Option<Vec<u8>>) {
        self.revision += 1;
        self.entries.insert(
            path.to_owned(),
            Entry {
                data,
                revision: self.revision,
            },
        );
    }
    pub fn copy(&mut self, from: &str, to: &str) -> Result<(), String> {
        let data = self
            .entries
            .get(from)
            .and_then(|e| e.data.clone())
            .ok_or("ENOENT")?;
        let parent = Path::new(to)
            .parent()
            .and_then(Path::to_str)
            .ok_or("EINVAL")?;
        self.list(parent)?;
        if self.entries.contains_key(to) {
            return Err("EEXIST".into());
        }
        self.put(to, Some(data));
        Ok(())
    }
}

struct Shell {
    cwd: String,
    line: Zeroizing<Vec<u8>>,
}
impl Shell {
    fn prompt(&self) -> String {
        format!("demo@oxide:{}$ ", self.cwd)
    }
    fn input(&mut self, bytes: &[u8], fs: &DemoFiles) -> Zeroizing<Vec<u8>> {
        let mut out = Zeroizing::new(Vec::new());
        for &b in bytes {
            match b {
                3 => {
                    self.line.zeroize();
                    out.extend_from_slice(b"^C\r\n");
                    out.extend(self.prompt().bytes());
                }
                8 | 127 => {
                    if !self.line.is_empty() {
                        while self.line.pop().is_some_and(|b| b & 0xc0 == 0x80) {}
                        out.extend_from_slice(b"\x08 \x08");
                    }
                }
                b'\r' | b'\n' => {
                    let raw = std::mem::take(&mut self.line);
                    let cmd = String::from_utf8_lossy(&raw);
                    let (name, args) = cmd
                        .trim()
                        .split_once(char::is_whitespace)
                        .unwrap_or((cmd.trim(), ""));
                    let path = DemoFiles::path(&self.cwd, args.trim());
                    let result = match name {
                        "help" => {
                            "help pwd ls cd cat echo uname whoami clear date neofetch oxide".into()
                        }
                        "pwd" => self.cwd.clone(),
                        "whoami" => "demo".into(),
                        "uname" => "OxideTerm wasm32".into(),
                        "date" => String::from(js_sys::Date::new_0().to_iso_string()),
                        "clear" => "\x1b[2J\x1b[H".into(),
                        "neofetch" | "oxide" => "OxideTerm\nHost: browser\nArch: wasm32".into(),
                        "echo" => args.into(),
                        "cd" => {
                            let path = if args.is_empty() { LOCAL.into() } else { path };
                            match fs.list(&path) {
                                Ok(_) => {
                                    self.cwd = path;
                                    String::new()
                                }
                                Err(e) => e,
                            }
                        }
                        "ls" => {
                            let path = DemoFiles::path(
                                &self.cwd,
                                args.split_whitespace()
                                    .find(|a| !a.starts_with('-'))
                                    .unwrap_or(""),
                            );
                            match fs.list(&path) {
                                Ok(items) => items
                                    .iter()
                                    .map(|(p, e)| {
                                        format!(
                                            "{}{}",
                                            p.rsplit('/').next().unwrap_or(p),
                                            if e.data.is_none() { "/" } else { "" }
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("  "),
                                Err(e) => e,
                            }
                        }
                        "cat" => fs
                            .entries
                            .get(&path)
                            .and_then(|e| e.data.as_ref())
                            .map(|v| String::from_utf8_lossy(v).trim_end().to_string())
                            .unwrap_or_else(|| "ENOENT".into()),
                        "" => String::new(),
                        _ => format!("{name}: ENOENT"),
                    };
                    out.extend_from_slice(b"\r\n");
                    if !result.is_empty() {
                        out.extend(result.replace('\n', "\r\n").bytes());
                        out.extend_from_slice(b"\r\n");
                    }
                    out.extend(self.prompt().bytes());
                }
                32..=255 if self.line.len() < 65536 => {
                    self.line.push(b);
                    out.push(b);
                }
                _ => {}
            }
        }
        out
    }
}

pub struct DemoTransport {
    fs: Files,
    terminals: HashMap<u64, Shell>,
    events: Events,
}
impl DemoTransport {
    pub fn new(fs: Files, events: Events) -> Self {
        Self {
            fs,
            events,
            terminals: HashMap::new(),
        }
    }
}
impl WebTransport for DemoTransport {
    fn send(&mut self, request: GatewayRequest) -> Result<(), ()> {
        let event = match request {
            GatewayRequest::Open { session, node, .. } => {
                let shell = Shell {
                    cwd: if node == "local" { LOCAL } else { REMOTE }.into(),
                    line: Default::default(),
                };
                let bytes = Zeroizing::new(shell.prompt().into_bytes());
                self.terminals.insert(session, shell);
                GatewayEvent::Output { session, bytes }
            }
            GatewayRequest::Input { session, bytes } => GatewayEvent::Output {
                session,
                bytes: self
                    .terminals
                    .get_mut(&session)
                    .ok_or(())?
                    .input(&bytes, &self.fs.borrow()),
            },
            GatewayRequest::Unsubscribe { session } => {
                self.terminals.remove(&session);
                return Ok(());
            }
            GatewayRequest::Copy {
                from, to, transfer, ..
            } => {
                if self.fs.borrow_mut().copy(&from, &to).is_ok() {
                    GatewayEvent::Copied { transfer }
                } else {
                    GatewayEvent::Rejected { request: transfer }
                }
            }
            GatewayRequest::Resize { .. } | GatewayRequest::Cancel { .. } => return Ok(()),
        };
        self.events.unbounded_send(event).map_err(|_| ())
    }
}
