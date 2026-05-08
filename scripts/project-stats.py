#!/usr/bin/env python3
"""OxideTerm project-stats — cloc 风格代码行数统计"""

from __future__ import annotations

import os
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

EXCLUDE_DIRS = {
    "target", ".git", "node_modules", "dist", "build",
    ".cache", "out", "coverage", ".turbo", ".next",
    "tauri版本代码",
}

EXT_TO_LANG = {
    ".rs":   "Rust",
    ".toml": "TOML",
    ".md":   "Markdown",
    ".sh":   "Shell",
    ".bash": "Shell",
    ".json": "JSON",
    ".yaml": "YAML",
    ".yml":  "YAML",
    ".html": "HTML",
    ".htm":  "HTML",
    ".css":  "CSS",
    ".scss": "CSS",
    ".ts":   "TypeScript",
    ".tsx":  "TypeScript",
    ".js":   "JavaScript",
    ".jsx":  "JavaScript",
    ".mjs":  "JavaScript",
    ".cjs":  "JavaScript",
}

SINGLE_COMMENT = {
    "Rust":       ("//",),
    "TypeScript": ("//",),
    "JavaScript": ("//",),
    "Shell":      ("#",),
    "TOML":       ("#",),
    "YAML":       ("#",),
}

BLOCK_COMMENT_LANGS = {"Rust", "TypeScript", "JavaScript"}


def is_colored() -> bool:
    return sys.stdout.isatty()


def color(text: str, code: str) -> str:
    if not is_colored():
        return text
    return f"\033[{code}m{text}\033[0m"


def bold(t: str) -> str: return color(t, "1")
def cyan(t: str) -> str: return color(t, "36")
def yellow(t: str) -> str: return color(t, "33")
def green(t: str) -> str: return color(t, "32")
def dim(t: str) -> str: return color(t, "2")


def fmt_num(n: int) -> str:
    return f"{n:,}"


def fmt_bytes(b: int) -> str:
    if b >= 1_048_576:
        return f"{b / 1_048_576:.1f} MB"
    if b >= 1_024:
        return f"{b / 1_024:.1f} KB"
    return f"{b} B"


# ── 行类型统计 ────────────────────────────────────────────────────────────────

def count_lines(path: Path, lang: str) -> tuple[int, int, int]:
    """返回 (code, comment, blank)，忽略无法解码的文件。"""
    try:
        text = path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return 0, 0, 0

    single_prefixes = SINGLE_COMMENT.get(lang, ())
    use_block = lang in BLOCK_COMMENT_LANGS

    code = comment = blank = 0
    in_block = False

    for raw in text.splitlines():
        line = raw.strip()

        if not line:
            blank += 1
            continue

        if in_block:
            comment += 1
            if "*/" in line:
                in_block = False
            continue

        if use_block and line.startswith("/*"):
            comment += 1
            if "*/" not in line[2:]:
                in_block = True
            continue

        if any(line.startswith(p) for p in single_prefixes):
            comment += 1
        else:
            code += 1

    return code, comment, blank


# ── 文件遍历 ──────────────────────────────────────────────────────────────────

def collect(root: Path) -> dict[str, dict]:
    """
    返回 {lang: {files, code, comment, blank, bytes}}
    """
    stats: dict[str, dict] = {}

    for dirpath, dirnames, filenames in os.walk(root):
        # 原地过滤，避免递归进排除目录
        dirnames[:] = [d for d in dirnames if d not in EXCLUDE_DIRS and not d.startswith(".")]

        for fname in filenames:
            ext = Path(fname).suffix.lower()
            lang = EXT_TO_LANG.get(ext)
            if lang is None:
                continue

            fpath = Path(dirpath) / fname
            try:
                size = fpath.stat().st_size
            except OSError:
                continue

            code, comment, blank = count_lines(fpath, lang)

            if lang not in stats:
                stats[lang] = {"files": 0, "code": 0, "comment": 0, "blank": 0, "bytes": 0}
            s = stats[lang]
            s["files"]   += 1
            s["code"]    += code
            s["comment"] += comment
            s["blank"]   += blank
            s["bytes"]   += size

    return stats


