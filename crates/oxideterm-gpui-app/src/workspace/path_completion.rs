use super::*;


#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PathCompletionOwner {
    FileManager,
    SftpLocal(sftp::SftpSurfaceId),
    SftpRemote(sftp::SftpSurfaceId),
}

impl PathCompletionOwner {
    fn ime_target(self) -> WorkspaceImeTarget {
        match self {
            Self::FileManager => {
                WorkspaceImeTarget::FileManager(file_manager::FileManagerInput::Path)
            }
            Self::SftpLocal(surface) => {
                WorkspaceImeTarget::Sftp(surface, sftp::SftpInput::LocalPath)
            }
            Self::SftpRemote(surface) => {
                WorkspaceImeTarget::Sftp(surface, sftp::SftpInput::RemotePath)
            }
        }
    }

    fn popup_id(self) -> &'static str {
        match self {
            Self::FileManager => "file-manager-path-completion",
            Self::SftpLocal(_) => "sftp-local-path-completion",
            Self::SftpRemote(_) => "sftp-remote-path-completion",
        }
    }
}

pub(super) use oxideterm_gpui_file_manager::PathCompletionState;
use oxideterm_local_files::split_path_completion_request;
pub(super) use oxideterm_local_files::{
    PathCompletionCandidate, PathCompletionRequest, local_path_completion_request,
};

pub(super) fn remote_path_completion_request(input: &str) -> Option<PathCompletionRequest> {
    if input.trim().is_empty() {
        return None;
    }
    let normalized = oxideterm_sftp::normalize_remote_path(input);
    split_path_completion_request(
        &normalized,
        input.ends_with('/') || normalized == "/",
        |character| character == '/',
        "/",
    )
}

impl WorkspaceApp {
    pub(super) fn render_path_completion_overlay(
        &self,
        owner: PathCompletionOwner,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (visible, selected_index, scroll_handle, suggestions) =
            self.path_completion_snapshot(owner, cx);
        if !visible {
            return None;
        }
        let anchor = self
            .text_input_anchors
            .get(owner.ime_target().anchor_id())?;
        let workspace = cx.weak_entity();
        Some(oxideterm_gpui_file_manager::render_completion_popup(
            owner.popup_id(), &self.tokens, anchor.bounds, selected_index, scroll_handle, &suggestions,
            move |index, cx| {
                let _ = workspace.update(cx, |workspace, cx| {
                    workspace.accept_path_completion(owner, index, cx);
                    cx.notify();
                });
            },
        ))
    }

    fn path_completion_snapshot(
        &self,
        owner: PathCompletionOwner,
        cx: &App,
    ) -> (
        bool,
        Option<usize>,
        ScrollHandle,
        Vec<PathCompletionCandidate>,
    ) {
        match owner {
            PathCompletionOwner::FileManager => {
                let state = &self.file_manager.read(cx).path_completion;
                (
                    state.is_visible(),
                    state.selected_index(),
                    state.scroll_handle.clone(),
                    state.suggestions().to_vec(),
                )
            }
            PathCompletionOwner::SftpLocal(surface) => {
                let _scope = self.enter_sftp_surface(surface);
                let sftp = self.sftp_view().read(cx);
                let state = &sftp.local_path_completion;
                (
                    state.is_visible(),
                    state.selected_index(),
                    state.scroll_handle.clone(),
                    state.suggestions().to_vec(),
                )
            }
            PathCompletionOwner::SftpRemote(surface) => {
                let _scope = self.enter_sftp_surface(surface);
                let sftp = self.sftp_view().read(cx);
                let state = &sftp.remote_path_completion;
                (
                    state.is_visible(),
                    state.selected_index(),
                    state.scroll_handle.clone(),
                    state.suggestions().to_vec(),
                )
            }
        }
    }

