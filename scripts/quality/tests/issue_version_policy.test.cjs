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

test('reads a stable version from the dedicated issue form field', () => {
  assert.equal(policy.readReportedStableVersion(issueBody('v2.0.9')).value, '2.0.9');
  assert.equal(policy.readReportedStableVersion(issueBody('OxideTerm 2.0.9 (stable)')).value, '2.0.9');
  assert.equal(policy.readReportedStableVersion(issueBody('2.0.9+package.1')).value, '2.0.9');
});

test('ignores prerelease and non-stable channel versions', () => {
  const versions = ['2.1.0-beta.1', '2.1.0 beta', 'gpui-v2.1.0', 'native-v2.1.0', 'nightly'];
  for (const version of versions) {
    assert.equal(policy.readReportedStableVersion(issueBody(version)), null);
  }
});

test('ignores missing, malformed, and ambiguous version answers', () => {
  assert.equal(policy.readReportedStableVersion('### Summary / 简述\n\nNo version'), null);
  assert.equal(policy.readReportedStableVersion(issueBody('version two')), null);
  assert.equal(policy.readReportedStableVersion(issueBody('2.0.8 or 2.0.9')), null);
  assert.equal(policy.readReportedStableVersion(issueBody('2.0.9.1')), null);
  assert.equal(policy.readReportedStableVersion(issueBody('2.0.9custom')), null);
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

test('builds a bilingual obsolete notice with a distinct marker', () => {
  const message = policy.buildObsoleteVersionNotice({
    reported: { value: '1.6.11' },
    latest: { value: '2.0.15', releaseUrl: 'https://github.com/AnalyseDeCircuit/oxideterm/releases/tag/v2.0.15' },
  });

  assert.equal(message.includes(policy.OBSOLETE_VERSION_NOTICE_MARKER), true);
  assert.equal(message.includes(policy.VERSION_REMINDER_MARKER), false);
  assert.equal(message.includes('**v1.6.11**'), true);
  assert.equal(message.includes('[v2.0.15]('), true);
});

test('recognizes only an existing bot obsolete notice for retry deduplication', () => {
  const noticeBody = `${policy.OBSOLETE_VERSION_NOTICE_MARKER}\nNotice`;

  assert.equal(policy.hasObsoleteVersionNotice([
    { user: { type: 'Bot' }, body: noticeBody },
  ]), true);
  assert.equal(policy.hasObsoleteVersionNotice([
    { user: { type: 'User' }, body: noticeBody },
    { user: { type: 'Bot' }, body: 'Unrelated comment' },
  ]), false);
});

test('builds a bilingual reminder with a stable marker and release link', () => {
  const result = policy.findOutdatedStableVersion(
    issueBody('2.0.8'),
    [release('v2.0.9')]
  );
  const message = policy.buildVersionReminder(result);

  assert.equal(message.includes(policy.VERSION_REMINDER_MARKER), true);
  assert.equal(message.includes('**v2.0.8**'), true);
  assert.equal(message.includes('[v2.0.9]('), true);
  assert.equal(message.includes('此提醒仅比较稳定版'), false);
});

test('recognizes only an existing bot reminder for retry deduplication', () => {
  const reminderBody = `${policy.VERSION_REMINDER_MARKER}\nReminder`;

  assert.equal(policy.hasVersionReminder([
    { user: { type: 'Bot' }, body: reminderBody },
  ]), true);
  assert.equal(policy.hasVersionReminder([
    { user: { type: 'User' }, body: reminderBody },
    { user: { type: 'Bot' }, body: 'Unrelated comment' },
  ]), false);
});
