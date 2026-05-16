fn import_knowledge_file(
    store: &oxideterm_ai::RagStore,
    collection_id: &str,
    path: &std::path::Path,
) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > KNOWLEDGE_MAX_IMPORT_FILE_SIZE {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document");
        return Err(format!(
            "File \"{file_name}\" exceeds 5 MB limit ({} MB)",
            (metadata.len() as f64 / 1024.0 / 1024.0).round() as u64
        ));
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document")
        .to_string();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let format = match extension.as_str() {
        "md" | "markdown" => "markdown",
        "txt" => "plaintext",
        _ => return Err(format!("Unsupported document type: {file_name}")),
    };
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    oxideterm_ai::rag_add_document(
        store,
        oxideterm_ai::RagAddDocumentRequest {
            collection_id: collection_id.to_string(),
            title: file_name,
            content,
            format: format.to_string(),
            source_path: Some(path.to_string_lossy().to_string()),
        },
    )
    .map(|_| ())
}

fn open_path_external(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?.wait()?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn()?
            .wait()?;
        return Ok(());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?.wait()?;
        Ok(())
    }
}

fn reconnect_max_attempt_options() -> [i64; 8] {
    [1, 2, 3, 5, 8, 10, 15, 20]
}

fn reconnect_base_delay_options() -> [(i64, &'static str); 6] {
    [
        (500, "0.5s"),
        (1_000, "1s"),
        (2_000, "2s"),
        (3_000, "3s"),
        (5_000, "5s"),
        (10_000, "10s"),
    ]
}

fn reconnect_max_delay_options() -> [(i64, &'static str); 5] {
    [
        (5_000, "5s"),
        (10_000, "10s"),
        (15_000, "15s"),
        (30_000, "30s"),
        (60_000, "60s"),
    ]
}

fn reconnect_attempt_label(value: i64) -> String {
    value.to_string()
}

fn reconnect_delay_label(value: i64) -> String {
    if value % 1_000 == 0 {
        format!("{}s", value / 1_000)
    } else {
        format!("{:.1}s", value as f64 / 1_000.0)
    }
}
