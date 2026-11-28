use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    path::{Component, Path},
    rc::Rc,
};

pub const LOCAL: &str = "/home/demo";
#[path = "storage/archive.rs"]
mod archive;
#[cfg(target_family = "wasm")]
pub(crate) use archive::{MAX_BYTES as MAX_ARCHIVE_BYTES, MAX_ENTRIES as MAX_ARCHIVE_ENTRIES, unpack_zip};
#[path = "storage/transfer.rs"]
pub(super) mod transfer;
pub const REMOTE: &str = "/srv/oxideterm";
pub type Files = Rc<RefCell<DemoFiles>>;

#[derive(Clone)]
pub struct Entry {
    pub data: Option<Vec<u8>>,
    pub revision: u64,
    pub created: Option<i64>,
    pub modified: Option<i64>,
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
    pub fn duplicate(&mut self, from: &str, to: &str, cut: bool) -> Result<(), String> {
        if self.entries.contains_key(to) || Path::new(to).starts_with(from) {
            return Err("EEXIST".into());
        }
        let parent = Path::new(to)
            .parent()
            .and_then(Path::to_str)
            .ok_or("EINVAL")?;
        if !self
            .entries
            .get(parent)
            .is_some_and(|entry| entry.data.is_none())
        {
            return Err("ENOTDIR".into());
        }
        if !self.entries.contains_key(from) {
            return Err("ENOENT".into());
        }
        let paths: Vec<_> = self
            .entries
            .keys()
            .filter(|key| Path::new(key).starts_with(from))
            .cloned()
            .collect();
        for path in paths {
            let data = if cut {
                self.entries.remove(&path).unwrap().data
            } else {
                self.entries[&path].data.clone()
            };
            self.put(&format!("{to}{}", &path[from.len()..]), data);
        }
        Ok(())
    }

    pub fn copy_batch_entry(
        &mut self,
        from: &str,
        destination: Option<&str>,
        cut: bool,
    ) -> Result<(), String> {
        let source = Path::new(from);
        let name = source.file_name().ok_or("EINVAL")?;
        let target = destination
            .map(|d| Path::new(d).join(name))
            .unwrap_or_else(|| source.into());
        let available = oxideterm_local_files::unique_copy_path_with(&target, |p| {
            p.to_str().is_some_and(|s| self.entries.contains_key(s))
        });
        self.duplicate(from, available.to_str().ok_or("EINVAL")?, cut)
    }

    pub fn replace_file(
        &mut self,
        path: &str,
        expected_revision: u64,
        data: Vec<u8>,
    ) -> Option<&Entry> {
        let entry = self.entries.get(path)?;
        if entry.revision != expected_revision || entry.data.is_none() {
            return None;
        }
        self.put(path, Some(data));
        self.entries.get(path)
    }

