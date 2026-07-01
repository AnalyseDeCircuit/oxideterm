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

fn paste_target_directory(location: &IdeLocation, is_directory: bool) -> Option<(String, String)> {
    match location {
        IdeLocation::Remote { node_id, path } if is_directory => {
            Some((node_id.clone(), normalize_remote_path(path)))
        }
        IdeLocation::Remote { node_id, path } => {
            Some((node_id.clone(), parent_remote_path(path)))
        }
        IdeLocation::Local { .. } => None,
    }
}

fn remote_path_contains_path(parent: &str, child: &str) -> bool {
    // The tree clipboard blocks self-nesting before it reaches Agent or SFTP backends.
    let parent = normalize_remote_path(parent);
    let child = normalize_remote_path(child);
    if parent == "/" {
        return child.starts_with('/');
    }
    let parent = parent.trim_end_matches('/');
    child == parent || child.starts_with(&format!("{parent}/"))
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
    if trimmed == "~" {
        return "~".to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        let rest = rest.trim_end_matches('/');
        return if rest.is_empty() {
            "~".to_string()
        } else {
            format!("~/{rest}")
        };
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

fn group_search_matches(matches: Vec<IdeSearchMatch>) -> Vec<SearchResultGroup> {
    let mut groups = Vec::<SearchResultGroup>::new();
    for hit in matches {
        if let Some(group) = groups.iter_mut().find(|group| group.path == hit.path) {
            group.matches.push(hit);
        } else {
            groups.push(SearchResultGroup {
                path: hit.path.clone(),
                matches: vec![hit],
            });
        }
    }
    groups
}

fn is_absolute_search_path(path: &str) -> bool {
    path.starts_with('/') || path.as_bytes().get(1) == Some(&b':')
}

fn resolve_search_match_path(root_path: &str, match_path: &str) -> String {
    if is_absolute_search_path(match_path) {
        normalize_remote_path(match_path)
    } else {
        join_remote_child(root_path, match_path)
    }
}

fn validate_file_name(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return Some("ide.validation.nameEmpty".to_string());
    }
    if name.contains('/') {
        return Some("ide.validation.nameContainsSlash".to_string());
    }
    if name == "." || name == ".." {
        return Some("ide.validation.nameInvalid".to_string());
    }
    if name
        .chars()
        .any(|ch| matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*') || ch.is_control())
    {
        return Some("ide.validation.nameInvalidChars".to_string());
    }
    if name.len() > 255 {
        return Some("ide.validation.nameTooLong".to_string());
    }
    None
}

fn watch_refresh_path(root_path: &str, event_path: &str) -> String {
    let root_path = normalize_remote_path(root_path);
    let event_path = normalize_remote_path(event_path);
    if event_path == root_path {
        root_path
    } else {
        parent_remote_path(&event_path)
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn watch_refresh_path_matches_tauri_parent_refresh() {
        assert_eq!(watch_refresh_path("/srv/app", "/srv/app"), "/srv/app");
        assert_eq!(
            watch_refresh_path("/srv/app", "/srv/app/src/main.rs"),
            "/srv/app/src"
        );
    }

    #[test]
    fn normalize_remote_path_preserves_home_expansion_inputs() {
        assert_eq!(normalize_remote_path("~"), "~");
        assert_eq!(normalize_remote_path("~/project/"), "~/project");
    }

    #[test]
    fn search_match_path_resolution_matches_tauri_panel() {
        assert_eq!(
            resolve_search_match_path("/srv/app", "/srv/app/src/main.rs"),
            "/srv/app/src/main.rs"
        );
        assert_eq!(
            resolve_search_match_path("/srv/app", "src/main.rs"),
            "/srv/app/src/main.rs"
        );
    }
}

fn language_for_location(location: &IdeLocation, source: &str) -> Option<LanguageId> {
    match location {
        IdeLocation::Local { path } => LanguageId::detect(Some(path.as_path()), source),
        IdeLocation::Remote { path, .. } => LanguageId::detect(Some(Path::new(path)), source),
    }
}
