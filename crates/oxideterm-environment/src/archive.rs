use std::collections::HashSet;

use crate::shell::shell_quote;

/// Formats shared with the existing SFTP extraction entry point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveKind {
    Zip,
    Tar,
    TarGzip,
    TarBzip2,
    TarXz,
    TarZstd,
}

const ARCHIVE_SUFFIXES: &[(&str, ArchiveKind)] = &[
    (".tar.gz", ArchiveKind::TarGzip),
    (".tar.bz2", ArchiveKind::TarBzip2),
    (".tar.xz", ArchiveKind::TarXz),
    (".tar.zst", ArchiveKind::TarZstd),
    (".tgz", ArchiveKind::TarGzip),
    (".tbz2", ArchiveKind::TarBzip2),
    (".tbz", ArchiveKind::TarBzip2),
    (".txz", ArchiveKind::TarXz),
    (".tzst", ArchiveKind::TarZstd),
    (".tar", ArchiveKind::Tar),
    (".zip", ArchiveKind::Zip),
];

pub fn archive_kind(file_name: &str) -> Option<ArchiveKind> {
    let lower = file_name.to_ascii_lowercase();
    ARCHIVE_SUFFIXES
        .iter()
        .find_map(|(suffix, kind)| lower.ends_with(suffix).then_some(*kind))
}

