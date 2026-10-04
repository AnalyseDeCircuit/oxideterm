// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_editor_syntax::{LanguageId, PluginGrammar, PluginGrammarSource, SyntaxSession};
use oxideterm_plugin_manifest::NativePluginManifest;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Supply a language plugin directory")?,
    );
    let manifest: NativePluginManifest =
        serde_json::from_slice(&fs::read(directory.join("plugin.json"))?)?;
    let language = manifest
        .contributes
        .and_then(|value| value.language)
        .ok_or("Missing language contribution")?;
    let id = LanguageId::from_plugin_key(&language.id).ok_or("Unknown plugin language")?;
    let parser = directory.join(manifest.runtime.ok_or("Missing runtime")?.entry);
    let highlights = directory.join(language.highlights);
    for (path, checksum) in [
        (&parser, &language.parser_sha256),
        (&highlights, &language.highlights_sha256),
    ] {
        if format!("{:x}", Sha256::digest(fs::read(path)?)) != *checksum {
            return Err("Checksum mismatch".into());
        }
    }
    let grammar = PluginGrammar::new(PluginGrammarSource {
        language: id,
        parser,
        highlights,
        parser_sha256: language.parser_sha256,
        highlights_sha256: language.highlights_sha256,
    });
    let expectation: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("sample.json"))?)?;
    let sample = expectation["source"]
        .as_str()
        .ok_or("Missing source sample")?;
    let expected_text = expectation["highlight"]["text"]
        .as_str()
        .ok_or("Missing expected highlighted text")?;
    let expected_scope = expectation["highlight"]["scope"]
        .as_str()
        .ok_or("Missing expected highlight scope")?;
    let session = SyntaxSession::parse_plugin(&grammar, &sample, None)?;
    if session.root_has_error() {
        return Err("Grammar rejected its source sample".into());
    }
    let spans = session.highlight_spans(&sample);
    if !spans.iter().any(|span| {
        format!("{:?}", span.scope) == expected_scope
            && &sample[span.range.start.0..span.range.end.0] == expected_text
    }) {
        return Err(format!("Missing {expected_scope} capture for {expected_text:?}").into());
    }
    println!(
        "Verified {} parser, ABI, queries and highlighting",
        language.id
    );
    Ok(())
}
