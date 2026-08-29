#!/usr/bin/env python3
"""Focused supply-chain checks for the pinned OxideSpice helper fetcher."""

from pathlib import Path
import json
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "release"))

import fetch_oxide_spice_helper


class OxideSpiceChecksumTests(unittest.TestCase):
    def test_windows_checksum_line_ending_is_accepted(self) -> None:
        digest = "a" * 64

        self.assertEqual(
            fetch_oxide_spice_helper.parse_checksum(
                f"{digest}  oxide-spice-helper.zip\r\n"
            ),
            (digest, "oxide-spice-helper.zip"),
        )

    def test_parent_archive_path_is_rejected(self) -> None:
        with self.assertRaises(fetch_oxide_spice_helper.HelperArtifactError):
            fetch_oxide_spice_helper.safe_archive_path("../helper")


class OxideSpiceMetadataTests(unittest.TestCase):
    def test_artifact_requires_the_exact_capability_contract(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory)
            (artifact / "bin").mkdir()
            (artifact / "bin" / "oxide-spice-helper").write_bytes(b"helper")
            (artifact / "licenses").mkdir()
            (artifact / "licenses" / "LICENSE").write_text(
                "license\n", encoding="utf-8"
            )
            for name in (
                "LICENSE",
                "THIRD-PARTY-NOTICES.md",
                "oxide-spice-helper.cdx.json",
            ):
                (artifact / name).write_text("document\n", encoding="utf-8")
            (artifact / "helper-metadata.json").write_text(
                json.dumps(
                    {
                        "helperVersion": "0.1.0",
                        "ipcProtocolVersion": 1,
                        "target": "x86_64-unknown-linux-gnu",
                        "capabilities": ["core-session"],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaises(fetch_oxide_spice_helper.HelperArtifactError):
                fetch_oxide_spice_helper.validate_artifact(
                    artifact,
                    target="x86_64-unknown-linux-gnu",
                    version="0.1.0",
                    ipc_protocol_version=1,
                    required_capabilities=["core-session", "usb-redir"],
                )


if __name__ == "__main__":
    unittest.main()
