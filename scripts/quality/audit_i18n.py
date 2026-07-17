#!/usr/bin/env python3
"""Audit native locale catalogs against each other and source key usage."""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


DEFAULT_LOCALE_ROOT = Path("crates/oxideterm-i18n/locales")
DEFAULT_SOURCE_ROOTS = (Path("crates"),)
PLACEHOLDER_RE = re.compile(r"\{\{\s*([A-Za-z0-9_.-]+)\s*\}\}")
RUST_KEY_PATTERNS = (
    # Native Rust code generally calls I18n::t or the WorkspaceApp helper.
    re.compile(r"(?:^|[^\w])(?:i18n_with|t)\(\s*\"([A-Za-z0-9_.:-]+)\""),
    re.compile(r"\.t\(\s*\"([A-Za-z0-9_.:-]+)\""),
)
DEFAULT_IGNORED_SOURCE_KEYS = frozenset(
    {
        # This sentinel is intentionally used by oxideterm-i18n fallback tests.
        "missing.key",
    }
)


@dataclass
class LocaleCatalog:
    locale: str
    files: dict[str, Path] = field(default_factory=dict)
    values: dict[str, str] = field(default_factory=dict)
    placeholders: dict[str, set[str]] = field(default_factory=dict)
    duplicates: dict[str, list[str]] = field(default_factory=lambda: defaultdict(list))


@dataclass
class AuditResult:
    catalogs: dict[str, LocaleCatalog]
    parse_errors: list[str]
    source_keys: dict[str, list[str]]
    missing_files: dict[str, list[str]]
    missing_by_locale: dict[str, list[str]]
    source_absent_everywhere: list[str]
    source_missing_by_locale: dict[str, list[str]]
    placeholder_mismatches: dict[str, dict[str, list[str]]]
    english_copies: dict[str, list[str]]

    def has_errors(self) -> bool:
        return bool(
            self.parse_errors
            or any(catalog.duplicates for catalog in self.catalogs.values())
            or self.missing_files
            or self.missing_by_locale
            or self.source_absent_everywhere
            or self.source_missing_by_locale
            or self.placeholder_mismatches
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Check native i18n JSON catalogs for missing keys, duplicate flattened "
            "keys, placeholder drift, and source-used keys that no locale defines."
        )
    )
    parser.add_argument(
        "--locale-root",
        type=Path,
        default=DEFAULT_LOCALE_ROOT,
        help="Directory containing locale subdirectories.",
    )
    parser.add_argument(
        "--source-root",
        type=Path,
        action="append",
        default=None,
        help="Source root to scan for i18n key usage. May be passed multiple times.",
    )
    parser.add_argument(
        "--fail-on-english-copy",
        action="store_true",
        help="Treat non-English values identical to English as errors.",
    )
    parser.add_argument(
        "--ignore-source-key",
        action="append",
        default=[],
        help="Static source key to ignore. Intended for explicit fallback-test sentinels.",
    )
    parser.add_argument(
        "--show-all",
        action="store_true",
        help="Print every finding instead of truncating long sections.",
    )
    return parser.parse_args()


def flatten_json(value: Any, prefix: str, out: dict[str, str]) -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            next_prefix = f"{prefix}.{key}" if prefix else str(key)
            flatten_json(child, next_prefix, out)
        return
    if isinstance(value, str):
        out[prefix] = value


def load_catalogs(locale_root: Path) -> tuple[dict[str, LocaleCatalog], list[str]]:
    catalogs: dict[str, LocaleCatalog] = {}
    parse_errors: list[str] = []
    for locale_dir in sorted(path for path in locale_root.iterdir() if path.is_dir()):
        catalog = LocaleCatalog(locale=locale_dir.name)
        for json_file in sorted(locale_dir.glob("*.json")):
            catalog.files[json_file.name] = json_file
            try:
                data = json.loads(json_file.read_text(encoding="utf-8"))
            except Exception as error:  # noqa: BLE001 - report exact parser failure.
                parse_errors.append(f"{json_file}: {error}")
                continue
            flattened: dict[str, str] = {}
            flatten_json(data, "", flattened)
            for key, text in flattened.items():
                if key in catalog.values:
                    catalog.duplicates[key].append(str(json_file))
                    continue
                catalog.values[key] = text
                catalog.placeholders[key] = set(PLACEHOLDER_RE.findall(text))
        catalogs[catalog.locale] = catalog
    return catalogs, parse_errors


def scan_source_keys(
    source_roots: tuple[Path, ...], ignored_source_keys: set[str]
) -> dict[str, list[str]]:
    found: dict[str, set[str]] = defaultdict(set)
    for root in source_roots:
        if not root.exists():
            continue
        for source_file in root.rglob("*.rs"):
            if any(part in {"target", ".git"} for part in source_file.parts):
                continue
            try:
                text = source_file.read_text(encoding="utf-8")
            except UnicodeDecodeError:
                continue
            for pattern in RUST_KEY_PATTERNS:
                for match in pattern.finditer(text):
                    key = match.group(1)
                    if "." not in key or key in ignored_source_keys:
                        continue
                    found[key].add(str(source_file))
    return {key: sorted(paths) for key, paths in sorted(found.items())}


