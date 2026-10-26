const { execFileSync, spawnSync } = require('node:child_process');
const { readFileSync } = require('node:fs');

function needsRustChecks(paths) {
  return paths.some((path) => !(
    path.startsWith('docs/')
    || /^[^/]+\.md$/i.test(path)
    || /(^|\/)(README|CHANGELOG|CONTRIBUTING|AGENTS)(\.[^/]+)?\.md$/i.test(path)
    || (path.startsWith('.github/') && path.endsWith('.md'))
  ));
}

if (require.main === module) {
  let rust = true;
  let base;
  if (process.env.GITHUB_EVENT_NAME === 'pull_request') {
    // The workflow fetches the PR merge commit and both parents.
    base = 'HEAD^1';
  } else if (process.env.GITHUB_EVENT_NAME === 'push') {
    const { before } = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
    if (!/^[0-9a-f]{40}$/.test(before)) {
      throw new Error('Push event is missing a valid before commit');
    }
    // A newly created branch has no previous tree to compare; validate it fully.
    if (before !== '0'.repeat(40)) {
      base = before;
      // Multi-commit pushes can start beyond the shallow checkout's history.
      if (spawnSync('git', ['cat-file', '-e', `${base}^{commit}`], { stdio: 'ignore' }).status !== 0) {
        execFileSync('git', ['fetch', '--no-tags', '--depth=1', 'origin', base], { stdio: 'inherit' });
      }
    }
  }
  if (base) {
    // Include removed code paths when files move into docs. Failed fetches or
    // diffs must fail the check rather than skip Rust validation.
    const paths = execFileSync('git', [
      'diff', '--name-only', '--no-renames', '-z', base, 'HEAD',
    ], { encoding: 'utf8' }).split('\0').filter(Boolean);
    rust = needsRustChecks(paths);
  }
  process.stdout.write(`rust=${rust}\n`);
}

module.exports = { needsRustChecks };