    pub fn put(&mut self, path: &str, data: Option<Vec<u8>>) {
        self.revision += 1;
        let now = web_time::SystemTime::now()
            .duration_since(web_time::SystemTime::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs() as i64);
        let created = self.entries.get(path).and_then(|e| e.created).or(now);
        self.entries.insert(
            path.to_owned(),
            Entry {
                data,
                revision: self.revision,
                created,
                modified: now,
            },
        );
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Drive {
    Node(String),
    Endpoint {
        host: String,
        port: u16,
        username: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub path: String,
}

impl Endpoint {
    pub fn label(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.username.is_empty() {
            format!("{host}:{}", self.port)
        } else {
            format!("{}@{host}:{}", self.username, self.port)
        }
    }

    fn pane(&self) -> (Drive, &str) {
        (
            Drive::Endpoint {
                host: self.host.clone(),
                port: self.port,
                username: self.username.clone(),
            },
            if self.path.is_empty() {
                REMOTE
            } else {
                &self.path
            },
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SftpProfile {
    pub primary: Endpoint,
    pub secondary: Option<Endpoint>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SftpTarget {
    pub id: String,
    pub profile: Option<SftpProfile>,
}

impl SftpTarget {
    pub fn panes(&self) -> [(Drive, &str); 2] {
        let local = (Drive::Node("local".into()), LOCAL);
        match &self.profile {
            Some(SftpProfile {
                primary,
                secondary: Some(second),
            }) => [primary.pane(), second.pane()],
            Some(profile) => [local, profile.primary.pane()],
            None => [local, (Drive::Node(self.id.clone()), REMOTE)],
        }
    }
}

#[derive(Clone, Default)]
pub struct DemoDrives(Rc<RefCell<HashMap<Drive, Files>>>);

impl DemoDrives {
    pub fn files(&self, drive: Drive) -> Files {
        self.0
            .borrow_mut()
            .entry(drive)
            .or_insert_with(DemoFiles::seeded)
            .clone()
    }
}

pub fn copy_file(source: &Files, from: &str, destination: &Files, to: &str) -> Result<u64, String> {
    let data = source
        .borrow()
        .entries
        .get(from)
        .and_then(|e| e.data.clone())
        .ok_or("ENOENT")?;
    let mut dest = destination.borrow_mut();
    dest.list(
        Path::new(to)
            .parent()
            .and_then(Path::to_str)
            .ok_or("EINVAL")?,
    )?;
    if dest.entries.contains_key(to) {
        return Err("EEXIST".into());
    }
    let size = data.len() as u64;
    dest.put(to, Some(data));
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contents(drive: &Files, path: &str) -> Option<Vec<u8>> {
        drive
            .borrow()
            .entries
            .get(path)
            .and_then(|entry| entry.data.clone())
    }

    #[test]
    fn dual_remote_transfer_uses_each_endpoint_and_its_initial_directory() {
        let endpoint = |port, path: &str| Endpoint {
            host: "files.example".into(),
            port,
            username: "ops".into(),
            path: path.into(),
        };
        let target = SftpTarget {
            id: "replica".into(),
            profile: Some(SftpProfile {
                primary: endpoint(22, "/srv/oxideterm"),
                secondary: Some(endpoint(2222, "/home/demo")),
            }),
        };
        let drives = DemoDrives::default();
        let address = |port| Drive::Endpoint {
            host: "files.example".into(),
            port,
            username: "ops".into(),
        };
        let a = drives.files(address(22));
        let b = drives.files(address(2222));
        a.borrow_mut()
            .put("/srv/oxideterm/item", Some(b"from A".to_vec()));
        let [(left, cwd_a), (right, cwd_b)] = target.panes();
        let left = drives.files(left);
        let right = drives.files(right);
        let from = DemoFiles::path(cwd_a, "item");
        let to = DemoFiles::path(cwd_b, "item");
        copy_file(&left, &from, &right, &to).unwrap();
        assert_eq!(
            contents(&b, "/home/demo/item").as_deref(),
            Some(&b"from A"[..])
        );
        assert_eq!(contents(&a, "/home/demo/item"), None);
        b.borrow_mut()
            .put("/home/demo/item", Some(b"from B".to_vec()));
        assert_eq!(copy_file(&right, &to, &left, &from), Err("EEXIST".into()));
        copy_file(&right, &to, &left, &DemoFiles::path(cwd_a, "reply")).unwrap();
        assert_eq!(
            contents(&a, "/srv/oxideterm/reply").as_deref(),
            Some(&b"from B"[..])
        );
        assert_eq!(
            contents(&a, "/srv/oxideterm/item").as_deref(),
            Some(&b"from A"[..])
        );
    }

    #[test]
    fn duplicate_and_move_keep_existing_files_and_their_extensions() {
        let drive = DemoFiles::seeded();
        let mut fs = drive.borrow_mut();
        fs.put("/home/demo/notes (1).txt", Some(b"previous".to_vec()));
        fs.copy_batch_entry("/home/demo/notes.txt", None, false)
            .unwrap();
        assert_eq!(
            fs.entries["/home/demo/notes (2).txt"].data.as_deref(),
            Some(&b"OxideTerm\n"[..])
        );
        assert_eq!(
            fs.entries["/home/demo/notes (1).txt"].data.as_deref(),
            Some(&b"previous"[..])
        );
        fs.put("/srv/oxideterm/notes.txt", Some(b"remote".to_vec()));
        fs.copy_batch_entry("/home/demo/notes.txt", Some(REMOTE), true)
            .unwrap();
        assert!(!fs.entries.contains_key("/home/demo/notes.txt"));
        assert_eq!(
            fs.entries["/srv/oxideterm/notes (1).txt"].data.as_deref(),
            Some(&b"OxideTerm\n"[..])
        );
        assert_eq!(
            fs.entries["/srv/oxideterm/notes.txt"].data.as_deref(),
            Some(&b"remote"[..])
        );
    }

    #[test]
    fn picked_files_replace_only_the_reviewed_file_in_the_original_directory() {
        let file = |data| BTreeMap::from([(String::new(), Some(data))]);
        let drive = DemoFiles::seeded();
        let mut files = drive.borrow_mut();
        let name = "数据.bin";
        let path = "/srv/oxideterm/数据.bin";
        let review = |fs: &DemoFiles| {
            transfer::TransferDestination::capture(fs, REMOTE, [name].into_iter()).unwrap()
        };
        let first = review(&files);
        first
            .import_tree(&mut files, path, false, file(vec![0, 255]))
            .unwrap();
        assert!(
            first
                .import_tree(&mut files, path, true, file(vec![1]))
                .is_err()
        );
        assert_eq!(files.entries[path].data, Some(vec![0, 255]));
        let confirmed = review(&files);
        assert!(
            confirmed
                .import_tree(&mut files, path, false, file(vec![2]))
                .is_err()
        );
        confirmed
            .import_tree(&mut files, path, true, file(vec![7, 8]))
            .unwrap();
        assert!(
            confirmed
                .import_tree(&mut files, path, true, file(vec![3]))
                .is_err()
        );
        assert_eq!(files.entries[path].data, Some(vec![7, 8]));
        files.put(REMOTE, None);
        assert!(
            confirmed
                .import_tree(&mut files, "/srv/oxideterm/late", false, file(vec![4]))
                .is_err()
        );
        assert!(!files.entries.contains_key("/srv/oxideterm/late"));
    }

    #[test]
    fn reopening_a_node_keeps_its_files_without_sharing_them_with_another_node() {
        let drives = DemoDrives::default();
        let open = |id: &str| {
            let target = SftpTarget {
                id: id.into(),
                profile: None,
            };
            let [(left, _), (right, _)] = target.panes();
            (drives.files(left), drives.files(right))
        };
        let (local, first) = open("node-a");
        local
            .borrow_mut()
            .put("/home/demo/config", Some(b"first".to_vec()));
        copy_file(&local, "/home/demo/config", &first, "/srv/config").unwrap();
        drop(first);
        let (local, second) = open("node-b");
        local
            .borrow_mut()
            .put("/home/demo/config", Some(b"second".to_vec()));
        copy_file(&local, "/home/demo/config", &second, "/srv/config").unwrap();
        assert_eq!(
            contents(&open("node-a").1, "/srv/config").as_deref(),
            Some(&b"first"[..])
        );
        assert_eq!(
            contents(&second, "/srv/config").as_deref(),
            Some(&b"second"[..])
        );
        let mut files = second.borrow_mut();
        let opened = files.entries["/srv/config"].revision;
        files
            .replace_file("/srv/config", opened, b"edited in IDE".to_vec())
            .unwrap();
        assert!(
            files
                .replace_file("/srv/config", opened, b"stale SFTP draft".to_vec())
                .is_none()
        );
        assert_eq!(
            files.entries["/srv/config"].data.as_deref(),
            Some(&b"edited in IDE"[..])
        );
    }
}
