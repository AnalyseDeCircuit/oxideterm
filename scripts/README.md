# Repository Scripts

Scripts are grouped by responsibility:

- `automation/`: maintenance routing, safety policy, and shadow-analysis tools;
- `build/`: local artifact builders for the CLI and remote agent;
- `ci/`: shared CI environment setup;
- `quality/`: repository and vendored-source audits;
- `release/`: versioning, legal notices, packaging, and package verification;
- `tests/`: unit tests for repository automation.

Run scripts from the repository root unless a script documents otherwise.

## Project statistics

Use Python 3.11+ and Git; no Python packages or Cargo build are needed:

```bash
python3 scripts/project-stats.py
python3 scripts/project-stats.py --by-crate --by-dir
python3 scripts/project-stats.py --include-vendor
```

The same options are available through `just stats`, for example
`just stats --by-crate --by-dir` or `just stats --include-vendor`.

The script counts tracked files and non-ignored local files. Project source is
reported separately from documentation/configuration. `--include-vendor` adds a
separate table for GPUI-CE, Alacritty, russh-sftp, and vte, including their local
patches; it does not add those lines to the project-source total. External Cargo
dependencies, including the standalone russh repository, are outside the scan.
Nested Cargo packages such as `fernomade/*` and the standalone `agent` are resolved
from their own manifests.

Build outputs, asset/resource directories, and the generated theme/icon Rust
tables are excluded. `.github` workflows and Python scripts are included.
Code/comment counts are line-based estimates, not language-parser results;
multiline strings can resemble comments. Test code remains in source totals;
the old brace-based inline-test estimate is no longer reported as a test ratio.
