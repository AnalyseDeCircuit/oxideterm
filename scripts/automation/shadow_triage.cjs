#!/usr/bin/env node
'use strict';

const fs = require('node:fs');
const path = require('node:path');

const { analyzeIssue, scanTextForCredentials } = require('./maintenance_policy.cjs');

function argumentValue(argumentsList, name) {
  const index = argumentsList.indexOf(name);
  if (index === -1 || !argumentsList[index + 1]) {
    throw new Error(`missing required argument: ${name}`);
  }
  return argumentsList[index + 1];
}

function writeFile(targetPath, contents) {
  fs.mkdirSync(path.dirname(targetPath), { recursive: true });
  fs.writeFileSync(targetPath, contents, 'utf8');
}

function markdownSummary(report) {
  const issueReference = report.issue.url
    ? `[#${report.issue.number}](${report.issue.url})`
    : `#${report.issue.number}`;
  const rows = [
    ['Issue', issueReference],
    ['Mode', 'Shadow — no repository writes'],
    ['Category', report.category],
    ['Platforms', report.platforms.join(', ') || 'not proven'],
    ['Route', report.route],
    ['Confidence', report.confidence],
    ['Reasons', report.reasons.join(', ') || 'none'],
    ['Suggested labels', report.recommendedLabels.join(', ') || 'none'],
  ];
  return [
    '## OxideTerm maintenance automation',
    '',
    '| Field | Result |',
    '| --- | --- |',
    ...rows.map(([key, value]) => `| ${key} | ${value} |`),
    '',
    '> This run is observational. It did not comment, label, close, modify, or open a pull request.',
    '',
  ].join('\n');
}

function main(argumentsList) {
  const inputPath = argumentValue(argumentsList, '--input');
  const outputPath = argumentValue(argumentsList, '--output');
  const summaryPath = argumentValue(argumentsList, '--summary');
  const issue = JSON.parse(fs.readFileSync(inputPath, 'utf8'));
  const report = analyzeIssue(issue);
  const serializedReport = `${JSON.stringify(report, null, 2)}\n`;
  const credentialFindings = scanTextForCredentials(serializedReport);
  if (credentialFindings.length > 0) {
    throw new Error(`refusing to write unsafe shadow report: ${credentialFindings.join(', ')}`);
  }

  writeFile(outputPath, serializedReport);
  fs.appendFileSync(summaryPath, markdownSummary(report), 'utf8');
}

if (require.main === module) {
  main(process.argv.slice(2));
}

module.exports = {
  main,
  markdownSummary,
};
