'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const gate = require('../issue_quality_policy.cjs');

function featureBody({ version = '2.0.5', problem = '会话录制文件无法被转换工具读取。', proposal = '让录制文件保持标准格式兼容。' } = {}) {
  return `### OxideTerm version / 版本

${version}

### Problem or use case / 问题或使用场景

${problem}

### Proposed solution / 期望方案

${proposal}

### Alternatives considered / 已考虑的替代方案

_No response_
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

test('accepts a complete feature request with an unanswered optional section', () => {
  const report = gate.evaluateIssue({
    title: '会话录制支持标准格式转换',
    body: featureBody(),
    labels: ['enhancement'],
    releasedVersions: ['2.0.5'],
  });

  assert.deepEqual(report.blockingFindings, []);
  assert.deepEqual(report.reviewFindings, []);
});

test('requires a descriptive title without imposing a type prefix', () => {
  const report = gate.evaluateIssue({
    title: '录制',
    body: featureBody(),
    labels: ['enhancement'],
    releasedVersions: ['2.0.5'],
  });

  assert.deepEqual(report.blockingFindings.map((item) => item.code), ['title_needs_detail']);
});

test('checks required template sections but ignores optional no-response text', () => {
  const report = gate.evaluateIssue({
    title: '会话录制转换功能',
    body: featureBody({ proposal: '_No response_' }),
    labels: ['enhancement'],
    releasedVersions: ['2.0.5'],
  });

  assert.equal(report.blockingFindings.length, 1);
  assert.equal(report.blockingFindings[0].code, 'required_section_missing');
  assert.equal(report.blockingFindings[0].heading, 'Proposed solution / 期望方案');
});

test('reads the product version only from the dedicated template section', () => {
  const body = `${bugBody()}\n### Additional environment details / 其他相关环境信息\n\nmacOS 15.0\n`;
  const report = gate.evaluateIssue({
    title: '停止录制后文件没有保存',
    body,
    labels: ['bug'],
    releasedVersions: ['2.0.5'],
  });

  assert.equal(gate.readSubmittedVersion(body), '2.0.5');
  assert.equal(
    report.reviewFindings.some((item) => item.code === 'release_version_unverified'),
    false
  );
});

test('blocks an issue that reports a version that was never released', () => {
  const report = gate.evaluateIssue({
    title: '停止录制后文件没有保存',
    body: bugBody({ version: '99.0.0' }),
    labels: ['bug'],
    releasedVersions: ['2.0.5'],
  });

  assert.deepEqual(report.reviewFindings, []);
  assert.deepEqual(
    report.blockingFindings.map((item) => item.code),
    ['release_version_unverified']
  );
});

test('accepts a real released version without blocking the issue', () => {
  const report = gate.evaluateIssue({
    title: '停止录制后文件没有保存',
    body: bugBody({ version: '2.0.5' }),
    labels: ['bug'],
    releasedVersions: ['2.0.5', '2.0.4', '2.0.3'],
  });

  assert.deepEqual(report.blockingFindings, []);
  assert.deepEqual(report.reviewFindings, []);
});

test('keeps a fabricated future version out of the review labels', () => {
  const report = gate.evaluateIssue({
    title: '停止录制后文件没有保存',
    body: bugBody({ version: '2.1.17' }),
    labels: ['bug'],
    releasedVersions: ['2.0.5'],
  });

  assert.equal(
    report.blockingFindings.some((item) => item.code === 'release_version_unverified'),
    true
  );
  assert.deepEqual(gate.labelsForReviewFindings(report.reviewFindings), []);
});

test('keeps thin reproduction evidence as a non-blocking review finding', () => {
  const report = gate.evaluateIssue({
    title: '停止录制后文件没有保存',
    body: bugBody({ reproduction: '点击录制' }),
    labels: ['bug'],
    releasedVersions: ['2.0.5'],
  });

  assert.deepEqual(report.blockingFindings, []);
  assert.equal(
    report.reviewFindings.some((item) => item.code === 'reproduction_evidence_thin'),
    true
  );
});

test('uses a stable marker while replacing the correction notice content', () => {
  const first = gate.buildCorrectionNotice([{ code: 'title_needs_detail' }]);
  const second = gate.buildRecoveryNotice([]);

  assert.equal(first.includes(gate.GATE_COMMENT_MARKER), true);
  assert.equal(second.includes(gate.GATE_COMMENT_MARKER), true);
  assert.notEqual(first, second);
});

test('closes a newly blocked issue', () => {
  assert.equal(
    gate.decideStateChange({
      currentState: 'open',
      hasBlockingFindings: true,
      correctionLabelPresent: false,
      gateCommentPresent: false,
    }),
    'close'
  );
});

test('reopens only a corrected closure owned by the quality gate', () => {
  assert.equal(
    gate.decideStateChange({
      currentState: 'closed',
      hasBlockingFindings: false,
      correctionLabelPresent: true,
      gateCommentPresent: true,
    }),
    'reopen'
  );
  assert.equal(
    gate.decideStateChange({
      currentState: 'closed',
      hasBlockingFindings: false,
      correctionLabelPresent: false,
      gateCommentPresent: true,
    }),
    'keep'
  );
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