pub fn is_archive_path(path: &str) -> bool {
    archive_kind(path).is_some()
        || path.to_ascii_lowercase().ends_with(".7z")
        || path.to_ascii_lowercase().ends_with(".rar")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveShell {
    Posix,
    Fish,
    PowerShell,
    Cmd,
}

impl ArchiveShell {
    pub fn detect(shell: &str) -> Option<Self> {
        let name = shell
            .rsplit(['/', '\\'])
            .next()?
            .trim_start_matches('-')
            .to_ascii_lowercase();
        match name.trim_end_matches(".exe") {
            "sh" | "bash" | "zsh" | "dash" | "ksh" | "git-bash" => Some(Self::Posix),
            "fish" => Some(Self::Fish),
            "powershell" | "pwsh" => Some(Self::PowerShell),
            "cmd" => Some(Self::Cmd),
            _ => None,
        }
    }

    fn quote(self, value: &str) -> Result<String, ArchiveCommandError> {
        if value.is_empty() || value.chars().any(char::is_control) {
            return Err(ArchiveCommandError::InvalidPath);
        }
        Ok(match self {
            Self::Posix => shell_quote(value),
            Self::Fish => format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'")),
            Self::PowerShell => format!("'{}'", value.replace('\'', "''")),
            Self::Cmd => {
                // cmd expands variables even inside quotes; do not offer a lossy command.
                if value.contains(['"', '%', '!']) {
                    return Err(ArchiveCommandError::InvalidPath);
                }
                format!("\"{value}\"")
            }
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveCommandError {
    UnsupportedFormat,
    InvalidPath,
    DirectoryUnavailable,
    ToolUnavailable,
}

/// Probe output contains only capabilities and environment identity, never terminal contents.
pub struct ArchiveCapabilities {
    tools: HashSet<String>,
    pub shell: Option<ArchiveShell>,
    pub home: String,
    pub windows: bool,
    msys: bool,
}

impl ArchiveCapabilities {
    pub fn parse(output: &str) -> Option<Self> {
        let values: Vec<_> = output
            .lines()
            .filter_map(|line| line.trim().strip_prefix("__OXIDE_ARCHIVE__="))
            .collect();
        if !values.contains(&"ready") {
            return None;
        }
        Some(Self {
            tools: values
                .iter()
                .filter_map(|value| value.strip_prefix("tool:").map(str::to_owned))
                .collect(),
            shell: values
                .iter()
                .find_map(|value| value.strip_prefix("shell:").and_then(ArchiveShell::detect)),
            home: values
                .iter()
                .find_map(|value| value.strip_prefix("home:"))
                .unwrap_or("")
                .to_string(),
            windows: values.contains(&"windows"),
            msys: values.contains(&"msys"),
        })
    }

    fn has(&self, tool: &str) -> bool {
        self.tools.contains(tool)
    }
}

// Fixed scripts run through a separate, bounded process or the existing SSH connection.
// They never write a command into the interactive PTY or change its current directory.
pub const UNIX_ARCHIVE_PROBE: &str = r#"
printf '__OXIDE_ARCHIVE__=shell:%s\n__OXIDE_ARCHIVE__=home:%s\n' "$SHELL" "$HOME"
case "$(uname -s 2>/dev/null)" in MINGW*|MSYS*) printf '__OXIDE_ARCHIVE__=msys\n';; esac
for tool in tar unzip gzip bzip2 xz zstd 7z 7zz; do
  if command -v "$tool" >/dev/null 2>&1; then printf '__OXIDE_ARCHIVE__=tool:%s\n' "$tool"; fi
done
for tool in 7zz 7z; do
  if command -v "$tool" >/dev/null 2>&1; then
    case "$("$tool" i 2>/dev/null)" in *" Rar "*|*" Rar5 "*) printf '__OXIDE_ARCHIVE__=tool:rar-%s\n' "$tool";; esac
  fi
done
case "$(tar --version 2>/dev/null)" in *bsdtar*) printf '__OXIDE_ARCHIVE__=tool:bsdtar\n';; esac
tar_help=$(tar --help 2>/dev/null)
for codec in gzip bzip2 xz zstd; do
  case "$tar_help" in *"$codec"*) printf '__OXIDE_ARCHIVE__=tool:tar-%s\n' "$codec";; esac
done
printf '__OXIDE_ARCHIVE__=ready\n'
"#;

pub const POWERSHELL_ARCHIVE_PROBE: &str = r#"
Write-Output '__OXIDE_ARCHIVE__=shell:powershell'
Write-Output ('__OXIDE_ARCHIVE__=home:' + $HOME)
if ([Environment]::OSVersion.Platform -eq 'Win32NT') { Write-Output '__OXIDE_ARCHIVE__=windows' }
foreach ($tool in @('tar','unzip','gzip','bzip2','xz','zstd','7z','7zz')) {
  if (Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue) { Write-Output ('__OXIDE_ARCHIVE__=tool:' + $tool) }
}
if (Get-Command Expand-Archive -ErrorAction SilentlyContinue) { Write-Output '__OXIDE_ARCHIVE__=tool:Expand-Archive' }
foreach ($tool in @('7zz','7z')) {
  if (Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue) {
    if ((& $tool i 2>$null | Out-String) -match '\bRar5?\b') { Write-Output ('__OXIDE_ARCHIVE__=tool:rar-' + $tool) }
  }
}
if (Get-Command tar -CommandType Application -ErrorAction SilentlyContinue) {
  if ((tar --version 2>$null) -match 'bsdtar') { Write-Output '__OXIDE_ARCHIVE__=tool:bsdtar' }
  $tarHelp = tar --help 2>$null | Out-String
  foreach ($codec in @('gzip','bzip2','xz','zstd')) {
    if ($tarHelp.Contains($codec)) { Write-Output ('__OXIDE_ARCHIVE__=tool:tar-' + $codec) }
  }
}
Write-Output '__OXIDE_ARCHIVE__=ready'
"#;

pub struct ArchiveCommandPlan {
    pub archive: String,
    pub destination: String,
    pub command: String,
}

pub fn plan_archive_command(
    path: &str,
    cwd: Option<&str>,
    shell: ArchiveShell,
    capabilities: &ArchiveCapabilities,
) -> Result<ArchiveCommandPlan, ArchiveCommandError> {
    if !is_archive_path(path) {
        return Err(ArchiveCommandError::UnsupportedFormat);
    }
    let windows = capabilities.windows;
    let normalize = |value: &str| {
        // Git Bash tools consume MSYS paths; WSL mount points must not be guessed.
        if capabilities.msys
            && value
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
            && value.as_bytes().get(1) == Some(&b':')
            && value
                .as_bytes()
                .get(2)
                .is_some_and(|ch| matches!(ch, b'/' | b'\\'))
        {
            format!(
                "/{}/{}",
                value[..1].to_ascii_lowercase(),
                value[3..].replace('\\', "/")
            )
        } else {
            value.to_string()
        }
    };
    let path = normalize(path);
    let cwd = cwd.map(normalize);
    let home = normalize(&capabilities.home);
    let absolute = |path: &str| {
        (!windows && path.starts_with('/'))
            || (windows
                && (path.starts_with("\\\\")
                    || path.starts_with("//")
                    || (path.as_bytes().get(1) == Some(&b':')
                        && path
                            .as_bytes()
                            .get(2)
                            .is_some_and(|ch| matches!(ch, b'\\' | b'/')))))
    };
    let separator = if windows { '\\' } else { '/' };
    let archive = if let Some(relative) = path
        .strip_prefix("~/")
        .or_else(|| windows.then(|| path.strip_prefix("~\\")).flatten())
    {
        if home.is_empty() {
            return Err(ArchiveCommandError::DirectoryUnavailable);
        }
        format!("{}{separator}{relative}", home.trim_end_matches(separator))
    } else if absolute(&path) {
        path
    } else if windows && path.starts_with(['/', '\\']) {
        let drive = cwd
            .as_deref()
            .filter(|cwd| cwd.as_bytes().get(1) == Some(&b':'))
            .map(|cwd| &cwd[..2])
            .ok_or(ArchiveCommandError::DirectoryUnavailable)?;
        format!("{drive}{path}")
    } else {
        // Do not reinterpret unresolved drive paths as ordinary relative paths.
        if path.as_bytes().get(1) == Some(&b':') {
            return Err(ArchiveCommandError::InvalidPath);
        }
        let cwd = cwd
            .as_deref()
            .filter(|cwd| absolute(cwd))
            .ok_or(ArchiveCommandError::DirectoryUnavailable)?;
        format!("{}{separator}{path}", cwd.trim_end_matches(separator))
    };
    let lower = archive.to_ascii_lowercase();
    let suffix = ARCHIVE_SUFFIXES
        .iter()
        .find_map(|(suffix, _)| lower.ends_with(suffix).then_some(*suffix))
        .or_else(|| {
            [".7z", ".rar"]
                .into_iter()
                .find(|suffix| lower.ends_with(suffix))
        })
        .ok_or(ArchiveCommandError::UnsupportedFormat)?;
    let destination = archive[..archive.len() - suffix.len()].to_string();
    let a = shell.quote(&archive)?;
    let d = shell.quote(&destination)?;
    let seven_zip = ["7zz", "7z"].into_iter().find(|tool| {
        capabilities.has(tool)
            && (!lower.ends_with(".rar") || capabilities.has(&format!("rar-{tool}")))
    });
    let native = |command: String| {
        if shell == ArchiveShell::PowerShell {
            format!("& {command}")
        } else {
            command
        }
    };
    let extract = match archive_kind(&archive) {
        Some(ArchiveKind::Zip)
            if shell == ArchiveShell::PowerShell && capabilities.has("Expand-Archive") =>
        {
            format!("Expand-Archive -LiteralPath {a} -DestinationPath {d}")
        }
        Some(ArchiveKind::Zip) if capabilities.has("unzip") => {
            native(format!("unzip -n {a} -d {d}"))
        }
        Some(ArchiveKind::Zip) if capabilities.has("bsdtar") => {
            native(format!("tar -xf {a} -C {d}"))
        }
        Some(ArchiveKind::Zip)
            if shell == ArchiveShell::Cmd && capabilities.has("Expand-Archive") =>
        {
            let a = ArchiveShell::PowerShell.quote(&archive)?;
            let d = ArchiveShell::PowerShell.quote(&destination)?;
            format!(
                "powershell -NoProfile -Command \"Expand-Archive -LiteralPath {a} -DestinationPath {d}\""
            )
        }
        Some(ArchiveKind::Zip) | None => {
            let tool = seven_zip.ok_or(ArchiveCommandError::ToolUnavailable)?;
            let destination_arg = shell.quote(&format!("-o{destination}"))?;
            native(format!("{tool} x -aos {destination_arg} {a}"))
        }
        Some(kind) => {
            if !capabilities.has("tar") {
                return Err(ArchiveCommandError::ToolUnavailable);
            }
            let (tool, flag) = match kind {
                ArchiveKind::Tar => ("tar", ""),
                ArchiveKind::TarGzip => ("gzip", "-z "),
                ArchiveKind::TarBzip2 => ("bzip2", "-j "),
                ArchiveKind::TarXz => ("xz", "-J "),
                ArchiveKind::TarZstd => ("zstd", "--zstd "),
                ArchiveKind::Zip => unreachable!(),
            };
            if capabilities.has("bsdtar") {
                native(format!("tar -xf {a} -C {d}"))
            } else if capabilities.has(tool)
                && (kind == ArchiveKind::Tar || capabilities.has(&format!("tar-{tool}")))
            {
                native(format!("tar {flag}-xf {a} -C {d}"))
            } else {
                return Err(ArchiveCommandError::ToolUnavailable);
            }
        }
    };
    // A new directory avoids merging into existing files without an overwrite decision.
    let command = match shell {
        ArchiveShell::Posix | ArchiveShell::Cmd => format!("mkdir {d} && {extract}"),
        ArchiveShell::Fish => format!("mkdir {d}; and {extract}"),
        ArchiveShell::PowerShell => format!(
            "New-Item -ItemType Directory -Path {d} -ErrorAction Stop | Out-Null; if ($?) {{ {extract} }}"
        ),
    };
    Ok(ArchiveCommandPlan {
        archive,
        destination,
        command,
    })
}
