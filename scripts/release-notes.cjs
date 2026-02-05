#!/usr/bin/env node
/**
 * OxideTerm Release Notes Generator
 * 
 * Usage:
 *   node scripts/release-notes.cjs <version>
 *   pnpm release:notes 1.4.4
 * 
 * This script:
 *   1. Extracts release notes from docs/changelog/YYYY-MM.md
 *   2. Adds platform-specific installation instructions
 *   3. Outputs to RELEASE_NOTES.md (for GitHub release)
 */

const fs = require('fs');
const path = require('path');

const ROOT_DIR = path.resolve(__dirname, '..');
const CHANGELOG_DIR = path.join(ROOT_DIR, 'docs', 'changelog');
const OUTPUT_FILE = path.join(ROOT_DIR, 'RELEASE_NOTES.md');

// Platform-specific installation notes
const INSTALL_NOTES = {
  macos: `### 🍎 macOS 安装说明

> **重要**：从网络下载的 .dmg 文件会被 macOS Gatekeeper 隔离。

在终端中执行以下命令移除隔离属性：

\`\`\`bash
# 对于 .dmg 文件
xattr -cr ~/Downloads/OxideTerm_*.dmg

# 或者安装后对应用执行
xattr -cr /Applications/OxideTerm.app
\`\`\`

如果出现 "已损坏，无法打开" 错误，请确保执行上述命令。

---

### 🍎 macOS Installation

> **Important**: Downloaded .dmg files are quarantined by macOS Gatekeeper.

Run this command in Terminal to remove the quarantine attribute:

\`\`\`bash
# For .dmg files
xattr -cr ~/Downloads/OxideTerm_*.dmg

# Or for the installed app
xattr -cr /Applications/OxideTerm.app
\`\`\`

If you see "damaged and can't be opened" error, make sure to run the command above.`,

  windows: `### 🪟 Windows 安装说明

1. 下载 \`.msi\` 或 \`.exe\` 安装包
2. 如果 Windows Defender SmartScreen 弹出警告，点击 "更多信息" → "仍要运行"
3. 按照安装向导完成安装

---

### 🪟 Windows Installation

1. Download the \`.msi\` or \`.exe\` installer
2. If Windows Defender SmartScreen shows a warning, click "More info" → "Run anyway"
3. Follow the installation wizard`,

  linux: `### 🐧 Linux 安装说明

**AppImage (推荐)**：
\`\`\`bash
chmod +x OxideTerm_*.AppImage
./OxideTerm_*.AppImage
\`\`\`

**Debian/Ubuntu (.deb)**：
\`\`\`bash
sudo dpkg -i oxideterm_*.deb
sudo apt-get install -f  # 安装依赖
\`\`\`

---

### 🐧 Linux Installation

**AppImage (Recommended)**:
\`\`\`bash
chmod +x OxideTerm_*.AppImage
./OxideTerm_*.AppImage
\`\`\`

**Debian/Ubuntu (.deb)**:
\`\`\`bash
sudo dpkg -i oxideterm_*.deb
sudo apt-get install -f  # Install dependencies
\`\`\``
};

function findChangelogEntry(version) {
  // Try to find changelog in current month's file, then previous months
  const now = new Date();
  const searchMonths = [];
  
  for (let i = 0; i < 6; i++) {
    const d = new Date(now.getFullYear(), now.getMonth() - i, 1);
    const filename = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}.md`;
    searchMonths.push(filename);
  }

  for (const filename of searchMonths) {
    const filePath = path.join(CHANGELOG_DIR, filename);
    if (!fs.existsSync(filePath)) continue;

    const content = fs.readFileSync(filePath, 'utf8');
    
    // Look for version header: ## YYYY-MM-DD: ... (vX.Y.Z)
    const versionRegex = new RegExp(
      `^## \\d{4}-\\d{2}-\\d{2}:[^\\n]*\\(v${version.replace(/\./g, '\\.')}\\)`,
      'm'
    );
    
    const match = content.match(versionRegex);
    if (match) {
      const startIndex = match.index;
      
      // Find the next ## header or end of file
      const restContent = content.slice(startIndex);
      const nextHeaderMatch = restContent.match(/\n## \d{4}-\d{2}-\d{2}:/);
      
      let endIndex;
      if (nextHeaderMatch) {
        endIndex = startIndex + nextHeaderMatch.index;
      } else {
        endIndex = content.length;
      }
      
      const entry = content.slice(startIndex, endIndex).trim();
      return { entry, file: filename };
    }
  }

  return null;
}

