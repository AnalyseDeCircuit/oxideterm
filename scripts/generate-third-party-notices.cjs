/* eslint-disable no-console */

const fs = require('fs');
const cp = require('child_process');

function exec(cmd) {
  return cp.execSync(cmd, {
    encoding: 'utf8',
    maxBuffer: 1024 * 1024 * 100,
    stdio: ['ignore', 'pipe', 'pipe']
  });
}

function escPipe(value) {
  return String(value ?? '').replace(/\|/g, '\\|').replace(/\r?\n/g, ' ');
}

function tryRead(path) {
  try {
    return fs.readFileSync(path, 'utf8');
  } catch {
    return null;
  }
}

function firstNonEmptyLines(text, maxLines) {
  const lines = String(text || '').split(/\r?\n/);
  const out = [];
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    out.push(trimmed);
    if (out.length >= maxLines) break;
  }
  return out;
}

function buildNotices() {
  const generatedAt = new Date().toISOString();

  const raw = exec('pnpm licenses list --json --prod');
  const data = JSON.parse(raw);

  const packages = [];
  for (const [license, items] of Object.entries(data)) {
    for (const item of items) {
      packages.push({
        name: item.name,
        versions: (item.versions || []).join(', '),
        license,
        homepage: item.homepage || ''
      });
    }
  }

  packages.sort((a, b) => a.name.localeCompare(b.name));

  const licenseCounts = new Map();
  for (const p of packages) {
    licenseCounts.set(p.license, (licenseCounts.get(p.license) || 0) + 1);
  }

  const summaryLines = [...licenseCounts.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([lic, count]) => `- ${lic}: ${count}`);

  const fonts = [
    {
      name: 'JetBrains Mono (Subset)',
      license: 'SIL Open Font License 1.1',
      licenseFile: 'public/fonts/JetBrainsMono/OFL.txt',
      displayPath: '../public/fonts/JetBrainsMono/OFL.txt',
      note: 'Modified Version (Subset) — character set reduced for CJK terminal use per OFL §1'
    },
    {
      name: 'Meslo (Subset)',
      license: 'Apache License 2.0',
      licenseFile: 'public/fonts/Meslo/LICENSE.txt',
      displayPath: '../public/fonts/Meslo/LICENSE.txt',
      note: 'Modified Version (Subset) — character set reduced for CJK terminal use per Apache 2.0'
    },
    {
      name: 'Maple Mono (Subset)',
      license: 'SIL Open Font License 1.1',
      licenseFile: 'public/fonts/MapleMono/LICENSE.txt',
      displayPath: '../public/fonts/MapleMono/LICENSE.txt',
      note: 'Modified Version (Subset) — character set reduced for CJK terminal use per OFL §1'
    }
  ];

  let out = '';
  out += '# Third-Party Notices (Frontend)\n\n';
  out += 'This file lists third-party components used by the frontend (including transitive production dependencies) and their declared licenses.\n';
  out += `Generated: ${generatedAt}\n\n`;

  out += '## Summary\n';
  out += `${summaryLines.join('\n')}\n\n`;

  out += '## NPM Production Dependencies\n\n';
  out += '| Package | Version(s) | License | Homepage |\n';
  out += '|---|---:|---|---|\n';
  for (const p of packages) {
    out += `| ${escPipe(p.name)} | ${escPipe(p.versions)} | ${escPipe(p.license)} | ${p.homepage ? escPipe(p.homepage) : ''} |\n`;
  }
  out += '\n';

  out += '## Bundled Fonts / Assets\n\n';
  for (const f of fonts) {
    out += `- ${f.name} — ${f.license} (see ${f.displayPath || f.licenseFile})\n`;

    if (f.note) {
      out += `  - **${f.note}**\n`;
    }
    const licText = tryRead(f.licenseFile);
    if (licText) {
      const excerpt = firstNonEmptyLines(licText, 2);
      if (excerpt.length) {
        out += `  - Excerpt: ${excerpt.join(' / ')}\n`;
      }
    }
  }
  out += '\n';

  out += '## Notes\n\n';
  out += '- **Multi-license policy**: Where a dependency offers multiple licenses (e.g. `MIT OR Apache-2.0`), OxideTerm always exercises the most permissive option available.\n';
  out += '- Licenses are taken from package metadata reported by pnpm at generation time.\n';
  out += '- This list is intended for notice/compliance purposes and does not replace the full license texts included by upstream projects.\n';
  out += 'Licensing Strategy for OxideTerm\nOxideTerm is licensed under the PolyForm Noncommercial License 1.0.0. To ensure full compatibility and respect the terms of all upstream dependencies, OxideTerm strictly adheres to the following policy: Whenever a third-party dependency offers multiple licensing options (e.g., dual-licensing under MIT and Apache-2.0), OxideTerm elects to exercise the most permissive license available (typically MIT or ISC). > This choice is made to maintain the integrity of our non-commercial licensing model while fulfilling all attribution requirements of the open-source community.\n';

  return { out, count: packages.length };
}

function main() {
  const { out, count } = buildNotices();
  const outputPath = 'src/THIRD_PARTY_NOTICES.md';
  fs.writeFileSync(outputPath, out);
  console.log(`Wrote ${outputPath} with ${count} production dependency entries.`);
}

main();
