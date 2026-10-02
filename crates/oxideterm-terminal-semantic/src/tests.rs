// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    SemanticClass, SemanticLineRole, SemanticScheme, classify_line,
    classify_line_with_compiled_scheme, classify_line_with_scheme, compiled_builtin_scheme,
    semantic_line_emphasis, semantic_output_role_for_command,
};
#[cfg(feature = "shell-syntax")]
use crate::{SemanticShellDialect, classify_line_with_compiled_scheme_and_shell};

fn matched_texts(text: &str) -> Vec<(&str, SemanticClass)> {
    classify_line(text, SemanticLineRole::Output)
        .into_iter()
        .map(|span| (&text[span.range], span.class))
        .collect()
}

#[test]
fn uuid_identifiers_are_atomic_and_keep_container_precedence() {
    let uuid = "01a0ad3f-3b2d-7c72-9518-8ff3f1cb012a";
    let uppercase = uuid.to_uppercase();
    for (text, expected) in [
        (format!("Session ID: {uuid}"), uuid),
        (format!("会话：{{{uuid}}}"), uuid),
        (uppercase.clone(), uppercase.as_str()),
    ] {
        for scheme in [SemanticScheme::Balanced, SemanticScheme::Conservative] {
            let spans = classify_line_with_compiled_scheme(
                &text,
                SemanticLineRole::Output,
                compiled_builtin_scheme(scheme),
            );
            let identifiers: Vec<_> = spans
                .iter()
                .filter(|span| span.class == SemanticClass::Variable)
                .map(|span| &text[span.range.clone()])
                .collect();
            assert_eq!(identifiers, [expected]);
            assert!(!spans.iter().any(|span| span.class == SemanticClass::Number));
        }
    }
    for text in [
        format!("x{uuid}"),
        format!("{uuid}0"),
        "01a0ad3f-3b2d-7c72-9518-8ff3f1cb012g".into(),
        "01a0ad3f-3b2d-7c72-9518-8ff3f1cb012".into(),
    ] {
        assert!(
            !matched_texts(&text)
                .iter()
                .any(|(_, class)| *class == SemanticClass::Variable)
        );
    }
    for (text, expected) in [
        (format!("https://example.test/{uuid}"), SemanticClass::Link),
        (format!("/tmp/{uuid}"), SemanticClass::Path),
        (format!("\"{uuid}\""), SemanticClass::String),
    ] {
        assert_eq!(matched_texts(&text), [(text.as_str(), expected)]);
    }
}

