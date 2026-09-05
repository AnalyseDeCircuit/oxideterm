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
fn file_metadata_commands_classify_structured_fields() {
    for (command, role) in [
        ("ls -lah /var/log", SemanticLineRole::FileListingOutput),
        ("stat /var/log/app", SemanticLineRole::FileStatOutput),
        ("getfacl /srv/app", SemanticLineRole::FileAclOutput),
    ] {
        assert_eq!(semantic_output_role_for_command(command), role);
    }

    let listing = "lrwxr-xr-x@ 1 alice staff 11K Sep 4 12:30 current -> /opt/app";
    let listing_matches = classify_line(listing, SemanticLineRole::FileListingOutput)
        .into_iter()
        .map(|span| (&listing[span.range], span.class))
        .collect::<Vec<_>>();
    for expected in [
        ("l", SemanticClass::Keyword),
        ("alice", SemanticClass::Variable),
        ("staff", SemanticClass::Variable),
        ("11K", SemanticClass::Number),
        ("->", SemanticClass::Operator),
    ] {
        assert!(
            listing_matches.contains(&expected),
            "missing {expected:?} in {listing_matches:?}"
        );
    }

    let stat = "Access: (4755/-rwsr-xr-x) Uid: ( 1000/ alice) Gid: ( 100/ staff)";
    let stat_matches = classify_line(stat, SemanticLineRole::FileStatOutput)
        .into_iter()
        .map(|span| (&stat[span.range], span.class))
        .collect::<Vec<_>>();
    assert!(stat_matches.contains(&("Access", SemanticClass::Keyword)));
    assert!(stat_matches.contains(&("s", SemanticClass::PermissionSpecial)));

    let acl = "user:deploy:rwx";
    let acl_matches = classify_line(acl, SemanticLineRole::FileAclOutput)
        .into_iter()
        .map(|span| (&acl[span.range], span.class))
        .collect::<Vec<_>>();
    assert!(acl_matches.contains(&("user", SemanticClass::Keyword)));
    assert!(acl_matches.contains(&("deploy", SemanticClass::Variable)));
    assert!(acl_matches.contains(&("x", SemanticClass::PermissionExecute)));
}

#[test]
fn resource_commands_classify_usage_columns_and_capacity() {
    for (command, role) in [
        ("df -h", SemanticLineRole::DiskUsageOutput),
        ("free -h", SemanticLineRole::MemoryUsageOutput),
    ] {
        assert_eq!(semantic_output_role_for_command(command), role);
    }

    let disk = "/dev/nvme0n1p2 100G 91G 9G 91% /";
    let disk_matches = classify_line(disk, SemanticLineRole::DiskUsageOutput)
        .into_iter()
        .map(|span| (&disk[span.range], span.class))
        .collect::<Vec<_>>();
    assert!(disk_matches.contains(&("/dev/nvme0n1p2", SemanticClass::Path)));
    assert!(disk_matches.contains(&("100G", SemanticClass::Number)));
    assert!(disk_matches.contains(&("91%", SemanticClass::Error)));
    assert!(disk_matches.contains(&("/", SemanticClass::Path)));

    let memory = "Mem: 15GiB 8GiB 2GiB 1GiB 5GiB 6GiB";
    let memory_matches = classify_line(memory, SemanticLineRole::MemoryUsageOutput)
        .into_iter()
        .map(|span| (&memory[span.range], span.class))
        .collect::<Vec<_>>();
    assert!(memory_matches.contains(&("Mem", SemanticClass::Keyword)));
    assert!(memory_matches.contains(&("15GiB", SemanticClass::Number)));
}

