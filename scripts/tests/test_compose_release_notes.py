"""Tests for release-note composition and generated stable download links."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = ROOT / ".github" / "scripts" / "compose_release_notes.py"
SPEC = importlib.util.spec_from_file_location("compose_release_notes", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
COMPOSE_RELEASE_NOTES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(COMPOSE_RELEASE_NOTES)


class ComposeReleaseNotesTests(unittest.TestCase):
    def test_stable_notes_include_versioned_download_matrix(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            base = root / "base.md"
            changelog = root / "changelog.md"
            base.write_text(
                "# Stable\n\n<!-- RELEASE_DOWNLOADS -->\n\n<!-- RELEASE_CHANGELOG -->\n",
                encoding="utf-8",
            )
            changelog.write_text("## 2.0.0\n\nFirst stable release.\n", encoding="utf-8")

            notes = COMPOSE_RELEASE_NOTES.compose_notes(
                "2.0.0", "v2.0.0", base, changelog
            )

        self.assertIn("## Download for your system", notes)
        self.assertIn("OxideTerm_2.0.0_windows_x64-setup.exe", notes)
        self.assertIn("OxideTerm_2.0.0_macos_arm64.dmg", notes)
        self.assertIn("OxideTerm_2.0.0_linux_arm64.rpm", notes)
        self.assertNotIn("RELEASE_DOWNLOADS", notes)

    def test_preview_notes_without_download_marker_remain_supported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            base = root / "base.md"
            changelog = root / "changelog.md"
            base.write_text("# Preview\n\n<!-- RELEASE_CHANGELOG -->\n", encoding="utf-8")
            changelog.write_text(
                "## 2.0.0-preview.1\n\nPreview notes.\n", encoding="utf-8"
            )

            notes = COMPOSE_RELEASE_NOTES.compose_notes(
                "2.0.0-preview.1", "gpui-v2.0.0-preview.1", base, changelog
            )

        self.assertNotIn("Download for your system", notes)
        self.assertIn("Preview notes.", notes)


if __name__ == "__main__":
    unittest.main()
