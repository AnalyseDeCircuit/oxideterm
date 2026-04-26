#!/usr/bin/env node
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

const args = new Set(process.argv.slice(2));
const checkOnly = args.has('--check');

const repoRoot = path.resolve(__dirname, '..');
const pluginDocsRoot = path.join(repoRoot, 'plugin-development');
const sourceZhPath = path.join(pluginDocsRoot, 'README.zh-CN.md');
const sourceEnPath = path.join(pluginDocsRoot, 'README.md');
const sourceApiPath = path.join(pluginDocsRoot, 'plugin-api.d.ts');
const rootGuidePath = path.join(repoRoot, 'PLUGIN_DEVELOPMENT.md');
const rootApiPath = path.join(repoRoot, 'plugin-api.d.ts');
const webRoot = process.env.OXIDETERM_WEB_REPO || path.join(repoRoot, 'oxideterm-web');
const webZhPath = path.join(webRoot, 'src/content/docs/zh-hans/docs/plugin-development.mdx');
const webEnPath = path.join(webRoot, 'src/content/docs/docs/plugin-development.mdx');

function read(file) {
  return fs.readFileSync(file, 'utf8');
}

function sha256(text) {
  return crypto.createHash('sha256').update(text).digest('hex');
}

function normalizeNewline(text) {
  return text.endsWith('\n') ? text : `${text}\n`;
}

function toZhMdx(markdown) {
  const body = normalizeNewline(markdown).replace(/^# OxideTerm Plugin Development Guide\s*\n+/, '');
  const hash = sha256(normalizeNewline(markdown));
  return `---\ntitle: 插件开发指南\ndescription: OxideTerm 插件开发完全参考 — 适用于 Plugin API v3\n---\n\n{/* AUTO-GENERATED from OxideTerm/plugin-development/README.zh-CN.md. Do not edit manually. */}\n{/* source-sha256: ${hash} */}\n\n${body}`;
}

function toEnMdx(markdown) {
  const body = normalizeNewline(markdown)
    .replace(/^<!-- translated-from:[^\n]*-->\n?/m, '')
    .replace(/^<!-- translated-from-sha256:[^\n]*-->\n?/m, '')
    .replace(/^# OxideTerm Plugin Development Guide\s*\n+/, '');
  const hash = sha256(normalizeNewline(markdown));
  return `---\ntitle: Plugin Development Guide\ndescription: Complete reference for OxideTerm plugin development — Plugin API v3\n---\n\n{/* AUTO-GENERATED from OxideTerm/plugin-development/README.md. Do not edit manually. */}\n{/* source-sha256: ${hash} */}\n\n${body}`;
}

function generatedRootGuide(markdown) {
  const source = normalizeNewline(markdown);
  const hash = sha256(source);
  return `<!-- AUTO-GENERATED from plugin-development/README.zh-CN.md. Do not edit manually. -->\n<!-- source-sha256: ${hash} -->\n\n${source}`;
}

function generatedRootApi(apiText) {
  const source = normalizeNewline(apiText);
  const hash = sha256(source);
  return source.replace(
    /^\/\*\*/m,
    `/**\n * AUTO-GENERATED from plugin-development/plugin-api.d.ts. Do not edit manually.\n * source-sha256: ${hash}\n *`
  );
}

function checkTranslationMarker(file, sourceHash) {
  const content = read(file);
  const match = content.match(/translated-from-sha256:\s*([a-f0-9]{64})/);
  if (match?.[1] === sourceHash) {
    return;
  }

  const message =
    `${path.relative(repoRoot, file)} is not marked as translated from the current ` +
    `plugin-development/README.zh-CN.md (${sourceHash})`;
  if (checkOnly) {
    throw new Error(message);
  }
  console.warn(`[sync-plugin-development-docs] ${message}`);
}

function writeOrCheck(file, expected, options = {}) {
  const normalized = normalizeNewline(expected);
  const exists = fs.existsSync(file);
  const current = exists ? read(file) : '';

  if (
    current === normalized ||
    (checkOnly && options.allowCurrentMatchesSource && current === options.allowCurrentMatchesSource)
  ) {
    return { file, changed: false };
  }

  if (checkOnly) {
    throw new Error(`${path.relative(repoRoot, file)} is out of sync`);
  }

  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, normalized);
  return { file, changed: true };
}

function main() {
  const sourceZh = normalizeNewline(read(sourceZhPath));
  const sourceEn = normalizeNewline(read(sourceEnPath));
  const sourceApi = normalizeNewline(read(sourceApiPath));
  checkTranslationMarker(sourceEnPath, sha256(sourceZh));
  const results = [
    writeOrCheck(rootGuidePath, generatedRootGuide(sourceZh), {
      // First run compatibility: accept the historical root copy and normalize it on --sync.
      allowCurrentMatchesSource: sourceZh,
    }),
    writeOrCheck(rootApiPath, generatedRootApi(sourceApi), {
      allowCurrentMatchesSource: sourceApi,
    }),
  ];

  if (fs.existsSync(webRoot)) {
    results.push(writeOrCheck(webZhPath, toZhMdx(sourceZh)));
    results.push(writeOrCheck(webEnPath, toEnMdx(sourceEn)));
  }

  for (const result of results) {
    const rel = path.relative(repoRoot, result.file);
    console.log(`${result.changed ? 'updated' : 'ok'} ${rel}`);
  }
}

try {
  main();
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