#[test]
fn network_commands_classify_interfaces_states_addresses_and_loss() {
    for (command, role) in [
        ("ip address show", SemanticLineRole::IpOutput),
        ("ss -lnt", SemanticLineRole::SocketOutput),
        ("ping 1.1.1.1", SemanticLineRole::PingOutput),
    ] {
        assert_eq!(semantic_output_role_for_command(command), role);
    }

    let interface = "2: eth0: <BROADCAST,MULTICAST,UP> mtu 1500 state UP qlen 1000";
    let interface_matches = classify_line(interface, SemanticLineRole::IpOutput)
        .into_iter()
        .map(|span| (&interface[span.range], span.class))
        .collect::<Vec<_>>();
    assert!(interface_matches.contains(&("eth0", SemanticClass::Variable)));
    assert!(interface_matches.contains(&("mtu", SemanticClass::Keyword)));
    assert!(interface_matches.contains(&("UP", SemanticClass::Success)));

    let route = "default via 192.168.1.1 dev eth0 src 192.168.1.20 metric 100";
    let route_matches = classify_line(route, SemanticLineRole::IpOutput)
        .into_iter()
        .map(|span| (&route[span.range], span.class))
        .collect::<Vec<_>>();
    assert!(route_matches.contains(&("192.168.1.1", SemanticClass::Address)));
    assert!(route_matches.contains(&("eth0", SemanticClass::Variable)));

    let socket = "LISTEN 0 4096 0.0.0.0:22 0.0.0.0:*";
    let socket_matches = classify_line(socket, SemanticLineRole::SocketOutput)
        .into_iter()
        .map(|span| (&socket[span.range], span.class))
        .collect::<Vec<_>>();
    assert!(socket_matches.contains(&("LISTEN", SemanticClass::Info)));
    assert!(socket_matches.contains(&("0.0.0.0:22", SemanticClass::Address)));

    let ping = "64 bytes from 1.1.1.1: icmp_seq=1 ttl=57 time=12.4 ms";
    let ping_matches = classify_line(ping, SemanticLineRole::PingOutput)
        .into_iter()
        .map(|span| (&ping[span.range], span.class))
        .collect::<Vec<_>>();
    assert!(ping_matches.contains(&("1.1.1.1", SemanticClass::Address)));
    assert!(ping_matches.contains(&("icmp_seq", SemanticClass::Variable)));
    assert!(ping_matches.contains(&("12.4", SemanticClass::Number)));

    let loss = "4 packets transmitted, 3 received, 25% packet loss";
    let loss_matches = classify_line(loss, SemanticLineRole::PingOutput)
        .into_iter()
        .map(|span| (&loss[span.range], span.class))
        .collect::<Vec<_>>();
    assert!(loss_matches.contains(&("25%", SemanticClass::Warning)));
    assert!(
        classify_line("UP is ordinary output", SemanticLineRole::Output)
            .iter()
            .all(|span| span.class != SemanticClass::Success)
    );
}

#[test]
fn ubuntu_motd_status_phrases_receive_semantic_roles() {
    let error = "Expanded Security Maintenance is not enabled.";
    let success = "247 additional security updates can be applied immediately.";

    assert!(matched_texts(error).contains(&("not enabled", SemanticClass::Error)));
    assert!(matched_texts(success).contains(&("247", SemanticClass::Number)));
    assert!(matched_texts(success).contains(&("can be applied", SemanticClass::Success)));
}

#[test]
fn structured_terminal_values_are_classified_without_overlap() {
    let text = "2026-08-18 15:20:06 host 192.168.1.52 mac 02:3B:4C:5D:6E:7F /var/log/app.log temperature -12";
    let spans = classify_line(text, SemanticLineRole::Output);
    let matches = spans
        .iter()
        .map(|span| (&text[span.range.clone()], span.class))
        .collect::<Vec<_>>();

    assert!(matches.contains(&("2026-08-18 15:20:06", SemanticClass::Timestamp)));
    assert!(matches.contains(&("192.168.1.52", SemanticClass::Address)));
    assert!(matches.contains(&("02:3B:4C:5D:6E:7F", SemanticClass::Address)));
    assert!(matches.contains(&("/var/log/app.log", SemanticClass::Path)));
    assert!(!matches.contains(&("-12", SemanticClass::Option)));
    for pair in spans.windows(2) {
        assert!(pair[0].range.end <= pair[1].range.start);
    }
}

