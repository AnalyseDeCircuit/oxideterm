use super::*;
use crate::{ClientRef, TerminalRef};
use serde_json::json;
use std::time::{Instant, SystemTime};
use zeroize::Zeroizing;
fn args(lines: usize, bytes: usize) -> HistoryArgs {
    HistoryArgs {
        request: HistoryRequest::ReleaseSlot { slot: 0 },
        line_limit: Some(lines),
        byte_limit: Some(bytes),
    }
}
fn target(terminal: &TerminalRef, session: &str) -> HistoryTarget {
    HistoryTarget {
        terminal: terminal.clone(),
        session: session.into(),
    }
}
fn capture(now: Instant, content: u64, rows: Option<&[&str]>) -> HistoryCapture {
    HistoryCapture {
        stamp: HistoryStamp {
            identity: 1,
            content,
            switches: 0,
            cleared: 0,
            evicted: 0,
        },
        captured: now,
        wall_time: SystemTime::UNIX_EPOCH,
        screen_rows: 2,
        retained_rows: 2,
        active_buffer: HistoryBuffer::Primary,
        rows: rows.map(|rows| {
            rows.iter()
                .map(|s| HistoryRow {
                    text: Zeroizing::new(s.to_string()),
                    soft_wrapped: false,
                })
                .collect()
        }),
    }
}
#[test]
fn pages_are_frozen_retryable_unicode_safe_and_client_scoped() {
    let mut store = HistoryStore::default();
    let client = ClientRef::new();
    let terminal = TerminalRef::new();
    let now = Instant::now();
    let r = store
        .reserve(
            &client,
            0,
            target(&terminal, "old"),
            HistoryBuffer::Primary,
            HistorySelection::All {},
            None,
            now,
        )
        .unwrap();
    let first = store
        .publish(
            r,
            capture(now, 1, Some(&["你好ab", "end"])),
            &args(1, 4),
            now,
        )
        .unwrap();
    assert_eq!(
        first["rows"],
        json!([{"row_index":0,"text":"你","byte_start":0,"row_complete":false,"soft_wrapped":false}])
    );
    let cursor = first["next_cursor"].as_str().unwrap();
    let next = store.page(&client, cursor, &args(1, 4), now).unwrap();
    assert_eq!(next["rows"][0]["text"], "好a");
    assert_eq!(next, store.page(&client, cursor, &args(1, 4), now).unwrap());
    assert_eq!(
        store
            .page(&ClientRef::new(), cursor, &args(1, 4), now)
            .unwrap_err(),
        HistoryError::SnapshotUnavailable
    );
    let last_part = store
        .page(
            &client,
            next["next_cursor"].as_str().unwrap(),
            &args(2, 4),
            now,
        )
        .unwrap();
    assert_eq!(
        last_part["rows"],
        json!([
        {"row_index":0,"text":"b","byte_start":7,"row_complete":true,"soft_wrapped":false},
        {"row_index":1,"text":"end","byte_start":0,"row_complete":true,"soft_wrapped":false}])
    );
    assert_eq!(last_part["next_cursor"], json!(null));
    assert_eq!(last_part["has_more"], false);
    store.release(&ClientRef::new(), first["snapshot_ref"].as_str().unwrap());
    assert_eq!(store.page(&client, cursor, &args(1, 4), now).unwrap(), next);
}
#[test]
fn replacement_release_expiry_and_reconnect_have_distinct_continuity() {
    let mut store = HistoryStore::default();
    let c = ClientRef::new();
    let t = TerminalRef::new();
    let now = Instant::now();
    let r = store
        .reserve(
            &c,
            0,
            target(&t, "one"),
            HistoryBuffer::Primary,
            HistorySelection::All {},
            None,
            now,
        )
        .unwrap();
    let a = store
        .publish(
            r,
            capture(now, 1, Some(&["old", "screen"])),
            &args(1, 100),
            now,
        )
        .unwrap();
    let rev = a["content_revision"].as_str().unwrap();
    store.release_slot(&c, 0);
    let r = store
        .reserve(
            &c,
            0,
            target(&t, "one"),
            HistoryBuffer::Primary,
            HistorySelection::All {},
            Some(rev),
            now,
        )
        .unwrap();
    assert!(r.unchanged_stamp().is_some());
    let unchanged = store
        .publish(r, capture(now, 1, None), &args(1, 100), now)
        .unwrap();
    assert_eq!(unchanged["status"], "unchanged");
    store.revoke_terminal(&t, true);
    assert_eq!(
        store
            .page(&c, a["next_cursor"].as_str().unwrap(), &args(1, 100), now)
            .unwrap_err(),
        HistoryError::SnapshotUnavailable
    );
    let r = store
        .reserve(
            &c,
            0,
            target(&t, "two"),
            HistoryBuffer::Primary,
            HistorySelection::All {},
            Some(rev),
            now,
        )
        .unwrap();
    assert!(r.unchanged_stamp().is_none());
    let b = store
        .publish(
            r,
            capture(now, 1, Some(&["new", "shell"])),
            &args(2, 100),
            now,
        )
        .unwrap();
    assert_eq!(b["gap_reasons"], json!(["session_replaced"]));
    assert_eq!(b["rows"][0]["text"], "new");
    store.release(&c, a["snapshot_ref"].as_str().unwrap());
    assert_eq!(
        store
            .page(&c, b["snapshot_ref"].as_str().unwrap(), &args(2, 100), now)
            .unwrap(),
        b
    );
    let expired = now + HISTORY_TTL;
    assert_eq!(
        store
            .page(
                &c,
                b["snapshot_ref"].as_str().unwrap(),
                &args(2, 100),
                expired
            )
            .unwrap_err(),
        HistoryError::SnapshotUnavailable
    );
    let r = store
        .reserve(
            &c,
            0,
            target(&t, "two"),
            HistoryBuffer::Primary,
            HistorySelection::All {},
            Some(rev),
            expired,
        )
        .unwrap();
    let page = store
        .publish(
            r,
            capture(expired, 1, Some(&["new", "shell"])),
            &args(2, 100),
            expired,
        )
        .unwrap();
    assert_eq!(page["continuity"], "unknown");
    assert_eq!(page["gap_reasons"], json!([]));
}
#[test]
fn revoked_reservation_cannot_republish_after_regrant() {
    let mut s = HistoryStore::default();
    let c = ClientRef::new();
    let t = TerminalRef::new();
    let now = Instant::now();
    let r = s
        .reserve(
            &c,
            0,
            target(&t, "one"),
            HistoryBuffer::Primary,
            HistorySelection::All {},
            None,
            now,
        )
        .unwrap();
    s.revoke_client(&c);
    let fresh = s
        .reserve(
            &c,
            0,
            target(&t, "one"),
            HistoryBuffer::Primary,
            HistorySelection::All {},
            None,
            now,
        )
        .unwrap();
    assert_eq!(
        s.publish(
            r,
            capture(now, 1, Some(&["secret", "old"])),
            &args(2, 100),
            now
        )
        .unwrap_err(),
        HistoryError::TerminalUnavailable
    );
    let page = s
        .publish(
            fresh,
            capture(now, 2, Some(&["current", "grant"])),
            &args(2, 100),
            now,
        )
        .unwrap();
    assert_eq!(page["rows"][0]["text"], "current");
}
#[test]
fn strict_schema_rejects_duplicate_targets_and_obsolete_selection() {
    let terminal = TerminalRef::new();
    for bad in [
        json!({"terminal_ref":terminal,"request":{"kind":"page","cursor":"history_test"}}),
        json!({"request":{"kind":"page","cursor":"history_test","terminal_ref":terminal}}),
        json!({"request":{"kind":"snapshot","terminal_ref":terminal,"buffer":"primary","selection":{"kind":"tail","rows":10}}}),
        json!({"request":{"kind":"snapshot","terminal_ref":terminal,"buffer":"primary","selection":{"kind":"screen","rows":10}}}),
    ] {
        assert!(serde_json::from_value::<HistoryArgs>(bad).is_err());
    }
    let release: HistoryArgs =
        serde_json::from_value(json!({"request":{"kind":"release_slot","slot":0},"line_limit":2}))
            .unwrap();
    assert!(!release.validate());
}

