'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const policy = require('../maintenance_policy.cjs');

function bugIssue({
  number = 400,
  platform = 'macOS',
  title = '切换页面后终端停止刷新',
  reproduction = '建立 SSH 连接，切换到设置页，再切回终端页面。',
  extra = '',
  labels = ['bug'],
  state = 'open',
} = {}) {
  return {
    number,
    html_url: `https://github.com/AnalyseDeCircuit/oxideterm/issues/${number}`,
    title,
    state,
    labels,
    body: `### OxideTerm version / 版本

2.0.11

### Platform / 平台

${platform}

### Summary / 简述

终端在页面切换后不再刷新。

### Steps to reproduce / 复现步骤

${reproduction}

### Expected vs actual / 预期与实际

预期继续刷新，实际画面停止。

${extra}

### Checklist

- [x] I tested with the latest release and searched existing issues.
`,
  };
}

test('routes a bounded non-sensitive bug to the future agent candidate queue', () => {
  const report = policy.analyzeIssue(bugIssue());

  assert.equal(report.route, 'candidate_for_agent');
  assert.equal(report.reasons.includes('release_or_update_boundary'), false);
  assert.equal(report.confidence, 'high');
  assert.deepEqual(report.recommendedLabels, ['automation:candidate']);
  assert.equal(report.mutationAllowed, false);
  assert.equal(report.writesPerformed, false);
});

test('accepts GraphQL uppercase issue states during offline audits', () => {
  const report = policy.analyzeIssue(bugIssue({ state: 'OPEN' }));

  assert.equal(report.route, 'candidate_for_agent');
});

test('keeps Windows-only reports behind platform validation', () => {
  const report = policy.analyzeIssue(bugIssue({ platform: 'Windows 11' }));

  assert.equal(report.route, 'needs_human');
  assert.deepEqual(report.platforms, ['windows']);
  assert.deepEqual(report.recommendedLabels, [
    'automation:needs-human',
    'automation:windows-validation',
  ]);
  assert.equal(report.reasons.includes('windows_only_validation'), true);
});

test('keeps credential and authentication work out of automatic implementation', () => {
  const report = policy.analyzeIssue(bugIssue({
    title: '私钥认证失败',
    extra: '使用 private key 登录时 authentication failed。',
  }));

  assert.equal(report.route, 'needs_human');
  assert.equal(report.reasons.includes('credential_or_secret_boundary'), true);
  assert.equal(report.reasons.includes('authentication_boundary'), true);
});

test('keeps feature decisions with the maintainer', () => {
  // Feature requests must satisfy their own template before policy routing begins.
  const issue = {
    ...bugIssue({ labels: ['enhancement'], title: '增加 SPICE 远程桌面支持' }),
    body: `### OxideTerm version / 版本

2.0.11

### Problem or use case / 问题或使用场景

VNC 无法满足虚拟机控制场景中的低延迟和设备共享需求。

### Proposed solution / 期望方案

增加 SPICE 远程桌面协议支持，并由维护者确定产品边界。
`,
  };
  const report = policy.analyzeIssue(issue);

  assert.equal(report.route, 'needs_human');
  assert.equal(report.reasons.includes('product_decision_required'), true);
});

test('respects the existing quality gate without taking closure ownership', () => {
  const report = policy.analyzeIssue(bugIssue({
    labels: ['bug', 'incomplete'],
  }));

  assert.equal(report.route, 'blocked_by_quality_gate');
  assert.equal(report.mutationAllowed, false);
});

test('never copies raw issue content into the shadow report', () => {
  const sentinel = 'DO-NOT-UPLOAD-RAW-TERMINAL-CONTENT';
  const report = policy.analyzeIssue(bugIssue({ extra: sentinel }));

  assert.equal(JSON.stringify(report).includes(sentinel), false);
});

test('managed comments are idempotent and never claim a fix or release', () => {
  const report = policy.analyzeIssue(bugIssue());
  const body = policy.buildManagedComment(report);
  const existing = {
    id: 42,
    body,
    user: { login: 'oxideterm-maintainer[bot]', type: 'Bot' },
  };

  assert.equal(body.includes(policy.MANAGED_COMMENT_MARKER), true);
  assert.equal(body.includes('已经修复'), false);
  assert.equal(body.includes('已经发布'), false);
  assert.deepEqual(policy.decideCommentMutation(null, body), {
    action: 'create',
    body,
  });
  assert.deepEqual(policy.decideCommentMutation(existing, body), { action: 'none' });
  assert.equal(
    policy.findManagedComment([existing], 'oxideterm-maintainer[bot]'),
    existing
  );
});

test('updates one existing managed comment when routing changes', () => {
  const candidate = policy.buildManagedComment(policy.analyzeIssue(bugIssue()));
  const needsHuman = policy.buildManagedComment(policy.analyzeIssue(
    bugIssue({ platform: 'Windows 11' })
  ));
  const existing = {
    id: 43,
    body: candidate,
    user: { login: 'oxideterm-maintainer[bot]', type: 'Bot' },
  };

  assert.deepEqual(policy.decideCommentMutation(existing, needsHuman), {
    action: 'update',
    commentId: 43,
    body: needsHuman,
  });
});

test('blocks automation control files and isolates sensitive product paths', () => {
  const result = policy.classifyChangedPaths([
    '.github/workflows/maintenance-automation.yml',
    'scripts/automation/maintenance_policy.cjs',
    'crates/oxideterm-secret-store/src/lib.rs',
    'crates/oxideterm-gpui-terminal/src/app.rs',
  ]);

  assert.deepEqual(result.protected, [
    '.github/workflows/maintenance-automation.yml',
    'scripts/automation/maintenance_policy.cjs',
  ]);
  assert.deepEqual(result.humanReview, [
    'crates/oxideterm-secret-store/src/lib.rs',
  ]);
  assert.deepEqual(result.allowed, [
    'crates/oxideterm-gpui-terminal/src/app.rs',
  ]);
});

test('detects configured secrets and common credential material', () => {
  const configuredSecret = 'not-a-real-secret-value';

  assert.deepEqual(
    policy.scanTextForCredentials(`output=${configuredSecret}`, [configuredSecret]),
    ['configured_secret_value']
  );
  assert.deepEqual(
    policy.scanTextForCredentials('-----BEGIN PRIVATE KEY-----'),
    ['private_key_material']
  );
});
