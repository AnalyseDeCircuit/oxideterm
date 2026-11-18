#!/usr/bin/env python3
"""Zero-compile static audit for Rust test quality across diffs or directories."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


DEFAULT_SOURCE_ROOT = Path("crates")
VENDORED_CRATE_DIRS = frozenset(
    {
        "alacritty-terminal",
        "fernomade",
        "gpui-ce",
        "russh-sftp",
        "vte",
    }
)

TEST_ATTR_RE = re.compile(
    r"^#\[\s*(?:test|gpui::test|tokio::test|async_std::test|rstest)\b"
)
ATTR_LINE_RE = re.compile(r"^#\[")
FN_SIG_RE = re.compile(
    r"\b(?:pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^>]*>)?\s*\("
)
SHOULD_PANIC_RE = re.compile(r"#\[\s*(?:should_panic|ignore)\b")
BENCHMARK_NAME_RE = re.compile(r"(?:_benchmark|_overhead|_memory|_cost)$")
ASSERT_MACRO_RE = re.compile(
    r"\b(assert|assert_eq|assert_ne|assert_matches|debug_assert|debug_assert_eq|debug_assert_ne)\s*!\s*\("
)
ASSERT_HELPER_RE = re.compile(
    r"\b(?:assert|verify|check|expect|wait_for)_[A-Za-z0-9_]+\s*\(|\b(?:panic|unreachable)\s*!\s*\("
)

WEAK_ASSERT_EXPR_RE = re.compile(
    r"""^(?:
        !\s*[A-Za-z0-9_.:()?\[\]&*\s]+\.(?:is_empty|is_some|is_ok)\(\s*\)
        |[A-Za-z0-9_.:()?\[\]&*\s]+\.(?:is_some|is_ok)\(\s*\)
        |(?:matches!\s*\([^,]+,\s*(?:Ok|Some)\s*\(\s*_\s*\)\s*\))
    )$""",
    re.VERBOSE,
)
WEAK_LEN_COMPARE_RE = re.compile(r"^[A-Za-z0-9_.:()?\[\]&*\s]+\.len\(\s*\)$")
LITERAL_BOOL_RE = re.compile(r"^(?:true|false)$")
SIMPLE_LITERAL_RE = re.compile(
    r"""^(?:true|false|-?\d+(?:_\d+)*(?:[ui](?:8|16|32|64|128|size)|f(?:32|64))?|"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')$"""
)

