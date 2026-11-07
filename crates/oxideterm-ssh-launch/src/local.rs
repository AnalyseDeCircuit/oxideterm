use serde::{Deserialize, Serialize};
use std::{
    env, fmt,
    io::Read,
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

// Leave room for the authenticated single-instance envelope (64 KiB total).
pub const MAX_LOCAL_LAUNCH_BYTES: usize = 60 * 1024;

#[derive(Eq, PartialEq, Serialize, Deserialize)]
pub struct LocalTerminalLaunch {
    pub cwd: PathBuf,
    pub program: Option<PathBuf>,
    pub args: Zeroizing<Vec<String>>,
    pub path: Option<Zeroizing<String>>,
}

impl fmt::Debug for LocalTerminalLaunch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalTerminalLaunch")
            .field("command", &self.program.as_ref().map(|_| "[redacted]"))
            .field("argument_count", &self.args.len())
            .finish_non_exhaustive()
    }
}

impl LocalTerminalLaunch {
    pub fn read_from(input: impl Read) -> Result<Self, &'static str> {
        let mut bytes = Zeroizing::new(Vec::new());
        input
            .take(MAX_LOCAL_LAUNCH_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Could not read the local terminal request")?;
        if bytes.len() > MAX_LOCAL_LAUNCH_BYTES {
            return Err("Local terminal request exceeds 60 KiB");
        }
        let launch: Self =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid local terminal request")?;
        launch.validate()?;
        Ok(launch)
    }

    pub fn resolve(
        cwd: Option<PathBuf>,
        command: Zeroizing<Vec<String>>,
    ) -> Result<Self, &'static str> {
        let cwd = match cwd {
            Some(cwd) => cwd,
            None => env::current_dir().map_err(|_| "Could not determine the current directory")?,
        };
        let cwd = cwd
            .canonicalize()
            .map_err(|_| "Working directory is unavailable")?;
        if !cwd.is_dir() {
            return Err("Working directory is not a directory");
        }
        let path = env::var("PATH").ok().map(Zeroizing::new);
        let program = command
            .first()
            .map(|program| {
                resolve_program(program, &cwd, path.as_deref().map(|path| path.as_str()))
            })
            .transpose()?;
        let args = Zeroizing::new(command.iter().skip(1).cloned().collect());
        let launch = Self {
            cwd,
            program,
            args,
            path,
        };
        launch.validate()?;
        Ok(launch)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.cwd.is_absolute() || !self.cwd.is_dir() {
            return Err("Working directory is unavailable");
        }
        if self.args.iter().any(|arg| arg.contains('\0')) {
            return Err("Arguments cannot contain NUL bytes");
        }
        if let Some(program) = &self.program {
            if !program.is_absolute() || !executable(program) {
                return Err("Requested program is unavailable or not executable");
            }
        } else if !self.args.is_empty() {
            return Err("Arguments require a program");
        }
        Ok(())
    }
}

fn resolve_program(program: &str, cwd: &Path, path: Option<&str>) -> Result<PathBuf, &'static str> {
    if program.is_empty() || program.contains('\0') {
        return Err("Program name is invalid");
    }
    let name = Path::new(program);
    if name.is_absolute() || name.components().count() > 1 {
        let candidate = cwd.join(name);
        return executable(&candidate)
            .then_some(candidate)
            .ok_or("Requested program is unavailable or not executable");
    }
    for directory in env::split_paths(path.unwrap_or_default()) {
        let candidate = cwd.join(directory).join(name);
        if executable(&candidate) {
            return Ok(candidate);
        }
        #[cfg(windows)]
        if candidate.extension().is_none() {
            for extension in ["exe", "com"] {
                let candidate = candidate.with_extension(extension);
                if executable(&candidate) {
                    return Ok(candidate);
                }
            }
        }
    }
    Err("Requested program was not found in PATH")
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(windows)]
    {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe") || ext.eq_ignore_ascii_case("com"))
    }
    #[cfg(not(any(unix, windows)))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn resolves_program_and_retains_literal_arguments_without_debug_disclosure() {
        let launch = LocalTerminalLaunch::resolve(
            Some(env::current_dir().unwrap()),
            Zeroizing::new(vec![
                "/bin/echo".into(),
                "a b".into(),
                "$(touch never)".into(),
                "token=synthetic-secret".into(),
            ]),
        )
        .unwrap();
        assert_eq!(launch.program, Some(PathBuf::from("/bin/echo")));
        assert_eq!(
            launch.args.as_slice(),
            ["a b", "$(touch never)", "token=synthetic-secret"]
        );
        assert!(!format!("{launch:?}").contains("synthetic-secret"));
        let wire = Zeroizing::new(serde_json::to_vec(&launch).unwrap());
        let delivered = LocalTerminalLaunch::read_from(wire.as_slice()).unwrap();
        assert_eq!(
            delivered.args.as_slice(),
            ["a b", "$(touch never)", "token=synthetic-secret"]
        );
        assert_eq!(
            LocalTerminalLaunch::read_from(vec![b'x'; MAX_LOCAL_LAUNCH_BYTES + 1].as_slice())
                .unwrap_err(),
            "Local terminal request exceeds 60 KiB"
        );
        assert_eq!(
            LocalTerminalLaunch::read_from(b"synthetic-secret invalid JSON".as_slice())
                .unwrap_err(),
            "Invalid local terminal request"
        );
        assert_eq!(
            resolve_program("missing-program", Path::new("/"), Some("/nonexistent")),
            Err("Requested program was not found in PATH")
        );
        assert_eq!(
            LocalTerminalLaunch::resolve(
                Some(PathBuf::from("/bin/echo")),
                Zeroizing::new(Vec::new())
            )
            .unwrap_err(),
            "Working directory is not a directory"
        );
    }
}
