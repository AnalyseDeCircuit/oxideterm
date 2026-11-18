#!/usr/bin/env python3
"""Tests for the static Rust test quality audit script."""

from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "quality"))

import audit_tests


class AuditTestsUnitTests(unittest.TestCase):
    def test_weak_assertions_only_flags_is_ok_and_is_some_without_value_check(self) -> None:
        source = """
#[test]
fn weak_only() {
    let out = parse_item("abc");
    assert!(out.is_ok());
    assert!(!out.unwrap().is_empty());
}
"""
        parsed = audit_tests.parse_rust_tests(Path("src/lib.rs"), source)
        self.assertEqual([t.name for t in parsed], ["weak_only"])
        findings = audit_tests.audit_parsed_test(parsed[0])
        self.assertEqual([f.rule for f in findings], ["weak-assertions-only"])

    def test_strong_equality_alongside_is_some_passes(self) -> None:
        source = """
#[gpui::test]
fn strong_plus_bounds() {
    assert!(cx.debug_bounds("button").is_some());
    assert_eq!(dispatched, vec![Action::Open]);
}
"""
        parsed = audit_tests.parse_rust_tests(Path("src/view.rs"), source)
        self.assertEqual(len(parsed), 1)
        self.assertEqual(audit_tests.audit_parsed_test(parsed[0]), [])

    def test_placeholder_concurrency_name_without_async_or_threads_fails(self) -> None:
        source = """
#[test]
fn concurrent_workers_keep_state_isolated() {
    let state = State::default();
    assert_eq!(state.active(), false);
}
"""
        parsed = audit_tests.parse_rust_tests(Path("src/state.rs"), source)
        findings = audit_tests.audit_parsed_test(parsed[0])
        self.assertEqual([f.rule for f in findings], ["placeholder-concurrency-or-cancel"])

    def test_match_panic_and_ignored_benchmarks_do_not_false_positive(self) -> None:
        source = """
#[test]
fn match_with_panic_arm_counts_as_assertion() {
    let res = parse("secret");
    match res {
        Auth::Saved { password: None } => {}
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
#[ignore]
fn manual_smoke_run() {
    exercise_device();
}
"""
        parsed = audit_tests.parse_rust_tests(Path("src/auth.rs"), source)
        self.assertEqual(len(parsed), 2)
        self.assertEqual(audit_tests.audit_parsed_test(parsed[0]), [])
        self.assertEqual(audit_tests.audit_parsed_test(parsed[1]), [])

    def test_raw_string_with_escaped_quotes_and_braces_parses_assertions(self) -> None:
        source = r'''
#[test]
fn raw_json_preview() {
    let payload = r#"{"cmd":"echo \"{\"","cwd":"/tmp"}"#;
    let parsed = parse_json(payload);
    assert_eq!(parsed["cwd"], "/tmp");
}
'''
        parsed = audit_tests.parse_rust_tests(Path("src/msg.rs"), source)
        self.assertEqual([t.name for t in parsed], ["raw_json_preview"])
        self.assertEqual(audit_tests.audit_parsed_test(parsed[0]), [])

    def test_report_shows_counts_by_default_and_details_with_flags(self) -> None:
        source = """
#[test]
fn roundtrip_only() {
    let item = Config::new();
    assert_eq!(item.flag, false);
}
"""
        parsed = audit_tests.parse_rust_tests(Path("src/cfg.rs"), source)
        findings = audit_tests.audit_parsed_test(parsed[0])

        compact_lines, code = audit_tests.render_report(
            parsed,
            parsed,
            findings,
            scan_all=False,
            max_new_tests=None,
            strict=False,
            show_warnings=False,
            show_new_tests=False,
            show_all=False,
        )
        self.assertEqual(code, 0)
        self.assertEqual(
            compact_lines,
            [
                "Audited tests: 1",
                "Brand-new tests in scope: 1",
                "Errors: 0",
                "Warnings: 1",
            ],
        )

        verbose_lines, _ = audit_tests.render_report(
            parsed,
            parsed,
            findings,
            scan_all=False,
            max_new_tests=None,
            strict=False,
            show_warnings=True,
            show_new_tests=True,
            show_all=False,
        )
        self.assertTrue(any("roundtrip_only" in line for line in verbose_lines))
        self.assertTrue(any("constructor-roundtrip-suspect" in line for line in verbose_lines))

    def test_trivial_tautology_and_no_assertion_tests_are_caught(self) -> None:
        source = """
#[test]
fn missing_asserts() {
    let _ = build_something();
}

#[test]
fn self_equal() {
    let value = compute(42);
    assert_eq!(value, value);
}

#[test]
#[should_panic(expected = "bad input")]
fn panic_contract() {
    parse_strict("bad");
}
"""
        parsed = audit_tests.parse_rust_tests(Path("src/mod.rs"), source)
        self.assertEqual(len(parsed), 3)
        self.assertEqual([f.rule for f in audit_tests.audit_parsed_test(parsed[0])], ["no-assertions"])
        self.assertEqual([f.rule for f in audit_tests.audit_parsed_test(parsed[1])], ["trivial-assertion"])
        self.assertEqual(audit_tests.audit_parsed_test(parsed[2]), [])


if __name__ == "__main__":
    unittest.main()
