#[cfg(test)]
use oxideterm_gpui_file_manager::PathCompletionState;
use oxideterm_gpui_sftp::remote_path_completion_request;
#[cfg(test)]
use oxideterm_local_files::PathCompletionCandidate;
use oxideterm_local_files::local_path_completion_request;

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
        const INITIAL_VIEWPORT_ROWS: usize = 8;
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
        for _ in 0..INITIAL_VIEWPORT_ROWS {
            assert!(state.move_selection(1));
        }

        assert_eq!(state.selected_index(), Some(INITIAL_VIEWPORT_ROWS));
    }
}