# ── 输出 ──────────────────────────────────────────────────────────────────────

def print_table(stats: dict[str, dict]) -> None:
    if not stats:
        print("没有找到任何源文件。")
        return

    # 按代码行数降序排列
    rows = sorted(stats.items(), key=lambda kv: kv[1]["code"], reverse=True)

    col_lang    = max(len("语言"),    max(len(k) for k in stats)) + 2
    col_files   = max(len("文件数"),  max(len(fmt_num(v["files"]))   for v in stats.values())) + 2
    col_code    = max(len("代码行"),  max(len(fmt_num(v["code"]))    for v in stats.values())) + 2
    col_comment = max(len("注释行"),  max(len(fmt_num(v["comment"])) for v in stats.values())) + 2
    col_blank   = max(len("空白行"),  max(len(fmt_num(v["blank"]))   for v in stats.values())) + 2
    col_size    = max(len("大小"),    max(len(fmt_bytes(v["bytes"])) for v in stats.values())) + 2

    sep = (
        "─" * col_lang + "┬" +
        "─" * col_files + "┬" +
        "─" * col_code + "┬" +
        "─" * col_comment + "┬" +
        "─" * col_blank + "┬" +
        "─" * col_size
    )

    header = (
        bold(cyan(f"{'语言':<{col_lang}}")) + "│" +
        bold(f"{'文件数':>{col_files}}") + "│" +
        bold(green(f"{'代码行':>{col_code}}")) + "│" +
        bold(dim(f"{'注释行':>{col_comment}}")) + "│" +
        bold(dim(f"{'空白行':>{col_blank}}")) + "│" +
        bold(f"{'大小':>{col_size}}")
    )

    print()
    print(header)
    print(sep)

    total = {"files": 0, "code": 0, "comment": 0, "blank": 0, "bytes": 0}
    for lang, v in rows:
        print(
            cyan(f"{lang:<{col_lang}}") + "│" +
            f"{fmt_num(v['files']):>{col_files}}" + "│" +
            green(f"{fmt_num(v['code']):>{col_code}}") + "│" +
            dim(f"{fmt_num(v['comment']):>{col_comment}}") + "│" +
            dim(f"{fmt_num(v['blank']):>{col_blank}}") + "│" +
            f"{fmt_bytes(v['bytes']):>{col_size}}"
        )
        for k in total:
            total[k] += v[k]

    print(sep)
    print(
        bold(f"{'合计':<{col_lang}}") + "│" +
        bold(f"{fmt_num(total['files']):>{col_files}}") + "│" +
        bold(green(f"{fmt_num(total['code']):>{col_code}}")) + "│" +
        bold(dim(f"{fmt_num(total['comment']):>{col_comment}}")) + "│" +
        bold(dim(f"{fmt_num(total['blank']):>{col_blank}}")) + "│" +
        bold(f"{fmt_bytes(total['bytes']):>{col_size}}")
    )
    print()


# ── 入口 ──────────────────────────────────────────────────────────────────────

def main() -> None:
    root = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT
    print(dim(f"扫描目录: {root}"))
    t0 = time.monotonic()
    stats = collect(root)
    elapsed = time.monotonic() - t0
    print_table(stats)
    print(dim(f"耗时 {elapsed:.2f}s"))


if __name__ == "__main__":
    main()


class LineCount:
    __slots__ = ("total", "code", "comment", "blank")

    def __init__(self) -> None:
        self.total = self.code = self.comment = self.blank = 0

    def __iadd__(self, other: "LineCount") -> "LineCount":
        self.total   += other.total
        self.code    += other.code
        self.comment += other.comment
        self.blank   += other.blank
        return self


class CrateStats:
    def __init__(self) -> None:
        self.files = 0
        self.lines = LineCount()


