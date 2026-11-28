use super::storage::{DemoFiles, Files, LOCAL};
use zeroize::{Zeroize, Zeroizing};

pub(super) struct Shell {
    fs: Files,
    pub(super) cwd: String,
    line: Zeroizing<Vec<u8>>,
}
impl Shell {
    pub(super) fn new(fs: Files, cwd: &str) -> Self {
        Self {
            fs,
            cwd: cwd.into(),
            line: Default::default(),
        }
    }

    pub(super) fn change_directory(
        &mut self,
        path: &str,
    ) -> Result<Zeroizing<Vec<u8>>, &'static str> {
        if !self.line.is_empty() {
            return Err("sftp.toast.sync_terminal_busy");
        }
        if !path.starts_with('/')
            || path.chars().any(char::is_control)
            || self.fs.borrow().list(path).is_err()
        {
            return Err("sftp.toast.sync_terminal_failed");
        }
        // A file action changes this demo shell directly: it never feeds a path into the input parser.
        self.cwd = path.into();
        Ok(Zeroizing::new(
            format!(
                "cd {}\r\n{}",
                oxideterm_environment::shell_quote(path),
                self.prompt()
            )
            .into_bytes(),
        ))
    }

    pub(super) fn prompt(&self) -> String {
        format!("demo@oxide:{}$ ", self.cwd)
    }
    pub(super) fn input(&mut self, bytes: &[u8]) -> Zeroizing<Vec<u8>> {
        let fs = self.fs.borrow();
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
                        "date" => {
                            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                        }
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
