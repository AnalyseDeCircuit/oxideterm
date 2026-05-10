fn tree_svg_icon(path: &'static str, size: f32, color: u32) -> AnyElement {
    svg()
        .path(path)
        .size(px(size))
        .text_color(rgb(color))
        .into_any_element()
}

fn apply_editor_runtime_settings(
    editor: &Entity<TextEditorView>,
    tokens: ThemeTokens,
    runtime_settings: IdeRuntimeSettings,
    cx: &mut Context<IdeSurface>,
) {
    editor.update(cx, |editor, cx| {
        editor.apply_ide_runtime_settings(
            &tokens,
            runtime_settings.editor_font_size,
            runtime_settings.editor_line_height,
            runtime_settings.word_wrap,
            runtime_settings.background_active,
            cx,
        );
    });
}

async fn open_project_with_root_listing(
    fs: NodeAgentIdeFileSystem,
    node_id: String,
    root_path: String,
) -> Result<ProjectOpenResult, oxideterm_ide_core::IdeFileError> {
    let project = fs.open_project(node_id.clone(), root_path).await?;
    let root = IdeLocation::remote(node_id.clone(), project.root_path.clone());
    let children = fs.list_dir(&root).await.map(sort_tree_entries)?;
    Ok(ProjectOpenResult {
        node_id,
        root,
        title: project.name,
        git_branch: project.git_branch,
        children,
    })
}

async fn open_text_file(
    fs: NodeAgentIdeFileSystem,
    location: IdeLocation,
) -> Result<FileOpenResult, oxideterm_ide_core::IdeFileError> {
    let (node_id, path) = match &location {
        IdeLocation::Remote { node_id, path } => (node_id.clone(), path.clone()),
        IdeLocation::Local { .. } => {
            return Err(oxideterm_ide_core::IdeFileError::new(
                oxideterm_ide_core::IdeFileErrorKind::Unsupported,
                "GPUI IDE node surface only opens node SFTP files",
            ));
        }
    };
    match fs.check_file(node_id, path).await? {
        IdeFileCheck::Editable { .. } => {
            let data = fs.read_file(&location).await?;
            Ok(FileOpenResult {
                location,
                text: data.text,
                version: data.version,
            })
        }
        IdeFileCheck::TooLarge { size, limit } => Err(oxideterm_ide_core::IdeFileError::new(
            oxideterm_ide_core::IdeFileErrorKind::Unsupported,
            format!("File is too large to edit ({size} > {limit})"),
        )),
        IdeFileCheck::Binary => Err(oxideterm_ide_core::IdeFileError::new(
            oxideterm_ide_core::IdeFileErrorKind::Unsupported,
            "File is binary",
        )),
        IdeFileCheck::NotEditable { reason } => Err(oxideterm_ide_core::IdeFileError::new(
            oxideterm_ide_core::IdeFileErrorKind::Unsupported,
            reason,
        )),
    }
}

async fn await_ide_backend<T>(
    handle: tokio::task::JoinHandle<Result<T, oxideterm_ide_core::IdeFileError>>,
) -> Result<T, oxideterm_ide_core::IdeFileError> {
    handle.await.unwrap_or_else(|error| {
        Err(oxideterm_ide_core::IdeFileError::new(
            oxideterm_ide_core::IdeFileErrorKind::Other,
            format!("IDE backend task failed: {error}"),
        ))
    })
}

fn sort_tree_entries(mut entries: Vec<FileTreeEntry>) -> Vec<FileTreeEntry> {
    entries.sort_by(|left, right| {
        let left_dir = matches!(left.kind, FileKind::Directory);
        let right_dir = matches!(right.kind, FileKind::Directory);
        right_dir
            .cmp(&left_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    entries
}

fn location_path(location: IdeLocation) -> String {
    match location {
        IdeLocation::Remote { path, .. } => path,
        IdeLocation::Local { path } => path.display().to_string(),
    }
}

fn remote_path(location: &IdeLocation) -> Option<&str> {
    match location {
        IdeLocation::Remote { path, .. } => Some(path.as_str()),
        IdeLocation::Local { .. } => None,
    }
}

fn format_conflict_mtime(mtime: Option<i64>) -> String {
    mtime
        .filter(|mtime| *mtime > 0)
        .map(|mtime| mtime.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn folder_picker_dirs(entries: Vec<FileTreeEntry>) -> Vec<FileTreeEntry> {
    let mut folders = entries
        .into_iter()
        .filter(|entry| matches!(entry.kind, FileKind::Directory))
        .collect::<Vec<_>>();
    folders.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    folders
}

fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "/".to_string();
    }
    if trimmed == "/" {
        return "/".to_string();
    }
    let without_trailing = trimmed.trim_end_matches('/');
    if without_trailing.starts_with('/') {
        without_trailing.to_string()
    } else {
        format!("/{without_trailing}")
    }
}

fn join_remote_child(parent: &str, child: &str) -> String {
    if parent == "/" {
        format!("/{child}")
    } else {
        format!("{}/{child}", parent.trim_end_matches('/'))
    }
}

fn parent_remote_path(path: &str) -> String {
    let path = normalize_remote_path(path);
    if path == "/" {
        return "/".to_string();
    }
    path.rsplit_once('/')
        .map(|(parent, _)| {
            if parent.is_empty() {
                "/".to_string()
            } else {
                parent.to_string()
            }
        })
        .unwrap_or_else(|| "/".to_string())
}

fn language_for_location(location: &IdeLocation, source: &str) -> Option<LanguageId> {
    match location {
        IdeLocation::Local { path } => LanguageId::detect(Some(path.as_path()), source),
        IdeLocation::Remote { path, .. } => LanguageId::detect(Some(Path::new(path)), source),
    }
}
