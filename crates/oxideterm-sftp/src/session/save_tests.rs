use super::*;
use russh_sftp::{
    protocol::{Attrs, Data, File, Handle, Name, Packet, Status, StatusCode, Version},
    server::Handler,
};
use std::sync::Mutex;

#[derive(Default)]
struct Files {
    bytes: HashMap<String, Vec<u8>>,
    atomic: bool,
    reject_install: bool,
    reject_restore: bool,
    change_during_upload: bool,
    replacements: usize,
    removed: Vec<String>,
}

struct Server {
    files: Arc<Mutex<Files>>,
    listed: bool,
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: String::new(),
        language_tag: String::new(),
    }
}

impl Handler for Server {
    type Error = StatusCode;
    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }
    async fn init(&mut self, _: u32, _: HashMap<String, String>) -> Result<Version, StatusCode> {
        let mut version = Version::new();
        if self.files.lock().unwrap().atomic {
            version
                .extensions
                .insert("posix-rename@openssh.com".into(), "1".into());
        }
        Ok(version)
    }
    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, StatusCode> {
        let path = if path == "." { "/".into() } else { path };
        Ok(Name {
            id,
            files: vec![File::dummy(path)],
        })
    }
    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        let state = self.files.lock().unwrap();
        let data = state.bytes.get(&path).ok_or(StatusCode::NoSuchFile)?;
        Ok(Attrs {
            id,
            attrs: FileAttributes {
                size: Some(data.len() as u64),
                mtime: Some(1),
                permissions: Some(0o100600),
                ..Default::default()
            },
        })
    }
    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        self.stat(id, path).await
    }
    async fn open(
        &mut self,
        id: u32,
        path: String,
        flags: OpenFlags,
        _: FileAttributes,
    ) -> Result<Handle, StatusCode> {
        let mut state = self.files.lock().unwrap();
        if flags.contains(OpenFlags::CREATE) {
            if flags.contains(OpenFlags::EXCLUDE) && state.bytes.contains_key(&path) {
                return Err(StatusCode::Failure);
            }
            state.bytes.entry(path.clone()).or_default();
        }
        if !state.bytes.contains_key(&path) {
            return Err(StatusCode::NoSuchFile);
        }
        Ok(Handle { id, handle: path })
    }
    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, StatusCode> {
        let state = self.files.lock().unwrap();
        let bytes = state.bytes.get(&handle).ok_or(StatusCode::NoSuchFile)?;
        let start = offset as usize;
        if start >= bytes.len() {
            return Err(StatusCode::Eof);
        }
        Ok(Data {
            id,
            data: bytes[start..bytes.len().min(start + len as usize)]
                .to_vec()
                .into(),
        })
    }
    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, StatusCode> {
        let mut state = self.files.lock().unwrap();
        let bytes = state.bytes.get_mut(&handle).ok_or(StatusCode::NoSuchFile)?;
        let start = offset as usize;
        bytes.resize(bytes.len().max(start + data.len()), 0);
        bytes[start..start + data.len()].copy_from_slice(&data);
        if state.change_during_upload {
            state.bytes.insert("/file.txt".into(), b"bbbb".to_vec());
            state.change_during_upload = false;
        }
        Ok(ok(id))
    }
    async fn close(&mut self, id: u32, _: String) -> Result<Status, StatusCode> {
        Ok(ok(id))
    }
    async fn setstat(
        &mut self,
        id: u32,
        _: String,
        _: FileAttributes,
    ) -> Result<Status, StatusCode> {
        Ok(ok(id))
    }
    async fn remove(&mut self, id: u32, path: String) -> Result<Status, StatusCode> {
        let mut state = self.files.lock().unwrap();
        state.bytes.remove(&path).ok_or(StatusCode::NoSuchFile)?;
        state.removed.push(path);
        Ok(ok(id))
    }
    async fn rename(
        &mut self,
        id: u32,
        source: String,
        target: String,
    ) -> Result<Status, StatusCode> {
        let mut state = self.files.lock().unwrap();
        if (state.reject_install && source.ends_with(".tmp"))
            || (state.reject_restore && source.ends_with(".bak"))
        {
            return Err(StatusCode::Failure);
        }
        if state.bytes.contains_key(&target) {
            return Err(StatusCode::Failure);
        }
        let bytes = state.bytes.remove(&source).ok_or(StatusCode::NoSuchFile)?;
        state.bytes.insert(target, bytes);
        Ok(ok(id))
    }
    async fn extended(
        &mut self,
        id: u32,
        request: String,
        data: Vec<u8>,
    ) -> Result<Packet, StatusCode> {
        if request != "posix-rename@openssh.com" {
            return Err(StatusCode::OpUnsupported);
        }
        let paths: russh_sftp::extensions::HardlinkExtension =
            russh_sftp::de::from_bytes(&mut data.into()).unwrap();
        let mut state = self.files.lock().unwrap();
        let bytes = state
            .bytes
            .remove(&paths.oldpath)
            .ok_or(StatusCode::NoSuchFile)?;
        state.bytes.insert(paths.newpath, bytes);
        state.replacements += 1;
        Ok(ok(id).into())
    }
    async fn opendir(&mut self, id: u32, _: String) -> Result<Handle, StatusCode> {
        self.listed = false;
        Ok(Handle {
            id,
            handle: "directory".into(),
        })
    }
    async fn readdir(&mut self, id: u32, _: String) -> Result<Name, StatusCode> {
        if self.listed {
            return Err(StatusCode::Eof);
        }
        self.listed = true;
        let state = self.files.lock().unwrap();
        Ok(Name {
            id,
            files: state
                .bytes
                .iter()
                .map(|(path, bytes)| {
                    File::new(
                        path.trim_start_matches('/'),
                        FileAttributes {
                            size: Some(bytes.len() as u64),
                            permissions: Some(0o100600),
                            ..Default::default()
                        },
                    )
                })
                .collect(),
        })
    }
}

