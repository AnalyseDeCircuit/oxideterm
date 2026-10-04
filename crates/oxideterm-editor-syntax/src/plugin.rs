// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{LanguageId, SyntaxError, session::LanguageQueries};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};
use tree_sitter::{Language, Parser, WasmStore, wasmtime};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginGrammarSource {
    pub language: LanguageId,
    pub parser: PathBuf,
    pub highlights: PathBuf,
    pub parser_sha256: String,
    pub highlights_sha256: String,
}

/// One immutable plugin revision; document parsers keep their own execution stores.
pub struct PluginGrammar {
    pub(crate) source: PluginGrammarSource,
    loaded: OnceLock<Result<LoadedGrammar, String>>,
}

struct LoadedGrammar {
    engine: wasmtime::Engine,
    language: Language,
    queries: Arc<LanguageQueries>,
    _loader: Mutex<WasmStore>,
}

impl PluginGrammar {
    pub fn new(source: PluginGrammarSource) -> Self {
        Self {
            source,
            loaded: OnceLock::new(),
        }
    }

    pub fn failed(&self) -> bool {
        self.loaded.get().is_some_and(Result::is_err)
    }

    pub fn source(&self) -> &PluginGrammarSource {
        &self.source
    }

    pub(crate) fn parser(&self) -> Result<(Parser, Language, Arc<LanguageQueries>), SyntaxError> {
        let loaded = self
            .loaded
            .get_or_init(|| self.load())
            .as_ref()
            .map_err(|error| SyntaxError::Plugin(error.clone()))?;
        let store = WasmStore::new(&loaded.engine)
            .map_err(|error| SyntaxError::Plugin(error.to_string()))?;
        let mut parser = Parser::new();
        parser.set_wasm_store(store)?;
        parser.set_language(&loaded.language)?;
        Ok((parser, loaded.language.clone(), loaded.queries.clone()))
    }

    fn load(&self) -> Result<LoadedGrammar, String> {
        let name = self
            .source
            .language
            .plugin_key()
            .ok_or("Language is built in")?;
        let wasm = read_verified(
            &self.source.parser,
            &self.source.parser_sha256,
            16 * 1024 * 1024,
        )?;
        let highlights = String::from_utf8(read_verified(
            &self.source.highlights,
            &self.source.highlights_sha256,
            1024 * 1024,
        )?)
        .map_err(|_| "Highlight query is not UTF-8")?;
        let engine = wasmtime::Engine::default();
        let mut loader = WasmStore::new(&engine).map_err(|error| error.to_string())?;
        // Grammar IDs such as c-sharp export C symbols using underscores.
        let language = loader
            .load_language(&name.replace('-', "_"), &wasm)
            .map_err(|error| error.to_string())?;
        let queries = Arc::new(
            LanguageQueries::for_plugin(&language, &highlights)
                .map_err(|error| error.to_string())?,
        );
        Ok(LoadedGrammar {
            engine,
            language,
            queries,
            _loader: Mutex::new(loader),
        })
    }
}

fn read_verified(path: &std::path::Path, checksum: &str, maximum: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > maximum {
        return Err("Language asset exceeds its size limit".into());
    }
    if format!("{:x}", Sha256::digest(&bytes)) != checksum {
        return Err("Language asset checksum mismatch".into());
    }
    Ok(bytes)
}