def audit(
    locale_root: Path, source_roots: tuple[Path, ...], ignored_source_keys: set[str]
) -> AuditResult:
    catalogs, parse_errors = load_catalogs(locale_root)
    source_keys = scan_source_keys(source_roots, ignored_source_keys)

    file_union = sorted({file_name for catalog in catalogs.values() for file_name in catalog.files})
    missing_files = {
        locale: [file_name for file_name in file_union if file_name not in catalog.files]
        for locale, catalog in catalogs.items()
    }
    missing_files = {locale: files for locale, files in missing_files.items() if files}

    key_union = sorted({key for catalog in catalogs.values() for key in catalog.values})
    missing_by_locale = {
        locale: [key for key in key_union if key not in catalog.values]
        for locale, catalog in catalogs.items()
    }
    missing_by_locale = {locale: keys for locale, keys in missing_by_locale.items() if keys}

    any_locale_keys = set(key_union)
    source_absent_everywhere = sorted(key for key in source_keys if key not in any_locale_keys)
    source_missing_by_locale = {
        locale: sorted(key for key in source_keys if key in any_locale_keys and key not in catalog.values)
        for locale, catalog in catalogs.items()
    }
    source_missing_by_locale = {
        locale: keys for locale, keys in source_missing_by_locale.items() if keys
    }

    placeholder_mismatches: dict[str, dict[str, list[str]]] = {}
    for key in key_union:
        expected_sets = {
            locale: catalog.placeholders.get(key, set())
            for locale, catalog in catalogs.items()
            if key in catalog.values
        }
        unique_sets = {tuple(sorted(placeholders)) for placeholders in expected_sets.values()}
        if len(unique_sets) <= 1:
            continue
        placeholder_mismatches[key] = {
            locale: sorted(placeholders) for locale, placeholders in expected_sets.items()
        }

    english_copies: dict[str, list[str]] = {}
    english = catalogs.get("en")
    if english:
        for locale, catalog in catalogs.items():
            if locale == "en":
                continue
            copied = []
            for key, english_text in english.values.items():
                local_text = catalog.values.get(key)
                if not local_text or local_text != english_text:
                    continue
                # Keep empty strings and pure tokens out of the signal.
                if not english_text.strip() or not re.search(r"[A-Za-z]{4,}", english_text):
                    continue
                copied.append(key)
            if copied:
                english_copies[locale] = copied

    return AuditResult(
        catalogs=catalogs,
        parse_errors=parse_errors,
        source_keys=source_keys,
        missing_files=missing_files,
        missing_by_locale=missing_by_locale,
        source_absent_everywhere=source_absent_everywhere,
        source_missing_by_locale=source_missing_by_locale,
        placeholder_mismatches=placeholder_mismatches,
        english_copies=english_copies,
    )


def print_limited(title: str, items: list[str], limit: int | None = 30) -> None:
    print(f"\n{title}: {len(items)}")
    visible_items = items if limit is None else items[:limit]
    for item in visible_items:
        print(f"  - {item}")
    if limit is not None and len(items) > limit:
        print(f"  ... {len(items) - limit} more")


def print_result(result: AuditResult, fail_on_english_copy: bool, show_all: bool) -> int:
    limit = None if show_all else 30
    print(f"Locales: {', '.join(sorted(result.catalogs))}")
    print(f"Source i18n keys scanned: {len(result.source_keys)}")

    print_limited("JSON parse errors", result.parse_errors, limit)

    duplicate_lines = []
    for locale, catalog in sorted(result.catalogs.items()):
        for key, files in sorted(catalog.duplicates.items()):
            duplicate_lines.append(f"{locale}:{key} ({', '.join(files)})")
    print_limited("Duplicate flattened keys", duplicate_lines, limit)

    missing_file_lines = [
        f"{locale}: {', '.join(files)}" for locale, files in sorted(result.missing_files.items())
    ]
    print_limited("Missing locale files", missing_file_lines, limit)

    missing_key_lines = [
        f"{locale}: {key}"
        for locale, keys in sorted(result.missing_by_locale.items())
        for key in keys
    ]
    print_limited("Keys missing from locale catalogs", missing_key_lines, limit)

    absent_lines = [
        f"{key} <- {', '.join(paths[:3])}"
        for key, paths in result.source_keys.items()
        if key in result.source_absent_everywhere
    ]
    print_limited("Source-used keys absent from every locale", absent_lines, limit)

    source_missing_lines = [
        f"{locale}: {key}"
        for locale, keys in sorted(result.source_missing_by_locale.items())
        for key in keys
    ]
    print_limited("Source-used keys missing from some locales", source_missing_lines, limit)

    placeholder_lines = [
        f"{key}: {per_locale}"
        for key, per_locale in sorted(result.placeholder_mismatches.items())
    ]
    print_limited("Placeholder mismatches", placeholder_lines, limit)

    english_copy_lines = [
        f"{locale}: {key}"
        for locale, keys in sorted(result.english_copies.items())
        for key in keys
    ]
    heading = "English-copy warnings"
    if fail_on_english_copy:
        heading = "English-copy errors"
    print_limited(heading, english_copy_lines, limit)

    has_structural_errors = result.has_errors()
    has_english_copy_errors = fail_on_english_copy and bool(result.english_copies)
    if has_structural_errors or has_english_copy_errors:
        return 1
    return 0


def main() -> int:
    args = parse_args()
    source_roots = tuple(args.source_root) if args.source_root else DEFAULT_SOURCE_ROOTS
    ignored_source_keys = set(DEFAULT_IGNORED_SOURCE_KEYS)
    ignored_source_keys.update(args.ignore_source_key)
    result = audit(args.locale_root, source_roots, ignored_source_keys)
    return print_result(result, args.fail_on_english_copy, args.show_all)


if __name__ == "__main__":
    sys.exit(main())
