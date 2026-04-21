#!/usr/bin/env node
/* eslint-disable no-console */

const { spawnSync } = require('child_process');
const path = require('path');

const repoRoot = path.join(__dirname, '..');
const tauriRoot = path.join(repoRoot, 'src-tauri');

const CASES = [
  {
    id: 'upload-file',
    description: 'single-file upload path',
    command: 'pnpm',
    args: [
      'exec',
      'vitest',
      'run',
      'src/test/lib/terminal/trzsz/TauriFileReader.test.ts',
      '-t',
      'opens an upload handle lazily and advances chunk offsets for file reads',
    ],
    cwd: repoRoot,
  },
  {
    id: 'upload-directory',
    description: 'directory upload entry hydration',
    command: 'pnpm',
    args: [
      'exec',
      'vitest',
      'run',
      'src/test/lib/terminal/trzsz/TauriFileReader.test.ts',
      '-t',
      'builds readers for recursive directory uploads and preserves relative paths',
    ],
    cwd: repoRoot,
  },
  {
    id: 'download-file',
    description: 'single-file download write and finish path',
    command: 'cargo',
    args: ['test', 'trzsz::download::tests::writes_and_finishes_download_via_temp_file', '--lib'],
    cwd: tauriRoot,
  },
  {
    id: 'download-directory',
    description: 'directory download creation path',
    command: 'cargo',
    args: ['test', 'trzsz::download::tests::creates_empty_directory_inside_prepared_root', '--lib'],
    cwd: tauriRoot,
  },
  {
    id: 'cancel',
    description: 'abort cleanup for canceled download',
    command: 'cargo',
    args: ['test', 'trzsz::download::tests::abort_removes_temp_file', '--lib'],
    cwd: tauriRoot,
  },
  {
    id: 'malicious-path',
    description: 'path traversal rejection',
    command: 'cargo',
    args: ['test', 'trzsz::path_guard::tests::rejects_traversal_components', '--lib'],
    cwd: tauriRoot,
  },
  {
    id: 'frontend-suite',
    description: 'full frontend trzsz maintenance suite',
    command: 'pnpm',
    args: [
      'exec',
      'vitest',
      'run',
      'src/test/lib/terminal/trzsz/transport.test.ts',
      'src/test/lib/terminal/trzsz/controller.test.ts',
      'src/test/lib/terminal/trzsz/filter.test.ts',
      'src/test/lib/terminal/trzsz/TauriFileReader.test.ts',
      'src/test/lib/terminal/trzsz/TauriFileWriter.test.ts',
    ],
    cwd: repoRoot,
  },
  {
    id: 'rust-suite',
    description: 'full Rust trzsz suite',
    command: 'cargo',
    args: ['test', 'trzsz::', '--lib'],
    cwd: tauriRoot,
  },
];

function printCases() {
  console.log('[trzsz-regression] Cases:');
  for (const testCase of CASES) {
    const rendered = [testCase.command, ...testCase.args].join(' ');
    console.log(`- ${testCase.id}: ${testCase.description}`);
    console.log(`  ${rendered}`);
  }
}

function runCase(testCase) {
  console.log(`\n[trzsz-regression] ${testCase.id}: ${testCase.description}`);
  const result = spawnSync(testCase.command, testCase.args, {
    cwd: testCase.cwd,
    stdio: 'inherit',
    env: process.env,
  });
  if (result.error) {
    console.error(result.error);
    process.exit(1);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function main() {
  const args = process.argv.slice(2);
  const listOnly = args.includes('--list');
  const caseIndex = args.indexOf('--case');
  const selectedCaseId = caseIndex >= 0 ? args[caseIndex + 1] : null;

  if (listOnly) {
    printCases();
    return;
  }

  const selectedCases = selectedCaseId
    ? CASES.filter((testCase) => testCase.id === selectedCaseId)
    : CASES;

  if (selectedCases.length === 0) {
    console.error(`[trzsz-regression] Unknown case: ${selectedCaseId}`);
    process.exit(1);
  }

  console.log('[trzsz-regression] Matrix source: src/lib/terminal/trzsz/UPSTREAM_DIFF.md');
  for (const testCase of selectedCases) {
    runCase(testCase);
  }
  console.log('\n[trzsz-regression] All requested cases passed.');
}

main();