"""Regression coverage for repository scope and line-count estimates."""

import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "project-stats.py"
spec = importlib.util.spec_from_file_location("project_stats", SCRIPT)
project_stats = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = project_stats
spec.loader.exec_module(project_stats)


class ProjectStatsTests(unittest.TestCase):
    def test_comment_estimates_preserve_code_after_block_comments(self):
        for language, source, expected in [
            ("Rust", '// note\n\n/* outer\n /* inner */\n */ let n = 1;\nlet url = "https://example.test";\n', (1, 3, 2)),
            ("Python", '# note\n\nprint("# code")\n', (1, 1, 1)),
            ("Rust", '/* comment */\n/* comment */ fn main() {}\n', (0, 1, 1)),
        ]:
            with self.subTest(language=language, source=source):
                counts = project_stats.count_lines(source, language)
                self.assertEqual((counts.blank, counts.comment, counts.code), expected)

    def test_git_scope_vendor_separation_and_nested_packages(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            fixtures = {
                ".gitignore": "target/\nprivate/\n",
                ".github/workflows/check.yml": "name: Check\n",
                "crates/fernomade/wire/Cargo.toml": '[package]\nname = "fernomade-wire"\n',
                "crates/fernomade/wire/src/lib.rs": "pub fn wire() {}\n",
                "crates/gpui-ce/gpui/Cargo.toml": '[package]\nname = "gpui"\n',
                "crates/gpui-ce/gpui/src/lib.rs": "pub fn draw() {}\n",
                "agent/Cargo.toml": '[package]\nname = "oxideterm-agent"\n',
                "agent/src/main.rs": "fn main() {}\n",
                "scripts/build/build-agent.sh": "echo build\n",
                "scripts/local file.py": "print('local')\n",
                "README.md": "# Docs\n",
                "target/build.rs": "fn generated() {}\n",
                "private/scratch.rs": "fn private() {}\n",
                "crates/oxideterm-theme/src/generated.rs": "fn table() {}\n",
                "crates/oxideterm-gpui-ui/resources/icons.json": '{}\n',
                "deleted.rs": "fn removed() {}\n",
            }
            for name, content in fixtures.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content, encoding="utf-8")
            subprocess.run(["git", "-C", str(root), "add", "--", "agent", "deleted.rs"], check=True)
            (root / "deleted.rs").unlink()
            (root / "linked.rs").symlink_to(root / "agent/src/main.rs")
            expected = {
                ".github/workflows/check.yml", "crates/fernomade/wire/Cargo.toml",
                "crates/fernomade/wire/src/lib.rs", "agent/Cargo.toml", "agent/src/main.rs",
                "scripts/build/build-agent.sh", "scripts/local file.py", "README.md",
            }
            self.assertEqual(set(map(str, project_stats.collect_files(root))), expected)
            self.assertEqual(
                set(map(str, project_stats.collect_files(root, True))),
                expected | {"crates/gpui-ce/gpui/Cargo.toml", "crates/gpui-ce/gpui/src/lib.rs"},
            )
            for relative, package in [
                ("crates/fernomade/wire/src/lib.rs", "fernomade-wire"),
                ("agent/src/main.rs", "oxideterm-agent"),
                ("crates/gpui-ce/gpui/src/lib.rs", "gpui"),
            ]:
                self.assertEqual(project_stats.crate_name(root, Path(relative), {}), package)
            result = subprocess.run(
                [sys.executable, str(SCRIPT), str(root), "--by-crate", "--by-dir", "--include-vendor"],
                check=True, capture_output=True, text=True,
            ).stdout
            project = result.split("Project source\n")[1].split("Bundled third-party source")[0]
            vendor = result.split("Bundled third-party source (including local patches)\n")[1].split("Documentation")[0]
            self.assertEqual(next(line.split() for line in project.splitlines() if line.startswith("TOTAL")), ["TOTAL", "4", "0", "0", "4"])
            self.assertEqual(next(line.split() for line in vendor.splitlines() if line.startswith("TOTAL")), ["TOTAL", "1", "0", "0", "1"])
            self.assertIn("Vendor / gpui", result)


if __name__ == "__main__":
    unittest.main()
