#!/usr/bin/env python3
"""Audit WorkspaceApp ownership and root-dispatch structure."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable


WORKSPACE_FILE = Path("crates/oxideterm-gpui-app/src/workspace.rs")
WORKSPACE_DIRECTORY = Path("crates/oxideterm-gpui-app/src/workspace")
ROOT_RENDER_FILE = WORKSPACE_DIRECTORY / "root/render.rs"
WINDOW_SHELL_FILE = WORKSPACE_DIRECTORY / "window_shell.rs"
ROOT_INIT_FILE = WORKSPACE_DIRECTORY / "root/init.rs"
WORKSPACE_STRUCT_PATTERNS = (
    re.compile(r"pub\(crate\)\s+struct\s+WorkspaceApp\s*\{"),
    re.compile(r"pub\(crate\)\s+struct\s+WorkspaceSession\s*\{"),
)
WORKSPACE_IMPL_PATTERN = re.compile(r"(?m)^\s*impl\s+WorkspaceApp\b")
ROOT_RENDER_OWNER_PATTERNS = (
    (
        "WorkspaceWindowShell",
        re.compile(r"impl\s+Render\s+for\s+WorkspaceWindowShell\s*\{"),
    ),
    ("WorkspaceApp", re.compile(r"impl\s+Render\s+for\s+WorkspaceApp\s*\{")),
)
HEARTBEAT_ANCHOR = "Timer::after(Duration::from_millis(530)).await;"
HIGH_LOAD_AREAS = {
    "host_tools": (WORKSPACE_DIRECTORY / "connection_monitor",),
    "remote_desktop": (WORKSPACE_DIRECTORY / "remote_desktop",),
    "ai": (
        WORKSPACE_DIRECTORY / "sidebar/ai",
        WORKSPACE_DIRECTORY / "settings/ai",
    ),
    "plugins": (
        WORKSPACE_DIRECTORY / "plugin_lifecycle",
        WORKSPACE_DIRECTORY / "plugin_manager.rs",
        WORKSPACE_DIRECTORY / "plugin_ui.rs",
    ),
    "forwarding": (WORKSPACE_DIRECTORY / "forwards",),
}


@dataclass(frozen=True)
class RenderDispatchMetrics:
    poll_calls: int
    refresh_calls: int
    try_recv_calls: int
    recv_calls: int
    dispatch_calls: tuple[str, ...]


@dataclass(frozen=True)
class ChannelFieldMetrics:
    receiver_fields: int
    sender_fields: int
    receiver_names: tuple[str, ...]
    sender_names: tuple[str, ...]


@dataclass(frozen=True)
class AuditMetrics:
    workspace_app_fields: int
    workspace_app_struct_lines: int
    workspace_impl_blocks: int
    workspace_impl_files: int
    workspace_rs_lines: int
    workspace_directory_lines: int
    root_render: RenderDispatchMetrics
    heartbeat_calls: int
    heartbeat_call_names: tuple[str, ...]
    channel_fields: ChannelFieldMetrics
    high_load_area_lines: dict[str, int]
    high_load_total_lines: int
    high_load_workspace_weak_references: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Measure WorkspaceApp fields, dispatch, channels, and ownership boundaries."
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="Repository root to audit.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Print the measured metrics as JSON.",
    )
    parser.add_argument(
        "--expect",
        type=Path,
        help="Optional JSON baseline. Exit non-zero when any metric differs.",
    )
    return parser.parse_args()


def read_source(repo_root: Path, relative_path: Path) -> str:
    return (repo_root / relative_path).read_text(encoding="utf-8")


def extract_braced_body(source: str, opening_brace: int) -> tuple[str, int]:
    depth = 0
    for index in range(opening_brace, len(source)):
        character = source[index]
        if character == "{":
            depth += 1
        elif character == "}":
            depth -= 1
            if depth == 0:
                return source[opening_brace + 1 : index], index
    raise ValueError("unterminated braced body")


def extract_named_function_body(source: str, search_start: int, name: str) -> str:
    function_match = re.search(rf"\bfn\s+{re.escape(name)}\s*\(", source[search_start:])
    if function_match is None:
        raise ValueError(f"function {name} was not found")
    function_start = search_start + function_match.start()
    opening_brace = source.index("{", function_start)
    body, _ = extract_braced_body(source, opening_brace)
    return body


def strip_line_comments(source: str) -> str:
    return re.sub(r"//[^\n]*", "", source)


def split_top_level_fields(struct_body: str) -> list[str]:
    fields: list[str] = []
    current: list[str] = []
    angle_depth = 0
    parenthesis_depth = 0
    bracket_depth = 0
    brace_depth = 0
    for character in strip_line_comments(struct_body):
        if character == "<":
            angle_depth += 1
        elif character == ">" and angle_depth > 0:
            angle_depth -= 1
        elif character == "(":
            parenthesis_depth += 1
        elif character == ")":
            parenthesis_depth -= 1
        elif character == "[":
            bracket_depth += 1
        elif character == "]":
            bracket_depth -= 1
        elif character == "{":
            brace_depth += 1
        elif character == "}":
            brace_depth -= 1

        if (
            character == ","
            and angle_depth == 0
            and parenthesis_depth == 0
            and bracket_depth == 0
            and brace_depth == 0
        ):
            candidate = "".join(current).strip()
            if candidate:
                fields.append(candidate)
            current = []
        else:
            current.append(character)

    candidate = "".join(current).strip()
    if candidate:
        fields.append(candidate)
    return fields


def parse_field(field_source: str) -> tuple[str, str] | None:
    field_match = re.match(
        r"(?:pub(?:\([^)]*\))?\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(.+)",
        field_source,
        re.DOTALL,
    )
    if field_match is None:
        return None
    return field_match.group(1), " ".join(field_match.group(2).split())


def rust_files(path: Path) -> Iterable[Path]:
    if path.is_file():
        if path.suffix == ".rs":
            yield path
        return
    yield from sorted(path.rglob("*.rs"))


def line_count(path: Path) -> int:
    with path.open("r", encoding="utf-8") as source_file:
        return sum(1 for _ in source_file)


def area_line_count(repo_root: Path, area_paths: tuple[Path, ...]) -> int:
    return sum(
        line_count(source_file)
        for area_path in area_paths
        for source_file in rust_files(repo_root / area_path)
    )


def collect_workspace_struct(
    workspace_source: str,
) -> tuple[list[tuple[str, str]], int]:
    struct_match = next(
        (
            candidate
            for pattern in WORKSPACE_STRUCT_PATTERNS
            if (candidate := pattern.search(workspace_source)) is not None
        ),
        None,
    )
    if struct_match is None:
        raise ValueError("WorkspaceApp or WorkspaceSession struct was not found")
    opening_brace = struct_match.end() - 1
    struct_body, closing_brace = extract_braced_body(workspace_source, opening_brace)
    parsed_fields = [
        parsed_field
        for field_source in split_top_level_fields(struct_body)
        if (parsed_field := parse_field(field_source)) is not None
    ]
    start_line = workspace_source.count("\n", 0, struct_match.start()) + 1
    end_line = workspace_source.count("\n", 0, closing_brace) + 1
    return parsed_fields, end_line - start_line + 1


def collect_root_render_metrics(*render_sources: str) -> RenderDispatchMetrics:
    render_source = ""
    render_impl_match: re.Match[str] | None = None
    for _owner_name, owner_pattern in ROOT_RENDER_OWNER_PATTERNS:
        for candidate_source in render_sources:
            candidate_match = owner_pattern.search(candidate_source)
            if candidate_match is not None:
                render_source = candidate_source
                render_impl_match = candidate_match
                break
        if render_impl_match is not None:
            break
    if render_impl_match is None:
        raise ValueError(
            "Render implementation for WorkspaceWindowShell or WorkspaceApp was not found"
        )
    render_body = extract_named_function_body(
        render_source, render_impl_match.start(), "render"
    )
    dispatch_calls = tuple(
        re.findall(
            r"\bself\.((?:poll_|maybe_refresh_)[A-Za-z0-9_]+)\s*\(",
            render_body,
        )
    )
    return RenderDispatchMetrics(
        poll_calls=sum(call_name.startswith("poll_") for call_name in dispatch_calls),
        refresh_calls=sum(
            call_name.startswith("maybe_refresh_") for call_name in dispatch_calls
        ),
        try_recv_calls=len(re.findall(r"\.try_recv\s*\(", render_body)),
        recv_calls=len(re.findall(r"(?<!try_)\.recv\s*\(", render_body)),
        dispatch_calls=dispatch_calls,
    )


def collect_heartbeat_calls(init_source: str) -> tuple[str, ...]:
    heartbeat_start = init_source.find(HEARTBEAT_ANCHOR)
    if heartbeat_start < 0:
        return ()
    update_start = init_source.find(
        "weak.update(cx, |workspace, cx| {", heartbeat_start
    )
    if update_start < 0:
        raise ValueError("Workspace heartbeat update closure was not found")
    opening_brace = init_source.index("{", update_start)
    heartbeat_body, _ = extract_braced_body(init_source, opening_brace)
    return tuple(
        re.findall(
            r"\bworkspace\.([A-Za-z_][A-Za-z0-9_]*)\s*\(",
            heartbeat_body,
        )
    )


def collect_channel_fields(
    fields: list[tuple[str, str]],
) -> ChannelFieldMetrics:
    receiver_names = tuple(
        field_name
        for field_name, field_type in fields
        if field_name.endswith("_rx") or "Receiver" in field_type
    )
    sender_names = tuple(
        field_name
        for field_name, field_type in fields
        if field_name.endswith("_tx") or "Sender" in field_type
    )
    return ChannelFieldMetrics(
        receiver_fields=len(receiver_names),
        sender_fields=len(sender_names),
        receiver_names=receiver_names,
        sender_names=sender_names,
    )


def collect_metrics(repo_root: Path) -> AuditMetrics:
    workspace_source = read_source(repo_root, WORKSPACE_FILE)
    render_source = read_source(repo_root, ROOT_RENDER_FILE)
    window_shell_path = repo_root / WINDOW_SHELL_FILE
    window_shell_source = (
        window_shell_path.read_text(encoding="utf-8")
        if window_shell_path.is_file()
        else ""
    )
    init_source = read_source(repo_root, ROOT_INIT_FILE)
    workspace_fields, workspace_struct_lines = collect_workspace_struct(workspace_source)

    workspace_sources = [
        repo_root / WORKSPACE_FILE,
        *rust_files(repo_root / WORKSPACE_DIRECTORY),
    ]
    impl_files = [
        source_file
        for source_file in workspace_sources
        if WORKSPACE_IMPL_PATTERN.search(source_file.read_text(encoding="utf-8"))
    ]
    impl_blocks = sum(
        len(
            WORKSPACE_IMPL_PATTERN.findall(
                source_file.read_text(encoding="utf-8")
            )
        )
        for source_file in impl_files
    )

    high_load_area_lines = {
        area_name: area_line_count(repo_root, area_paths)
        for area_name, area_paths in HIGH_LOAD_AREAS.items()
    }
    high_load_sources = [
        source_file
        for area_paths in HIGH_LOAD_AREAS.values()
        for area_path in area_paths
        for source_file in rust_files(repo_root / area_path)
    ]
    weak_reference_pattern = re.compile(
        r"WeakEntity\s*<\s*(?:crate::workspace::)?WorkspaceApp\s*>"
    )
    high_load_weak_references = sum(
        len(
            weak_reference_pattern.findall(
                source_file.read_text(encoding="utf-8")
            )
        )
        for source_file in high_load_sources
    )
    heartbeat_call_names = collect_heartbeat_calls(init_source)
    return AuditMetrics(
        workspace_app_fields=len(workspace_fields),
        workspace_app_struct_lines=workspace_struct_lines,
        workspace_impl_blocks=impl_blocks,
        workspace_impl_files=len(impl_files),
        workspace_rs_lines=line_count(repo_root / WORKSPACE_FILE),
        workspace_directory_lines=sum(
            line_count(source_file)
            for source_file in rust_files(repo_root / WORKSPACE_DIRECTORY)
        ),
        root_render=collect_root_render_metrics(window_shell_source, render_source),
        heartbeat_calls=len(heartbeat_call_names),
        heartbeat_call_names=heartbeat_call_names,
        channel_fields=collect_channel_fields(workspace_fields),
        high_load_area_lines=high_load_area_lines,
        high_load_total_lines=sum(high_load_area_lines.values()),
        high_load_workspace_weak_references=high_load_weak_references,
    )


def print_human(metrics: AuditMetrics) -> None:
    print(f"WorkspaceApp direct fields: {metrics.workspace_app_fields}")
    print(f"WorkspaceApp struct lines: {metrics.workspace_app_struct_lines}")
    print(
        "impl WorkspaceApp blocks/files: "
        f"{metrics.workspace_impl_blocks}/{metrics.workspace_impl_files}"
    )
    print(f"workspace.rs lines: {metrics.workspace_rs_lines}")
    print(f"workspace directory Rust lines: {metrics.workspace_directory_lines}")
    print(
        "root render poll/refresh/try_recv/recv: "
        f"{metrics.root_render.poll_calls}/"
        f"{metrics.root_render.refresh_calls}/"
        f"{metrics.root_render.try_recv_calls}/"
        f"{metrics.root_render.recv_calls}"
    )
    print(f"530ms heartbeat calls: {metrics.heartbeat_calls}")
    print(
        "WorkspaceApp receiver/sender fields: "
        f"{metrics.channel_fields.receiver_fields}/"
        f"{metrics.channel_fields.sender_fields}"
    )
    for area_name, area_lines in metrics.high_load_area_lines.items():
        print(f"high-load {area_name} Rust lines: {area_lines}")
    print(f"high-load total Rust lines: {metrics.high_load_total_lines}")
    print(
        "high-load WeakEntity<WorkspaceApp> references: "
        f"{metrics.high_load_workspace_weak_references}"
    )


def compare_expected(metrics: AuditMetrics, expected_path: Path) -> list[str]:
    actual = asdict(metrics)
    expected = json.loads(expected_path.read_text(encoding="utf-8"))
    differences: list[str] = []

    def compare(prefix: str, expected_value: object, actual_value: object) -> None:
        if isinstance(expected_value, dict) and isinstance(actual_value, dict):
            for key, child_expected in expected_value.items():
                child_prefix = f"{prefix}.{key}" if prefix else key
                if key not in actual_value:
                    differences.append(f"{child_prefix}: missing from actual metrics")
                    continue
                compare(child_prefix, child_expected, actual_value[key])
            return
        if expected_value != actual_value:
            differences.append(
                f"{prefix}: expected {expected_value!r}, got {actual_value!r}"
            )

    compare("", expected, actual)
    return differences


def main() -> int:
    args = parse_args()
    metrics = collect_metrics(args.repo_root.resolve())
    if args.json:
        print(json.dumps(asdict(metrics), indent=2, sort_keys=True))
    else:
        print_human(metrics)

    if args.expect is None:
        return 0
    expected_path = (
        args.expect
        if args.expect.is_absolute()
        else args.repo_root.resolve() / args.expect
    )
    differences = compare_expected(metrics, expected_path)
    if not differences:
        print(f"baseline check passed: {expected_path}")
        return 0
    print(f"baseline check failed: {expected_path}", file=sys.stderr)
    for difference in differences:
        print(f"  - {difference}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