function generateReleaseNotes(version, changelogEntry) {
  const notes = [];
  
  // Header
  notes.push(`# OxideTerm v${version} Release Notes\n`);
  
  // Changelog content
  if (changelogEntry) {
    notes.push('## 📋 What\'s Changed\n');
    // Remove the version header from changelog entry (we already have our own)
    const entryWithoutHeader = changelogEntry.entry.replace(/^## [^\n]+\n/, '');
    notes.push(entryWithoutHeader);
    notes.push('\n---\n');
  } else {
    notes.push('> ⚠️ No changelog entry found for this version.\n');
    notes.push('> Please update `docs/changelog/YYYY-MM.md` with release notes.\n');
    notes.push('\n---\n');
  }

  // Downloads section
  notes.push('## 📦 Downloads\n');
  notes.push('| Platform | File | Notes |');
  notes.push('|----------|------|-------|');
  notes.push('| macOS (Universal) | `OxideTerm_x.y.z_universal.dmg` | Requires `xattr -cr` |');
  notes.push('| macOS (Intel) | `OxideTerm_x.y.z_x64.dmg` | Requires `xattr -cr` |');
  notes.push('| macOS (Apple Silicon) | `OxideTerm_x.y.z_aarch64.dmg` | Requires `xattr -cr` |');
  notes.push('| Windows (64-bit) | `OxideTerm_x.y.z_x64-setup.exe` | Installer |');
  notes.push('| Windows (64-bit) | `OxideTerm_x.y.z_x64_en-US.msi` | MSI package |');
  notes.push('| Linux (AppImage) | `OxideTerm_x.y.z_amd64.AppImage` | Portable |');
  notes.push('| Linux (Debian) | `oxideterm_x.y.z_amd64.deb` | Debian/Ubuntu |');
  notes.push('\n---\n');

  // Installation instructions
  notes.push('## 🔧 Installation Instructions\n');
  notes.push(INSTALL_NOTES.macos);
  notes.push('\n---\n');
  notes.push(INSTALL_NOTES.windows);
  notes.push('\n---\n');
  notes.push(INSTALL_NOTES.linux);
  notes.push('\n---\n');

  // Footer
  notes.push('## 🔗 Links\n');
  notes.push('- [Documentation](https://github.com/AnalyseDeCircuit/OxideTerm/tree/main/docs)');
  notes.push('- [Report Issues](https://github.com/AnalyseDeCircuit/OxideTerm/issues)');
  notes.push('- [Full Changelog](https://github.com/AnalyseDeCircuit/OxideTerm/tree/main/docs/changelog)');

  return notes.join('\n');
}

function getCurrentVersion() {
  const pkg = JSON.parse(fs.readFileSync(path.join(ROOT_DIR, 'package.json'), 'utf8'));
  return pkg.version;
}

function main() {
  const args = process.argv.slice(2);
  
  if (args.length === 0 || args[0] === '--help' || args[0] === '-h') {
    console.log(`
OxideTerm Release Notes Generator

Usage:
  node scripts/release-notes.cjs <version>
  pnpm release:notes <version>

Examples:
  pnpm release:notes 1.4.4
  pnpm release:notes           # Uses current version from package.json

Options:
  --stdout     Output to stdout instead of file
  --help, -h   Show this help message

Current version: ${getCurrentVersion()}
`);
    process.exit(0);
  }

  const toStdout = args.includes('--stdout');
  const version = args.find(a => !a.startsWith('--')) || getCurrentVersion();

  console.log(`\n📝 Generating release notes for v${version}...\n`);

  // Find changelog entry
  const changelogEntry = findChangelogEntry(version);
  
  if (changelogEntry) {
    console.log(`✅ Found changelog entry in ${changelogEntry.file}`);
  } else {
    console.log(`⚠️  No changelog entry found for v${version}`);
    console.log(`   Expected format in docs/changelog/YYYY-MM.md:`);
    console.log(`   ## YYYY-MM-DD: Title (v${version})`);
  }

  // Generate release notes
  const releaseNotes = generateReleaseNotes(version, changelogEntry);

  if (toStdout) {
    console.log('\n' + '='.repeat(60) + '\n');
    console.log(releaseNotes);
  } else {
    fs.writeFileSync(OUTPUT_FILE, releaseNotes);
    console.log(`\n✨ Release notes written to RELEASE_NOTES.md`);
    console.log(`\n📋 Next steps:`);
    console.log(`   1. Review RELEASE_NOTES.md`);
    console.log(`   2. Create GitHub release with this content`);
    console.log(`   3. Upload build artifacts`);
  }
}

main();