#[test]
fn full_cache_recovers_without_the_lost_snapshot_handle() {
    let mut store = HistoryStore::default();
    let now = Instant::now();
    let terminal = TerminalRef::new();
    let clients = (0..8).map(|_| ClientRef::new()).collect::<Vec<_>>();
    let text = "x".repeat(15 * 1024 * 1024);
    let mut first_cursor = String::new();
    for client in &clients {
        let r = store
            .reserve(
                client,
                0,
                target(&terminal, "live"),
                HistoryBuffer::Primary,
                HistorySelection::All {},
                None,
                now,
            )
            .unwrap();
        let page = store
            .publish(r, capture(now, 1, Some(&[&text, ""])), &args(1, 4), now)
            .unwrap();
        if client == &clients[0] {
            first_cursor = page["next_cursor"].as_str().unwrap().into();
        }
    }
    let failed = store.reserve(
        &clients[0],
        0,
        target(&terminal, "live"),
        HistoryBuffer::Primary,
        HistorySelection::All {},
        None,
        now,
    );
    assert!(matches!(failed, Err(HistoryError::ResourceLimit)));
    store.release_slot(&clients[0], 0);
    assert_eq!(
        store
            .page(&clients[0], &first_cursor, &args(1, 4), now)
            .unwrap_err(),
        HistoryError::SnapshotUnavailable
    );
    let r = store
        .reserve(
            &clients[0],
            0,
            target(&terminal, "live"),
            HistoryBuffer::Primary,
            HistorySelection::Screen {},
            None,
            now,
        )
        .unwrap();
    let page = store
        .publish(
            r,
            capture(now, 2, Some(&["recovered", "> "])),
            &args(2, 100),
            now,
        )
        .unwrap();
    assert_eq!(page["rows"][0]["text"], "recovered");
    assert_eq!(page["rows"][1]["text"], "> ");
}