#[test]
fn ipv6_and_windows_paths_are_classified_as_structured_values() {
    let text =
        r"host 2001:db8::1 loopback ::1 paths C:\Users\alice\app.log \\server\share\report.txt";
    let matches = matched_texts(text);

    assert!(matches.contains(&("2001:db8::1", SemanticClass::Address)));
    assert!(matches.contains(&("::1", SemanticClass::Address)));
    assert!(matches.contains(&(r"C:\Users\alice\app.log", SemanticClass::Path)));
    assert!(matches.contains(&(r"\\server\share\report.txt", SemanticClass::Path)));
}

#[test]
fn process_table_time_formats_are_classified_as_complete_timestamps() {
    let text = "Sun03PM Mon07PM 6:52PM 11:04 PM 16Aug26 6月22 7月03 0:00.82 154:04 2255:55 19:21:57 2026-08-19 19:21:57";
    let matches = matched_texts(text);

    for timestamp in [
        "Sun03PM",
        "Mon07PM",
        "6:52PM",
        "11:04 PM",
        "16Aug26",
        "6月22",
        "7月03",
        "0:00.82",
        "154:04",
        "2255:55",
        "19:21:57",
        "2026-08-19 19:21:57",
    ] {
        assert!(
            matches.contains(&(timestamp, SemanticClass::Timestamp)),
            "missing timestamp {timestamp:?} in {matches:?}"
        );
    }
}

#[test]
fn weekday_and_month_names_use_distinct_classes_in_ordinary_text() {
    let text = "Last login: Fri Sep 4; maintenance runs Friday through September";
    let matches = matched_texts(text);

    for expected in [
        ("Fri", SemanticClass::Weekday),
        ("Sep", SemanticClass::Month),
        ("Friday", SemanticClass::Weekday),
        ("September", SemanticClass::Month),
    ] {
        assert!(
            matches.contains(&expected),
            "missing {expected:?} in {matches:?}"
        );
    }
}

