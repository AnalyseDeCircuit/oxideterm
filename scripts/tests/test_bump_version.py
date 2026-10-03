from __future__ import annotations

import contextlib
import importlib.util
import io
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest.mock import patch


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "release" / "bump_version.py"
SPEC = importlib.util.spec_from_file_location("bump_version", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
BUMP_VERSION = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUMP_VERSION)


class BumpVersionTests(unittest.TestCase):
    def test_dynamic_release_badges_do_not_block_version_updates(self) -> None:
        for options in (["--dry-run"], ["--no-lock"], []):
            with (
                self.subTest(options=options),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                manifest = root / "Cargo.toml"
                manifest.write_text(
                    '[workspace.package]\nversion = "2.2.0"\n', encoding="utf-8"
                )
                readme = root / "README.md"
                badge = (
                    "[![Release](https://img.shields.io/github/v/release/"
                    "AnalyseDeCircuit/oxideterm?label=release)]"
                    "(https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)\n"
                )
                readme.write_text(badge, encoding="utf-8")
                with (
                    patch.object(BUMP_VERSION, "ROOT_DIR", root),
                    patch.object(BUMP_VERSION, "WORKSPACE_MANIFEST", manifest),
                    patch.object(
                        BUMP_VERSION.sys,
                        "argv",
                        [str(SCRIPT_PATH), "2.2.1-beta.1", *options],
                    ),
                    patch.object(BUMP_VERSION, "run") as cargo,
                    contextlib.redirect_stdout(io.StringIO()),
                    contextlib.redirect_stderr(io.StringIO()),
                ):
                    self.assertEqual(BUMP_VERSION.main(), 0)
                expected = "2.2.0" if options == ["--dry-run"] else "2.2.1-beta.1"
                self.assertEqual(
                    tomllib.loads(manifest.read_text(encoding="utf-8"))["workspace"]["package"]["version"],
                    expected,
                )
                self.assertEqual(readme.read_text(encoding="utf-8"), badge)
                if options:
                    cargo.assert_not_called()
                else:
                    cargo.assert_called_once_with(
                        ["cargo", "update", "--workspace", "--offline"]
                    )


if __name__ == "__main__":
    unittest.main()
