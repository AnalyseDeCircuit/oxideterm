#!/usr/bin/env node
/**
 * i18n Key Checker Script
 * 
 * This script compares translation files against the English (en) base locale
 * and reports missing or extra keys.
 * 
 * Usage:
 *   node scripts/check-i18n.cjs [language]
 * 
 * Examples:
 *   node scripts/check-i18n.cjs           # Check all languages
 *   node scripts/check-i18n.cjs zh-CN     # Check only Chinese Simplified
 *   node scripts/check-i18n.cjs de,ja,ko  # Check multiple languages
 */

const fs = require('fs');
const path = require('path');

const LOCALES_DIR = path.join(__dirname, '../src/locales');
const BASE_LOCALE = 'en';

// ANSI color codes
const colors = {
  reset: '\x1b[0m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
  bold: '\x1b[1m',
};

/**
 * Recursively extract all keys from a nested object
 * @param {object} obj - The object to extract keys from
 * @param {string} prefix - Current key prefix
 * @returns {Set<string>} Set of all keys in dot notation
 */
function extractKeys(obj, prefix = '') {
  const keys = new Set();
  
  for (const [key, value] of Object.entries(obj)) {
    const fullKey = prefix ? `${prefix}.${key}` : key;
    
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      // Recurse into nested objects
      const nestedKeys = extractKeys(value, fullKey);
      nestedKeys.forEach(k => keys.add(k));
    } else {
      // Leaf node
      keys.add(fullKey);
    }
  }
  
  return keys;
}

/**
 * Load all JSON files from a locale directory and merge keys
 * @param {string} locale - Locale code (e.g., 'en', 'zh-CN')
 * @returns {Map<string, Set<string>>} Map of filename to keys
 */
function loadLocaleKeys(locale) {
  const localeDir = path.join(LOCALES_DIR, locale);
  const fileKeys = new Map();
  
  if (!fs.existsSync(localeDir)) {
    console.error(`${colors.red}Error: Locale directory not found: ${localeDir}${colors.reset}`);
    process.exit(1);
  }
  
  const files = fs.readdirSync(localeDir).filter(f => f.endsWith('.json'));
  
  for (const file of files) {
    const filePath = path.join(localeDir, file);
    try {
      const content = JSON.parse(fs.readFileSync(filePath, 'utf-8'));
      const keys = extractKeys(content);
      fileKeys.set(file, keys);
    } catch (err) {
      console.error(`${colors.red}Error parsing ${filePath}: ${err.message}${colors.reset}`);
    }
  }
  
  return fileKeys;
}

/**
 * Compare a target locale against the base locale
 * @param {string} targetLocale - Locale to check
 * @param {Map<string, Set<string>>} baseKeys - Base locale keys by file
 * @returns {object} Comparison results
 */
function compareLocale(targetLocale, baseKeys) {
  const targetKeys = loadLocaleKeys(targetLocale);
  const results = {
    locale: targetLocale,
    missing: new Map(),
    extra: new Map(),
    totalMissing: 0,
    totalExtra: 0,
  };
  
  // Check each file in base locale
  for (const [file, keys] of baseKeys) {
    const targetFileKeys = targetKeys.get(file) || new Set();
    const missingKeys = [];
    
    for (const key of keys) {
      if (!targetFileKeys.has(key)) {
        missingKeys.push(key);
      }
    }
    
    if (missingKeys.length > 0) {
      results.missing.set(file, missingKeys);
      results.totalMissing += missingKeys.length;
    }
  }
  
  // Check for extra keys in target (not in base)
  for (const [file, keys] of targetKeys) {
    const baseFileKeys = baseKeys.get(file) || new Set();
    const extraKeys = [];
    
    for (const key of keys) {
      if (!baseFileKeys.has(key)) {
        extraKeys.push(key);
      }
    }
    
    if (extraKeys.length > 0) {
      results.extra.set(file, extraKeys);
      results.totalExtra += extraKeys.length;
    }
  }
  
  return results;
}

/**
 * Print comparison results
 * @param {object} results - Comparison results
 */