#[test]
fn slots_failures_and_expiry_during_freeze_preserve_the_contract() {
    let mut store = HistoryStore::default();
    let c = ClientRef::new();
    let t = TerminalRef::new();
    let now = Instant::now();
    let mut pages = Vec::new();
    for slot in 0..2 {
        let r = store
            .reserve(
                &c,
                slot,
                target(&t, "live"),
                HistoryBuffer::Primary,
                HistorySelection::All {},
                None,
                now,
            )
            .unwrap();
        pages.push(
            store
                .publish(
                    r,
                    capture(now, 1, Some(&["first", "frozen"])),
                    &args(1, 100),
                    now,
                )
                .unwrap(),
        );
    }
    let old_cursor = pages[0]["next_cursor"].as_str().unwrap();
    let revision = pages[0]["content_revision"].as_str().unwrap();
    let r = store
        .reserve(
            &c,
            0,
            target(&t, "live"),
            HistoryBuffer::Primary,
            HistorySelection::All {},
            None,
            now,
        )
        .unwrap();
    let oversized = "x".repeat(HISTORY_SNAPSHOT_BYTES);
    assert_eq!(
        store
            .publish(r, capture(now, 2, Some(&[&oversized])), &args(1, 100), now)
            .unwrap_err(),
        HistoryError::SnapshotTooLarge
    );
    assert_eq!(
        store.page(&c, old_cursor, &args(1, 100), now).unwrap()["rows"][0]["text"],
        "frozen"
    );
    let r = store
        .reserve(
            &c,
            0,
            target(&t, "live"),
            HistoryBuffer::Primary,
            HistorySelection::Screen {},
            Some(revision),
            now,
        )
        .unwrap();
    assert!(r.unchanged_stamp().is_none());
    let changed = store
        .publish(
            r,
            capture(now, 2, Some(&["new", "screen"])),
            &args(1, 100),
            now,
        )
        .unwrap();
    assert_eq!(
        store.page(&c, old_cursor, &args(1, 100), now).unwrap_err(),
        HistoryError::SnapshotUnavailable
    );
    assert_eq!(
        store
            .page(
                &c,
                pages[1]["next_cursor"].as_str().unwrap(),
                &args(1, 100),
                now
            )
            .unwrap()["rows"][0]["text"],
        "frozen"
    );
    store.release(&c, pages[0]["snapshot_ref"].as_str().unwrap());
    assert_eq!(
        store
            .page(
                &c,
                changed["next_cursor"].as_str().unwrap(),
                &args(1, 100),
                now
            )
            .unwrap()["rows"][0]["text"],
        "screen"
    );
    let r = store
        .reserve(
            &c,
            0,
            target(&t, "live"),
            HistoryBuffer::Primary,
            HistorySelection::All {},
            Some(revision),
            now,
        )
        .unwrap();
    let expired = now + HISTORY_TTL;
    assert!(!r.revision_valid_at(expired));
    let fresh = store
        .publish(
            r,
            capture(expired, 2, Some(&["new", "screen"])),
            &args(2, 100),
            expired,
        )
        .unwrap();
    assert_eq!(fresh["status"], "snapshot");
    assert_eq!(fresh["continuity"], "unknown");
    assert_eq!(fresh["rows"][0]["text"], "new");
}