#[test]
fn option_and_variable_assignments_keep_separate_semantic_parts() {
    let text = "grep --color=auto NODE_ENV=production rw,fsname=portal,subtype=fuse";
    let matches = matched_texts(text);

    for expected in [
        ("--color", SemanticClass::Option),
        ("NODE_ENV", SemanticClass::Variable),
        ("fsname", SemanticClass::Variable),
        ("subtype", SemanticClass::Variable),
        ("=", SemanticClass::Operator),
        ("auto", SemanticClass::String),
        ("production", SemanticClass::String),
        ("portal", SemanticClass::String),
        ("fuse", SemanticClass::String),
    ] {
        assert!(
            matches.contains(&expected),
            "missing {expected:?} in {matches:?}"
        );
    }
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

    assert!(
        !matched_texts("question ? remains generic")
            .iter()
            .any(|match_| *match_ == ("?", SemanticClass::Info))
    );

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

#[test]
fn compiler_output_roles_classify_diagnostics_and_source_locations() {
    for (command, expected) in [
        ("cargo check", SemanticLineRole::RustToolOutput),
        (
            "env RUSTFLAGS=-Dwarnings rustc src/main.rs",
            SemanticLineRole::RustToolOutput,
        ),
        (
            "RUSTFLAGS=-Dwarnings cargo check",
            SemanticLineRole::RustToolOutput,
        ),
        ("clang++ src/main.cc", SemanticLineRole::CCompilerOutput),
        (
            "aarch64-linux-gnu-gcc src/main.c",
            SemanticLineRole::CCompilerOutput,
        ),
    ] {
        assert_eq!(
            semantic_output_role_for_command(command),
            expected,
            "incorrect output role for {command:?}"
        );
    }

    let rust_location = "  --> crates/app/src/main.rs:42:17";
    let rust_matches = classify_line(rust_location, SemanticLineRole::RustToolOutput)
        .into_iter()
        .map(|span| (&rust_location[span.range], span.class))
        .collect::<Vec<_>>();
    for expected in [
        ("-->", SemanticClass::Operator),
        ("crates/app/src/main.rs", SemanticClass::Path),
        ("42", SemanticClass::Number),
        ("17", SemanticClass::Number),
    ] {
        assert!(
            rust_matches.contains(&expected),
            "missing {expected:?} in {rust_matches:?}"
        );
    }

    let c_diagnostic = "/tmp/main.c:12:7: error: use of undeclared identifier 'value'";
    let c_matches = classify_line(c_diagnostic, SemanticLineRole::CCompilerOutput)
        .into_iter()
        .map(|span| (&c_diagnostic[span.range], span.class))
        .collect::<Vec<_>>();
    for expected in [
        ("/tmp/main.c", SemanticClass::Path),
        ("12", SemanticClass::Number),
        ("7", SemanticClass::Number),
        ("error", SemanticClass::Error),
    ] {
        assert!(
            c_matches.contains(&expected),
            "missing {expected:?} in {c_matches:?}"
        );
    }
    assert_eq!(
        semantic_line_emphasis(c_diagnostic, SemanticLineRole::CCompilerOutput),
        Some(SemanticClass::Error)
    );

    let cargo_phase = "   Finished `dev` profile in 0.42s";
    assert!(
        classify_line(cargo_phase, SemanticLineRole::RustToolOutput)
            .iter()
            .any(|span| {
                &cargo_phase[span.range.clone()] == "Finished"
                    && span.class == SemanticClass::Success
            })
    );
}

#[test]
fn git_output_roles_keep_status_and_diff_meaning_local_to_git() {
    assert_eq!(
        semantic_output_role_for_command("git -C workspace status --short"),
        SemanticLineRole::GitStatusOutput
    );
    assert_eq!(
        semantic_output_role_for_command("git diff --cached"),
        SemanticLineRole::GitDiffOutput
    );

    let status = " M src/main.rs";
    let status_matches = classify_line(status, SemanticLineRole::GitStatusOutput)
        .into_iter()
        .map(|span| (&status[span.range], span.class))
        .collect::<Vec<_>>();
    assert!(status_matches.contains(&(" M", SemanticClass::Warning)));
    assert!(status_matches.contains(&("src/main.rs", SemanticClass::Path)));

    let addition = "+let connected = true;";
    let addition_matches = classify_line(addition, SemanticLineRole::GitDiffOutput)
        .into_iter()
        .map(|span| (&addition[span.range], span.class))
        .collect::<Vec<_>>();
    assert_eq!(addition_matches, vec![(addition, SemanticClass::Success)]);
}

#[test]
fn systemd_output_roles_classify_service_state_and_journal_time() {
    assert_eq!(
        semantic_output_role_for_command("sudo systemctl status sshd"),
        SemanticLineRole::SystemdOutput
    );
    assert_eq!(
        semantic_output_role_for_command("journalctl -u sshd"),
        SemanticLineRole::SystemdOutput
    );

    let state = "     Active: failed (Result: exit-code)";
    let state_matches = classify_line(state, SemanticLineRole::SystemdOutput)
        .into_iter()
        .map(|span| (&state[span.range], span.class))
        .collect::<Vec<_>>();
    assert!(state_matches.contains(&("Active", SemanticClass::Keyword)));
    assert!(state_matches.contains(&("failed", SemanticClass::Error)));

    let journal = "Sep  4 10:30:00 host sshd[42]: connection closed";
    assert!(
        classify_line(journal, SemanticLineRole::SystemdOutput)
            .iter()
            .any(|span| {
                &journal[span.range.clone()] == "Sep  4 10:30:00"
                    && span.class == SemanticClass::Timestamp
            })
    );
}

#[test]
fn test_runner_output_roles_classify_common_result_markers() {
    assert_eq!(
        semantic_output_role_for_command("cargo test"),
        SemanticLineRole::RustToolOutput
    );
    for command in ["pytest -q", "go test ./...", "npm test"] {
        assert_eq!(
            semantic_output_role_for_command(command),
            SemanticLineRole::TestOutput,
            "incorrect output role for {command:?}"
        );
    }

    for (line, label, expected) in [
        (
            "test parser::accepts_input ... ok",
            "ok",
            SemanticClass::Success,
        ),
        (
            "tests/test_api.py::test_login FAILED",
            "FAILED",
            SemanticClass::Error,
        ),
        (
            "--- SKIP: TestNetwork (0.00s)",
            "--- SKIP",
            SemanticClass::Warning,
        ),
    ] {
        assert!(
            classify_line(line, SemanticLineRole::TestOutput)
                .iter()
                .any(|span| &line[span.range.clone()] == label && span.class == expected),
            "missing {expected:?} marker in {line:?}"
        );
    }
}

#[test]
fn container_output_roles_classify_health_and_readiness_states() {
    for command in ["docker ps", "podman container ls", "kubectl get pods"] {
        assert_eq!(
            semantic_output_role_for_command(command),
            SemanticLineRole::ContainerOutput,
            "incorrect output role for {command:?}"
        );
    }
    assert_eq!(
        semantic_output_role_for_command("docker logs api"),
        SemanticLineRole::Output
    );

    let line = "api-7df4 0/1 CrashLoopBackOff 5 2m";
    let matches = classify_line(line, SemanticLineRole::ContainerOutput)
        .into_iter()
        .map(|span| (&line[span.range], span.class))
        .collect::<Vec<_>>();
    assert!(matches.contains(&("0/1", SemanticClass::Warning)));
    assert!(matches.contains(&("CrashLoopBackOff", SemanticClass::Error)));
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
    assert!(!operators.contains(&"-12"));
}

#[test]
fn neutral_modal_and_negation_phrases_are_not_statuses() {
    let text =
        "Users can review changes; not every section has notes; no single format is required.";
    let matches = matched_texts(text);

    assert!(
        matches
            .iter()
            .all(|(_, class)| !matches!(class, SemanticClass::Error | SemanticClass::Success))
    );
}

#[test]
fn quoted_text_wins_over_nested_status_words_and_numbers() {
    let text = "message \"error 500\" returned";

    assert_eq!(
        matched_texts(text),
        vec![("\"error 500\"", SemanticClass::String)]
    );
}

#[test]
fn warning_terms_use_the_warning_class() {
    assert_eq!(
        matched_texts("Warning: update skipped"),
        vec![
            ("Warning", SemanticClass::Warning),
            ("skipped", SemanticClass::Warning),
        ]
    );
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

#[test]
fn every_span_uses_valid_utf8_boundaries() {
    let text = "连接 10.0.0.1 成功 true";

    for span in classify_line(text, SemanticLineRole::Output) {
        assert!(text.is_char_boundary(span.range.start));
        assert!(text.is_char_boundary(span.range.end));
        assert!(span.range.start < span.range.end);
    }
}

#[test]
fn multilingual_status_terms_receive_the_same_semantic_classes() {
    let cases = [
        ("连接失败", "失败", SemanticClass::Error),
        ("操作成功", "成功", SemanticClass::Success),
        ("警告：空间不足", "警告", SemanticClass::Warning),
        ("Échec de connexion", "Échec", SemanticClass::Error),
        ("Vorgang erfolgreich", "erfolgreich", SemanticClass::Success),
        ("작업 완료", "완료", SemanticClass::Success),
        ("Cảnh báo dung lượng", "Cảnh báo", SemanticClass::Warning),
    ];

    for (text, expected_text, expected_class) in cases {
        let matches = classify_line(text, SemanticLineRole::Output)
            .into_iter()
            .map(|span| (&text[span.range], span.class))
            .collect::<Vec<_>>();
        assert!(
            matches.contains(&(expected_text, expected_class)),
            "missing {expected_text:?} in {matches:?}"
        );
    }
}
