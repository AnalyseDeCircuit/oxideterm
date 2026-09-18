use std::time::SystemTime;

use suppaftp::list::ListParser;
use tokio_util::sync::CancellationToken;

use crate::{
    Error, FtpSession, Result,
    session::{COMMAND_TIMEOUT, validate_path},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub permissions: Option<u32>,
}

impl FtpSession {
    pub async fn stat(&mut self, path: &str, cancel: &CancellationToken) -> Result<Option<Entry>> {
        validate_path(path)?;
        let (parent, name) = path
            .rsplit_once('/')
            .map(|(parent, name)| (if parent.is_empty() { "/" } else { parent }, name))
            .unwrap_or((".", path));
        Ok(self
            .list(parent, cancel)
            .await?
            .into_iter()
            .find(|entry| entry.name == name))
    }

    pub async fn delete_recursive(
        &mut self,
        path: &str,
        cancel: &CancellationToken,
    ) -> Result<u64> {
        validate_path(path)?;
        let Some(entry) = self.stat(path, cancel).await? else {
            return Err(Error::Server(550));
        };
        let mut pending = vec![(path.to_owned(), entry.kind, false)];
        let mut removed = 0;
        while let Some((path, kind, visited)) = pending.pop() {
            if kind == EntryKind::Directory && !visited {
                let children = self.list(&path, cancel).await?;
                pending.push((path.clone(), kind, true));
                for child in children {
                    pending.push((
                        format!("{}/{}", path.trim_end_matches('/'), child.name),
                        child.kind,
                        false,
                    ));
                }
            } else {
                self.delete(&path, kind == EntryKind::Directory, cancel)
                    .await?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    pub async fn list(&mut self, path: &str, cancel: &CancellationToken) -> Result<Vec<Entry>> {
        validate_path(path)?;
        let machine = self.machine_listing;
        let (entries, machine) = self
            .operate(cancel, Some(COMMAND_TIMEOUT), |mut stream| async move {
                let (lines, machine) = if machine {
                    match stream.mlsd(Some(path)).await {
                        Ok(lines) => (lines, true),
                        Err(suppaftp::FtpError::UnexpectedResponse(r))
                            if matches!(r.status as u32, 500 | 502 | 504) =>
                        {
                            (stream.list(Some(path)).await?, false)
                        }
                        Err(error) => return Err(error.into()),
                    }
                } else {
                    (stream.list(Some(path)).await?, false)
                };
                let mut entries = Vec::with_capacity(lines.len());
                for line in lines {
                    if let Some(entry) = parse_entry(&line, machine)? {
                        entries.push(entry);
                    }
                }
                Ok((stream, (entries, machine)))
            })
            .await?;
        self.machine_listing = machine;
        Ok(entries)
    }
}

fn parse_entry(line: &str, machine: bool) -> Result<Option<Entry>> {
    if line.trim().is_empty() || (!machine && line.starts_with("total ")) {
        return Ok(None);
    }
    let facts = if machine {
        line.split_once(' ')
            .map(|(facts, _)| facts)
            .ok_or(Error::Listing)?
    } else {
        ""
    };
    if facts
        .split(';')
        .any(|f| f.eq_ignore_ascii_case("type=cdir") || f.eq_ignore_ascii_case("type=pdir"))
    {
        return Ok(None);
    }
    let file = if machine {
        ListParser::parse_mlsd(line)
    } else {
        ListParser::parse_posix(line).or_else(|_| ListParser::parse_dos(line))
    }
    .map_err(|_| Error::Listing)?;
    if matches!(file.name(), "." | "..") {
        return Ok(None);
    }
    validate_entry_name(file.name())?;
    let fact = |key: &str| {
        facts.split(';').find_map(|part| {
            part.split_once('=')
                .filter(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v)
        })
    };
    let kind = if file.is_symlink() {
        EntryKind::Symlink
    } else if file.is_directory() {
        EntryKind::Directory
    } else {
        EntryKind::File
    };
    let posix = !machine && matches!(line.as_bytes().first(), Some(b'-' | b'd' | b'l'));
    let permissions = if machine {
        fact("UNIX.mode").and_then(|v| u32::from_str_radix(v, 8).ok())
    } else if posix {
        use suppaftp::list::PosixPexQuery;
        let mut mode = 0;
        for (who, shift) in [
            (PosixPexQuery::Owner, 6),
            (PosixPexQuery::Group, 3),
            (PosixPexQuery::Others, 0),
        ] {
            let bits = (u32::from(file.can_read(who)) << 2)
                | (u32::from(file.can_write(who)) << 1)
                | u32::from(file.can_execute(who));
            mode |= bits << shift;
        }
        Some(mode)
    } else {
        None
    };
    Ok(Some(Entry {
        name: file.name().to_owned(),
        kind,
        size: (!machine || fact("size").is_some()).then_some(file.size() as u64),
        modified: (!machine || fact("modify").is_some()).then_some(file.modified()),
        permissions,
    }))
}

pub(crate) fn validate_entry_name(name: &str) -> Result<()> {
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.contains(['/', '\\', ':', '\0', '\r', '\n'])
    {
        Err(Error::InvalidInput)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listings_preserve_unknown_metadata_and_reject_path_escape() {
        let entry = parse_entry("type=file; report.md", true).unwrap().unwrap();
        assert_eq!(entry.name, "report.md");
        assert_eq!(
            (entry.kind, entry.size, entry.modified, entry.permissions),
            (EntryKind::File, None, None, None)
        );
        let entry = parse_entry(
            "type=file;size=123;modify=20260101000000;UNIX.mode=0640; report.md",
            true,
        )
        .unwrap()
        .unwrap();
        assert_eq!((entry.size, entry.permissions), (Some(123), Some(0o640)));
        for line in [
            "type=file; ../escape",
            "type=file; C:escape",
            "type=file; x\\y",
        ] {
            assert!(matches!(parse_entry(line, true), Err(Error::InvalidInput)));
        }
        let entry = parse_entry("01-01-26  12:00PM       <DIR>          notes", false)
            .unwrap()
            .unwrap();
        assert_eq!(
            (entry.name.as_str(), entry.kind, entry.permissions),
            ("notes", EntryKind::Directory, None)
        );
    }
}