#[test]
fn reconnect_revokes_pending_publication_and_keeps_other_terminals() {
    let store = std::sync::Arc::new(std::sync::Mutex::new(HistoryStore::default()));
    let c = ClientRef::new();
    let t = TerminalRef::new();
    let other = TerminalRef::new();
    let now = Instant::now();
    let prepare = |terminal, slot| {
        store
            .lock()
            .unwrap()
            .reserve(
                &c,
                slot,
                target(terminal, "same-public-session-id"),
                HistoryBuffer::Primary,
                HistorySelection::All {},
                None,
                now,
            )
            .unwrap()
    };
    let r = prepare(&other, 1);
    let other_page = store
        .lock()
        .unwrap()
        .publish(
            r,
            capture(now, 1, Some(&["other", "intact"])),
            &args(1, 100),
            now,
        )
        .unwrap();
    let r = prepare(&t, 0);
    let old = store
        .lock()
        .unwrap()
        .publish(
            r,
            capture(now, 1, Some(&["old", "session"])),
            &args(1, 100),
            now,
        )
        .unwrap();
    let pending = prepare(&t, 0);
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    let worker_store = store.clone();
    let worker = std::thread::spawn(move || {
        let frozen = capture(now, 1, Some(&["pending", "old-session"]));
        ready_tx.send(()).unwrap();
        resume_rx.recv().unwrap();
        worker_store
            .lock()
            .unwrap()
            .publish(pending, frozen, &args(1, 100), now)
    });
    ready_rx.recv().unwrap();
    store.lock().unwrap().revoke_terminal(&t, true);
    resume_tx.send(()).unwrap();
    assert_eq!(
        worker.join().unwrap().unwrap_err(),
        HistoryError::TerminalUnavailable
    );
    let mut s = store.lock().unwrap();
    assert_eq!(
        s.page(&c, old["next_cursor"].as_str().unwrap(), &args(1, 100), now)
            .unwrap_err(),
        HistoryError::SnapshotUnavailable
    );
    assert_eq!(
        s.page(
            &c,
            other_page["next_cursor"].as_str().unwrap(),
            &args(1, 100),
            now
        )
        .unwrap()["rows"][0]["text"],
        "intact"
    );
    let r = s
        .reserve(
            &c,
            0,
            target(&t, "same-public-session-id"),
            HistoryBuffer::Primary,
            HistorySelection::All {},
            old["content_revision"].as_str(),
            now,
        )
        .unwrap();
    let mut new = capture(now, 1, Some(&["replacement", "shell"]));
    new.stamp.identity = 2;
    let page = s.publish(r, new, &args(2, 100), now).unwrap();
    assert_eq!(page["continuity"], "gap");
    assert_eq!(page["gap_reasons"], json!(["session_replaced"]));
    assert_eq!(page["rows"][0]["text"], "replacement");
}

#[test]
fn serialized_mcp_page_bounds_both_text_and_structured_content() {
    let mut s = HistoryStore::default();
    let c = ClientRef::new();
    let t = TerminalRef::new();
    let now = Instant::now();
    let r = s
        .reserve(
            &c,
            0,
            target(&t, "live"),
            HistoryBuffer::Primary,
            HistorySelection::All {},
            None,
            now,
        )
        .unwrap();
    let text = "\u{0001}\"\\中文".repeat(40_000);
    let page = s
        .publish(
            r,
            capture(now, 1, Some(&[&text, "end"])),
            &args(1000, 256 * 1024),
            now,
        )
        .unwrap();
    let fragment = page["rows"][0]["text"].as_str().unwrap();
    assert!(fragment.starts_with("\u{0001}\"\\中文"));
    assert_eq!(page["has_more"], true);
    let response =
        rmcp::model::CallToolResult::structured(json!({"outcome":"completed","data":page}));
    assert!(serde_json::to_vec(&response).unwrap().len() < 512 * 1024);
}

