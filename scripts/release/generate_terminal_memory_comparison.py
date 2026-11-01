#!/usr/bin/env python3
"""Render the 2.3.0 history-memory figure in the pre-2.1.0 release-chart style.

Run with: uv run --with matplotlib==3.11.2 scripts/release/generate_terminal_memory_comparison.py
Data: docs/development/performance-and-benchmarking.md, shared-attribute and
text-run history sections dated 2026-10-26. The baseline includes tail compression.
"""

from __future__ import annotations

import json
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Patch


ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / ".github/release-notes/assets/terminal-memory-2.3.0-comparison"
WORKLOADS = (
    ("Full ASCII lines", 196.47, 15.50),
    ("Ten color runs per line", 196.47, 21.26),
)

BACKGROUND = "#f8fafc"
TEXT = "#1e293b"
MUTED = "#64748b"
GRID = "#cbd5e1"
BEFORE = "#cbd5e1"
AFTER = "#2563eb"


def main() -> None:
    provenance = {
        "source": "docs/development/performance-and-benchmarking.md",
        "sections": [
            "Shared-Attribute History (2026-10-26)",
            "Text-Run History And Sequential Reads (2026-10-26)",
        ],
        "measurement": "median retained terminal/parser heap, decimal MB",
        "history_rows": 20000,
        "screen_rows": 40,
        "columns": 400,
        "warmups": 1,
        "measured_samples": 5,
        "environment": "Apple M5, macOS 26.6.2, Rust 1.97.0, optimized bench profile",
        "baseline": "full-cell history with repeated-tail compression",
        "comparison": "same workloads across the documented optimization stages",
        "data": [
            {"workload": name, "before_mb": before, "after_mb": after}
            for name, before, after in WORKLOADS
        ],
        "reduction": "100 * (before_mb - after_mb) / before_mb, rounded to 1 decimal",
    }
    with plt.rc_context({
        "font.family": "DejaVu Sans",
        "text.color": TEXT,
        "axes.labelcolor": MUTED,
        "xtick.color": TEXT,
        "ytick.color": MUTED,
        "svg.fonttype": "none",
    }):
        fig = plt.figure(figsize=(14, 7.5), facecolor=BACKGROUND)
        fig.text(0.075, 0.94, "OxideTerm 2.3.0 terminal memory", fontsize=24, weight="bold")
        fig.text(0.075, 0.885, "20,000 history rows × 400 columns · +40 screen rows · lower is better",
                 fontsize=13.5, color=MUTED)
        fig.legend(
            handles=[
                Patch(facecolor=BEFORE, label="Before · full-cell history"),
                Patch(facecolor=AFTER, label="2.3.0 · compact history"),
            ],
            loc="upper left", bbox_to_anchor=(0.067, 0.835),
            frameon=False, ncol=2, fontsize=14, handlelength=1.3, columnspacing=2.2,
        )

        ax = fig.add_axes((0.075, 0.19, 0.905, 0.52), facecolor=BACKGROUND)
        centers = (0.0, 1.6)
        offset, width = 0.23, 0.36
        for center, (_, before, after) in zip(centers, WORKLOADS):
            for x, value, color in (
                (center - offset, before, BEFORE),
                (center + offset, after, AFTER),
            ):
                ax.bar(x, value, width=width, color=color, zorder=3)
                ax.text(x, value + 6, f"{value:.2f}", ha="center", va="bottom",
                        fontsize=17, color=TEXT, weight="bold")
            reduction = 100 * (before - after) / before
            ax.text(center, 1.04, f"{reduction:.1f}% less memory",
                    transform=ax.get_xaxis_transform(), ha="center", va="bottom",
                    fontsize=17, color=AFTER, weight="bold")

        ax.set_xlim(-0.85, 2.45)
        ax.set_ylim(0, 220)
        ax.set_yticks((0, 50, 100, 150, 200))
        ax.set_xticks(centers, [row[0] for row in WORKLOADS])
        ax.set_ylabel("Retained heap (MB)", fontsize=14, labelpad=12)
        ax.tick_params(axis="x", labelsize=15, length=0, pad=13)
        ax.tick_params(axis="y", labelsize=13, length=0, pad=10)
        ax.grid(axis="y", color=GRID, linewidth=0.8, zorder=0)
        for spine in ax.spines.values():
            spine.set_color("#94a3b8")
            spine.set_linewidth(0.8)

        fig.text(0.075, 0.085, "Terminal + parser heap · decimal MB · median of 5 runs · Apple M5 / macOS 26.6.2",
                 fontsize=12, color=MUTED)
        fig.text(0.075, 0.045, "Same-workload staged benchmarks · baseline includes tail compression · Rust 1.97.0",
                 fontsize=12, color=MUTED)

        OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        metadata = {
            "Title": "OxideTerm 2.3.0 terminal history memory comparison",
            "Description": json.dumps(provenance, ensure_ascii=False),
        }
        png = OUTPUT.parent / f"{OUTPUT.name}.png"
        svg = OUTPUT.parent / f"{OUTPUT.name}.svg"
        fig.savefig(png, dpi=160, facecolor=BACKGROUND, metadata=metadata)
        fig.savefig(svg, facecolor=BACKGROUND, metadata=metadata)
        # Matplotlib leaves trailing spaces in SVG path coordinates.
        svg.write_text(
            "\n".join(line.rstrip() for line in svg.read_text(encoding="utf-8").splitlines()) + "\n",
            encoding="utf-8",
        )
        plt.close(fig)
    print(png)
    print(svg)


if __name__ == "__main__":
    main()
