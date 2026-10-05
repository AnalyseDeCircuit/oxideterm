'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const gate = require('../issue_quality_policy.cjs');

const featureChecklist = `### Checklist

- [x] I am using the latest release, confirmed this feature does not already exist, and searched existing issues / 我正在使用最新正式版，已确认该功能尚不存在，并已搜索过已有 Issue
- [x] This is one focused request within OxideTerm's scope / 这是一个聚焦且属于 OxideTerm 范围内的请求
- [x] I have described a concrete problem/use case and proposed solution; I understand feature requests are handled best-effort, and vague or inactive issues may be closed.
我已描述具体问题/使用场景和期望方案；我理解功能请求会尽力处理，描述模糊或长期无回复的 Issue 可能会被关闭。
`;

const bugChecklist = `### Checklist

- [x] I tested with the latest release, can still reproduce the issue, and searched existing issues / 我已使用最新正式版测试，问题仍然存在，并已搜索过已有 Issue
- [x] This is one reproducible bug, not a usage question or feature request / 这是一个可复现的 bug，而不是使用问题或功能建议
- [x] I provided the OxideTerm version, platform, and steps to reproduce; I understand vague, incomplete, or inactive issues may be closed.
我已提供 OxideTerm 版本、平台及复现步骤；我理解描述模糊、信息不足或长期无回复的 Issue 可能会被关闭。
- [x] I removed passwords, private keys, and other secrets from this report.
我已从本报告中删除密码、私钥及其他敏感信息。
`;

function featureBody({ version = '2.0.5', problem = '会话录制文件无法被转换工具读取。', proposal = '让录制文件保持标准格式兼容。', importance = '团队每周导出多次录制用于审计。' } = {}) {
  return `### OxideTerm version / 版本

${version}

### Problem or use case / 问题或使用场景

${problem}

### Proposed solution / 期望方案

${proposal}

### Why is this important? / 为什么这个功能对你重要？

${importance}

${featureChecklist}
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

${bugChecklist}
`;
}

test('requires each template acknowledgement even without issue labels', () => {
  const missing = featureBody().split('### Checklist')[0];
  const requiredItem = "This is one focused request within OxideTerm's scope / 这是一个聚焦且属于 OxideTerm 范围内的请求";
  for (const [name, body, expected] of [
    ['missing checklist', missing, [{ code: 'required_section_missing', heading: 'Checklist' }]],
    ['unchecked item', featureBody().replace(`- [x] ${requiredItem}`, `- [ ] ${requiredItem}`), [{ code: 'required_checkbox_unchecked', heading: 'Checklist', item: requiredItem }]],
    ['missing item', featureBody().replace(`- [x] ${requiredItem}\n`, ''), [{ code: 'required_checkbox_unchecked', heading: 'Checklist', item: requiredItem }]],
    ['unrelated checkmark', featureBody().replace(`- [x] ${requiredItem}`, '- [x] Confirmed'), [{ code: 'required_checkbox_unchecked', heading: 'Checklist', item: requiredItem }]],
    ['quoted checkbox', featureBody().replace(`- [x] ${requiredItem}`, `> - [x] ${requiredItem}`), [{ code: 'required_checkbox_unchecked', heading: 'Checklist', item: requiredItem }]],
    ['code example', featureBody().replace(`- [x] ${requiredItem}`, `\`\`\`markdown\n- [x] ${requiredItem}\n\`\`\``), [{ code: 'required_checkbox_unchecked', heading: 'Checklist', item: requiredItem }]],
    ['optional contribution unchecked', `${featureBody()}\n### Contribution / 参与贡献\n\n- [ ] I am willing to submit a pull request to help resolve this issue / 我愿意提交 PR 协助解决此问题\n`, []],
    ['uppercase checkmarks and CRLF', featureBody().replaceAll('[x]', '[X]').replaceAll('\n', '\r\n'), []],
  ]) {
    const report = gate.evaluateIssue({ title: '[Feature] SFTP follows terminal directory', body, labels: [] });
    assert.deepEqual(report.blockingFindings, expected, name);
  }
  assert.deepEqual(gate.evaluateIssue({title: 'SFTP follows terminal directory', body: missing, labels: []}).blockingFindings,
    [{ code: 'required_section_missing', heading: 'Checklist' }]);
});

test('reads a version with platform annotations without accepting a partial version', () => {
  for (const [input, expected] of [
    ['2.2.0（macOS）', '2.2.0'], ['v2.2.0 (Windows)', '2.2.0'],
    ['`2.2.0`', '2.2.0'], ['2.2.1-beta.1（arm64）', '2.2.1-beta.1'],
    ['2.2', null], ['2.2.0.1', null],
  ]) assert.equal(gate.readSubmittedVersion(featureBody({version: input})), expected, input);
  assert.deepEqual(gate.evaluateIssue({title: '[Feature] Add recording export', body: featureBody({version: '99.0.0（macOS）'}), labels: [], releasedVersions: ['2.2.0']}).blockingFindings,
    [{code: 'release_version_unverified', version: '99.0.0'}]);
});

test('checks compatibility and plugin API acknowledgements using their own templates', () => {
  for (const { title, labels, body, heading, item } of [
    {
      title: '[Compatibility] Cannot authenticate with test SSH server',
      labels: ['compatibility'],
      body: `### OxideTerm version / 版本
2.2.0
### Client platform / 客户端平台
Windows (x86_64)
### Authentication method / 认证方式
Password
### SSH server details / 服务端信息
OpenSSH 9.6 on Ubuntu 24.04
### Error message or behavior / 错误信息或现象
Authentication is rejected.
### Working client comparison / 可正常连接的客户端对比
OpenSSH connects successfully using the same credentials.
### Checklist
- [x] I tested with the latest release, the issue persists, and another SSH client can connect with the same credentials and server / 我已使用最新正式版测试，问题仍然存在，且其他 SSH 客户端使用相同凭据和服务端可以正常连接
- [x] I searched existing issues and provided server details, error messages, and a working-client comparison / 我已搜索过已有 Issue，并提供了服务端信息、错误信息和可正常连接的客户端对比
- [x] I understand vague, incomplete, or inactive issues may be closed.
我理解描述模糊、信息不足或长期无回复的 Issue 可能会被关闭。`,
      heading: 'Checklist',
      item: 'I searched existing issues and provided server details, error messages, and a working-client comparison / 我已搜索过已有 Issue，并提供了服务端信息、错误信息和可正常连接的客户端对比',
    },
    {
      title: '[Plugin API] Read selected terminal text',
      labels: ['plugin-api', 'enhancement'],
      body: `### What are you trying to build? / 你想构建什么功能？
A text formatting plugin for selected terminal output.
### What API do you need? / 你需要什么接口？
A read-only API returning the current terminal selection.
### Before submitting / 提交前确认
- [x] I searched existing issues for this API request / 我已搜索过已有 Issue，确认此接口尚未被请求
- [x] This is a plugin host API request, not a general feature request / 这是一个插件接口请求，不是通用功能建议`,
      heading: 'Before submitting / 提交前确认',
      item: 'I searched existing issues for this API request / 我已搜索过已有 Issue，确认此接口尚未被请求',
    },
  ]) {
    for (const issueLabels of [labels, []]) {
      assert.deepEqual(gate.evaluateIssue({title, body, labels: issueLabels}), {blockingFindings: [], reviewFindings: []});
      assert.deepEqual(gate.evaluateIssue({title, body: body.replace(`- [x] ${item}`, `- [ ] ${item}`), labels: issueLabels}).blockingFindings,
        [{code: 'required_checkbox_unchecked', heading, item}]);
    }
  }
});

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
