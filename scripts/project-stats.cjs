#!/usr/bin/env node
/* eslint-disable no-console */
const fs = require("fs");
const path = require("path");

const ROOT = process.cwd();
const args = new Set(process.argv.slice(2));
const asJson = args.has("--json");
const includeHidden = args.has("--include-hidden");
const byDir = args.has("--by-dir");

const EXCLUDE_DIRS = new Set([
  ".git",
  "node_modules",
  "dist",
  "build",
  "target",
  ".turbo",
  ".next",
  ".cache",
  "out",
  "coverage",
  "vendor",
]);

const TEXT_EXTENSIONS = new Set([
  ".ts",
  ".tsx",
  ".js",
  ".jsx",
  ".mjs",
  ".cjs",
  ".json",
  ".md",
  ".yml",
  ".yaml",
  ".toml",
  ".css",
  ".scss",
  ".less",
  ".html",
  ".htm",
  ".rs",
  ".go",
  ".py",
  ".java",
  ".sh",
]);

// 按语言分组的扩展名
const LANG_MAP = {
  TypeScript: [".ts", ".tsx"],
  JavaScript: [".js", ".jsx", ".mjs", ".cjs"],
  Rust: [".rs"],
  CSS: [".css", ".scss", ".less"],
  JSON: [".json"],
  Markdown: [".md"],
  Other: [],
};

function shouldSkipDir(name) {
  if (!includeHidden && name.startsWith(".")) return true;
  return EXCLUDE_DIRS.has(name);
}

// 更精确的行数统计（排除空行和简单注释）
function countCodeLines(buffer, ext) {
  const text = buffer.toString("utf-8");
  const lines = text.split(/\r?\n/);
  let codeLines = 0;
  let commentLines = 0;
  let blankLines = 0;

  const isSingleComment = (line) => {
    const trimmed = line.trim();
    if (!trimmed) return false;
    if (ext === ".rs" && trimmed.startsWith("//")) return true;
    if ([".ts", ".tsx", ".js", ".jsx"].includes(ext) && trimmed.startsWith("//"))
      return true;
    if ([".py", ".sh"].includes(ext) && trimmed.startsWith("#")) return true;
    return false;
  };

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) {
      blankLines++;
    } else if (isSingleComment(line)) {
      commentLines++;
    } else {
      codeLines++;
    }
  }

  return {
    total: lines.length,
    code: codeLines,
    comment: commentLines,
    blank: blankLines,
  };
}

function getLangGroup(ext) {
  for (const [lang, exts] of Object.entries(LANG_MAP)) {
    if (exts.includes(ext)) return lang;
  }
  return "Other";
}

const stats = {
  files: 0,
  textFiles: 0,
  totalBytes: 0,
  totalLines: { total: 0, code: 0, comment: 0, blank: 0 },
  byExtension: {},
  byDir: {},
};

function walk(dir, currentDir = "") {
  const entries = fs.readdirSync(dir, { withFileTypes: true });

  for (const entry of entries) {
    if (entry.isDirectory()) {
      if (shouldSkipDir(entry.name)) continue;
      const nextDir = currentDir
        ? `${currentDir}/${entry.name}`
        : entry.name;
      walk(path.join(dir, entry.name), nextDir);
      continue;
    }

    if (entry.isFile()) {
      const fullPath = path.join(dir, entry.name);
      const ext = path.extname(entry.name).toLowerCase();
      const stat = fs.statSync(fullPath);

      stats.files++;
      stats.totalBytes += stat.size;

      // 按目录统计
      if (!stats.byDir[currentDir]) {
        stats.byDir[currentDir] = { files: 0, bytes: 0, lines: 0 };
      }
      stats.byDir[currentDir].files++;

      if (!TEXT_EXTENSIONS.has(ext)) continue;

      // 流式读取，避免大文件阻塞
      const fd = fs.openSync(fullPath, "r");
      const buffer = Buffer.alloc(Math.min(stat.size, 10 * 1024 * 1024));
      const bytesRead = fs.readSync(fd, buffer, 0, buffer.length, 0);
      fs.closeSync(fd);

      const content = buffer.slice(0, bytesRead);
      const lines = countCodeLines(content, ext);

      stats.textFiles++;
      stats.totalLines.total += lines.total;
      stats.totalLines.code += lines.code;
      stats.totalLines.comment += lines.comment;
      stats.totalLines.blank += lines.blank;

      stats.byDir[currentDir].lines += lines.total;
      stats.byDir[currentDir].bytes += stat.size;

      if (!stats.byExtension[ext]) {
        stats.byExtension[ext] = {
          files: 0,
          bytes: 0,
          lines: { total: 0, code: 0, comment: 0, blank: 0 },
          lang: getLangGroup(ext),
        };
      }

      const extStat = stats.byExtension[ext];
      extStat.files++;
      extStat.bytes += stat.size;
      extStat.lines.total += lines.total;
      extStat.lines.code += lines.code;
      extStat.lines.comment += lines.comment;
      extStat.lines.blank += lines.blank;
    }
  }
}

