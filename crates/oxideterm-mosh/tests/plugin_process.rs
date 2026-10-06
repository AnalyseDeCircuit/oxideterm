// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_mosh::MoshSessionKey;
use oxideterm_mosh::{
    MoshIpFamily, MoshPluginSessions, MoshSessionConfig, MoshSessionEvent, start_mosh_session,
};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

// Requires a built plugin and a local mosh-server, not a remote account.
#[tokio::test]
#[cfg(unix)]
#[ignore = "set MOSH_TEST_HELPER and install mosh-server"]
async fn native_plugin_exchanges_real_udp_terminal_data_and_reaps_on_disable() {
    let helper =
        PathBuf::from(std::env::var("MOSH_TEST_HELPER").expect("MOSH_TEST_HELPER is required"));
    let helper_name = helper.file_name().unwrap().to_string_lossy().into_owned();
    let server = tokio::process::Command::new("mosh-server")
        .args(["new", "-s", "-i", "127.0.0.1", "-l", "LANG=en_US.UTF-8", "--", "/bin/sh", "-c",
            "stty -echo; printf 'MOSH_PROBE_READY\\n'; IFS= read -r line; printf 'MOSH_REPLY:%s\\n' \"$line\"; stty size; sleep 15"])
        .output().await.expect("local mosh-server must start");
    let stdout = Zeroizing::new(server.stdout);
    let stderr = Zeroizing::new(server.stderr);
    assert!(server.status.success(), "local mosh-server failed");
    let line = stdout
        .split(|byte| *byte == b'\n')
        .chain(stderr.split(|byte| *byte == b'\n'))
        .find(|line| line.starts_with(b"MOSH CONNECT "))
        .expect("missing bootstrap response");
    let mut fields = std::str::from_utf8(line).unwrap().split_whitespace();
    assert_eq!(fields.next(), Some("MOSH"));
    assert_eq!(fields.next(), Some("CONNECT"));
    let port = fields.next().unwrap().parse().unwrap();
    let key = MoshSessionKey::decode(fields.next().unwrap()).expect("valid synthetic session key");
    let sessions = Arc::new(MoshPluginSessions::default());
    let (mut client, owner) = start_mosh_session(MoshSessionConfig {
        remote_host: "127.0.0.1".into(),
        remote_port: port,
        ip_family: MoshIpFamily::Ipv4,
        columns: 80,
        rows: 24,
        key,
        executable: helper,
        lease: sessions.acquire().unwrap(),
    })
    .await
    .expect("native plugin handshake");
    let processes = tokio::process::Command::new("ps")
        .args(["-ww", "-axo", "ppid,pid,comm"])
        .output()
        .await
        .unwrap();
    let helper_pid = String::from_utf8(processes.stdout)
        .unwrap()
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let parent = fields.next()?.parse::<u32>().ok()?;
            let pid = fields.next()?.parse::<u32>().ok()?;
            (parent == std::process::id() && line.trim_end().ends_with(&helper_name)).then_some(pid)
        })
        .expect("test-owned Mosh helper must be running");
    drop(stdout);
    drop(stderr);
    let mut text = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !String::from_utf8_lossy(&text).contains("MOSH_PROBE_READY") {
            if let Some(MoshSessionEvent::Output(bytes)) = client.next_event().await {
                text.extend(bytes);
            }
        }
    })
    .await
    .expect("server output must reach terminal");
    let before = Instant::now();
    client.resize(96, 30).await.unwrap();
    client
        .send_input_for_prediction(7, "插件连接成功\n".as_bytes().to_vec())
        .await
        .unwrap();
    let mut acknowledged = false;
    tokio::time::timeout(Duration::from_secs(5), async {
        while !acknowledged
            || !String::from_utf8_lossy(&text).contains("MOSH_REPLY:插件连接成功")
            || !String::from_utf8_lossy(&text).contains("30 96")
        {
            match client.next_event().await {
                Some(MoshSessionEvent::Output(bytes)) => text.extend(bytes),
                Some(MoshSessionEvent::PredictionAcknowledged(7)) => acknowledged = true,
                None => panic!("Mosh plugin closed before returning output"),
                _ => {}
            }
        }
    })
    .await
    .expect("UTF-8 input and prediction acknowledgement must return");
    eprintln!("Local UDP input/output round trip: {:?}", before.elapsed());
    let retirement = sessions.stop();
    tokio::time::timeout(
        Duration::from_secs(3),
        tokio::task::spawn_blocking(move || retirement.join().unwrap()),
    )
    .await
    .unwrap()
    .unwrap();
    let exited = tokio::process::Command::new("ps")
        .args(["-p", &helper_pid.to_string(), "-o", "pid="])
        .output()
        .await
        .unwrap();
    assert!(
        !exited.status.success(),
        "retired Mosh helper was not reaped"
    );
    // Completion follows child.wait, so package replacement can proceed safely.
    assert!(
        client
            .send_input_for_prediction(8, b"after-disable".to_vec())
            .await
            .is_err()
    );
    drop(owner);
}
