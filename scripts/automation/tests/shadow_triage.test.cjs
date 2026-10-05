'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const { main } = require('../shadow_triage.cjs');

function sampleIssue() {
  return {
    number: 401,
    html_url: 'https://github.com/AnalyseDeCircuit/oxideterm/issues/401',
    title: '面包屑无法横向滚动',
    state: 'open',
    labels: ['bug'],
    body: `### OxideTerm version / 版本

2.0.11

### Platform / 平台

macOS

### Summary / 简述

很长的目录路径超出宽度后无法滚动。

### Steps to reproduce / 复现步骤

打开文件管理器并进入超过面板宽度的深层目录。

### Expected vs actual / 预期与实际

预期能够横向滚动，实际无法查看后面的目录。

### Checklist

- [x] I tested with the latest release, can still reproduce the issue, and searched existing issues / 我已使用最新正式版测试，问题仍然存在，并已搜索过已有 Issue
- [x] This is one reproducible bug, not a usage question or feature request / 这是一个可复现的 bug，而不是使用问题或功能建议
- [x] I provided the OxideTerm version, platform, and steps to reproduce; I understand vague, incomplete, or inactive issues may be closed.
我已提供 OxideTerm 版本、平台及复现步骤；我理解描述模糊、信息不足或长期无回复的 Issue 可能会被关闭。
- [x] I removed passwords, private keys, and other secrets from this report.
我已从本报告中删除密码、私钥及其他敏感信息。
`,
  };
}

test('writes a sanitized report and an explicit no-write summary', () => {
  const temporaryDirectory = fs.mkdtempSync(path.join(os.tmpdir(), 'oxideterm-shadow-'));
  const inputPath = path.join(temporaryDirectory, 'issue.json');
  const outputPath = path.join(temporaryDirectory, 'report.json');
  const summaryPath = path.join(temporaryDirectory, 'summary.md');
  fs.writeFileSync(inputPath, JSON.stringify(sampleIssue()), 'utf8');
  fs.writeFileSync(summaryPath, '', 'utf8');

  main([
    '--input', inputPath,
    '--output', outputPath,
    '--summary', summaryPath,
  ]);

  const report = JSON.parse(fs.readFileSync(outputPath, 'utf8'));
  const summary = fs.readFileSync(summaryPath, 'utf8');
  assert.equal(report.route, 'candidate_for_agent');
  assert.equal(Object.hasOwn(report.issue, 'body'), false);
  assert.equal(summary.includes('Shadow — no repository writes'), true);
  assert.equal(summary.includes('did not comment, label, close, modify, or open'), true);
});