function formatBytes(bytes) {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

function printClocReport() {
  // 按语言聚合统计
  const langStats = {};
  for (const [ext, data] of Object.entries(stats.byExtension)) {
    const lang = data.lang;
    if (!langStats[lang]) {
      langStats[lang] = { files: 0, blank: 0, comment: 0, code: 0 };
    }
    langStats[lang].files += data.files;
    langStats[lang].blank += data.lines.blank;
    langStats[lang].comment += data.lines.comment;
    langStats[lang].code += data.lines.code;
  }

  // 按代码行数排序
  const sortedLangs = Object.entries(langStats)
    .filter(([, d]) => d.files > 0)
    .sort((a, b) => b[1].code - a[1].code);

  // 计算总计
  const total = {
    files: 0,
    blank: 0,
    comment: 0,
    code: 0,
  };
  for (const [, d] of sortedLangs) {
    total.files += d.files;
    total.blank += d.blank;
    total.comment += d.comment;
    total.code += d.code;
  }

  // cloc 风格的表格输出
  console.log("\n" + "═".repeat(70));
  console.log("📊 代码统计 (cloc 风格)");
  console.log("═".repeat(70));
  console.log(`\n位置: ${ROOT}\n`);

  // 表头
  console.log(
    "Language".padEnd(12) +
      "files".padStart(8) +
      "blank".padStart(10) +
      "comment".padStart(10) +
      "code".padStart(12)
  );
  console.log("-".repeat(52));

  // 数据行
  for (const [lang, d] of sortedLangs) {
    console.log(
      lang.padEnd(12) +
        String(d.files).padStart(8) +
        String(d.blank).toLocaleString().padStart(10) +
        String(d.comment).toLocaleString().padStart(10) +
        String(d.code).toLocaleString().padStart(12)
    );
  }

  console.log("-".repeat(52));
  console.log(
    "TOTAL".padEnd(12) +
      String(total.files).padStart(8) +
      String(total.blank).toLocaleString().padStart(10) +
      String(total.comment).toLocaleString().padStart(10) +
      String(total.code).toLocaleString().padStart(12)
  );

  // 扩展名详情（更详细的分类）
  console.log("\n" + "═".repeat(70));
  console.log("🔍 扩展名详情 (按代码行排序)");
  console.log("═".repeat(70));
  console.log(
    "Extension".padEnd(12) +
      "files".padStart(8) +
      "blank".padStart(10) +
      "comment".padStart(10) +
      "code".padStart(12) +
      "size".padStart(10)
  );
  console.log("-".repeat(62));

  const sortedExts = Object.entries(stats.byExtension)
    .sort((a, b) => b[1].lines.code - a[1].lines.code);

  for (const [ext, d] of sortedExts) {
    console.log(
      (ext || "<none>").padEnd(12) +
        String(d.files).padStart(8) +
        String(d.lines.blank).toLocaleString().padStart(10) +
        String(d.lines.comment).toLocaleString().padStart(10) +
        String(d.lines.code).toLocaleString().padStart(12) +
        formatBytes(d.bytes).padStart(10)
    );
  }

  // 百分比概览
  const totalLines = total.blank + total.comment + total.code;
  console.log("\n" + "═".repeat(70));
  console.log("📈 代码构成分析");
  console.log("═".repeat(70));
  console.log(`总代码行:     ${totalLines.toLocaleString().padStart(10)}`);
  console.log(`  ├─ 空行:    ${((total.blank / totalLines) * 100).toFixed(1)}%  (${total.blank.toLocaleString()})`);
  console.log(`  ├─ 注释:    ${((total.comment / totalLines) * 100).toFixed(1)}%  (${total.comment.toLocaleString()})`);
  console.log(`  └─ 代码:    ${((total.code / totalLines) * 100).toFixed(1)}%  (${total.code.toLocaleString()})`);

  // 按目录统计
  if (byDir) {
    console.log("\n" + "═".repeat(70));
    console.log("📂 按目录统计 (Top 10)");
    console.log("═".repeat(70));

    const sortedDirs = Object.entries(stats.byDir)
      .sort((a, b) => (b[1].lines || 0) - (a[1].lines || 0))
      .slice(0, 10);

    console.log("Directory".padEnd(35) + "files".padStart(8) + "lines".padStart(12) + "size".padStart(12));
    console.log("-".repeat(67));

    for (const [dir, d] of sortedDirs) {
      console.log(
        (dir || ".").slice(0, 35).padEnd(35) +
          String(d.files).padStart(8) +
          String(d.lines || 0).toLocaleString().padStart(12) +
          formatBytes(d.bytes).padStart(12)
      );
    }
  }

  console.log("\n" + "═".repeat(70));
}

// 执行
const startTime = Date.now();
walk(ROOT);
const duration = Date.now() - startTime;

if (asJson) {
  console.log(
    JSON.stringify(
      { root: ROOT, scanTime: `${duration}ms`, ...stats },
      null,
      2
    )
  );
} else {
  printClocReport();
  console.log(`\n⏱️  扫描耗时: ${duration}ms\n`);
}
