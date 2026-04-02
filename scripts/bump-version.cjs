#!/usr/bin/env node
/**
 * OxideTerm Version Bump Script
 * 
 * Usage:
 *   node scripts/bump-version.cjs <version>
 *   pnpm version:bump 1.4.4
 * 
 * This script updates version numbers in:
 *   - package.json
 *   - src-tauri/Cargo.toml
 *   - cli/Cargo.toml
 *   - src-tauri/Cargo.lock
 *   - cli/Cargo.lock
 *   - src-tauri/tauri.conf.json
 *   - README.md and docs/readme/README.*.md (optional badges)
 *   - website/index.html (hero badge, figcaption, footer)
 */

const fs = require('fs');
const path = require('path');

const ROOT_DIR = path.resolve(__dirname, '..');

// Files to update
const FILES = {
  packageJson: path.join(ROOT_DIR, 'package.json'),
  cargoToml: path.join(ROOT_DIR, 'src-tauri', 'Cargo.toml'),
  cliCargoToml: path.join(ROOT_DIR, 'cli', 'Cargo.toml'),
  tauriConf: path.join(ROOT_DIR, 'src-tauri', 'tauri.conf.json'),
  readme: path.join(ROOT_DIR, 'README.md'),
  readmeZhHans: path.join(ROOT_DIR, 'docs', 'readme', 'README.zh-Hans.md'),
  readmeZhHant: path.join(ROOT_DIR, 'docs', 'readme', 'README.zh-Hant.md'),
  readmeJa: path.join(ROOT_DIR, 'docs', 'readme', 'README.ja.md'),
  readmeKo: path.join(ROOT_DIR, 'docs', 'readme', 'README.ko.md'),
  readmeFr: path.join(ROOT_DIR, 'docs', 'readme', 'README.fr.md'),
  readmeDe: path.join(ROOT_DIR, 'docs', 'readme', 'README.de.md'),
  readmeEs: path.join(ROOT_DIR, 'docs', 'readme', 'README.es.md'),
  readmeIt: path.join(ROOT_DIR, 'docs', 'readme', 'README.it.md'),
  readmePtBr: path.join(ROOT_DIR, 'docs', 'readme', 'README.pt-BR.md'),
  readmeVi: path.join(ROOT_DIR, 'docs', 'readme', 'README.vi.md'),
  websiteIndex: path.join(ROOT_DIR, 'website', 'index.html'),
};

function validateVersion(version) {
  const semverRegex = /^\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?$/;
  if (!semverRegex.test(version)) {
    console.error(`❌ Invalid version format: ${version}`);
    console.error('   Expected format: X.Y.Z or X.Y.Z-beta.1');
    process.exit(1);
  }
  return version;
}

function getCurrentVersion() {
  const pkg = JSON.parse(fs.readFileSync(FILES.packageJson, 'utf8'));
  return pkg.version;
}

function updatePackageJson(version) {
  const content = JSON.parse(fs.readFileSync(FILES.packageJson, 'utf8'));
  const oldVersion = content.version;
  content.version = version;
  fs.writeFileSync(FILES.packageJson, JSON.stringify(content, null, 2) + '\n');
  return oldVersion;
}

