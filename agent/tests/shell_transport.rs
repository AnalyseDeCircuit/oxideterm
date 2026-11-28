// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

const SESSION: &str = "0123456789abcdef0123456789abcdef";

struct ShellAgent {
    process: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    root: std::path::PathBuf,
}

impl ShellAgent {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "oxideterm-shell-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let mut process = Command::new(env!("CARGO_BIN_EXE_oxideterm-agent"))
            .args(["--shell-session", SESSION])
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = process.stdin.take().unwrap();
        let mut output = BufReader::new(process.stdout.take().unwrap());
        let mut ready = String::new();
        output.read_line(&mut ready).unwrap();
        assert_eq!(ready, format!("\x1e{SESSION}:READY\n"));
        Self {
            process,
            input,
            output,
            root,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let request = json!({"id": 7, "session": SESSION, "method": method, "params": params});
        writeln!(self.input, "{request}").unwrap();
        self.input.flush().unwrap();
        let mut header = String::new();
        self.output.read_line(&mut header).unwrap();
        let count: usize = header
            .strip_prefix(&format!("\x1e{SESSION}:"))
            .unwrap()
            .trim_end()
            .parse()
            .unwrap();
        let mut bytes = vec![0; count];
        self.output.read_exact(&mut bytes).unwrap();
        let mut newline = [0];
        self.output.read_exact(&mut newline).unwrap();
        assert_eq!(newline, [b'\n']);
        let response: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(response["id"], 7);
        response
    }
}

