// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

// Included only by test modules. Keep actual plugin packages alive for every
// parser using them, without adding fixture loading to the production API.
pub(crate) fn grammar(language: LanguageId) -> &'static PluginGrammar {
    use std::sync::OnceLock;
    type Fixture = (tempfile::TempDir, PluginGrammar);
    static C: OnceLock<Fixture> = OnceLock::new();
    static RUST: OnceLock<Fixture> = OnceLock::new();
    static ELIXIR: OnceLock<Fixture> = OnceLock::new();
    let (slot, bytes): (&OnceLock<Fixture>, &[u8]) = match language {
        LanguageId::C => (&C, include_bytes!("../fixtures/c.zip")),
        LanguageId::Rust => (&RUST, include_bytes!("../fixtures/rust.zip")),
        LanguageId::Elixir => (&ELIXIR, include_bytes!("../fixtures/elixir.zip")),
        _ => panic!("No plugin fixture for {language:?}"),
    };
    &slot
        .get_or_init(|| {
            let directory = tempfile::tempdir().unwrap();
            zip::ZipArchive::new(std::io::Cursor::new(bytes))
                .unwrap()
                .extract(directory.path())
                .unwrap();
            let manifest: serde_json::Value = serde_json::from_slice(
                &std::fs::read(directory.path().join("plugin.json")).unwrap(),
            )
            .unwrap();
            let declared = &manifest["contributes"]["language"];
            let grammar = PluginGrammar::new(PluginGrammarSource {
                grammar_name: declared["id"].as_str().unwrap().into(),
                language,
                parser: directory.path().join("parser.wasm"),
                highlights: directory.path().join("highlights.scm"),
                parser_sha256: declared["parserSha256"].as_str().unwrap().into(),
                highlights_sha256: declared["highlightsSha256"].as_str().unwrap().into(),
                injections: Vec::new(),
            });
            (directory, grammar)
        })
        .1
}

pub(crate) fn parse(language: LanguageId, source: &str) -> Result<SyntaxSession, SyntaxError> {
    parse_controlled(language, source, None)
}

pub(crate) fn parse_controlled(
    language: LanguageId,
    source: &str,
    work: Option<&SyntaxWork>,
) -> Result<SyntaxSession, SyntaxError> {
    if language.plugin_key().is_some() {
        SyntaxSession::parse_plugin(grammar(language), source, work)
    } else {
        SyntaxSession::parse_controlled(language, source, work)
    }
}
