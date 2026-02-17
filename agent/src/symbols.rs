//! Lightweight symbol extraction using regex patterns.
//!
//! Provides basic symbol indexing for popular languages:
//! - Function/method definitions
//! - Class/struct/interface/enum definitions
//! - Constants and top-level variable declarations
//!
//! This is NOT a full parser — it's a best-effort regex scraper
//! designed for quick completions and go-to-definition, not semantic
//! correctness. The 99% case for remote coding.

use std::fs;
use std::path::Path;

use crate::protocol::{SymbolInfo, SymbolKind};

// ═══════════════════════════════════════════════════════════════════════════
// Language pattern registry
// ═══════════════════════════════════════════════════════════════════════════

/// A regex pattern that extracts symbol names from source code.
struct SymbolPattern {
    /// Simplified regex (we use manual matching, not the regex crate).
    kind: SymbolKind,
    /// Prefix keywords to search for.
    keywords: &'static [&'static str],
    /// Whether to expect a name token after the keyword.
    expect_name_after: bool,
}

/// Language-specific pattern sets.
fn language_patterns(lang: &str) -> Vec<SymbolPattern> {
    match lang {
        "javascript" | "typescript" | "jsx" | "tsx" | "js" | "ts" => vec![
            SymbolPattern {
                kind: SymbolKind::Function,
                keywords: &["function ", "async function "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Class,
                keywords: &["class "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Interface,
                keywords: &["interface "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::TypeAlias,
                keywords: &["type "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Enum,
                keywords: &["enum "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Constant,
                keywords: &["const ", "let ", "var "],
                expect_name_after: true,
            },
        ],
        "python" | "py" => vec![
            SymbolPattern {
                kind: SymbolKind::Function,
                keywords: &["def ", "async def "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Class,
                keywords: &["class "],
                expect_name_after: true,
            },
        ],
        "rust" | "rs" => vec![
            SymbolPattern {
                kind: SymbolKind::Function,
                keywords: &["fn ", "pub fn ", "pub async fn ", "async fn ", "pub(crate) fn ", "pub(super) fn "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Struct,
                keywords: &["struct ", "pub struct ", "pub(crate) struct "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Enum,
                keywords: &["enum ", "pub enum ", "pub(crate) enum "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Trait,
                keywords: &["trait ", "pub trait ", "pub(crate) trait "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Constant,
                keywords: &["const ", "pub const ", "static ", "pub static "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::TypeAlias,
                keywords: &["type ", "pub type ", "pub(crate) type "],
                expect_name_after: true,
            },
        ],
        "go" => vec![
            SymbolPattern {
                kind: SymbolKind::Function,
                keywords: &["func "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Struct,
                keywords: &["type "],
                expect_name_after: true,
            },
        ],
        "java" | "kotlin" | "kt" => vec![
            SymbolPattern {
                kind: SymbolKind::Class,
                keywords: &["class ", "public class ", "abstract class ", "data class "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Interface,
                keywords: &["interface ", "public interface "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Enum,
                keywords: &["enum ", "public enum ", "enum class "],
                expect_name_after: true,
            },
        ],
        "c" | "cpp" | "h" | "hpp" | "cc" | "cxx" => vec![
            SymbolPattern {
                kind: SymbolKind::Class,
                keywords: &["class ", "template class ", "template<class T> class ", "template<typename T> class "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Struct,
                keywords: &["struct ", "typedef struct "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Enum,
                keywords: &["enum ", "enum class ", "typedef enum "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::TypeAlias,
                keywords: &["typedef ", "using "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Constant,
                keywords: &["#define ", "constexpr ", "const "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Module,
                keywords: &["namespace "],
                expect_name_after: true,
            },
        ],
        "csharp" | "cs" => vec![
            SymbolPattern {
                kind: SymbolKind::Class,
                keywords: &["class ", "public class ", "internal class ", "abstract class ", "sealed class ", "partial class ", "static class "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Struct,
                keywords: &["struct ", "public struct ", "readonly struct "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Interface,
                keywords: &["interface ", "public interface ", "internal interface "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Enum,
                keywords: &["enum ", "public enum "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Module,
                keywords: &["namespace "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Constant,
                keywords: &["const ", "readonly "],
                expect_name_after: true,
            },
        ],
        "swift" => vec![
            SymbolPattern {
                kind: SymbolKind::Function,
                keywords: &["func ", "public func ", "private func ", "internal func ", "static func ", "@objc func "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Class,
                keywords: &["class ", "final class ", "public class ", "open class "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Struct,
                keywords: &["struct ", "public struct "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Enum,
                keywords: &["enum ", "public enum "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Interface,
                keywords: &["protocol ", "public protocol "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::TypeAlias,
                keywords: &["typealias ", "public typealias "],
                expect_name_after: true,
            },
        ],
        "scala" => vec![
            SymbolPattern {
                kind: SymbolKind::Function,
                keywords: &["def ", "override def ", "private def ", "protected def "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Class,
                keywords: &["class ", "case class ", "abstract class ", "sealed class "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Interface,
                keywords: &["trait ", "sealed trait "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Module,
                keywords: &["object ", "case object "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::TypeAlias,
                keywords: &["type "],
                expect_name_after: true,
            },
        ],
        "lua" => vec![
            SymbolPattern {
                kind: SymbolKind::Function,
                keywords: &["function ", "local function "],
                expect_name_after: true,
            },
        ],
        "shell" | "bash" | "sh" | "zsh" => vec![
            SymbolPattern {
                kind: SymbolKind::Function,
                keywords: &["function "],
                expect_name_after: true,
            },
        ],
        "zig" => vec![
            SymbolPattern {
                kind: SymbolKind::Function,
                keywords: &["fn ", "pub fn ", "export fn "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Struct,
                keywords: &["const ", "pub const "],
                expect_name_after: true,
            },
        ],
        "elixir" | "ex" | "exs" => vec![
            SymbolPattern {
                kind: SymbolKind::Function,
                keywords: &["def ", "defp ", "defmacro ", "defmacrop "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Module,
                keywords: &["defmodule "],
                expect_name_after: true,
            },
        ],
        "haskell" | "hs" => vec![
            SymbolPattern {
                kind: SymbolKind::TypeAlias,
                keywords: &["data ", "type ", "newtype "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Class,
                keywords: &["class "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Module,
                keywords: &["module "],
                expect_name_after: true,
            },
        ],
        "ruby" | "rb" => vec![
            SymbolPattern {
                kind: SymbolKind::Function,
                keywords: &["def "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Class,
                keywords: &["class "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Module,
                keywords: &["module "],
                expect_name_after: true,
            },
        ],
        "php" => vec![
            SymbolPattern {
                kind: SymbolKind::Function,
                keywords: &["function "],
                expect_name_after: true,
            },
            SymbolPattern {
                kind: SymbolKind::Class,
                keywords: &["class "],
                expect_name_after: true,
            },
        ],
        _ => Vec::new(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Symbol extraction
// ═══════════════════════════════════════════════════════════════════════════

/// Extract a valid identifier name starting at the given position.
fn extract_name(line: &str, start: usize) -> Option<&str> {
    let rest = &line[start..];
    let end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    let name = &rest[..end];
    // Skip keywords that aren't real names
    if matches!(name, "if" | "else" | "for" | "while" | "return" | "true" | "false" | "null" | "undefined" | "new" | "this" | "self") {
        return None;
    }
    Some(name)
}

/// Detect language from file extension.
fn detect_language(path: &Path) -> Option<&str> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "js" | "mjs" | "cjs" => Some("javascript"),
        "ts" | "mts" | "cts" => Some("typescript"),
        "jsx" => Some("jsx"),
        "tsx" => Some("tsx"),
        "py" | "pyw" => Some("python"),
        "rs" => Some("rust"),
        "go" => Some("go"),
        "java" => Some("java"),
        "kt" | "kts" => Some("kotlin"),
        "c" => Some("c"),
        "cpp" | "cc" | "cxx" | "C" => Some("cpp"),
        "h" | "hpp" | "hxx" => Some("c"),
        "cs" => Some("csharp"),
        "swift" => Some("swift"),
        "scala" | "sc" => Some("scala"),
        "lua" => Some("lua"),
        "sh" | "bash" | "zsh" => Some("shell"),
        "zig" => Some("zig"),
        "ex" | "exs" => Some("elixir"),
        "hs" | "lhs" => Some("haskell"),
        "rb" => Some("ruby"),
        "php" => Some("php"),
        _ => None,
    }
}

/// Extract symbols from a single file.
fn extract_symbols_from_file(path: &Path) -> Vec<SymbolInfo> {
    let lang = match detect_language(path) {
        Some(l) => l,
        None => return Vec::new(),
    };

    let patterns = language_patterns(lang);
    if patterns.is_empty() {
        return Vec::new();
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut symbols = Vec::new();
    let mut in_block_comment = false;

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Skip block comments
        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if trimmed.starts_with("/*") {
            in_block_comment = true;
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }

        // Skip single-line comments
        if trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }

        // Try each pattern
        for pattern in &patterns {
            for &keyword in pattern.keywords {
                if let Some(pos) = trimmed.find(keyword) {
                    if pattern.expect_name_after {
                        let name_start = pos + keyword.len();
                        if let Some(name) = extract_name(trimmed, name_start) {
                            // For TS/JS const: skip if it looks like `const x = require(...)` imports
                            // but keep function-like: `const x = (` or `const x = async (`
                            symbols.push(SymbolInfo {
                                name: name.to_string(),
                                kind: pattern.kind,
                                path: path.to_string_lossy().to_string(),
                                line: (line_idx + 1) as u32,
                                column: (pos + keyword.len() + 1) as u32,
                                container: None,
                            });
                            break; // One match per line is enough
                        }
                    }
                }
            }
        }
    }

    symbols
}

// ═══════════════════════════════════════════════════════════════════════════
// Directory indexing
// ═══════════════════════════════════════════════════════════════════════════

const IGNORED_DIRS: &[&str] = &[
    ".git", "node_modules", ".hg", "__pycache__", "target", "dist",
    "build", ".next", ".nuxt", "vendor", ".venv", "venv",
];

/// Index all symbols in a directory (recursive).
pub fn index_directory(root: &Path, max_files: u32) -> Vec<SymbolInfo> {
    let mut symbols = Vec::new();
    let mut file_count: u32 = 0;
    index_recursive(root, &mut symbols, &mut file_count, max_files);
    symbols
}

fn index_recursive(
    dir: &Path,
    symbols: &mut Vec<SymbolInfo>,
    file_count: &mut u32,
    max_files: u32,
) {
    if *file_count >= max_files {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry_result in entries {
        if *file_count >= max_files {
            return;
        }

        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip ignored directories
        if IGNORED_DIRS.contains(&name_str.as_ref()) {
            continue;
        }

        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if metadata.is_dir() {
            index_recursive(&path, symbols, file_count, max_files);
        } else if metadata.is_file() && metadata.len() < 500_000 {
            // Only index files < 500KB
            *file_count += 1;
            let file_symbols = extract_symbols_from_file(&path);
            symbols.extend(file_symbols);
        }
    }
}

/// Find completions matching a prefix.
pub fn complete(symbols: &[SymbolInfo], prefix: &str, limit: u32) -> Vec<SymbolInfo> {
    let prefix_lower = prefix.to_lowercase();
    symbols
        .iter()
        .filter(|s| s.name.to_lowercase().starts_with(&prefix_lower))
        .take(limit as usize)
        .cloned()
        .collect()
}

/// Find definitions of a symbol by exact name match.
pub fn find_definitions(symbols: &[SymbolInfo], name: &str) -> Vec<SymbolInfo> {
    symbols
        .iter()
        .filter(|s| s.name == name)
        .cloned()
        .collect()
}