impl Drop for ShellAgent {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn shell_protocol_uses_inherited_directory_preserves_file_bytes_and_exits() {
    let mut agent = ShellAgent::new();
    // A second directory with the same file name catches accidental cwd substitution.
    fs::create_dir(agent.root.join("container")).unwrap();
    fs::write(agent.root.join("container/config.txt"), b"container value").unwrap();
    fs::write(agent.root.join("config.txt"), b"original\r\n").unwrap();
    let cwd = agent.call("sys/cwd", json!({}));
    assert_eq!(
        fs::canonicalize(cwd["result"].as_str().unwrap()).unwrap(),
        fs::canonicalize(&agent.root).unwrap()
    );
    let project = agent.call("fs/project", json!({"path": "container"}));
    assert_eq!(
        project["result"]["rootPath"],
        fs::canonicalize(agent.root.join("container"))
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );
    let read = agent.call(
        "fs/readFile",
        json!({"path": "config.txt", "encoding": "base64"}),
    );
    assert_eq!(read["result"]["content"], "b3JpZ2luYWwNCg==");
    let hash = read["result"]["hash"].as_str().unwrap();
    let write = agent.call("fs/writeIdeFile", json!({"path": "config.txt", "content": "bmV3Cg==", "encoding": "base64", "expect_hash": hash}));
    assert_eq!(write["result"]["size"], 4);
    assert_eq!(fs::read(agent.root.join("config.txt")).unwrap(), b"new\n");
    assert_eq!(
        fs::read(agent.root.join("container/config.txt")).unwrap(),
        b"container value"
    );
    let conflict = agent.call("fs/writeIdeFile", json!({"path": "config.txt", "content": "b3ZlcndyaXRl", "encoding": "base64", "expect_hash": hash}));
    assert_eq!(conflict["error"]["code"], -4);
    let exclusive = agent.call(
        "fs/createFile",
        json!({"path": "config.txt", "content": "", "encoding": "base64"}),
    );
    assert_eq!(exclusive["error"]["code"], -4);
    assert_eq!(fs::read(agent.root.join("config.txt")).unwrap(), b"new\n");
    let created = agent.call(
        "fs/createFile",
        json!({"path": "a 'quoted; name.txt", "content": "aGkK", "encoding": "base64"}),
    );
    assert_eq!(created["result"]["size"], 3);
    assert_eq!(
        fs::read(agent.root.join("a 'quoted; name.txt")).unwrap(),
        b"hi\n"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
        let target = agent.root.join("config.txt");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        let before = fs::metadata(&target).unwrap();
        symlink("config.txt", agent.root.join("config-link")).unwrap();
        let linked = agent.call(
            "fs/writeIdeFile",
            json!({"path": "config-link", "content": "bGlua2VkCg==", "encoding": "base64"}),
        );
        assert_eq!(linked["result"]["atomic"], true);
        assert!(
            fs::symlink_metadata(agent.root.join("config-link"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&target).unwrap(), b"linked\n");
        let after = fs::metadata(&target).unwrap();
        assert_eq!(
            (after.mode() & 0o777, after.uid(), after.gid()),
            (0o640, before.uid(), before.gid())
        );
        fs::remove_file(&target).unwrap();
        let deleted = agent.call("fs/writeIdeFile", json!({"path": "config.txt", "content": "", "encoding": "base64", "expect_hash": linked["result"]["hash"]}));
        assert_eq!(deleted["error"]["code"], -4);
        assert!(!target.exists());
    }
    let target = "container/空 'binary";
    let original = vec![0xa5; 32768];
    use base64::Engine;
    let codec = base64::engine::general_purpose::STANDARD;
    for finish in ["transfer/abort", "transfer/commit"] {
        agent.call("transfer/begin", json!({"id":SESSION,"path":target}));
        agent.call(
            "transfer/append",
            json!({"id":SESSION,"offset":0,"content":codec.encode(&original)}),
        );
        agent.call(
            "transfer/append",
            json!({"id":SESSION,"offset":32768,"content":"AP8="}),
        );
        assert!(!agent.root.join(target).exists());
        agent.call(finish, json!({"id":SESSION,"size":32770,"overwrite":false}));
        if finish == "transfer/abort" {
            assert!(!agent.root.join(target).exists());
        }
    }
    let mut expected = original;
    expected.extend([0, 255]);
    assert_eq!(fs::read(agent.root.join(target)).unwrap(), expected);
    let tail = agent.call("transfer/read", json!({"path":target,"offset":32768}));
    assert_eq!(tail["result"]["content"], "AP8=");
    assert_eq!(tail["result"]["size"], 32770);
    #[cfg(unix)]
    {
        let link = "download-link";
        std::os::unix::fs::symlink(target, agent.root.join(link)).unwrap();
        let linked = agent.call("transfer/read", json!({"path":link,"offset":0}));
        assert_eq!(
            linked["result"]["content"],
            codec.encode(&expected[..32768])
        );
        assert_eq!(linked["result"]["version"], tail["result"]["version"]);

        let path = agent.root.join(target);
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let replacement = agent.root.join("replacement");
        expected[32768] = 255;
        fs::write(&replacement, &expected).unwrap();
        fs::File::options()
            .write(true)
            .open(&replacement)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        fs::rename(&replacement, &path).unwrap();
        let replaced = agent.call("transfer/read", json!({"path":link,"offset":32768}));
        assert_eq!(replaced["result"]["content"], "//8=");
        assert_ne!(replaced["result"]["version"], tail["result"]["version"]);

        let device = agent.call("transfer/read", json!({"path":"/dev/null","offset":0}));
        assert_eq!(device["error"]["code"], -32602);
    }
    agent.call("transfer/begin", json!({"id":SESSION,"path":target}));
    assert_eq!(
        agent.call(
            "transfer/commit",
            json!({"id":SESSION,"size":0,"overwrite":false})
        )["error"]["code"],
        -4
    );
    assert_eq!(fs::read(agent.root.join(target)).unwrap(), expected);
    #[cfg(unix)]
    let original_metadata = {
        use std::os::unix::fs::{MetadataExt, PermissionsExt, chown};
        let path = agent.root.join(target);
        if path.metadata().unwrap().uid() == 0 {
            chown(&path, Some(1234), Some(2345)).unwrap();
        }
        fs::set_permissions(&path, fs::Permissions::from_mode(0o2750)).unwrap();
        path.metadata().unwrap()
    };
    agent.call(
        "transfer/commit",
        json!({"id":SESSION,"size":0,"overwrite":true}),
    );
    assert_eq!(fs::read(agent.root.join(target)).unwrap(), Vec::<u8>::new());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let saved = fs::metadata(agent.root.join(target)).unwrap();
        assert_eq!(
            (saved.uid(), saved.gid(), saved.mode()),
            (
                original_metadata.uid(),
                original_metadata.gid(),
                original_metadata.mode()
            )
        );
    }
    let empty = agent.call("transfer/read", json!({"path":target,"offset":0}));
    assert_eq!(empty["result"]["size"], 0);
    assert_eq!(empty["result"]["content"], "");
    agent.call("sys/shutdown", json!({}));
    assert!(agent.process.wait().unwrap().success());
}

#[test]
fn wrong_shell_session_cannot_modify_files() {
    let mut agent = ShellAgent::new();
    fs::write(agent.root.join("keep.txt"), b"keep this file").unwrap();
    writeln!(agent.input, "{}", json!({"id": 1, "session": "ffffffffffffffffffffffffffffffff", "method": "fs/remove", "params": {"path": "keep.txt", "recursive": false}})).unwrap();
    agent.input.flush().unwrap();
    assert!(!agent.process.wait().unwrap().success());
    assert_eq!(
        fs::read(agent.root.join("keep.txt")).unwrap(),
        b"keep this file"
    );
}

#[test]
fn binary_file_prefix_bounds_transfer_before_full_read() {
    use base64::Engine;
    let mut agent = ShellAgent::new();
    let path = agent.root.join("large.bin");
    let file = fs::File::create(&path).unwrap();
    file.set_len(50 * 1024 * 1024).unwrap();
    drop(file);
    // Compare the old full-file request and the bounded probe in the same agent and workspace.
    agent.call("sys/ping", json!({}));
    let start = std::time::Instant::now();
    let full = agent.call(
        "fs/readFile",
        json!({"path":"large.bin", "encoding":"base64"}),
    );
    let full_time = start.elapsed();
    assert_eq!(full["result"]["size"], 50 * 1024 * 1024);
    agent.call("sys/ping", json!({}));
    let start = std::time::Instant::now();
    let prefix = agent.call("fs/readFilePrefix", json!({"path":"large.bin"}));
    let prefix_time = start.elapsed();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(prefix["result"].as_str().unwrap())
        .unwrap();
    assert_eq!(decoded, vec![0; 8192]);
    eprintln!(
        "50 MiB binary probe: full={} Base64 bytes / {:?}; prefix={} Base64 bytes / {:?}",
        full["result"]["content"].as_str().unwrap().len(),
        full_time,
        prefix["result"].as_str().unwrap().len(),
        prefix_time
    );
}