    fn accept_path_completion(
        &mut self,
        owner: PathCompletionOwner,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        match owner {
            PathCompletionOwner::FileManager => self.accept_file_manager_path_completion(index, cx),
            PathCompletionOwner::SftpLocal(surface) => {
                if !self.has_sftp_surface(surface) {
                    return;
                }
                let _scope = self.enter_sftp_surface(surface);
                self.accept_sftp_path_completion(sftp::SftpPane::Local, index, cx)
            }
            PathCompletionOwner::SftpRemote(surface) => {
                if !self.has_sftp_surface(surface) {
                    return;
                }
                let _scope = self.enter_sftp_surface(surface);
                self.accept_sftp_path_completion(sftp::SftpPane::Remote, index, cx)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(name: &str, is_directory: bool) -> PathCompletionCandidate {
        PathCompletionCandidate {
            name: name.to_string(),
            path: format!("/root/{name}"),
            is_directory,
        }
    }

    #[test]
    fn remote_request_splits_parent_and_prefix() {
        let request = remote_path_completion_request("/root/a").unwrap();
        assert_eq!(request.parent_path, "/root");
        assert_eq!(request.prefix, "a");

        let directory_request = remote_path_completion_request("/root/").unwrap();
        assert_eq!(directory_request.parent_path, "/root");
        assert_eq!(directory_request.prefix, "");
    }

    #[test]
    fn local_request_splits_parent_and_prefix() {
        let request = local_path_completion_request("/root/a").unwrap();
        assert_eq!(request.parent_path, "/root");
        assert_eq!(request.prefix, "a");

        let directory_request = local_path_completion_request("/root/").unwrap();
        assert_eq!(directory_request.parent_path, "/root");
        assert_eq!(directory_request.prefix, "");
        assert!(local_path_completion_request("").is_none());
    }

    #[test]
    fn completion_keeps_directories_before_matching_files() {
        let mut state = PathCompletionState::default();
        let request = remote_path_completion_request("/root/a").unwrap();
        let (generation, parent_path) = state.request(request).unwrap();
        assert!(state.apply_entries(
            generation,
            &parent_path,
            vec![
                candidate("abc.txt", false),
                candidate("abc", true),
                candidate("a.txt", false),
                candidate("a", true),
                candidate("notes.txt", false),
            ],
        ));

        let labels = state
            .suggestions()
            .iter()
            .map(|candidate| candidate.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["a", "abc", "a.txt", "abc.txt"]);
    }

    #[test]
    fn stale_directory_result_does_not_replace_newer_request() {
        let mut state = PathCompletionState::default();
        let first = remote_path_completion_request("/root/a").unwrap();
        let (first_generation, first_parent) = state.request(first).unwrap();
        let second = remote_path_completion_request("/srv/b").unwrap();
        let (second_generation, second_parent) = state.request(second).unwrap();
        assert!(state.apply_entries(
            second_generation,
            &second_parent,
            vec![PathCompletionCandidate {
                name: "beta".to_string(),
                path: "/srv/beta".to_string(),
                is_directory: true,
            }],
        ));

        assert!(!state.apply_entries(first_generation, &first_parent, vec![candidate("a", true)],));
        assert_eq!(
            state
                .suggestions()
                .iter()
                .map(|entry| (entry.name.as_str(), entry.path.as_str()))
                .collect::<Vec<_>>(),
            vec![("beta", "/srv/beta")]
        );
        assert_eq!(state.parent_path(), Some("/srv"));
    }

    #[test]
    fn completion_does_not_select_a_loaded_or_cached_candidate() {
        let mut state = PathCompletionState::default();
        let (generation, parent_path) = state
            .request(remote_path_completion_request("/root/").unwrap())
            .unwrap();
        assert_eq!(state.acceptance_index("enter"), None);
        assert_eq!(state.acceptance_index("tab"), None);
        state.apply_entries(
            generation,
            &parent_path,
            vec![candidate("beta", true), candidate("alpha", true)],
        );
        assert_eq!(state.acceptance_index("enter"), None);
        assert_eq!(
            state
                .acceptance_index("tab")
                .and_then(|index| state.candidate(index))
                .map(|entry| entry.path.as_str()),
            Some("/root/alpha"),
        );
        state.move_selection(1);
        assert_eq!(
            state
                .acceptance_index("enter")
                .and_then(|index| state.candidate(index))
                .map(|entry| entry.path.as_str()),
            Some("/root/alpha"),
        );
        state.request(remote_path_completion_request("/root/b").unwrap());
        assert_eq!(state.acceptance_index("enter"), None);
        state.move_selection(-1);
        assert_eq!(
            state
                .acceptance_index("enter")
                .and_then(|index| state.candidate(index))
                .map(|entry| entry.path.as_str()),
            Some("/root/beta"),
        );
    }

    #[test]
    fn completion_selection_can_move_beyond_the_initial_viewport() {
        let mut state = PathCompletionState::default();
        let request = remote_path_completion_request("/root/").unwrap();
        let (generation, parent_path) = state.request(request).unwrap();
        let entries = (0..12)
            .map(|index| candidate(&format!("folder-{index:02}"), true))
            .collect();

        assert!(state.apply_entries(generation, &parent_path, entries));
        assert_eq!(state.suggestions().len(), 12);
        assert!(state.move_selection(1));
        assert_eq!(state.selected_index(), Some(0));
        for _ in 0..PATH_COMPLETION_VISIBLE_ROWS {
            assert!(state.move_selection(1));
        }

        assert_eq!(state.selected_index(), Some(PATH_COMPLETION_VISIBLE_ROWS));
    }
}