def count_file(path: Path, lang: str) -> LineCount:
    lc = LineCount()
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return lc

    single_prefixes = SINGLE_COMMENT.get(lang, ())
    use_block = lang in BLOCK_COMMENT_LANGS
    in_block = False

    for line in text.splitlines():
        lc.total += 1
        stripped = line.strip()

        if not stripped:
            lc.blank += 1
            continue

        if use_block:
            if in_block:
                lc.comment += 1
                if "*/" in stripped:
                    in_block = False
                continue
            if stripped.startswith("/*") and "*/" not in stripped[2:]:
                in_block = True
                lc.comment += 1
                continue

        if any(stripped.startswith(p) for p in single_prefixes):
            lc.comment += 1
        else:
            lc.code += 1

    return lc


def count_rust_inline_tests(path: Path) -> int:
    """Count lines inside #[cfg(test)] mod blocks."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return 0

    lines = text.splitlines()
    inline = 0
    i = 0
    while i < len(lines):
        stripped = lines[i].strip()
        if stripped in ("#[cfg(test)]", "#[cfg(test)] "):
            # Find the opening brace of the mod block
            depth = 0
            j = i + 1
            started = False
            block_lines = 0
            while j < len(lines):
                depth += lines[j].count("{") - lines[j].count("}")
                if depth > 0:
                    started = True
                block_lines += 1
                if started and depth <= 0:
                    inline += block_lines
                    i = j
                    break
                j += 1
        i += 1
    return inline


def is_test_file(rel: str) -> bool:
    parts = Path(rel).parts
    return (
        "tests" in parts
        or rel.endswith("_test.rs")
        or rel.endswith("tests.rs")
    )


def crate_name(rel: Path) -> str:
    parts = rel.parts
    if len(parts) >= 2 and parts[0] == "crates":
        return parts[1]
    return "(root)"


def collect_files(root: Path) -> list[Path]:
    result: list[Path] = []
    for dirpath, dirnames, filenames in os.walk(root):
        # prune excluded dirs in-place
        dirnames[:] = [
            d for d in dirnames
            if d not in EXCLUDE_DIRS and not d.startswith(".")
        ]
        for fname in filenames:
            ext = Path(fname).suffix.lower()
            if ext in EXT_TO_LANG:
                result.append(Path(dirpath) / fname)
    return result


def main() -> None:
    args = set(sys.argv[1:])
    by_crate = "--by-crate" in args
    by_dir   = "--by-dir"   in args

    SEP  = "═" * 70
    SEP2 = "─" * 70

    t0 = time.perf_counter()
    files = collect_files(ROOT)

    # Accumulators
    lang_stats: dict[str, dict] = {}   # lang → {files, lines: LineCount, bytes}
    crate_stats: dict[str, CrateStats] = {}
    dir_stats: dict[str, dict] = {}
    total = LineCount()
    total_bytes = 0

    test_files_count = 0
    test_code = 0
    rust_inline = 0

    for fpath in files:
        ext  = fpath.suffix.lower()
        lang = EXT_TO_LANG[ext]
        rel  = fpath.relative_to(ROOT)

        lc    = count_file(fpath, lang)
        fbytes = fpath.stat().st_size

        # Language
        if lang not in lang_stats:
            lang_stats[lang] = {"files": 0, "lines": LineCount(), "bytes": 0}
        ls = lang_stats[lang]
        ls["files"] += 1
        ls["lines"] += lc
        ls["bytes"] += fbytes

        # Totals
        total += lc
        total_bytes += fbytes

        # Crate
        cn = crate_name(rel)
        if cn not in crate_stats:
            crate_stats[cn] = CrateStats()
        cs = crate_stats[cn]
        cs.files += 1
        cs.lines += lc

        # Dir (top-level)
        top = str(rel.parts[0]) if rel.parts else "(root)"
        if top not in dir_stats:
            dir_stats[top] = {"files": 0, "lines": 0, "bytes": 0}
        dir_stats[top]["files"] += 1
        dir_stats[top]["lines"] += lc.total
        dir_stats[top]["bytes"] += fbytes

        # Tests
        if lang == "Rust":
            rel_s = str(rel)
            if is_test_file(rel_s):
                test_files_count += 1
                test_code += lc.code
            else:
                rust_inline += count_rust_inline_tests(fpath)

    elapsed_ms = (time.perf_counter() - t0) * 1000

    # ── Language table ─────────────────────────────────────────────────
    print(f"\n{bold(SEP)}")
    print(bold(cyan("📊 代码统计 (cloc 风格)")))
    print(bold(SEP))
    print(dim(f"位置: {ROOT}") + "\n")

    sorted_langs = sorted(lang_stats.items(), key=lambda x: x[1]["lines"].code, reverse=True)

    print(bold(f"{'Language':<14}{'files':>8}{'blank':>10}{'comment':>10}{'code':>12}"))
    print(SEP2)
    for lang, ls in sorted_langs:
        lc2 = ls["lines"]
        print(f"{lang:<14}{fmt_num(ls['files']):>8}{fmt_num(lc2.blank):>10}"
              f"{fmt_num(lc2.comment):>10}{fmt_num(lc2.code):>12}")
    print(SEP2)
    print(bold(f"{'TOTAL':<14}{fmt_num(sum(ls['files'] for ls in lang_stats.values())):>8}"
               f"{fmt_num(total.blank):>10}{fmt_num(total.comment):>10}{fmt_num(total.code):>12}"))

    # ── Composition ────────────────────────────────────────────────────
    total_all = total.blank + total.comment + total.code
    print(f"\n{bold(yellow('📈 代码构成分析'))}")
    print(SEP2)
    print(f"总行数:       {fmt_num(total_all):>12}")
    if total_all:
        print(f"  ├─ 空行:  {total.blank / total_all * 100:>6.1f}%  ({fmt_num(total.blank)})")
        print(f"  ├─ 注释:  {total.comment / total_all * 100:>6.1f}%  ({fmt_num(total.comment)})")
        print(f"  └─ 代码:  {total.code / total_all * 100:>6.1f}%  ({fmt_num(total.code)})")
    print(f"磁盘体积:     {fmt_bytes(total_bytes):>12}")

    # ── Test stats ─────────────────────────────────────────────────────
    test_total = test_code + rust_inline
    print(f"\n{bold(green('🧪 测试代码统计'))}")
    print(SEP2)
    print(f"测试文件:       {fmt_num(test_files_count):>10}")
    print(f"测试代码行:     {fmt_num(test_code):>10}")
    print(f"Rust inline:    {fmt_num(rust_inline):>10}  (#[cfg(test)] 块)")
    print(f"测试合计:       {fmt_num(test_total):>10}")
    if total.code:
        print(f"测试/代码比:    {test_total / total.code * 100:>9.1f}%")

    # ── By crate ───────────────────────────────────────────────────────
    if by_crate:
        print(f"\n{bold(cyan('📦 按 Crate 统计'))}")
        print(SEP2)
        print(bold(f"{'Crate':<32}{'files':>8}{'blank':>10}{'comment':>10}{'code':>12}"))
        print(SEP2)
        for cn, cs in sorted(crate_stats.items(), key=lambda x: x[1].lines.code, reverse=True):
            lc2 = cs.lines
            print(f"{cn:<32}{fmt_num(cs.files):>8}{fmt_num(lc2.blank):>10}"
                  f"{fmt_num(lc2.comment):>10}{fmt_num(lc2.code):>12}")

    # ── By dir ─────────────────────────────────────────────────────────
    if by_dir:
        print(f"\n{bold(cyan('📂 按顶级目录统计'))}")
        print(SEP2)
        print(bold(f"{'Directory':<24}{'files':>8}{'lines':>12}{'size':>10}"))
        print(SEP2)
        for dn, ds in sorted(dir_stats.items(), key=lambda x: x[1]["lines"], reverse=True):
            print(f"{dn:<24}{fmt_num(ds['files']):>8}{fmt_num(ds['lines']):>12}"
                  f"{fmt_bytes(ds['bytes']):>10}")

    print(f"\n{SEP}")
    print(dim(f"⏱  扫描耗时: {elapsed_ms:.0f}ms") + "\n")


if __name__ == "__main__":
    main()