async fn session(files: Files) -> (SftpSession, Arc<Mutex<Files>>) {
    let files = Arc::new(Mutex::new(files));
    let (client, server) = tokio::io::duplex(128 * 1024);
    tokio::spawn(russh_sftp::server::run(
        server,
        Server {
            files: files.clone(),
            listed: false,
        },
    ));
    let session = SftpSession {
        audit: None,
        sftp: Arc::new(RusshSftpSession::new(client).await.unwrap()),
        channel_factory: Arc::new(|| {
            Box::pin(async { panic!("test must use its owned SFTP channel") })
        }),
        _connection_owner: None,
        single_channel_transport: true,
        session_id: "save-test".into(),
        home: "/".into(),
        cwd: "/".into(),
    };
    (session, files)
}

#[tokio::test]
async fn replacement_is_atomic_only_with_the_extension_and_keeps_fallback_backup() {
    for atomic in [true, false] {
        let (sftp, state) = session(Files {
            atomic,
            bytes: HashMap::from([("/file.txt".into(), b"aaaa".to_vec())]),
            ..Default::default()
        })
        .await;
        let saved = sftp.write_content("/file.txt", b"new").await.unwrap();
        let state = state.lock().unwrap();
        assert_eq!(state.bytes["/file.txt"], b"new");
        assert_eq!(saved.atomic_write, atomic);
        assert_eq!(state.replacements, usize::from(atomic));
        assert!(!state.removed.iter().any(|path| path == "/file.txt"));
        if atomic {
            assert!(saved.backup_path.is_none());
        } else {
            assert_eq!(state.bytes[saved.backup_path.as_ref().unwrap()], b"aaaa");
        }
    }
}

#[tokio::test]
async fn failed_install_restores_the_original_or_leaves_it_at_the_reported_backup() {
    for reject_restore in [false, true] {
        let (sftp, state) = session(Files {
            reject_install: true,
            reject_restore,
            bytes: HashMap::from([("/file.txt".into(), b"aaaa".to_vec())]),
            ..Default::default()
        })
        .await;
        let error = sftp
            .write_content("/file.txt", b"new")
            .await
            .err()
            .expect("install must fail");
        let state = state.lock().unwrap();
        if reject_restore {
            let (path, bytes) = state
                .bytes
                .iter()
                .find(|(path, _)| path.ends_with(".bak"))
                .unwrap();
            assert_eq!(bytes, b"aaaa");
            assert!(error.to_string().contains(path));
        } else {
            assert_eq!(state.bytes["/file.txt"], b"aaaa");
        }
    }
}

#[tokio::test]
async fn editors_detect_equal_size_equal_time_changes_even_during_upload() {
    let (sftp, state) = session(Files {
        atomic: true,
        bytes: HashMap::from([("/file.txt".into(), b"aaaa".to_vec())]),
        ..Default::default()
    })
    .await;
    let PreviewContent::Text { version, data, .. } = sftp.preview("/file.txt").await.unwrap()
    else {
        panic!("text preview");
    };
    assert_eq!(data, "aaaa");
    let version = version.unwrap();
    state.lock().unwrap().change_during_upload = true;
    let error = sftp
        .write_content_checked("/file.txt", b"cccc", Some(&version))
        .await
        .err()
        .expect("must detect the external edit");
    assert!(matches!(error, SftpError::Conflict(_)));
    assert_eq!(state.lock().unwrap().bytes["/file.txt"], b"bbbb");
    let current = sftp.file_content_version("/file.txt").await.unwrap();
    let saved = sftp
        .write_content_checked("/file.txt", b"cccc", Some(&current))
        .await
        .unwrap();
    assert_eq!(state.lock().unwrap().bytes["/file.txt"], b"cccc");
    assert_ne!(saved.version, current);
}

