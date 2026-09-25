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
    def test_bilingual_notes_start_with_navigation_and_keep_chinese_first(self) -> None:
        for languages in (("English", "中文"), ("中文", "English")):
            for stable in (True, False):
                with (
                    self.subTest(languages=languages, stable=stable),
                    tempfile.TemporaryDirectory() as directory,
                ):
                    root = Path(directory)
                    base = root / "base.md"
                    changelog = root / "changelog.md"
                    base.write_text(
                        "<!-- RELEASE_CHANGELOG -->\n\n<!-- RELEASE_DOWNLOADS -->\n"
                        if stable
                        else "# Preview\n\n<!-- RELEASE_CHANGELOG -->\n",
                        encoding="utf-8",
                    )
                    content = {
                        "中文": "中文摘要。\n\n#### 更新\n\n- 中文内容。",
                        "English": "English summary.\n\n#### Changes\n\n- English content.",
                    }
                    changelog.write_text(
                        "## 2.1.0\n\n"
                        + "\n\n".join(
                            f"### {language}\n\n{content[language]}"
                            for language in languages
                        )
                        + "\n\n## 2.0.0\n\nOlder release.\n",
                        encoding="utf-8",
                    )
                    notes = COMPOSE_RELEASE_NOTES.compose_notes(
                        "2.1.0", "v2.1.0", base, changelog
                    )
                    expected = (
                        "[中文](#中文) | [English](#english)\n\n"
                        + ("" if stable else "# Preview\n\n## 2.1.0\n\n")
                        + "### 中文\n\n中文摘要。\n\n#### 更新\n\n- 中文内容。\n\n"
                        "### English\n\nEnglish summary.\n\n#### Changes\n\n- English content."
                    )
                    self.assertEqual(
                        notes.split("\n\n## 📥 Download for your system")[0].rstrip(),
                        expected,
                    )

    def test_stable_notes_include_versioned_download_matrix(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            base = root / "base.md"
            changelog = root / "changelog.md"
            base.write_text(
                "<!-- RELEASE_CHANGELOG -->\n\n<!-- RELEASE_DOWNLOADS -->\n",
                encoding="utf-8",
            )
            changelog.write_text(
                "## 2.0.0\n\nFirst stable release with a summary that is\n"
                "soft-wrapped in the changelog source.\n\n### Fixes\n\n- Fixed one issue.\n",
                encoding="utf-8",
            )

            notes = COMPOSE_RELEASE_NOTES.compose_notes(
                "2.0.0", "v2.0.0", base, changelog
            )

        self.assertIn("## 📥 Download for your system", notes)
        self.assertIn("OxideTerm_2.0.0_windows_x64-setup.exe", notes)
        self.assertIn("OxideTerm_2.0.0_macos_arm64.dmg", notes)
        self.assertIn("OxideTerm_2.0.0_linux_arm64.rpm", notes)
        self.assertNotIn("RELEASE_DOWNLOADS", notes)
        self.assertNotIn("# Stable", notes)
        self.assertNotIn("## 2.0.0", notes)
        self.assertTrue(
            notes.startswith(
                "First stable release with a summary that is soft-wrapped in the changelog source."
            )
        )
        self.assertNotIn("that is\nsoft-wrapped", notes)
        self.assertLess(notes.index("### Fixes"), notes.index("## 📥 Download for your system"))

    def test_markdown_block_at_start_is_not_unwrapped(self) -> None:
        section = "> Important first line.\n> Important second line.\n\nDetails."

        normalized = COMPOSE_RELEASE_NOTES.normalize_leading_summary(section)

        self.assertEqual(normalized, section)

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

        self.assertNotIn("📥 Download for your system", notes)
        self.assertIn("## 2.0.0-preview.1", notes)
        self.assertIn("Preview notes.", notes)


if __name__ == "__main__":
    unittest.main()
