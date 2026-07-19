#!/usr/bin/env python3
"""Print the channel-specific Git range used to draft OxideTerm release notes."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


CHANNELS = {
    "stable": {
        "version": re.compile(r"^\d+\.\d+\.\d+$"),
        "tag": lambda version: f"v{version}",
        "tag_pattern": re.compile(r"^v\d+\.\d+\.\d+$"),
        "changelog": ".github/release-notes/stable-changelog.md",
        "base": ".github/release-notes/stable.md",
    },
    "beta": {
        "version": re.compile(r"^\d+\.\d+\.\d+-beta\.\d+$"),
        "tag": lambda version: f"v{version}",
        "tag_pattern": re.compile(r"^v\d+\.\d+\.\d+-beta\.\d+$"),
        "changelog": ".github/release-notes/beta-changelog.md",
        "base": ".github/release-notes/beta.md",
    },
    "gpui-preview": {
        "version": re.compile(r"^\d+\.\d+\.\d+-gpui-preview\.\d+$"),
        "tag": lambda version: f"gpui-v{version}",
        "tag_pattern": re.compile(r"^gpui-v\d+\.\d+\.\d+-gpui-preview\.\d+$"),
        "changelog": ".github/release-notes/gpui-preview-changelog.md",
        "base": ".github/release-notes/gpui-preview.md",
    },
}
RELEASE_TAG_PATTERNS = tuple(config["tag_pattern"] for config in CHANNELS.values())


def run(repo: Path, *args: str) -> str:
    """Run Git in the selected repository and return normalized stdout."""
    result = subprocess.run(
        ["git", *args],
        cwd=repo,
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.rstrip()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Find the prior same-channel tag and summarize its delta to a target ref.",
    )
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--channel", choices=sorted(CHANNELS), required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--target", default="HEAD")
    return parser.parse_args()


def previous_release_tag(
    repo: Path,
    channel: str,
    target: str,
    desired_tag: str,
) -> tuple[str | None, bool]:
    """Return a same-channel tag, or the latest release tag for a new channel."""
    tag_pattern = CHANNELS[channel]["tag_pattern"]
    tags = run(repo, "tag", "--merged", target, "--sort=-creatordate").splitlines()
    same_channel_tag = next(
        (tag for tag in tags if tag != desired_tag and tag_pattern.fullmatch(tag)),
        None,
    )
    if same_channel_tag is not None:
        return same_channel_tag, False

    bootstrap_tag = next(
        (
            tag
            for tag in tags
            if tag != desired_tag
            and any(pattern.fullmatch(tag) for pattern in RELEASE_TAG_PATTERNS)
        ),
        None,
    )
    return bootstrap_tag, bootstrap_tag is not None


def main() -> int:
    args = parse_args()
    repo = args.repo.resolve()
    config = CHANNELS[args.channel]

    try:
        run(repo, "rev-parse", "--show-toplevel")
        run(repo, "rev-parse", "--verify", args.target)
    except subprocess.CalledProcessError as error:
        print(error.stderr.strip() or "error: invalid Git repository or target", file=sys.stderr)
        return 1

    if not config["version"].fullmatch(args.version):
        print(
            f"error: version {args.version!r} does not match channel {args.channel!r}",
            file=sys.stderr,
        )
        return 1

    desired_tag = config["tag"](args.version)
    previous_tag, used_bootstrap_tag = previous_release_tag(
        repo,
        args.channel,
        args.target,
        desired_tag,
    )
    if previous_tag is None:
        previous_tag = run(repo, "rev-list", "--max-parents=0", args.target).splitlines()[0]
        baseline_label = f"{previous_tag} (root commit; initial channel release)"
        revision_range = f"{previous_tag}..{args.target}"
    else:
        baseline_label = (
            f"{previous_tag} (bootstrap from previous release channel)"
            if used_bootstrap_tag
            else previous_tag
        )
        revision_range = f"{previous_tag}..{args.target}"

    print(f"channel: {args.channel}")
    print(f"version: {args.version}")
    print(f"tag: {desired_tag}")
    print(f"changelog: {config['changelog']}")
    print(f"base notes: {config['base']}")
    print(f"previous tag: {baseline_label}")
    print(f"range: {revision_range}")
    print("\ncommits:")
    print(run(repo, "log", "--oneline", "--no-merges", revision_range) or "(none)")
    print("\ndiff summary:")
    print(run(repo, "diff", "--stat", revision_range) or "(no committed changes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
