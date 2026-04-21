#!/usr/bin/env node
/* eslint-disable no-console */

const fs = require('fs');
const path = require('path');

const repoRoot = path.join(__dirname, '..');
const trzszRoot = path.join(repoRoot, 'src', 'lib', 'terminal', 'trzsz');

const baselineReadmePath = path.join(trzszRoot, 'README.md');
const diffReadmePath = path.join(trzszRoot, 'UPSTREAM_DIFF.md');
const commPath = path.join(trzszRoot, 'upstream', 'comm.ts');

function readText(filePath) {
  return fs.readFileSync(filePath, 'utf8');
}

function mustMatch(text, pattern, label) {
  const match = text.match(pattern);
  if (!match) {
    throw new Error(`Missing ${label}`);
  }
  return match[1];
}

function parseVendoredFiles(text) {
  return Array.from(text.matchAll(/^- `([^`]+)`:/gm), (match) => match[1]);
}

function main() {
  const baselineReadme = readText(baselineReadmePath);
  const diffReadme = readText(diffReadmePath);
  const comm = readText(commPath);

  const baselineVersion = mustMatch(
    baselineReadme,
    /- npm package: `trzsz@([^`]+)`/,
    'baseline trzsz version in README.md',
  );
  const baselineCommit = mustMatch(
    baselineReadme,
    /- release commit from npm registry `gitHead`: `([0-9a-f]{40})`/,
    'baseline trzsz commit in README.md',
  );
  const diffVersion = mustMatch(
    diffReadme,
    /- Upstream npm version: `([^`]+)`/,
    'upstream version in UPSTREAM_DIFF.md',
  );
  const diffCommit = mustMatch(
    diffReadme,
    /- Upstream commit: `([0-9a-f]{40})`/,
    'upstream commit in UPSTREAM_DIFF.md',
  );
  const commVersion = mustMatch(
    comm,
    /export const trzszVersion = '([^']+)'/,
    'trzszVersion export in upstream/comm.ts',
  );

  const errors = [];
  if (baselineVersion !== diffVersion) {
    errors.push(`README version mismatch: ${baselineVersion} !== ${diffVersion}`);
  }
  if (baselineVersion !== commVersion) {
    errors.push(`Vendored version mismatch: ${baselineVersion} !== ${commVersion}`);
  }
  if (baselineCommit !== diffCommit) {
    errors.push(`README commit mismatch: ${baselineCommit} !== ${diffCommit}`);
  }

  const vendoredFiles = parseVendoredFiles(diffReadme);
  if (vendoredFiles.length === 0) {
    errors.push('UPSTREAM_DIFF.md does not list any vendored files');
  }

  for (const fileName of vendoredFiles) {
    const filePath = path.join(trzszRoot, 'upstream', fileName);
    try {
      fs.lstatSync(filePath);
    } catch {
      errors.push(`Listed vendored file does not exist: ${fileName}`);
    }
  }

  if (errors.length > 0) {
    console.error('[trzsz-check] fork metadata check failed:');
    for (const error of errors) {
      console.error(`- ${error}`);
    }
    process.exit(1);
  }

  console.log(
    `[trzsz-check] OK version=${baselineVersion} commit=${baselineCommit} files=${vendoredFiles.length}`,
  );
}

main();