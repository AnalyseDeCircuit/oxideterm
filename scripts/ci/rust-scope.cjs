const { execFileSync } = require('node:child_process');

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
  if (process.env.GITHUB_EVENT_NAME === 'pull_request') {
    // The workflow fetches the PR merge commit and both parents. Disabling rename
    // detection includes the removed code path when a file is moved into docs.
    // A missing parent or failed diff must fail the check, not skip Rust validation.
    const paths = execFileSync('git', [
      'diff', '--name-only', '--no-renames', '-z', 'HEAD^1', 'HEAD',
    ], { encoding: 'utf8' }).split('\0').filter(Boolean);
    rust = needsRustChecks(paths);
  }
  process.stdout.write(`rust=${rust}\n`);
}

module.exports = { needsRustChecks };