CONCURRENCY_NAME_RE = re.compile(
    r"(?:^|_)(?:concurrent|concurrency|race|wakeup|wake_up|reentrant|out_of_order)(?:$|_)"
)
CONCURRENCY_EVIDENCE_RE = re.compile(
    r"\b(?:spawn|spawn_blocking|thread|tokio|executor|run_until_parked|select!|barrier|Barrier|"
    r"Notify|notify_one|notify_waiters|CancellationToken|try_recv|oneshot|mpsc|Mutex|RwLock|"
    r"Atomic[A-Za-z0-9_]*|JoinHandle|FuturesUnordered|park|unpark|poll|wake)\b"
)
ROUNDTRIP_FIELD_READ_RE = re.compile(
    r"\bassert_eq!\s*\(\s*[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*(?:\(\s*\))?\s*,"
)
METHOD_MUTATION_RE = re.compile(
    r"\.[A-Za-z_][A-Za-z0-9_]*\s*\([^\s)]"
)
SELF_CALL_RE = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(")


@dataclass(frozen=True)
class ParsedTest:
    path: Path
    name: str
    attr_start_line: int
    body_start_line: int
    end_line: int
    attrs_text: str
    body_text: str
    body_code_only: str


@dataclass(frozen=True)
class Finding:
    severity: str  # "error" or "warn"
    rule: str
    path: Path
    line: int
    test_name: str
    message: str


def strip_rust_comments_and_strings(text: str, keep_strings: bool = False) -> str:
    """Replace Rust comments (and optionally string literals) with spaces while keeping newlines."""
    out: list[str] = []
    i = 0
    n = len(text)
    block_comment_depth = 0
    in_line_comment = False

    while i < n:
        ch = text[i]

        if in_line_comment:
            if ch == "\n":
                in_line_comment = False
                out.append(ch)
            else:
                out.append(" ")
            i += 1
            continue

        if block_comment_depth > 0:
            if ch == "/" and i + 1 < n and text[i + 1] == "*":
                block_comment_depth += 1
                out.extend("  ")
                i += 2
                continue
            if ch == "*" and i + 1 < n and text[i + 1] == "/":
                block_comment_depth -= 1
                out.extend("  ")
                i += 2
                continue
            out.append("\n" if ch == "\n" else " ")
            i += 1
            continue

        if ch == "/" and i + 1 < n:
            nxt = text[i + 1]
            if nxt == "/":
                in_line_comment = True
                out.extend("  ")
                i += 2
                continue
            if nxt == "*":
                block_comment_depth = 1
                out.extend("  ")
                i += 2
                continue

        # Raw string literal: r"...", r#"..."#, br#"..."#
        if ch in ("r", "b") and i + 1 < n:
            j = i + 1
            if ch == "b" and j < n and text[j] == "r":
                j += 1
            if ch == "r" or j > i + 1:
                hashes = 0
                k = j
                while k < n and text[k] == "#":
                    hashes += 1
                    k += 1
                if k < n and text[k] == '"':
                    end_delim = '"' + ("#" * hashes)
                    end_idx = text.find(end_delim, k + 1)
                    if end_idx != -1:
                        slice_end = end_idx + len(end_delim)
                        seg = text[i:slice_end]
                        if keep_strings:
                            out.append(seg)
                        else:
                            out.append("".join("\n" if c == "\n" else " " for c in seg))
                        i = slice_end
                        continue

        # Normal string or byte string literal: "..." or b"..."
        if ch == '"' or (ch == "b" and i + 1 < n and text[i + 1] == '"'):
            start = i
            i += 2 if ch == "b" else 1
            while i < n:
                c = text[i]
                if c == "\\":
                    i += 2
                    continue
                if c == '"':
                    i += 1
                    break
                i += 1
            seg = text[start:i]
            if keep_strings:
                out.append(seg)
            else:
                out.append("".join("\n" if c == "\n" else " " for c in seg))
            continue

        # Char literal vs lifetime: distinguish 'a' or '\n' from 'a: / &'a
        if ch == "'" and i + 1 < n:
            if text[i + 1] == "\\":
                end = i + 2
                while end < n and text[end] not in ("'", "\n"):
                    end += 1
                if end < n and text[end] == "'":
                    end += 1
                    seg = text[i:end]
                    out.append(seg if keep_strings else (" " * len(seg)))
                    i = end
                    continue
            elif i + 2 < n and text[i + 2] == "'":
                seg = text[i : i + 3]
                out.append(seg if keep_strings else "   ")
                i += 3
                continue

        out.append(ch)
        i += 1

    return "".join(out)


def find_matching_delim(text: str, open_idx: int, open_ch: str, close_ch: str) -> int | None:
    """Find matching closing delimiter starting from an opening delimiter index."""
    depth = 0
    i = open_idx
    n = len(text)
    in_string = False
    while i < n:
        ch = text[i]
        if in_string:
            if ch == "\\":
                i += 2
                continue
            if ch == '"':
                in_string = False
            i += 1
            continue
        if ch == '"':
            in_string = True
            i += 1
            continue
        if ch == open_ch:
            depth += 1
        elif ch == close_ch:
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return None


def parse_rust_tests(path: Path, source: str) -> list[ParsedTest]:
    """Extract all #[test]-like functions in a Rust source file."""
    code_only = strip_rust_comments_and_strings(source, keep_strings=False)
    code_with_strings = strip_rust_comments_and_strings(source, keep_strings=True)
    lines = code_only.splitlines()
    raw_lines = code_with_strings.splitlines()

    tests: list[ParsedTest] = []
    i = 0
    total = len(lines)
    while i < total:
        stripped = lines[i].lstrip()
        if not TEST_ATTR_RE.match(stripped):
            i += 1
            continue

        attr_start = i
        j = i
        attr_chunks: list[str] = []
        # Consume contiguous attributes preceding the test function signature.
        while j < total:
            cur = lines[j].lstrip()
            if not cur:
                j += 1
                continue
            if ATTR_LINE_RE.match(cur):
                attr_chunks.append(raw_lines[j])
                bracket_balance = lines[j].count("[") - lines[j].count("]")
                while bracket_balance > 0 and j + 1 < total:
                    j += 1
                    attr_chunks.append(raw_lines[j])
                    bracket_balance += lines[j].count("[") - lines[j].count("]")
                j += 1
                continue
            break

        if j >= total:
            i += 1
            continue

        # Collect signature lines up to the opening `{` of the test body.
        sig_start = j
        sig_end = j
        open_brace_line = None
        open_brace_col = None
        paren_depth = 0
        while sig_end < total:
            line_text = lines[sig_end]
            for col, ch in enumerate(line_text):
                if ch == "(":
                    paren_depth += 1
                elif ch == ")":
                    paren_depth = max(0, paren_depth - 1)
                elif ch == "{" and paren_depth == 0:
                    open_brace_line = sig_end
                    open_brace_col = col
                    break
                elif ch == ";" and paren_depth == 0:
                    open_brace_line = -1
                    break
            if open_brace_line is not None:
                break
            sig_end += 1

        if open_brace_line is None or open_brace_line == -1 or open_brace_col is None:
            i = sig_end + 1
            continue

        sig_text = "\n".join(lines[sig_start : open_brace_line + 1])
        fn_match = FN_SIG_RE.search(sig_text)
        if not fn_match:
            i = open_brace_line + 1
            continue

        fn_name = fn_match.group(1)

        # Locate closing brace in line/offset coordinates using code_with_strings so strings
        # stay balanced accurately while comments are already stripped.
        line_offsets: list[int] = []
        offset = 0
        for line in raw_lines:
            line_offsets.append(offset)
            offset += len(line) + 1

        brace_global = line_offsets[open_brace_line] + open_brace_col
        close_global = find_matching_delim(code_only, brace_global, "{", "}")
        if close_global is None:
            i = open_brace_line + 1
            continue

        end_line_0 = code_only.count("\n", 0, close_global)
        body_code_only = code_only[brace_global + 1 : close_global]
        body_text = code_with_strings[brace_global + 1 : close_global]

        tests.append(
            ParsedTest(
                path=path,
                name=fn_name,
                attr_start_line=attr_start + 1,
                body_start_line=open_brace_line + 1,
                end_line=end_line_0 + 1,
                attrs_text="\n".join(attr_chunks),
                body_text=body_text,
                body_code_only=body_code_only,
            )
        )
        i = end_line_0 + 1

    return tests


def split_top_level_args(expr: str) -> list[str]:
    """Split macro arguments on top-level commas."""
    args: list[str] = []
    start = 0
    paren = bracket = brace = 0
    in_string = False
    i = 0
    n = len(expr)
    while i < n:
        ch = expr[i]
        if in_string:
            if ch == "\\":
                i += 2
                continue
            if ch == '"':
                in_string = False
            i += 1
            continue
        if ch == '"':
            in_string = True
        elif ch == "(":
            paren += 1
        elif ch == ")":
            paren = max(0, paren - 1)
        elif ch == "[":
            bracket += 1
        elif ch == "]":
            bracket = max(0, bracket - 1)
        elif ch == "{":
            brace += 1
        elif ch == "}":
            brace = max(0, brace - 1)
        elif ch == "," and paren == 0 and bracket == 0 and brace == 0:
            args.append(expr[start:i].strip())
            start = i + 1
        i += 1
    tail = expr[start:].strip()
    if tail:
        args.append(tail)
    return args


def normalize_ws(text: str) -> str:
    return re.sub(r"\s+", " ", text.strip())


def extract_assert_calls(body_code_only: str, body_text: str) -> list[tuple[str, list[str]]]:
    calls: list[tuple[str, list[str]]] = []
    pos = 0
    while True:
        match = ASSERT_MACRO_RE.search(body_code_only, pos)
        if not match:
            break
        macro_name = match.group(1)
        open_paren = match.end() - 1
        close_paren = find_matching_delim(body_code_only, open_paren, "(", ")")
        if close_paren is None:
            pos = match.end()
            continue
        inner = body_text[open_paren + 1 : close_paren]
        calls.append((macro_name, split_top_level_args(inner)))
        pos = close_paren + 1
    return calls


def is_weak_assertion(macro_name: str, args: list[str]) -> bool:
    if macro_name in ("assert", "debug_assert"):
        if not args:
            return True
        expr = normalize_ws(args[0])
        return bool(WEAK_ASSERT_EXPR_RE.match(expr))
    if macro_name in ("assert_eq", "debug_assert_eq", "assert_ne", "debug_assert_ne"):
        if len(args) < 2:
            return True
        lhs = normalize_ws(args[0])
        rhs = normalize_ws(args[1])
        # Pure length non-zero checks like assert_ne!(vec.len(), 0)
        if WEAK_LEN_COMPARE_RE.match(lhs) and macro_name.endswith("_ne") and rhs in ("0", "0usize"):
            return True
        if WEAK_LEN_COMPARE_RE.match(rhs) and macro_name.endswith("_ne") and lhs in ("0", "0usize"):
            return True
    return False


def is_trivial_assertion(macro_name: str, args: list[str]) -> bool:
    if macro_name in ("assert", "debug_assert") and args:
        return bool(LITERAL_BOOL_RE.match(normalize_ws(args[0])))
    if macro_name in ("assert_eq", "debug_assert_eq", "assert_ne", "debug_assert_ne") and len(args) >= 2:
        lhs = normalize_ws(args[0])
        rhs = normalize_ws(args[1])
        if lhs == rhs:
            return True
        if SIMPLE_LITERAL_RE.match(lhs) and SIMPLE_LITERAL_RE.match(rhs):
            return True
    return False


def audit_parsed_test(test: ParsedTest) -> list[Finding]:
    findings: list[Finding] = []
    should_panic = bool(SHOULD_PANIC_RE.search(test.attrs_text))
    calls = extract_assert_calls(test.body_code_only, test.body_text)
    has_helper_assert = bool(ASSERT_HELPER_RE.search(test.body_code_only))

    if not should_panic and not BENCHMARK_NAME_RE.search(test.name) and not calls and not has_helper_assert:
        findings.append(
            Finding(
                severity="error",
                rule="no-assertions",
                path=test.path,
                line=test.body_start_line,
                test_name=test.name,
                message="Test contains no assertion macro or assert_* helper and is not #[should_panic].",
            )
        )
        return findings

    for macro_name, args in calls:
        if is_trivial_assertion(macro_name, args):
            findings.append(
                Finding(
                    severity="error",
                    rule="trivial-assertion",
                    path=test.path,
                    line=test.body_start_line,
                    test_name=test.name,
                    message=f"Tautological or constant literal assertion in {macro_name}!(...).",
                )
            )

    if calls and not has_helper_assert and all(is_weak_assertion(m, a) for m, a in calls):
        findings.append(
            Finding(
                severity="error",
                rule="weak-assertions-only",
                path=test.path,
                line=test.body_start_line,
                test_name=test.name,
                message="All assertions only check existence/Ok/Some/non-empty without verifying concrete behavior, content, or order.",
            )
        )

    if CONCURRENCY_NAME_RE.search(test.name) and not CONCURRENCY_EVIDENCE_RE.search(test.body_text):
        findings.append(
            Finding(
                severity="error",
                rule="placeholder-concurrency-or-cancel",
                path=test.path,
                line=test.body_start_line,
                test_name=test.name,
                message="Test name claims concurrency, cancellation, wakeup, or stale handling without constructing threads, tasks, or cancellation primitives.",
            )
        )

    non_empty_lines = [line for line in test.body_text.splitlines() if line.strip()]
    if (
        len(non_empty_lines) <= 18
        and ROUNDTRIP_FIELD_READ_RE.search(test.body_text)
        and "::new(" in test.body_text
        and not METHOD_MUTATION_RE.search(test.body_text)
        and not any(m.group(1) not in ("new", "default", "from") for m in SELF_CALL_RE.finditer(test.body_text))
    ):
        findings.append(
            Finding(
                severity="warn",
                rule="constructor-roundtrip-suspect",
                path=test.path,
                line=test.body_start_line,
                test_name=test.name,
                message="Short test only constructs via ::new(...) and checks field equality; verify this guards a real default contract rather than constructor assignment readback.",
            )
        )

    return findings


def git_changed_lines_by_file(base: str | None) -> tuple[dict[Path, set[int]], set[Path]]:
    """Return changed/added line numbers (1-based) in HEAD/working-tree diff and untracked Rust files."""
    diff_target = base if base else "HEAD"
    cmd = ["git", "diff", "--unified=0", diff_target, "--", "*.rs"]
    proc = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if proc.returncode not in (0, 1):
        raise RuntimeError(f"git diff failed: {proc.stderr.strip()}")

    changed: dict[Path, set[int]] = {}
    current_path: Path | None = None
    hunk_re = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@")
    for line in proc.stdout.splitlines():
        if line.startswith("+++ b/"):
            current_path = Path(line[6:])
            changed.setdefault(current_path, set())
            continue
        if current_path is not None and line.startswith("@@ "):
            m = hunk_re.match(line)
            if m:
                start = int(m.group(1))
                count = int(m.group(2)) if m.group(2) is not None else 1
                if count > 0:
                    changed[current_path].update(range(start, start + count))

    untracked_cmd = ["git", "ls-files", "--others", "--exclude-standard", "--", "*.rs"]
    untracked_proc = subprocess.run(untracked_cmd, capture_output=True, text=True, check=False)
    untracked: set[Path] = set()
    if untracked_proc.returncode == 0:
        for rel in untracked_proc.stdout.splitlines():
            rel = rel.strip()
            if rel:
                untracked.add(Path(rel))

    return changed, untracked


def count_brand_new_tests(base: str | None, tests: list[ParsedTest]) -> list[ParsedTest]:
    """Identify parsed tests whose test attribute line did not exist in the comparison base."""
    diff_target = base if base else "HEAD"
    brand_new: list[ParsedTest] = []
    by_path: dict[Path, list[ParsedTest]] = {}
    for test in tests:
        by_path.setdefault(test.path, []).append(test)

    for path, file_tests in by_path.items():
        show = subprocess.run(
            ["git", "show", f"{diff_target}:{path.as_posix()}"],
            capture_output=True,
            text=True,
            check=False,
        )
        if show.returncode != 0:
            brand_new.extend(file_tests)
            continue
        old_names = {t.name for t in parse_rust_tests(path, show.stdout)}
        for test in file_tests:
            if test.name not in old_names:
                brand_new.append(test)
    return brand_new


def collect_target_tests(
    paths: list[Path],
    scan_all: bool,
    base: str | None,
) -> tuple[list[ParsedTest], list[ParsedTest]]:
    """Collect tests under scope and identify which ones are newly added vs baseline."""
    selected: list[ParsedTest] = []
    if scan_all or paths:
        roots = paths if paths else [DEFAULT_SOURCE_ROOT]
        files: list[Path] = []
        for root in roots:
            if root.is_file():
                files.append(root)
            elif root.is_dir():
                for file_path in sorted(root.rglob("*.rs")):
                    if scan_all and root == DEFAULT_SOURCE_ROOT:
                        parts = file_path.relative_to(DEFAULT_SOURCE_ROOT).parts
                        if parts and parts[0] in VENDORED_CRATE_DIRS:
                            continue
                    files.append(file_path)
        for file_path in files:
            source = file_path.read_text(encoding="utf-8", errors="replace")
            selected.extend(parse_rust_tests(file_path, source))
        brand_new = count_brand_new_tests(base, selected) if not scan_all else []
        return selected, brand_new

    changed_lines, untracked = git_changed_lines_by_file(base)
    touched_paths = sorted(set(changed_lines.keys()) | untracked)
    for file_path in touched_paths:
        if not file_path.exists():
            continue
        source = file_path.read_text(encoding="utf-8", errors="replace")
        file_tests = parse_rust_tests(file_path, source)
        if file_path in untracked:
            selected.extend(file_tests)
        else:
            lines = changed_lines.get(file_path, set())
            for test in file_tests:
                if any(test.attr_start_line <= ln <= test.end_line for ln in lines):
                    selected.append(test)
    brand_new = count_brand_new_tests(base, selected)
    return selected, brand_new


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Static quality audit for Rust #[test] / #[gpui::test] functions."
    )
    parser.add_argument(
        "--path",
        type=Path,
        action="append",
        default=[],
        help="File or directory to audit directly instead of git diff.",
    )
    parser.add_argument(
        "--all",
        action="store_true",
        help="Audit all Rust files under crates/ instead of only git diff.",
    )
    parser.add_argument(
        "--base",
        type=str,
        default=None,
        help="Git revision to diff against (default: HEAD).",
    )
    parser.add_argument(
        "--max-new-tests",
        type=int,
        default=None,
        help="Fail if more than N brand-new test functions are added relative to base.",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Treat warnings as errors.",
    )
    parser.add_argument(
        "--show-warnings",
        action="store_true",
        help="Print warning details (otherwise only the count is shown).",
    )
    parser.add_argument(
        "--show-new-tests",
        action="store_true",
        help="Print brand-new test locations and names (otherwise only the count is shown).",
    )
    parser.add_argument(
        "--show-all",
        action="store_true",
        help="Print all errors, warnings, and brand-new tests without truncation.",
    )
    return parser.parse_args()