#[tokio::test]
async fn identical_display_names_keep_the_selected_file_identity_for_all_operations() {
    let (sftp, state) = session(Files {
        bytes: HashMap::from([
            ("/\0ff.log".into(), b"first".to_vec()),
            ("/\0fe.log".into(), b"second".to_vec()),
        ]),
        ..Default::default()
    })
    .await;
    let entries = sftp.list_dir("/", None).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .all(|file| crate::display_remote_path(&file.name) == "�.log")
    );
    let chosen = entries
        .iter()
        .find(|file| file.path == "/\0fe.log")
        .unwrap();
    assert_eq!(sftp.read_file_bytes(&chosen.path).await.unwrap(), b"second");
    let directory = tempfile::tempdir().unwrap();
    let download = directory.path().join("chosen.log");
    sftp.download_file(
        &chosen.path,
        download.to_str().unwrap(),
        "raw-name",
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(download).unwrap(), b"second");
    sftp.rename(&chosen.path, "/renamed.log").await.unwrap();
    sftp.delete("/renamed.log").await.unwrap();
    let state = state.lock().unwrap();
    assert_eq!(state.bytes["/\0ff.log"], b"first");
    assert!(!state.bytes.contains_key("/\0fe.log"));
    assert!(!state.bytes.contains_key("/renamed.log"));
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires a local OpenSSH sftp-server executable; uses only a temporary directory"]
async fn openssh_preserves_raw_names_and_atomically_replaces_the_selected_file() {
    use std::os::unix::ffi::OsStringExt;
    let server = ["/usr/libexec/sftp-server", "/usr/lib/openssh/sftp-server"]
        .into_iter()
        .find(|path| Path::new(path).is_file())
        .expect("OpenSSH sftp-server executable required");
    let directory = tempfile::tempdir().unwrap();
    let first = directory
        .path()
        .join(std::ffi::OsString::from_vec(b"\xff.log".to_vec()));
    let second = directory
        .path()
        .join(std::ffi::OsString::from_vec(b"\xfe.log".to_vec()));
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let mut process = tokio::process::Command::new(server)
        .current_dir(directory.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = process.stdin.take().unwrap();
    let mut stdout = process.stdout.take().unwrap();
    let (client, proxy) = tokio::io::duplex(128 * 1024);
    let bridge = tokio::spawn(async move {
        let (mut read, mut write) = tokio::io::split(proxy);
        let _ = tokio::join!(
            tokio::io::copy(&mut read, &mut stdin),
            tokio::io::copy(&mut stdout, &mut write)
        );
    });
    let session = SftpSession {
        audit: None,
        sftp: Arc::new(RusshSftpSession::new(client).await.unwrap()),
        channel_factory: Arc::new(|| {
            Box::pin(async { panic!("interoperability test uses its process transport") })
        }),
        _connection_owner: None,
        single_channel_transport: true,
        session_id: "openssh-fixture".into(),
        home: directory.path().to_str().unwrap().into(),
        cwd: directory.path().to_str().unwrap().into(),
    };
    let entries = session.list_dir("", None).await.unwrap();
    assert_eq!(
        entries.iter().filter(|file| file.name == "�.log").count(),
        2
    );
    let path = &entries
        .iter()
        .find(|entry| entry.path.ends_with("\0fe.log"))
        .unwrap()
        .path;
    assert_eq!(session.read_file_bytes(path).await.unwrap(), b"second");
    let target = directory.path().join("download.log");
    session
        .download_file(
            path,
            target.to_str().unwrap(),
            "openssh-download",
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(target).unwrap(), b"second");
    let PreviewContent::Text { version, .. } = session.preview(path).await.unwrap() else {
        panic!("text snapshot");
    };
    let written = session
        .write_content_checked(path, b"edited", version.as_deref())
        .await
        .unwrap();
    assert!(written.atomic_write);
    assert_eq!(std::fs::read(&second).unwrap(), b"edited");
    let renamed = directory
        .path()
        .join("renamed.log")
        .to_str()
        .unwrap()
        .to_owned();
    session.rename(path, &renamed).await.unwrap();
    session.delete(&renamed).await.unwrap();
    assert_eq!(std::fs::read(first).unwrap(), b"first");
    assert!(!second.exists());
    session.sftp.close().await.unwrap();
    bridge.abort();
    process.kill().await.ok();
    process.wait().await.unwrap();
}
