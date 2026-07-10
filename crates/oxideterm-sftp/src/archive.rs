// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use thiserror::Error;

/// Archive formats supported by remote SFTP extraction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveKind {
    Zip,
    Tar,
    TarGzip,
    TarBzip2,
    TarXz,
    TarZstd,
}

/// A shell command and its recognized archive format.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveExtractionPlan {
    pub kind: ArchiveKind,
    pub command: String,
}

/// Errors produced while planning remote archive extraction.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ArchiveExtractionError {
    #[error("unsupported archive: {file_name}")]
    UnsupportedArchive { file_name: String },
}

/// Identifies an archive format from its file name.
pub fn archive_kind(file_name: &str) -> Option<ArchiveKind> {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        Some(ArchiveKind::Zip)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        Some(ArchiveKind::TarGzip)
    } else if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz") || lower.ends_with(".tbz2") {
        Some(ArchiveKind::TarBzip2)
    } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
        Some(ArchiveKind::TarXz)
    } else if lower.ends_with(".tar.zst") || lower.ends_with(".tzst") {
        Some(ArchiveKind::TarZstd)
    } else if lower.ends_with(".tar") {
        Some(ArchiveKind::Tar)
    } else {
        None
    }
}

/// Builds a non-destructive remote extraction command with shell-quoted paths.
pub fn plan_archive_extraction(
    file_name: &str,
    archive_path: &str,
    destination_path: &str,
) -> Result<ArchiveExtractionPlan, ArchiveExtractionError> {
    let kind =
        archive_kind(file_name).ok_or_else(|| ArchiveExtractionError::UnsupportedArchive {
            file_name: file_name.to_string(),
        })?;
    let archive = shell_quote(archive_path);
    let destination = shell_quote(destination_path);
    // Keep extraction non-destructive until SFTP has an archive conflict dialog.
    let command = match kind {
        ArchiveKind::Zip => format!("unzip -nq {archive} -d {destination}"),
        ArchiveKind::Tar => format!("tar -k -xf {archive} -C {destination}"),
        ArchiveKind::TarGzip => format!("tar -k -xzf {archive} -C {destination}"),
        ArchiveKind::TarBzip2 => format!("tar -k -xjf {archive} -C {destination}"),
        ArchiveKind::TarXz => format!("tar -k -xJf {archive} -C {destination}"),
        ArchiveKind::TarZstd => format!("tar -k --zstd -xf {archive} -C {destination}"),
    };

    Ok(ArchiveExtractionPlan { kind, command })
}

/// Quotes one value for use as data in a POSIX-compatible shell command.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_supported_archive_suffixes_case_insensitively() {
        let cases = [
            ("backup.ZIP", ArchiveKind::Zip),
            ("backup.tar", ArchiveKind::Tar),
            ("backup.tar.gz", ArchiveKind::TarGzip),
            ("backup.tgz", ArchiveKind::TarGzip),
            ("backup.tar.bz2", ArchiveKind::TarBzip2),
            ("backup.tbz", ArchiveKind::TarBzip2),
            ("backup.tbz2", ArchiveKind::TarBzip2),
            ("backup.tar.xz", ArchiveKind::TarXz),
            ("backup.txz", ArchiveKind::TarXz),
            ("backup.tar.zst", ArchiveKind::TarZstd),
            ("backup.tzst", ArchiveKind::TarZstd),
        ];

        for (file_name, expected) in cases {
            assert_eq!(archive_kind(file_name), Some(expected), "{file_name}");
        }
    }

    #[test]
    fn builds_commands_for_every_archive_kind() {
        let cases = [
            ("app.zip", "unzip -nq '/tmp/app.zip' -d '/tmp'"),
            ("app.tar", "tar -k -xf '/tmp/app.tar' -C '/tmp'"),
            ("app.tgz", "tar -k -xzf '/tmp/app.tgz' -C '/tmp'"),
            ("app.tbz2", "tar -k -xjf '/tmp/app.tbz2' -C '/tmp'"),
            ("app.txz", "tar -k -xJf '/tmp/app.txz' -C '/tmp'"),
            ("app.tzst", "tar -k --zstd -xf '/tmp/app.tzst' -C '/tmp'"),
        ];

        for (file_name, expected) in cases {
            let archive_path = format!("/tmp/{file_name}");
            let plan = plan_archive_extraction(file_name, &archive_path, "/tmp")
                .expect("archive should be supported");
            assert_eq!(plan.command, expected, "{file_name}");
        }
    }

    #[test]
    fn quotes_spaces_and_single_quotes_in_paths() {
        let plan = plan_archive_extraction(
            "backup.zip",
            "/srv/it's files/backup.zip",
            "/srv/it's files",
        )
        .expect("zip archives should be supported");

        assert_eq!(
            plan.command,
            "unzip -nq '/srv/it'\\''s files/backup.zip' -d '/srv/it'\\''s files'"
        );
    }

    #[test]
    fn rejects_unsupported_files_with_the_file_name() {
        assert_eq!(
            plan_archive_extraction("notes.txt", "/tmp/notes.txt", "/tmp"),
            Err(ArchiveExtractionError::UnsupportedArchive {
                file_name: "notes.txt".to_string(),
            })
        );
    }
}
