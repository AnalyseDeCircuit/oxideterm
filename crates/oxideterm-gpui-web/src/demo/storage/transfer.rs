use super::*;

/// Only the entries named by this batch can be replaced after a conflict prompt.
pub struct TransferDestination {
    directory: String,
    revision: u64,
    existing: HashMap<String, BTreeMap<String, u64>>,
}

impl TransferDestination {
    pub fn capture<'a>(
        files: &DemoFiles,
        directory: &str,
        names: impl Iterator<Item = &'a str>,
    ) -> Result<Self, String> {
        let revision = files
            .entries
            .get(directory)
            .filter(|e| e.data.is_none())
            .ok_or("ENOTDIR")?
            .revision;
        let existing = names
            .filter_map(|name| {
                let path = DemoFiles::path(directory, name);
                files.entries.contains_key(&path).then(|| {
                    let versions = subtree_versions(files, &path);
                    (path, versions)
                })
            })
            .collect();
        Ok(Self {
            directory: directory.to_owned(),
            revision,
            existing,
        })
    }

    fn check_write(&self, dest: &DemoFiles, to: &str, replace: bool) -> Result<(), String> {
        if Path::new(to).parent() != Some(Path::new(&self.directory))
            || dest.entries.get(&self.directory).map(|e| e.revision) != Some(self.revision)
        {
            return Err("sftp.toast.destination_changed".into());
        }
        if replace {
            if !self
                .existing
                .get(to)
                .is_some_and(|versions| *versions == subtree_versions(dest, to))
            {
                return Err("sftp.toast.destination_changed".into());
            }
        } else if dest.entries.contains_key(to) {
            return Err("sftp.conflict.description".into());
        }
        Ok(())
    }

    pub fn import_tree(
        &self,
        dest: &mut DemoFiles,
        to: &str,
        replace: bool,
        tree: BTreeMap<String, Option<Vec<u8>>>,
    ) -> Result<u64, String> {
        self.check_write(dest, to, replace)?;
        let root = tree.get("").ok_or("EINVAL")?;
        if dest
            .entries
            .get(to)
            .is_some_and(|entry| entry.data.is_none() != root.is_none())
        {
            return Err("EEXIST".into());
        }
        for path in tree.keys().filter(|p| !p.is_empty()) {
            if path.split('/').any(|p| matches!(p, "" | "." | ".."))
                || !tree
                    .get(path.rsplit_once('/').map_or("", |(p, _)| p))
                    .is_some_and(Option::is_none)
            {
                return Err("EINVAL".into());
            }
        }
        if let Some(existing) = self.existing.get(to).filter(|_| replace) {
            for old in existing.keys() {
                dest.entries.remove(old);
            }
        }
        let mut bytes = 0;
        for (relative, data) in tree {
            bytes += data.as_ref().map_or(0, |v| v.len() as u64);
            let path = if relative.is_empty() {
                to.to_owned()
            } else {
                format!("{to}/{relative}")
            };
            dest.put(&path, data);
        }
        Ok(bytes)
    }

    pub fn copy(
        &self,
        source: &Files,
        from: &str,
        destination: &Files,
        to: &str,
        replace: bool,
    ) -> Result<u64, String> {
        if Rc::ptr_eq(source, destination)
            && (Path::new(to).starts_with(from) || Path::new(from).starts_with(to))
        {
            return Err("EINVAL".into());
        }
        {
            let dest = destination.borrow();
            self.check_write(&dest, to, replace)?;
            if let Some(existing) = dest.entries.get(to)
                && source
                    .borrow()
                    .entries
                    .get(from)
                    .is_some_and(|entry| entry.data.is_none() != existing.data.is_none())
            {
                return Err("EEXIST".into());
            }
        }
        let source = source.borrow();
        let root = source.entries.get(from).ok_or("ENOENT")?;
        let mut copied = vec![(to.to_owned(), root.data.clone())];
        if root.data.is_none() {
            let prefix = format!("{from}/");
            copied.extend(
                source
                    .entries
                    .range(prefix.clone()..)
                    .take_while(|(path, _)| path.starts_with(&prefix))
                    .map(|(path, entry)| {
                        (
                            format!("{to}/{}", &path[prefix.len()..]),
                            entry.data.clone(),
                        )
                    }),
            );
        }
        drop(source);
        let mut dest = destination.borrow_mut();
        if let Some(existing) = self.existing.get(to).filter(|_| replace) {
            for old in existing.keys() {
                dest.entries.remove(old);
            }
        }
        let mut size = 0;
        for (path, data) in copied {
            size += data.as_ref().map_or(0, |bytes| bytes.len() as u64);
            dest.put(&path, data);
        }
        Ok(size)
    }
}

fn subtree_versions(files: &DemoFiles, path: &str) -> BTreeMap<String, u64> {
    let Some(root) = files.entries.get(path) else {
        return BTreeMap::new();
    };
    let mut revisions = BTreeMap::from([(path.to_owned(), root.revision)]);
    if root.data.is_none() {
        let prefix = format!("{path}/");
        revisions.extend(
            files
                .entries
                .range(prefix.clone()..)
                .take_while(|(key, _)| key.starts_with(&prefix))
                .map(|(key, e)| (key.clone(), e.revision)),
        );
    }
    revisions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_overwrite_requires_the_reviewed_destination_and_preserves_siblings() {
        let source = DemoFiles::seeded();
        let dest = DemoFiles::seeded();
        source.borrow_mut().put("/home/demo/sub", None);
        source
            .borrow_mut()
            .put("/home/demo/sub/log", Some(vec![0, 255]));
        dest.borrow_mut().put("/srv/oxideterm/sub", None);
        dest.borrow_mut()
            .put("/srv/oxideterm/sub/previous", Some(vec![1]));
        let review =
            || TransferDestination::capture(&dest.borrow(), REMOTE, ["sub"].into_iter()).unwrap();
        let original = review();
        dest.borrow_mut()
            .put("/srv/oxideterm/sub/previous", Some(vec![2]));
        let copy = |t: &TransferDestination, replace| {
            t.copy(
                &source,
                "/home/demo/sub",
                &dest,
                "/srv/oxideterm/sub",
                replace,
            )
        };
        assert!(copy(&original, true).is_err());
        assert_eq!(
            dest.borrow().entries["/srv/oxideterm/sub/previous"].data,
            Some(vec![2])
        );
        let current = review();
        let invalid = BTreeMap::from([
            (String::new(), None),
            ("../escape".to_owned(), Some(vec![8])),
        ]);
        assert!(
            current
                .import_tree(&mut dest.borrow_mut(), "/srv/oxideterm/sub", true, invalid)
                .is_err()
        );
        assert_eq!(
            dest.borrow().entries["/srv/oxideterm/sub/previous"].data,
            Some(vec![2])
        );
        assert!(copy(&current, false).is_err());
        assert_eq!(copy(&current, true).unwrap(), 2);
        let files = dest.borrow();
        assert_eq!(
            files.entries["/srv/oxideterm/sub/log"].data,
            Some(vec![0, 255])
        );
        assert!(!files.entries.contains_key("/srv/oxideterm/sub/previous"));
        assert!(files.entries.contains_key("/srv/oxideterm/README.md"));
    }
}