#[test]
fn hex_digests_are_atomic_without_overriding_paths_or_urls() {
    for digest in [
        "d41d8cd98f00b204e9800998ecf8427e",
        "da39a3ee5e6b4b0d3255bfef95601890afd80709",
        "d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f",
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7",
        concat!(
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a",
            "2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        ),
    ] {
        for text in [digest.to_owned(), digest.to_uppercase()] {
            for scheme in [SemanticScheme::Balanced, SemanticScheme::Conservative] {
                let spans = classify_line_with_scheme(&text, SemanticLineRole::Output, scheme);
                let matches: Vec<_> = spans
                    .into_iter()
                    .map(|span| (&text[span.range], span.class))
                    .collect();
                assert_eq!(matches, [(text.as_str(), SemanticClass::Variable)]);
            }
        }
    }
    let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    for text in [
        format!("sha256:{digest}"),
        format!("校验：{digest}  archive.tar"),
    ] {
        assert!(matched_texts(&text).contains(&(digest, SemanticClass::Variable)));
    }
    for text in [
        "abcdef",
        "deadbeef",
        "1234567890",
        "d41d8cd98f00b204e9800998ecf8427g",
        "d41d8cd98f00b204e9800998ecf8427e0",
        "prefix_d41d8cd98f00b204e9800998ecf8427e",
    ] {
        assert!(
            !matched_texts(text)
                .iter()
                .any(|(_, class)| *class == SemanticClass::Variable),
            "{text}"
        );
    }
    for (text, class) in [
        (
            format!("https://example.test/{digest}"),
            SemanticClass::Link,
        ),
        (format!("/tmp/{digest}"), SemanticClass::Path),
        (format!("\"{digest}\""), SemanticClass::String),
    ] {
        assert_eq!(matched_texts(&text), [(text.as_str(), class)]);
    }
}

#[test]
fn abbreviated_hashes_require_command_context() {
    for (command, text, expected) in [
        ("git log --oneline", "a1b2c3d fix parsing", vec!["a1b2c3d"]),
        (
            "git log --graph --oneline",
            "| * a1b2c3d (HEAD -> main) fix parsing",
            vec!["a1b2c3d"],
        ),
        (
            "git show --abbrev-commit",
            "commit a1b2c3d",
            vec!["a1b2c3d"],
        ),
        (
            "git diff",
            "index a1b2c3d..d4e5f60 100644",
            vec!["a1b2c3d", "d4e5f60"],
        ),
        (
            "docker ps",
            "d7a8f90b1234 nginx Up 5 minutes web",
            vec!["d7a8f90b1234"],
        ),
        (
            "podman ps",
            "d7a8f90b1234 nginx Up 5 minutes web",
            vec!["d7a8f90b1234"],
        ),
        ("git log", "    a1b2c3d is mentioned in the message", vec![]),
        (
            "git log --graph",
            "|     deadbeef is mentioned in the message",
            vec![],
        ),
        ("git diff", "+a1b2c3d", vec![]),
        ("docker logs web", "d7a8f90b1234 processing", vec![]),
        ("printf text", "a1b2c3d fix parsing", vec![]),
    ] {
        let matches: Vec<_> = classify_line(text, semantic_output_role_for_command(command))
            .into_iter()
            .filter(|span| span.class == SemanticClass::Variable)
            .map(|span| &text[span.range])
            .collect();
        assert_eq!(matches, expected, "{command}: {text}");
    }
}

#[test]
fn structured_output_variants_preserve_column_identity() {
    for (command, text, expected) in [
        (
            "ls -ln",
            "-rw-r--r-- 1 1000 100 42 Sep 4 12:30 file",
            vec![
                ("1000", SemanticClass::Variable),
                ("100", SemanticClass::Variable),
            ],
        ),
        (
            "ls -lo",
            "-rw-r--r-- 1 alice 42 Sep 4 12:30 file",
            vec![
                ("alice", SemanticClass::Variable),
                ("42", SemanticClass::Number),
            ],
        ),
        (
            "ls -lg",
            "-rw-r--r-- 1 staff 42 Sep 4 12:30 file",
            vec![
                ("staff", SemanticClass::Variable),
                ("42", SemanticClass::Number),
            ],
        ),
        (
            "ls -log",
            "-rw-r--r-- 1 42 Sep 4 12:30 file",
            vec![("42", SemanticClass::Number)],
        ),
        (
            "ls -l",
            "crw-rw---- 1 root tty 4, 64 Sep 4 12:30 ttyS0",
            vec![
                ("4, 64", SemanticClass::Number),
                ("tty", SemanticClass::Variable),
            ],
        ),
        (
            "ls -l /dev/null",
            "crw-rw-rw- 1 root wheel 0x3000002 Sep 5 11:19 /dev/null",
            vec![
                ("0x3000002", SemanticClass::Number),
                ("wheel", SemanticClass::Variable),
            ],
        ),
        (
            "df -hT",
            "/dev/sda1 ext4 100G 90G 10G 90% /media/my disk",
            vec![
                ("ext4", SemanticClass::Keyword),
                ("90%", SemanticClass::Error),
                ("/media/my disk", SemanticClass::Path),
            ],
        ),
        (
            "stat /bin/ls",
            "16777232 100 -rwxr-xr-x 1 root wheel 0 154208 \"Sep 4 12:30:00 2026\" 4096 80 /bin/ls",
            vec![
                ("root", SemanticClass::Variable),
                ("wheel", SemanticClass::Variable),
                ("154208", SemanticClass::Number),
            ],
        ),
        (
            "df -h",
            "/dev/disk1s1 100Gi 10Gi 90Gi 10% 459k 4.3G 1% /Volumes/My Disk",
            vec![
                ("100Gi", SemanticClass::Number),
                ("/Volumes/My Disk", SemanticClass::Path),
            ],
        ),
        (
            "free -h",
            "Mem: 15Gi 8.1Gi 2.0Gi 100Mi 4.9Gi 6.9Gi",
            vec![
                ("15Gi", SemanticClass::Number),
                ("100Mi", SemanticClass::Number),
            ],
        ),
        (
            "getfacl file",
            "default:user:deploy:rwx #effective:r-x",
            vec![
                ("deploy", SemanticClass::Variable),
                ("default", SemanticClass::Keyword),
            ],
        ),
    ] {
        let matches = classify_line_with_compiled_scheme(
            text,
            semantic_output_role_for_command(command),
            compiled_builtin_scheme(SemanticScheme::Balanced),
        )
        .into_iter()
        .map(|span| (&text[span.range], span.class))
        .collect::<Vec<_>>();
        for expected in expected {
            assert!(
                matches.contains(&expected),
                "{command}: missing {expected:?} in {matches:?}"
            );
        }
    }
}

#[test]
fn acl_and_network_fields_reject_misleading_values() {
    for text in [
        "garbage:deploy:rwx",
        "mask:deploy:rwx",
        "user:deploy:xwr",
        "user:deploy:rws",
    ] {
        assert!(
            classify_line(text, SemanticLineRole::FileAclOutput)
                .iter()
                .all(|span| !matches!(
                    span.class,
                    SemanticClass::PermissionRead
                        | SemanticClass::PermissionWrite
                        | SemanticClass::PermissionExecute
                        | SemanticClass::PermissionSpecial
                )),
            "{text}"
        );
    }
    let acl = "user:deploy:rwx #effective:r--";
    let spans = classify_line(acl, SemanticLineRole::FileAclOutput);
    let effective_start = acl.find("#effective:").unwrap() + "#effective:".len();
    assert!(
        spans
            .iter()
            .any(|span| span.range == (effective_start..effective_start + 1)
                && span.class == SemanticClass::PermissionRead)
    );
    for percent in ["0.5", "50.5", "100.0"] {
        let text = format!("1000 packets transmitted, {percent}% packet loss");
        let expected = if percent == "0.5" {
            SemanticClass::Warning
        } else {
            SemanticClass::Error
        };
        assert!(
            classify_line(&text, SemanticLineRole::PingOutput)
                .iter()
                .any(|span| span.class == expected
                    && &text[span.range.clone()] == format!("{percent}%"))
        );
    }
    for text in ["default dev UP", "2: UP: mtu 1500 state DOWN"] {
        assert!(
            classify_line(text, SemanticLineRole::IpOutput)
                .iter()
                .all(|span| !(span.class == SemanticClass::Success
                    && &text[span.range.clone()] == "UP"))
        );
    }
    let socket = "tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:*";
    assert!(
        classify_line(socket, SemanticLineRole::SocketOutput)
            .iter()
            .any(
                |span| span.class == SemanticClass::Info && &socket[span.range.clone()] == "LISTEN"
            )
    );
}

#[test]
fn unix_permission_fields_use_distinct_semantic_classes() {
    let text = "-rwsr-xr--+ 1 alice developers 16384 app";
    let permission_matches = classify_line_with_compiled_scheme(
        text,
        SemanticLineRole::Output,
        compiled_builtin_scheme(SemanticScheme::Balanced),
    )
    .into_iter()
    .filter(|span| {
        matches!(
            span.class,
            SemanticClass::PermissionRead
                | SemanticClass::PermissionWrite
                | SemanticClass::PermissionExecute
                | SemanticClass::PermissionSpecial
        )
    })
    .map(|span| (&text[span.range], span.class))
    .collect::<Vec<_>>();

    assert_eq!(
        permission_matches,
        vec![
            ("r", SemanticClass::PermissionRead),
            ("w", SemanticClass::PermissionWrite),
            ("s", SemanticClass::PermissionSpecial),
            ("r", SemanticClass::PermissionRead),
            ("x", SemanticClass::PermissionExecute),
            ("r", SemanticClass::PermissionRead),
        ]
    );

    for (candidate, role) in [
        ("-rwxr-xr-x ./script", SemanticLineRole::Command),
        ("rwxrwxrwx is ordinary text", SemanticLineRole::Output),
        ("-rwxr-xr-x-not-a-field", SemanticLineRole::Output),
    ] {
        assert!(
            classify_line(candidate, role).iter().all(|span| !matches!(
                span.class,
                SemanticClass::PermissionRead
                    | SemanticClass::PermissionWrite
                    | SemanticClass::PermissionExecute
                    | SemanticClass::PermissionSpecial
            )),
            "permission-like text was classified in {candidate:?}"
        );
    }
}

#[test]
fn command_output_roles_follow_executable_arguments_and_prefixes() {
    use SemanticLineRole::*;
    for (command, expected) in [
        ("ls -lah /var/log", FileListingOutput),
        ("stat /var/log/app", FileStatOutput),
        ("getfacl /srv/app", FileAclOutput),
        ("df -h", DiskUsageOutput),
        ("free -h", MemoryUsageOutput),
        ("ip address show", IpOutput),
        ("ss -lnt", SocketOutput),
        ("ping 1.1.1.1", PingOutput),
        ("cargo check", RustToolOutput),
        ("env RUSTFLAGS=-Dwarnings rustc src/main.rs", RustToolOutput),
        ("RUSTFLAGS=-Dwarnings cargo check", RustToolOutput),
        ("clang++ src/main.cc", CCompilerOutput),
        ("aarch64-linux-gnu-gcc src/main.c", CCompilerOutput),
        ("git -C workspace status --short", GitStatusOutput),
        ("git diff --cached", GitDiffOutput),
        ("sudo systemctl status sshd", SystemdOutput),
        ("journalctl -u sshd", SystemdOutput),
        ("cargo test", RustToolOutput),
        ("pytest -q", TestOutput),
        ("go test ./...", TestOutput),
        ("npm test", TestOutput),
        ("docker ps", ContainerOutput),
        ("podman container ls", ContainerOutput),
        ("kubectl get pods", ContainerOutput),
        ("docker logs api", Output),
    ] {
        assert_eq!(
            semantic_output_role_for_command(command),
            expected,
            "{command:?}"
        );
    }
}

#[test]
fn structured_output_fields_keep_their_command_specific_meaning() {
    use SemanticClass::*;
    use SemanticLineRole::*;
    let cases: &[(SemanticLineRole, &str, &[(&str, SemanticClass)])] = &[
        (
            FileListingOutput,
            "lrwxr-xr-x@ 1 alice staff 11K Sep 4 12:30 current -> /opt/app",
            &[
                ("l", Keyword),
                ("alice", Variable),
                ("staff", Variable),
                ("11K", Number),
                ("->", Operator),
            ],
        ),
        (
            FileStatOutput,
            "Access: (4755/-rwsr-xr-x) Uid: ( 1000/ alice) Gid: ( 100/ staff)",
            &[("Access", Keyword), ("s", PermissionSpecial)],
        ),
        (
            FileAclOutput,
            "user:deploy:rwx",
            &[
                ("user", Keyword),
                ("deploy", Variable),
                ("x", PermissionExecute),
            ],
        ),
        (
            DiskUsageOutput,
            "/dev/nvme0n1p2 100G 91G 9G 91% /",
            &[
                ("/dev/nvme0n1p2", Path),
                ("100G", Number),
                ("91%", Error),
                ("/", Path),
            ],
        ),
        (
            MemoryUsageOutput,
            "Mem: 15GiB 8GiB 2GiB 1GiB 5GiB 6GiB",
            &[("Mem", Keyword), ("15GiB", Number)],
        ),
        (
            IpOutput,
            "2: eth0: <BROADCAST,MULTICAST,UP> mtu 1500 state UP qlen 1000",
            &[("eth0", Variable), ("mtu", Keyword), ("UP", Success)],
        ),
        (
            IpOutput,
            "default via 192.168.1.1 dev eth0 src 192.168.1.20 metric 100",
            &[("192.168.1.1", Address), ("eth0", Variable)],
        ),
        (
            SocketOutput,
            "LISTEN 0 4096 0.0.0.0:22 0.0.0.0:*",
            &[("LISTEN", Info), ("0.0.0.0:22", Address)],
        ),
        (
            PingOutput,
            "64 bytes from 1.1.1.1: icmp_seq=1 ttl=57 time=12.4 ms",
            &[
                ("1.1.1.1", Address),
                ("icmp_seq", Variable),
                ("12.4", Number),
            ],
        ),
        (
            PingOutput,
            "4 packets transmitted, 3 received, 25% packet loss",
            &[("25%", Warning)],
        ),
        (
            RustToolOutput,
            "  --> crates/app/src/main.rs:42:17",
            &[
                ("-->", Operator),
                ("crates/app/src/main.rs", Path),
                ("42", Number),
                ("17", Number),
            ],
        ),
        (
            CCompilerOutput,
            "/tmp/main.c:12:7: error: use of undeclared identifier 'value'",
            &[
                ("/tmp/main.c", Path),
                ("12", Number),
                ("7", Number),
                ("error", Error),
            ],
        ),
        (
            RustToolOutput,
            "   Finished `dev` profile in 0.42s",
            &[("Finished", Success)],
        ),
        (
            GitStatusOutput,
            " M src/main.rs",
            &[(" M", Warning), ("src/main.rs", Path)],
        ),
        (
            SystemdOutput,
            "     Active: failed (Result: exit-code)",
            &[("Active", Keyword), ("failed", Error)],
        ),
        (
            SystemdOutput,
            "Sep  4 10:30:00 host sshd[42]: connection closed",
            &[("Sep  4 10:30:00", Timestamp)],
        ),
        (
            TestOutput,
            "test parser::accepts_input ... ok",
            &[("ok", Success)],
        ),
        (
            TestOutput,
            "tests/test_api.py::test_login FAILED",
            &[("FAILED", Error)],
        ),
        (
            TestOutput,
            "--- SKIP: TestNetwork (0.00s)",
            &[("--- SKIP", Warning)],
        ),
        (
            ContainerOutput,
            "api-7df4 0/1 CrashLoopBackOff 5 2m",
            &[("0/1", Warning), ("CrashLoopBackOff", Error)],
        ),
    ];
    for &(role, text, expected) in cases {
        let matches: Vec<_> = classify_line(text, role)
            .into_iter()
            .map(|span| (&text[span.range], span.class))
            .collect();
        for token in expected {
            assert!(
                matches.contains(token),
                "{role:?} {text:?}: missing {token:?} in {matches:?}"
            );
        }
    }
    let addition = "+let connected = true;";
    let matches: Vec<_> = classify_line(addition, GitDiffOutput)
        .into_iter()
        .map(|span| (&addition[span.range], span.class))
        .collect();
    assert_eq!(matches, [(addition, Success)]);
    assert!(
        classify_line("UP is ordinary output", Output)
            .iter()
            .all(|span| span.class != Success)
    );
    assert_eq!(
        semantic_line_emphasis(
            "/tmp/main.c:12:7: error: use of undeclared identifier 'value'",
            CCompilerOutput,
        ),
        Some(Error),
    );
}

#[test]
fn output_status_phrases_are_localized_without_promoting_neutral_prose() {
    use SemanticClass::*;
    let cases: &[(&str, &[(&str, SemanticClass)])] = &[
        (
            "Expanded Security Maintenance is not enabled.",
            &[("not enabled", Error)],
        ),
        (
            "247 additional security updates can be applied immediately.",
            &[("247", Number), ("can be applied", Success)],
        ),
        ("连接失败", &[("失败", Error)]),
        ("操作成功", &[("成功", Success)]),
        ("警告：空间不足", &[("警告", Warning)]),
        ("Échec de connexion", &[("Échec", Error)]),
        ("Vorgang erfolgreich", &[("erfolgreich", Success)]),
        ("작업 완료", &[("완료", Success)]),
        ("Cảnh báo dung lượng", &[("Cảnh báo", Warning)]),
    ];
    for &(text, expected) in cases {
        let matches = matched_texts(text);
        for token in expected {
            assert!(
                matches.contains(token),
                "{text:?}: missing {token:?} in {matches:?}"
            );
        }
    }
    let neutral =
        "Users can review changes; not every section has notes; no single format is required.";
    assert!(
        matched_texts(neutral)
            .iter()
            .all(|(_, class)| !matches!(class, Error | Success))
    );
}

#[test]
fn output_values_and_assignments_preserve_atomic_nonoverlapping_spans() {
    use SemanticClass::*;
    let cases: &[(&str, &[(&str, SemanticClass)])] = &[
        (
            "2026-08-18 15:20:06 host 192.168.1.52 mac 02:3B:4C:5D:6E:7F /var/log/app.log temperature -12",
            &[
                ("2026-08-18 15:20:06", Timestamp),
                ("192.168.1.52", Address),
                ("02:3B:4C:5D:6E:7F", Address),
                ("/var/log/app.log", Path),
            ],
        ),
        (
            r"host 2001:db8::1 loopback ::1 paths C:\Users\alice\app.log \\server\share\report.txt",
            &[
                ("2001:db8::1", Address),
                ("::1", Address),
                (r"C:\Users\alice\app.log", Path),
                (r"\\server\share\report.txt", Path),
            ],
        ),
        (
            "Sun03PM Mon07PM 6:52PM 11:04 PM 16Aug26 6月22 7月03 0:00.82 154:04 2255:55 19:21:57 2026-08-19 19:21:57",
            &[
                ("Sun03PM", Timestamp),
                ("Mon07PM", Timestamp),
                ("6:52PM", Timestamp),
                ("11:04 PM", Timestamp),
                ("16Aug26", Timestamp),
                ("6月22", Timestamp),
                ("7月03", Timestamp),
                ("0:00.82", Timestamp),
                ("154:04", Timestamp),
                ("2255:55", Timestamp),
                ("19:21:57", Timestamp),
                ("2026-08-19 19:21:57", Timestamp),
            ],
        ),
        (
            "Last login: Fri Sep 4; maintenance runs Friday through September",
            &[
                ("Fri", Weekday),
                ("Sep", Month),
                ("Friday", Weekday),
                ("September", Month),
            ],
        ),
        (
            "grep --color=auto NODE_ENV=production rw,fsname=portal,subtype=fuse",
            &[
                ("--color", Option),
                ("NODE_ENV", Variable),
                ("fsname", Variable),
                ("subtype", Variable),
                ("=", Operator),
                ("auto", String),
                ("production", String),
                ("portal", String),
                ("fuse", String),
            ],
        ),
    ];
    for &(text, expected) in cases {
        let spans = classify_line(text, SemanticLineRole::Output);
        let matches: Vec<_> = spans
            .iter()
            .map(|span| (&text[span.range.clone()], span.class))
            .collect();
        for token in expected {
            assert!(
                matches.contains(token),
                "{text:?}: missing {token:?} in {matches:?}"
            );
        }
        for pair in spans.windows(2) {
            assert!(
                pair[0].range.end <= pair[1].range.start,
                "{text:?}: {pair:?}"
            );
        }
    }
    assert!(!matched_texts(cases[0].0).contains(&("-12", Option)));
}

#[test]
fn ps_output_roles_classify_structured_columns_without_global_sentinels() {
    assert_eq!(
        semantic_output_role_for_command("ps aux | grep node"),
        SemanticLineRole::PsAuxOutput
    );
    assert_eq!(
        semantic_output_role_for_command("sudo /bin/ps -ef"),
        SemanticLineRole::PsFullOutput
    );

    let text = "lips 1172515 0.0 0.1 1159056 23396 ? Ssl 6月22 154:04 node /usr/local/bin/pnpm --filter=server NODE_ENV=production";
    let matches = classify_line(text, SemanticLineRole::PsAuxOutput)
        .into_iter()
        .map(|span| (&text[span.range], span.class))
        .collect::<Vec<_>>();

    for expected in [
        ("1172515", SemanticClass::Number),
        ("?", SemanticClass::Info),
        ("Ssl", SemanticClass::Info),
        ("6月22", SemanticClass::Timestamp),
        ("154:04", SemanticClass::Timestamp),
        ("--filter", SemanticClass::Option),
        ("NODE_ENV", SemanticClass::Variable),
    ] {
        assert!(
            matches.contains(&expected),
            "missing {expected:?} in {matches:?}"
        );
    }

    assert!(!matched_texts("question ? remains generic").contains(&("?", SemanticClass::Info)));

    let full_text = "root 717098 1 0 2025 ? 0:00 fuser -o rw,nosuid";
    let full_matches = classify_line(full_text, SemanticLineRole::PsFullOutput)
        .into_iter()
        .map(|span| (&full_text[span.range], span.class))
        .collect::<Vec<_>>();
    assert!(full_matches.contains(&("2025", SemanticClass::Timestamp)));
    assert!(full_matches.contains(&("?", SemanticClass::Info)));
    assert!(full_matches.contains(&("0:00", SemanticClass::Timestamp)));

    let conservative = classify_line_with_scheme(
        text,
        SemanticLineRole::PsAuxOutput,
        SemanticScheme::Conservative,
    );
    assert!(
        conservative
            .iter()
            .all(|span| { !matches!(span.class, SemanticClass::Number | SemanticClass::Info) })
    );
}

#[cfg(feature = "shell-syntax")]
#[test]
fn ps_command_column_uses_lightweight_semantic_classification() {
    let text = "lips 1172515 0.0 0.1 1159056 23396 ? Ssl 6月22 0:00 node --filter ./apps/server";
    let matches = classify_line_with_compiled_scheme_and_shell(
        text,
        SemanticLineRole::PsAuxOutput,
        compiled_builtin_scheme(SemanticScheme::Balanced),
        SemanticShellDialect::Bash,
    )
    .into_iter()
    .map(|span| (&text[span.range], span.class))
    .collect::<Vec<_>>();

    assert!(matches.contains(&("node", SemanticClass::Command)));
    assert!(matches.contains(&("--filter", SemanticClass::Option)));
}

#[test]
fn nested_ascii_and_unicode_bracket_pairs_are_classified() {
    let text = "outer(({[<value>]})) 【《（内容）》】 ⟦⌈item⌉⟧";
    let spans = classify_line(text, SemanticLineRole::Output);
    let brackets = spans
        .iter()
        .filter(|span| span.class == SemanticClass::Operator)
        .map(|span| &text[span.range.clone()])
        .collect::<String>();

    assert_eq!(brackets, "(({[<>]}))【《（）》】⟦⌈⌉⟧");
    let ascii_variants = spans
        .iter()
        .filter(|span| span.class == SemanticClass::Operator && span.range.start < 20)
        .map(|span| span.style_variant)
        .collect::<Vec<_>>();
    assert_eq!(
        ascii_variants,
        vec![
            Some(0),
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(4),
            Some(3),
            Some(2),
            Some(1),
            Some(0),
        ]
    );
}

#[cfg(feature = "shell-syntax")]
#[test]
fn nested_command_brackets_keep_depth_variants_after_shell_parsing() {
    let text = "if ((value + (nested * 2))); then echo ok; fi";
    let spans = classify_line_with_compiled_scheme_and_shell(
        text,
        SemanticLineRole::Command,
        compiled_builtin_scheme(SemanticScheme::Balanced),
        SemanticShellDialect::Bash,
    );
    let variants = spans
        .iter()
        .filter(|span| {
            span.class == SemanticClass::Operator && matches!(&text[span.range.clone()], "(" | ")")
        })
        .map(|span| span.style_variant)
        .collect::<Vec<_>>();

    assert_eq!(
        variants,
        vec![Some(0), Some(1), Some(2), Some(2), Some(1), Some(0),],
        "classified spans: {:?}",
        spans
            .iter()
            .map(|span| (&text[span.range.clone()], span.class, span.style_variant))
            .collect::<Vec<_>>()
    );
}

#[test]
fn bracket_pair_classification_ignores_quotes_escapes_and_mismatches() {
    let text = r#"don't "(ignored)" escaped \[value\] valid(ok) broken([)]"#;
    let brackets = classify_line(text, SemanticLineRole::Output)
        .into_iter()
        .filter(|span| span.class == SemanticClass::Operator)
        .map(|span| &text[span.range])
        .collect::<String>();

    assert_eq!(brackets, "()");

    let option = "--filter=<value>";
    let spans = classify_line(option, SemanticLineRole::Output);
    assert!(spans.iter().any(|span| {
        &option[span.range.clone()] == "--filter" && span.class == SemanticClass::Option
    }));
    let operators = spans
        .iter()
        .filter(|span| span.class == SemanticClass::Operator)
        .map(|span| &option[span.range.clone()])
        .collect::<Vec<_>>();
    assert_eq!(operators, vec!["="]);
}

#[test]
fn standalone_output_markers_are_operators_without_reclassifying_negative_numbers() {
    let text = "* item - divider | pipe = value -- boundary temperature -12";
    let operators = classify_line(text, SemanticLineRole::Output)
        .into_iter()
        .filter(|span| span.class == SemanticClass::Operator)
        .map(|span| &text[span.range])
        .collect::<Vec<_>>();

    assert_eq!(operators, vec!["*", "-", "|", "=", "--"]);
}

#[test]
fn quoted_status_words_remain_strings_while_unquoted_warnings_are_classified() {
    for (text, expected) in [
        (
            "message \"error 500\" returned",
            vec![("\"error 500\"", SemanticClass::String)],
        ),
        (
            "Warning: update skipped",
            vec![
                ("Warning", SemanticClass::Warning),
                ("skipped", SemanticClass::Warning),
            ],
        ),
    ] {
        assert_eq!(matched_texts(text), expected, "{text:?}");
    }
}

#[test]
fn explicit_log_severity_controls_line_emphasis() {
    for (text, expected) in [
        ("ERROR connection refused", SemanticClass::Error),
        ("[worker] [FATAL] service stopped", SemanticClass::Error),
        (
            "2026-09-04 10:30:00 warning: disk space is low",
            SemanticClass::Warning,
        ),
        ("error[E0308]: mismatched types", SemanticClass::Error),
        ("错误：连接已拒绝", SemanticClass::Error),
        ("[警告] 磁盘空间不足", SemanticClass::Warning),
        ("ÉCHEC: connexion refusée", SemanticClass::Error),
        ("Cảnh báo: dung lượng thấp", SemanticClass::Warning),
        ("level=error connection refused", SemanticClass::Error),
        (
            "severity: 'warning' disk space is low",
            SemanticClass::Warning,
        ),
        (
            r#"{"timestamp":"2026-09-04T10:30:00Z","level":"error","message":"offline"}"#,
            SemanticClass::Error,
        ),
    ] {
        assert_eq!(
            semantic_line_emphasis(text, SemanticLineRole::Output),
            Some(expected),
            "incorrect emphasis for {text:?}"
        );
    }

    assert_eq!(
        semantic_line_emphasis(
            "the command reports an error when offline",
            SemanticLineRole::Output
        ),
        None
    );
    assert_eq!(
        semantic_line_emphasis("ERROR is still being typed", SemanticLineRole::Command),
        None
    );
    assert_eq!(
        semantic_line_emphasis(
            r#"{"message":"the text mentions \"level\": \"error\""}"#,
            SemanticLineRole::Output
        ),
        None
    );
    assert_eq!(
        semantic_line_emphasis("level=info connected", SemanticLineRole::Output),
        None
    );
}

#[test]
fn conservative_scheme_omits_noisy_classes_but_keeps_structured_values() {
    let text = "Info: 247 updates on 192.168.1.52 failed";
    let matches =
        classify_line_with_scheme(text, SemanticLineRole::Output, SemanticScheme::Conservative)
            .into_iter()
            .map(|span| (&text[span.range], span.class))
            .collect::<Vec<_>>();

    assert!(!matches.contains(&("Info", SemanticClass::Info)));
    assert!(!matches.contains(&("247", SemanticClass::Number)));
    assert!(matches.contains(&("192.168.1.52", SemanticClass::Address)));
    assert!(matches.contains(&("failed", SemanticClass::Error)));
}

#[test]
fn command_role_colors_only_the_leading_command_token() {
    let text = "user@host:~$ sudo apt update --assume-yes";
    let command = classify_line(text, SemanticLineRole::Command);
    let output = classify_line(text, SemanticLineRole::Output);

    assert!(command.iter().any(|span| {
        &text[span.range.clone()] == "sudo" && span.class == SemanticClass::Command
    }));
    assert!(command.iter().any(|span| {
        &text[span.range.clone()] == "--assume-yes" && span.class == SemanticClass::Option
    }));
    assert!(
        !output
            .iter()
            .any(|span| span.class == SemanticClass::Command)
    );
}