function updateCargoToml(version) {
  let content = fs.readFileSync(FILES.cargoToml, 'utf8');
  // Only update the first version line (package version, not dependencies)
  const lines = content.split('\n');
  let found = false;
  for (let i = 0; i < lines.length; i++) {
    if (!found && lines[i].match(/^version\s*=\s*"/)) {
      lines[i] = `version = "${version}"`;
      found = true;
      break;
    }
  }
  fs.writeFileSync(FILES.cargoToml, lines.join('\n'));
  return found;
}

function updateCliCargoToml(version) {
  if (!fs.existsSync(FILES.cliCargoToml)) {
    return false;
  }
  let content = fs.readFileSync(FILES.cliCargoToml, 'utf8');
  const lines = content.split('\n');
  let found = false;
  for (let i = 0; i < lines.length; i++) {
    if (!found && lines[i].match(/^version\s*=\s*"/)) {
      lines[i] = `version = "${version}"`;
      found = true;
      break;
    }
  }
  fs.writeFileSync(FILES.cliCargoToml, lines.join('\n'));
  return found;
}

function updateCargoLockVersion(lockfilePath, packageName, version) {
  if (!fs.existsSync(lockfilePath)) {
    return false;
  }

  const content = fs.readFileSync(lockfilePath, 'utf8');
  const updatedContent = content.replace(
    new RegExp(`(\\[\\[package\\]\\]\\nname = "${packageName}"\\nversion = ")[^"]+(\")`, 'm'),
    `$1${version}$2`,
  );

  if (updatedContent === content) {
    return false;
  }

  fs.writeFileSync(lockfilePath, updatedContent);
  return true;
}

function updateTauriConf(version) {
  const content = JSON.parse(fs.readFileSync(FILES.tauriConf, 'utf8'));
  content.version = version;
  fs.writeFileSync(FILES.tauriConf, JSON.stringify(content, null, 2) + '\n');
  return true;
}

function updateReadmeBadges(version, filePath) {
  if (!fs.existsSync(filePath)) {
    return false;
  }
  
  let content = fs.readFileSync(filePath, 'utf8');
  // Update version badge: img src="https://img.shields.io/badge/version-X.Y.Z-blue"
  const badgeRegex = /(img\.shields\.io\/badge\/version-)[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?(-blue)/g;
  const newContent = content.replace(badgeRegex, `$1${version}$3`);
  
  if (newContent !== content) {
    fs.writeFileSync(filePath, newContent);
    return true;
  }
  return false;
}

function updateWebsiteVersion(version, currentVersion) {
  if (!fs.existsSync(FILES.websiteIndex)) {
    return false;
  }
  let content = fs.readFileSync(FILES.websiteIndex, 'utf8');
  // Replace version strings like "v0.20.1" with the new version
  const versionRegex = new RegExp(`v${currentVersion.replace(/\./g, '\\.')}`, 'g');
  const newContent = content.replace(versionRegex, `v${version}`);
  if (newContent !== content) {
    fs.writeFileSync(FILES.websiteIndex, newContent);
    return true;
  }
  return false;
}

function main() {
  const args = process.argv.slice(2);
  
  if (args.length === 0 || args[0] === '--help' || args[0] === '-h') {
    console.log(`
OxideTerm Version Bump Script

Usage:
  node scripts/bump-version.cjs <version>
  pnpm version:bump <version>

Examples:
  pnpm version:bump 1.4.4
  pnpm version:bump 1.5.0-beta.1

Current version: ${getCurrentVersion()}

Options:
  --dry-run    Show what would be changed without making changes
  --help, -h   Show this help message
`);
    process.exit(0);
  }

  const dryRun = args.includes('--dry-run');
  const version = validateVersion(args.find(a => !a.startsWith('--')) || '');
  const currentVersion = getCurrentVersion();

  console.log(`\n🔄 OxideTerm Version Bump`);
  console.log(`   ${currentVersion} → ${version}\n`);
  
  if (dryRun) {
    console.log('🔍 Dry run mode - no changes will be made\n');
  }

  const updates = [];

  // Update package.json
  if (!dryRun) {
    updatePackageJson(version);
  }
  updates.push({ file: 'package.json', status: '✅' });

  // Update Cargo.toml
  if (!dryRun) {
    updateCargoToml(version);
  }
  updates.push({ file: 'src-tauri/Cargo.toml', status: '✅' });

  // Update Cargo.lock for src-tauri
  if (!dryRun) {
    const updated = updateCargoLockVersion(path.join(ROOT_DIR, 'src-tauri', 'Cargo.lock'), 'oxideterm', version);
    updates.push({ file: 'src-tauri/Cargo.lock', status: updated ? '✅' : '⏭️ (not found)' });
  } else {
    updates.push({ file: 'src-tauri/Cargo.lock', status: '🔍' });
  }

  // Update CLI Cargo.toml
  if (!dryRun) {
    const updated = updateCliCargoToml(version);
    updates.push({ file: 'cli/Cargo.toml', status: updated ? '✅' : '⏭️ (not found)' });
  } else {
    updates.push({ file: 'cli/Cargo.toml', status: '🔍' });
  }

  // Update Cargo.lock for cli
  if (!dryRun) {
    const updated = updateCargoLockVersion(path.join(ROOT_DIR, 'cli', 'Cargo.lock'), 'oxide-cli', version);
    updates.push({ file: 'cli/Cargo.lock', status: updated ? '✅' : '⏭️ (not found)' });
  } else {
    updates.push({ file: 'cli/Cargo.lock', status: '🔍' });
  }

  // Update tauri.conf.json
  if (!dryRun) {
    updateTauriConf(version);
  }
  updates.push({ file: 'src-tauri/tauri.conf.json', status: '✅' });

  // Update website/index.html
  if (!dryRun) {
    const updated = updateWebsiteVersion(version, currentVersion);
    updates.push({ file: 'website/index.html', status: updated ? '✅' : '⏭️ (not found)' });
  } else {
    updates.push({ file: 'website/index.html', status: '🔍' });
  }

  // Update README badges
  const readmeFiles = [
    { path: FILES.readme, name: 'README.md' },
    { path: FILES.readmeZhHans, name: 'docs/readme/README.zh-Hans.md' },
    { path: FILES.readmeZhHant, name: 'docs/readme/README.zh-Hant.md' },
    { path: FILES.readmeJa, name: 'docs/readme/README.ja.md' },
    { path: FILES.readmeKo, name: 'docs/readme/README.ko.md' },
    { path: FILES.readmeFr, name: 'docs/readme/README.fr.md' },
    { path: FILES.readmeDe, name: 'docs/readme/README.de.md' },
    { path: FILES.readmeEs, name: 'docs/readme/README.es.md' },
    { path: FILES.readmeIt, name: 'docs/readme/README.it.md' },
    { path: FILES.readmePtBr, name: 'docs/readme/README.pt-BR.md' },
    { path: FILES.readmeVi, name: 'docs/readme/README.vi.md' },
  ];

  for (const readme of readmeFiles) {
    if (!dryRun) {
      const updated = updateReadmeBadges(version, readme.path);
      updates.push({ file: readme.name, status: updated ? '✅' : '⏭️ (no badge found)' });
    } else {
      updates.push({ file: readme.name, status: '🔍' });
    }
  }

  // Print summary
  console.log('📋 Updated files:');
  for (const u of updates) {
    console.log(`   ${u.status} ${u.file}`);
  }

  if (!dryRun) {
    console.log(`\n✨ Version bumped to ${version}`);
    console.log(`\n📝 Next steps:`);
    console.log(`   1. Update docs/changelog/$(date +%Y-%m).md with release notes`);
    console.log(`   2. Run: pnpm release:notes ${version}`);
    console.log(`   3. Commit: git add -A && git commit -m "chore: bump version to ${version}"`);
    console.log(`   4. Tag: git tag v${version}`);
    console.log(`   5. Push: git push && git push --tags`);
  }
}

main();