def format_limited_section(
    title: str,
    items: list[str],
    limit: int | None,
    *,
    show_items: bool,
) -> list[str]:
    if not show_items:
        return [f"{title}: {len(items)}"]
    lines = [f"{title}: {len(items)}"]
    visible = items if limit is None else items[:limit]
    lines.extend(f"  - {item}" for item in visible)
    if limit is not None and len(items) > limit:
        lines.append(f"  ... {len(items) - limit} more")
    return lines


def format_finding(finding: Finding) -> str:
    return (
        f"[{finding.severity.upper()}] {finding.path}:{finding.line} "
        f"({finding.test_name}) [{finding.rule}] {finding.message}"
    )


def render_report(
    tests: list[ParsedTest],
    brand_new: list[ParsedTest],
    findings: list[Finding],
    *,
    scan_all: bool,
    max_new_tests: int | None,
    strict: bool,
    show_warnings: bool,
    show_new_tests: bool,
    show_all: bool,
) -> tuple[list[str], int]:
    limit = None if show_all else 30
    errors = [f for f in findings if f.severity == "error"]
    warnings = [f for f in findings if f.severity == "warn"]
    over_limit = max_new_tests is not None and len(brand_new) > max_new_tests

    out: list[str] = [f"Audited tests: {len(tests)}"]

    if not scan_all:
        new_items = [
            f"{test.path}:{test.body_start_line} {test.name}" for test in brand_new
        ]
        out.extend(
            format_limited_section(
                "Brand-new tests in scope",
                new_items,
                limit,
                show_items=show_new_tests or show_all or over_limit,
            )
        )

    error_lines = [format_finding(f) for f in errors]
    if over_limit:
        error_lines.append(
            f"[ERROR] New tests added ({len(brand_new)}) exceed --max-new-tests {max_new_tests}."
        )
    out.extend(format_limited_section("Errors", error_lines, limit, show_items=True))

    warning_lines = [format_finding(f) for f in warnings]
    out.extend(
        format_limited_section(
            "Warnings",
            warning_lines,
            limit,
            show_items=show_warnings or show_all or strict,
        )
    )

    exit_code = 1 if (error_lines or (strict and warning_lines)) else 0
    return out, exit_code


def main() -> int:
    args = parse_args()
    tests, brand_new = collect_target_tests(args.path, args.all, args.base)

    findings: list[Finding] = []
    for test in tests:
        findings.extend(audit_parsed_test(test))

    lines, exit_code = render_report(
        tests,
        brand_new,
        findings,
        scan_all=args.all,
        max_new_tests=args.max_new_tests,
        strict=args.strict,
        show_warnings=args.show_warnings,
        show_new_tests=args.show_new_tests,
        show_all=args.show_all,
    )
    print("\n".join(lines))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