#[test]
fn small_pages_preserve_large_snapshot_budget_and_retryable_cursors() {
    let mut store = HistoryStore::default();
    let c = ClientRef::new();
    let t = TerminalRef::new();
    let now = Instant::now();
    let text = "abcd".repeat(15 * 1024 * 1024 / 4);
    let r = store
        .reserve(
            &c,
            0,
            target(&t, "live"),
            HistoryBuffer::Primary,
            HistorySelection::All {},
            None,
            now,
        )
        .unwrap();
    let first = store
        .publish(r, capture(now, 1, Some(&[&text, "end"])), &args(1, 4), now)
        .unwrap();
    let first_cursor = first["next_cursor"].as_str().unwrap().to_owned();
    let retry = store.page(&c, &first_cursor, &args(1, 4), now).unwrap();
    let mut page = first;
    let mut offset = 0;
    let mut pages = 0;
    loop {
        let fragment = &page["rows"][0];
        if fragment["row_index"] == 0 {
            let chunk = fragment["text"].as_str().unwrap();
            assert_eq!(fragment["byte_start"], offset);
            assert_eq!(chunk, &text[offset..offset + chunk.len()]);
            offset += chunk.len();
        } else {
            assert_eq!(fragment["text"], "end");
            assert_eq!(page["has_more"], false);
        }
        pages += 1;
        if !page["has_more"].as_bool().unwrap() {
            break;
        }
        let limits = if pages < 8192 {
            args(1, 4)
        } else {
            args(1, 256 * 1024)
        };
        page = store
            .page(&c, page["next_cursor"].as_str().unwrap(), &limits, now)
            .unwrap();
    }
    assert_eq!(offset, text.len());
    assert_eq!(
        store.page(&c, &first_cursor, &args(1, 4), now).unwrap(),
        retry
    );
    let mut forged = first_cursor.into_bytes();
    let last = forged.last_mut().unwrap();
    *last = if *last == b'0' { b'1' } else { b'0' };
    assert_eq!(
        store
            .page(&c, std::str::from_utf8(&forged).unwrap(), &args(1, 4), now)
            .unwrap_err(),
        HistoryError::SnapshotUnavailable
    );
}

#[test]
fn revoked_pending_text_keeps_budget_until_destroyed() {
    for revoke_client in [false, true] {
        let mut store = HistoryStore::default();
        let c = ClientRef::new();
        let t = TerminalRef::new();
        let now = Instant::now();
        let mut pending = Vec::new();
        for _ in 0..8 {
            let r = store
                .reserve(
                    &c,
                    0,
                    target(&t, "live"),
                    HistoryBuffer::Primary,
                    HistorySelection::All {},
                    None,
                    now,
                )
                .unwrap();
            pending.push((r, capture(now, 1, Some(&["paused", "old"]))));
            if revoke_client {
                store.revoke_client(&c);
            } else {
                store.revoke_terminal(&t, true);
            }
        }
        assert!(matches!(
            store.reserve(
                &c,
                0,
                target(&t, "new"),
                HistoryBuffer::Primary,
                HistorySelection::All {},
                None,
                now
            ),
            Err(HistoryError::ResourceLimit)
        ));
        let (r, frozen) = pending.pop().unwrap();
        assert_eq!(
            store.publish(r, frozen, &args(2, 100), now).unwrap_err(),
            HistoryError::TerminalUnavailable
        );
        let fresh = store
            .reserve(
                &c,
                0,
                target(&t, "new"),
                HistoryBuffer::Primary,
                HistorySelection::All {},
                None,
                now,
            )
            .unwrap();
        for (r, frozen) in pending {
            assert_eq!(
                store.publish(r, frozen, &args(2, 100), now).unwrap_err(),
                HistoryError::TerminalUnavailable
            );
        }
        let page = store
            .publish(
                fresh,
                capture(now, 2, Some(&["current", "grant"])),
                &args(2, 100),
                now,
            )
            .unwrap();
        assert_eq!(page["rows"][0]["text"], "current");
    }
}
