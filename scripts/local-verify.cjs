#!/usr/bin/env node
/**
 * Local verify — run i18n check, TypeScript (noEmit), and Rust cargo check in sequence.
 * 本地一键校验：翻译键、TS 类型、Rust 编译。
 *
 * Usage (from repo root):
 *   node scripts/local-verify.cjs
 *   pnpm verify:local
 *   pnpm verify:local --skip-i18n
 *   pnpm verify:local --full   # after default steps, also pnpm run build (tsc + vite)
 */
/* eslint-disable no-console */

const { spawnSync } = require("child_process");
const path = require("path");

const REPO_ROOT = path.join(__dirname, "..");
const TAURI_DIR = path.join(REPO_ROOT, "src-tauri");

const args = new Set(process.argv.slice(2));
const skipI18n = args.has("--skip-i18n");
const skipTs = args.has("--skip-ts");
const skipRust = args.has("--skip-rust");
const full = args.has("--full");

/**
 * @param {string} label
 * @param {string} command
 * @param {string[]} commandArgs
 * @param {import('child_process').SpawnSyncOptions} [opts]
 * @returns {boolean} true if OK or skipped
 */
function runStep(label, command, commandArgs, opts = {}) {
  console.log(`\n[local-verify] ${label}…`);
  const result = spawnSync(command, commandArgs, {
    stdio: "inherit",
    cwd: REPO_ROOT,
    env: process.env,
    ...opts,
  });
  if (result.error) {
    console.error(result.error);
    return false;
  }
  if (result.status !== 0) {
    console.error(`[local-verify] ${label} failed (exit ${result.status}).`);
    return false;
  }
  return true;
}

function main() {
  console.log("[local-verify] Repo root:", REPO_ROOT);

  if (!skipI18n) {
    const ok = runStep("i18n check", process.execPath, [
      path.join(__dirname, "check-i18n.cjs"),
    ]);
    if (!ok) process.exit(1);
  } else {
    console.log("[local-verify] skip i18n (--skip-i18n)");
  }

  if (!skipTs) {
    const ok = runStep("TypeScript (tsc --noEmit)", "pnpm", [
      "exec",
      "tsc",
      "--noEmit",
    ]);
    if (!ok) process.exit(1);
  } else {
    console.log("[local-verify] skip TypeScript (--skip-ts)");
  }

  if (!skipRust) {
    const ok = runStep(
      "Rust (cargo check)",
      "cargo",
      ["check"],
      { cwd: TAURI_DIR },
    );
    if (!ok) process.exit(1);
  } else {
    console.log("[local-verify] skip Rust (--skip-rust)");
  }

  if (full) {
    const ok = runStep("full frontend build", "pnpm", ["run", "build"]);
    if (!ok) process.exit(1);
  }

  console.log("\n[local-verify] All requested steps passed.");
  console.log(
    "[local-verify] trzsz fork maintenance: pnpm trzsz:maintain (details: src/lib/terminal/trzsz/UPSTREAM_DIFF.md).",
  );
}

main();
