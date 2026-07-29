#!/usr/bin/env python3
"""Tests for the WorkspaceApp ownership audit."""

from pathlib import Path
import sys
import unittest

# Import the audit helper from the scripts root without making scripts a package.
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import audit_workspace_app


class WorkspaceStructTests(unittest.TestCase):
    def test_multiline_generic_fields_are_counted_once(self) -> None:
        source = """
pub(crate) struct WorkspaceApp {
    first: usize,
    second:
        std::sync::mpsc::Receiver<Result<String, String>>,
    nested: HashMap<String, (usize, usize)>,
}
"""

        fields, struct_lines = audit_workspace_app.collect_workspace_struct(source)

        self.assertEqual([name for name, _ in fields], ["first", "second", "nested"])
        self.assertEqual(struct_lines, 6)

    def test_channel_fields_include_alias_receivers(self) -> None:
        fields = [
            ("worker_tx", "std::sync::mpsc::Sender<Event>"),
            ("worker_rx", "std::sync::mpsc::Receiver<Event>"),
            ("node_events", "NodeEventReceiver"),
            ("unrelated", "String"),
        ]

        metrics = audit_workspace_app.collect_channel_fields(fields)

        self.assertEqual(metrics.sender_names, ("worker_tx",))
        self.assertEqual(metrics.receiver_names, ("worker_rx", "node_events"))

    def test_shared_session_struct_is_supported(self) -> None:
        source = """
pub(crate) struct WorkspaceSession {
    session_id: u64,
    events: std::sync::mpsc::Receiver<Event>,
}
"""

        fields, struct_lines = audit_workspace_app.collect_workspace_struct(source)

        self.assertEqual([name for name, _ in fields], ["session_id", "events"])
        self.assertEqual(struct_lines, 4)


class RootDispatchTests(unittest.TestCase):
    def test_render_dispatch_counts_only_root_render_method(self) -> None:
        source = """
impl Render for WorkspaceApp {
    fn render(&mut self) {
        self.poll_worker();
        self.maybe_refresh_page();
        let _ = self.receiver.try_recv();
        let _ = self.receiver.recv();
    }

    fn helper(&mut self) {
        self.poll_not_render();
    }
}
"""

        metrics = audit_workspace_app.collect_root_render_metrics(source)

        self.assertEqual(metrics.poll_calls, 1)
        self.assertEqual(metrics.refresh_calls, 1)
        self.assertEqual(metrics.try_recv_calls, 1)
        self.assertEqual(metrics.recv_calls, 1)
        self.assertEqual(
            metrics.dispatch_calls,
            ("poll_worker", "maybe_refresh_page"),
        )

    def test_window_shell_render_takes_priority_over_legacy_app_render(self) -> None:
        legacy_source = """
impl Render for WorkspaceApp {
    fn render(&mut self) {
        self.poll_legacy_worker();
    }
}
"""
        shell_source = """
impl Render for WorkspaceWindowShell {
    fn render(&mut self) {
        self.poll_shell_worker();
        let _ = self.receiver.try_recv();
    }
}
"""

        metrics = audit_workspace_app.collect_root_render_metrics(
            legacy_source, shell_source
        )

        self.assertEqual(metrics.poll_calls, 1)
        self.assertEqual(metrics.try_recv_calls, 1)
        self.assertEqual(metrics.dispatch_calls, ("poll_shell_worker",))

    def test_heartbeat_calls_are_scoped_to_workspace_update(self) -> None:
        source = """
Timer::after(Duration::from_millis(530)).await;
weak.update(cx, |workspace, cx| {
    workspace.poll_worker(cx);
    if workspace.should_refresh() {
        workspace.refresh(cx);
    }
});
workspace.outside_the_heartbeat();
"""

        calls = audit_workspace_app.collect_heartbeat_calls(source)

        self.assertEqual(calls, ("poll_worker", "should_refresh", "refresh"))


if __name__ == "__main__":
    unittest.main()
