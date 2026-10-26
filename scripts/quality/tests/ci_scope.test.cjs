const test = require('node:test');
const assert = require('node:assert/strict');
const { execFileSync, spawnSync } = require('node:child_process');
const { mkdtempSync, mkdirSync, writeFileSync, renameSync, rmSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join, resolve } = require('node:path');
const { pathToFileURL } = require('node:url');
const { needsRustChecks } = require('../../ci/rust-scope.cjs');

const script = resolve(__dirname, '../../ci/rust-scope.cjs');

test('only known documentation paths skip Rust checks', () => {
  for (const paths of [[], ['README.md'], ['docs/guide.md', 'docs/images/example.png'],
    ['crates/example/README.md', '.github/PULL_REQUEST_TEMPLATE.md']]) {
    assert.equal(needsRustChecks(paths), false, JSON.stringify(paths));
  }
  for (const path of ['Cargo.lock', 'Cargo.toml', 'crates/example/src/lib.rs',
    'crates/example/tests/input.md', 'scripts/release/package.py',
    '.github/workflows/ci.yml', 'agent/src/main.rs', 'unknown-file']) {
    assert.equal(needsRustChecks(['docs/guide.md', path]), true, path);
  }
});

test('PR and push scopes include code changes across the full diff', (t) => {
  const cwd = mkdtempSync(join(tmpdir(), 'oxideterm-ci-scope-'));
  t.after(() => rmSync(cwd, { recursive: true, force: true }));
  const git = (...args) => execFileSync('git', args, { cwd, stdio: 'pipe' });
  git('init', '--initial-branch=main');
  git('config', 'user.name', 'CI fixture');
  git('config', 'user.email', 'ci@example.test');
  git('config', 'commit.gpgSign', 'false');
  mkdirSync(join(cwd, 'hooks'));
  git('config', 'core.hooksPath', join(cwd, 'hooks'));
  mkdirSync(join(cwd, 'crates/example/src'), { recursive: true });
  mkdirSync(join(cwd, 'docs'));
  writeFileSync(join(cwd, 'crates/example/src/lib.rs'), 'pub fn example() {}\n');
  writeFileSync(join(cwd, 'docs/guide.md'), '# Guide\n');
  git('add', '--', 'crates/example/src/lib.rs', 'docs/guide.md');
  git('commit', '-m', 'fixture base');
  const before = git('rev-parse', 'HEAD').toString().trim();
  const eventPath = join(cwd, 'push-event.json');
  writeFileSync(eventPath, JSON.stringify({ before }));
  const checkPush = (directory = cwd) => execFileSync(process.execPath, [script], {
    cwd: directory,
    env: { ...process.env, GITHUB_EVENT_NAME: 'push', GITHUB_EVENT_PATH: eventPath },
    encoding: 'utf8',
  });

  const check = () => execFileSync(process.execPath, [script], {
    cwd, env: { ...process.env, GITHUB_EVENT_NAME: 'pull_request' }, encoding: 'utf8',
  });
  git('switch', '-c', 'docs-only');
  writeFileSync(join(cwd, 'docs/guide.md'), '# Updated guide\n');
  git('add', '--', 'docs/guide.md');
  git('commit', '-m', 'docs fixture');
  git('switch', 'main');
  git('merge', '--no-ff', 'docs-only', '-m', 'merge docs fixture');
  assert.equal(check(), 'rust=false\n');
  assert.equal(checkPush(), 'rust=false\n');

  git('switch', '-c', 'move-code');
  renameSync(join(cwd, 'crates/example/src/lib.rs'), join(cwd, 'docs/lib.md'));
  git('add', '--', 'crates/example/src/lib.rs', 'docs/lib.md');
  git('commit', '-m', 'move code fixture');
  git('switch', 'main');
  git('merge', '--no-ff', 'move-code', '-m', 'merge code fixture');
  assert.equal(check(), 'rust=true\n');

  for (const title of ['Updated again', 'Final guide']) {
    writeFileSync(join(cwd, 'docs/guide.md'), `# ${title}\n`);
    git('add', '--', 'docs/guide.md');
    git('commit', '-m', title);
  }
  const checkout = join(cwd, 'checkout');
  git('clone', '--depth=2', pathToFileURL(cwd).href, checkout);
  assert.notEqual(spawnSync('git', ['cat-file', '-e', `${before}^{commit}`], {
    cwd: checkout, stdio: 'ignore',
  }).status, 0);
  assert.equal(checkPush(checkout), 'rust=true\n');

  writeFileSync(eventPath, JSON.stringify({ before: '0'.repeat(40) }));
  assert.equal(checkPush(), 'rust=true\n');
});

test('manual runs always retain Rust validation', () => {
  assert.equal(execFileSync(process.execPath, [script], {
    env: { ...process.env, GITHUB_EVENT_NAME: 'workflow_dispatch' }, encoding: 'utf8',
  }), 'rust=true\n');
});

test('an unavailable PR diff fails instead of skipping checks', (t) => {
  const cwd = mkdtempSync(join(tmpdir(), 'oxideterm-ci-missing-'));
  t.after(() => rmSync(cwd, { recursive: true, force: true }));
  const result = spawnSync(process.execPath, [script], {
    cwd, env: { ...process.env, GITHUB_EVENT_NAME: 'pull_request' }, encoding: 'utf8',
  });
  assert.notEqual(result.status, 0);
  assert.equal(result.stdout, '');
});
