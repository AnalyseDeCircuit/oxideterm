'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const gate = require('../issue_quality_policy.cjs');

function featureBody({ version = '2.0.5', problem = '会话录制文件无法被转换工具读取。', proposal = '让录制文件保持标准格式兼容。', importance = '团队每周导出多次录制用于审计。' } = {}) {
  return `### OxideTerm version / 版本

${version}

### Problem or use case / 问题或使用场景

${problem}

### Proposed solution / 期望方案

${proposal}

### Why is this important? / 为什么这个功能对你重要？

${importance}
`;
}

function bugBody({ version = '2.0.5', reproduction = '打开应用，建立连接，然后点击终端录制按钮。' } = {}) {
  return `### OxideTerm version / 版本

${version}

### Platform / 平台

macOS

### Summary / 简述

停止会话录制时应用没有保存文件。

### Steps to reproduce / 复现步骤

${reproduction}

### Expected vs actual / 预期与实际

预期保存文件，实际没有生成文件。
`;
}

test('evaluates template evidence without mixing blocking findings and review labels', () => {
  for (const { name, input, blocking = [], review = [], labels = [] } of [
    {
      name: 'complete feature with an unanswered optional section',
      input: {
        title: '会话录制支持标准格式转换',
        body: `${featureBody()}\n### Additional context / 补充信息\n\n_No response_\n`,
        labels: ['enhancement'],
      },
    },
    {
      name: 'title needs detail',
      input: { title: '录制', body: featureBody(), labels: ['enhancement'] },
      blocking: [{ code: 'title_needs_detail' }],
    },
    {
      name: 'required proposal missing',
      input: { body: featureBody({ proposal: '_No response_' }), labels: ['enhancement'] },
      blocking: [{ code: 'required_section_missing', heading: 'Proposed solution / 期望方案' }],
    },
    {
      name: 'environment version is not the product version',
      input: { body: `${bugBody()}\n### Additional environment details / 其他相关环境信息\n\nmacOS 15.0\n` },
    },
    { name: 'released version', input: { body: bugBody({ version: '2.0.5' }) } },
    ...['99.0.0', '2.1.17'].map((version) => ({
      name: `unreleased version ${version}`,
      input: { body: bugBody({ version }) },
      blocking: [{ code: 'release_version_unverified', version }],
    })),
    {
      name: 'thin reproduction requires review only',
      input: { body: bugBody({ reproduction: '点击录制' }) },
      review: [{ code: 'reproduction_evidence_thin' }],
      labels: ['needs-reproduction-steps'],
    },
  ]) {
    const report = gate.evaluateIssue({
      title: '停止录制后文件没有保存',
      labels: ['bug'],
      releasedVersions: ['2.0.5', '2.0.4', '2.0.3'],
      ...input,
    });
    assert.deepEqual(report, { blockingFindings: blocking, reviewFindings: review }, name);
    assert.deepEqual(gate.labelsForReviewFindings(report.reviewFindings), labels, name);
  }
});

test('uses a stable marker while replacing the correction notice content', () => {
  const first = gate.buildCorrectionNotice([{ code: 'title_needs_detail' }]);
  const second = gate.buildRecoveryNotice([]);

  assert.equal(first.includes(gate.GATE_COMMENT_MARKER), true);
  assert.equal(second.includes(gate.GATE_COMMENT_MARKER), true);
  assert.notEqual(first, second);
});

test('closes blocked issues and reopens only corrected closures owned by the gate', () => {
  for (const [currentState, hasBlockingFindings, correctionLabelPresent, gateCommentPresent, expected] of [
    ['open', true, false, false, 'close'],
    ['closed', true, true, true, 'keep'],
    ['closed', false, true, true, 'reopen'],
    ['closed', false, false, true, 'keep'],
    ['closed', false, true, false, 'keep'],
  ]) {
    const input = { currentState, hasBlockingFindings, correctionLabelPresent, gateCommentPresent };
    assert.equal(gate.decideStateChange(input), expected, JSON.stringify(input));
  }
});

test('keeps a maintainer-reopened issue outside later automatic closures', () => {
  assert.equal(
    gate.isTrustedManualReopen({
      action: 'reopened',
      senderType: 'User',
      actorPermission: 'write',
    }),
    true
  );
  assert.equal(
    gate.isTrustedManualReopen({
      action: 'reopened',
      senderType: 'User',
      actorPermission: 'read',
    }),
    false
  );
  assert.equal(
    gate.isTrustedManualReopen({
      action: 'reopened',
      senderType: 'Bot',
      actorPermission: 'write',
    }),
    false
  );
  assert.equal(
    gate.isTrustedManualReopen({
      action: 'edited',
      senderType: 'User',
      actorPermission: 'write',
    }),
    false
  );
});