function printResults(results) {
  const { locale, missing, extra, totalMissing, totalExtra } = results;
  
  // Header
  console.log(`\n${colors.bold}${colors.cyan}═══════════════════════════════════════════════════════════${colors.reset}`);
  console.log(`${colors.bold}  ${locale.toUpperCase()}${colors.reset}`);
  console.log(`${colors.cyan}═══════════════════════════════════════════════════════════${colors.reset}`);
  
  // Summary
  if (totalMissing === 0 && totalExtra === 0) {
    console.log(`${colors.green}✓ All keys match the base locale (${BASE_LOCALE})${colors.reset}`);
    return;
  }
  
  // Missing keys
  if (totalMissing > 0) {
    console.log(`\n${colors.red}Missing Keys (${totalMissing}):${colors.reset}`);
    
    for (const [file, keys] of missing) {
      console.log(`\n  ${colors.yellow}${file}${colors.reset} ${colors.dim}(${keys.length} missing)${colors.reset}`);
      for (const key of keys) {
        console.log(`    ${colors.red}✗${colors.reset} ${key}`);
      }
    }
  }
  
  // Extra keys (informational)
  if (totalExtra > 0) {
    console.log(`\n${colors.yellow}Extra Keys (${totalExtra}):${colors.reset} ${colors.dim}(keys not in base locale)${colors.reset}`);
    
    for (const [file, keys] of extra) {
      console.log(`\n  ${colors.yellow}${file}${colors.reset} ${colors.dim}(${keys.length} extra)${colors.reset}`);
      for (const key of keys) {
        console.log(`    ${colors.yellow}?${colors.reset} ${key}`);
      }
    }
  }
}

/**
 * Get all available locales
 * @returns {string[]} List of locale codes
 */
function getAvailableLocales() {
  return fs.readdirSync(LOCALES_DIR)
    .filter(f => {
      const stat = fs.statSync(path.join(LOCALES_DIR, f));
      return stat.isDirectory() && f !== BASE_LOCALE;
    });
}

/**
 * Print summary table
 * @param {object[]} allResults - All comparison results
 */
function printSummary(allResults) {
  console.log(`\n${colors.bold}${colors.cyan}═══════════════════════════════════════════════════════════${colors.reset}`);
  console.log(`${colors.bold}  SUMMARY${colors.reset}`);
  console.log(`${colors.cyan}═══════════════════════════════════════════════════════════${colors.reset}\n`);
  
  // Table header
  console.log(`  ${colors.dim}Locale${colors.reset}      ${colors.dim}Missing${colors.reset}    ${colors.dim}Extra${colors.reset}      ${colors.dim}Status${colors.reset}`);
  console.log(`  ${colors.dim}──────${colors.reset}      ${colors.dim}───────${colors.reset}    ${colors.dim}─────${colors.reset}      ${colors.dim}──────${colors.reset}`);
  
  for (const result of allResults) {
    const locale = result.locale.padEnd(10);
    const missing = String(result.totalMissing).padEnd(10);
    const extra = String(result.totalExtra).padEnd(10);
    
    let status;
    if (result.totalMissing === 0) {
      status = `${colors.green}✓ Complete${colors.reset}`;
    } else if (result.totalMissing > 50) {
      status = `${colors.red}✗ Incomplete${colors.reset}`;
    } else {
      status = `${colors.yellow}⚠ Partial${colors.reset}`;
    }
    
    console.log(`  ${locale}  ${missing}  ${extra}  ${status}`);
  }
  
  // Total
  const totalMissing = allResults.reduce((sum, r) => sum + r.totalMissing, 0);
  const totalExtra = allResults.reduce((sum, r) => sum + r.totalExtra, 0);
  
  console.log(`  ${colors.dim}──────${colors.reset}      ${colors.dim}───────${colors.reset}    ${colors.dim}─────${colors.reset}`);
  console.log(`  ${colors.bold}Total${colors.reset}       ${colors.bold}${totalMissing}${colors.reset}${' '.repeat(10 - String(totalMissing).length)}${colors.bold}${totalExtra}${colors.reset}`);
}

// Main execution
function main() {
  console.log(`${colors.bold}i18n Key Checker${colors.reset}`);
  console.log(`${colors.dim}Base locale: ${BASE_LOCALE}${colors.reset}`);
  
  // Load base locale keys
  const baseKeys = loadLocaleKeys(BASE_LOCALE);
  
  // Determine which locales to check
  let localesToCheck;
  const arg = process.argv[2];
  
  if (arg) {
    localesToCheck = arg.split(',').map(l => l.trim());
    // Validate locales
    const available = getAvailableLocales();
    for (const locale of localesToCheck) {
      if (!available.includes(locale)) {
        console.error(`${colors.red}Error: Unknown locale '${locale}'${colors.reset}`);
        console.log(`${colors.dim}Available locales: ${available.join(', ')}${colors.reset}`);
        process.exit(1);
      }
    }
  } else {
    localesToCheck = getAvailableLocales();
  }
  
  console.log(`${colors.dim}Checking: ${localesToCheck.join(', ')}${colors.reset}`);
  
  // Compare each locale
  const allResults = [];
  
  for (const locale of localesToCheck) {
    const results = compareLocale(locale, baseKeys);
    allResults.push(results);
    printResults(results);
  }
  
  // Print summary
  if (localesToCheck.length > 1) {
    printSummary(allResults);
  }
  
  // Exit with error code if any missing keys
  const hasMissing = allResults.some(r => r.totalMissing > 0);
  process.exit(hasMissing ? 1 : 0);
}

main();
