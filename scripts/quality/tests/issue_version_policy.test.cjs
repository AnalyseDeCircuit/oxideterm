'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const policy = require('../issue_version_policy.cjs');

function issueBody(version) {
  return `### OxideTerm version / 版本

${version}

### Summary / 简述

The reported behavior can be reproduced consistently.
`;
}

function release(tagName, options = {}) {
  return {
    draft: false,
    prerelease: false,
    html_url: `https://github.com/AnalyseDeCircuit/oxideterm/releases/tag/${tagName}`,
    tag_name: tagName,
    ...options,
  };
}

test('reads only unambiguous stable versions from the dedicated form field', () => {
  for (const [version, expected] of [
    ['v2.0.9', '2.0.9'],
    ['OxideTerm 2.0.9 (stable)', '2.0.9'],
    ['2.0.9+package.1', '2.0.9'],
    ['2.1.0-beta.1', null],
    ['2.1.0 beta', null],
    ['gpui-v2.1.0', null],
    ['native-v2.1.0', null],
    ['nightly', null],
    ['version two', null],
    ['2.0.8 or 2.0.9', null],
    ['2.0.9.1', null],
    ['2.0.9custom', null],
  ]) {
    const actual = policy.readReportedStableVersion(issueBody(version));
    assert.equal(actual === null ? null : actual.value, expected, version);
  }
  assert.equal(policy.readReportedStableVersion('### Summary / 简述\n\nNo version'), null);
});

test('selects the highest semantic stable release only', () => {
  const latest = policy.findLatestStableRelease([
    release('v2.9.0'),
    release('v2.10.0'),
    release('v3.0.0-beta.1', { prerelease: true }),
    release('gpui-v4.0.0'),
    release('v9.0.0', { draft: true }),
  ]);

  assert.equal(latest.value, '2.10.0');
});

test('reminds only when a reported stable version is older', () => {
  const releases = [release('v2.0.9'), release('v2.0.8')];

  assert.equal(
    policy.findOutdatedStableVersion(issueBody('2.0.8'), releases).latest.value,
    '2.0.9'
  );
  assert.equal(policy.findOutdatedStableVersion(issueBody('2.0.9'), releases), null);
  assert.equal(policy.findOutdatedStableVersion(issueBody('2.1.0'), releases), null);
  assert.equal(policy.findOutdatedStableVersion(issueBody('2.1.0-beta.1'), releases), null);
});

test('flags a previous-generation major version as obsolete', () => {
  const releases = [release('v2.0.15'), release('v1.6.11')];

  const obsolete = policy.findObsoleteStableVersion(issueBody('1.6.11'), releases);
  assert.ok(obsolete);
  assert.equal(obsolete.reported.value, '1.6.11');
  assert.equal(obsolete.latest.value, '2.0.15');

  // Same-generation old versions are outdated, not obsolete.
  assert.equal(policy.findObsoleteStableVersion(issueBody('2.0.8'), releases), null);
  // Current and future versions are never obsolete.
  assert.equal(policy.findObsoleteStableVersion(issueBody('2.0.15'), releases), null);
  assert.equal(policy.findObsoleteStableVersion(issueBody('2.1.0'), releases), null);
  assert.equal(policy.findObsoleteStableVersion(issueBody('1.6.11-beta.1'), releases), null);
});

test('version notices carry their own marker and deduplicate only bot comments', () => {
  const latest = release('v2.0.15');
  for (const [version, find, build, hasNotice, marker, otherMarker] of [
    ['1.6.11', policy.findObsoleteStableVersion, policy.buildObsoleteVersionNotice,
      policy.hasObsoleteVersionNotice, policy.OBSOLETE_VERSION_NOTICE_MARKER, policy.VERSION_REMINDER_MARKER],
    ['2.0.8', policy.findOutdatedStableVersion, policy.buildVersionReminder,
      policy.hasVersionReminder, policy.VERSION_REMINDER_MARKER, policy.OBSOLETE_VERSION_NOTICE_MARKER],
  ]) {
    const message = build(find(issueBody(version), [latest]));
    assert.ok(message.includes(marker), version);
    assert.equal(message.includes(otherMarker), false, version);
    assert.ok(message.includes(`**v${version}**`), version);
    assert.ok(message.includes(`[v2.0.15](${latest.html_url})`), version);
    assert.ok(message.includes('You reported'), version);
    assert.ok(message.includes('你提交 Issue 时填写的是'), version);
    assert.equal(hasNotice([{ user: { type: 'Bot' }, body: message }]), true, version);
    assert.equal(hasNotice([
      { user: { type: 'User' }, body: message },
      { user: { type: 'Bot' }, body: `${otherMarker}\nOther notice` },
      { user: { type: 'Bot' }, body: 'Unrelated comment' },
    ]), false, version);
  }
});
