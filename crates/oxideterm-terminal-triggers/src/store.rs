// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use oxideterm_atomic_file::durable_write_with_before_replace;

use crate::{
    TERMINAL_TRIGGERS_SCHEMA_VERSION, TerminalTriggerError, TerminalTriggersSnapshot,
    validate_snapshot,
};

const TERMINAL_TRIGGERS_FILENAME: &str = "terminal-triggers.json";
const MAX_TERMINAL_TRIGGERS_FILE_BYTES: u64 = 512 * 1024;
static TERMINAL_TRIGGER_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_ATOMIC_REPLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn terminal_triggers_path(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join(TERMINAL_TRIGGERS_FILENAME)
}

pub fn default_snapshot() -> TerminalTriggersSnapshot {
    TerminalTriggersSnapshot {
        version: TERMINAL_TRIGGERS_SCHEMA_VERSION,
        triggers: Vec::new(),
        updated_at: now_ms(),
    }
}

pub fn load_snapshot(
    settings_path: &Path,
) -> Result<TerminalTriggersSnapshot, TerminalTriggerError> {
    let path = terminal_triggers_path(settings_path);
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(default_snapshot()),
        Err(error) => return Err(error.into()),
    };
    if metadata.len() > MAX_TERMINAL_TRIGGERS_FILE_BYTES {
        return Err(TerminalTriggerError::FileTooLarge);
    }
    let contents = fs::read_to_string(path)?;
    if contents.trim().is_empty() {
        return Ok(default_snapshot());
    }
    let snapshot = serde_json::from_str::<TerminalTriggersSnapshot>(&contents)?;
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

pub fn save_snapshot(
    settings_path: &Path,
    snapshot: &TerminalTriggersSnapshot,
) -> Result<(), TerminalTriggerError> {
    // Validation compiles every matcher before any configuration reaches disk.
    validate_snapshot(snapshot)?;
    let path = terminal_triggers_path(settings_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(snapshot)?;
    if json.len() as u64 > MAX_TERMINAL_TRIGGERS_FILE_BYTES {
        return Err(TerminalTriggerError::FileTooLarge);
    }
    durable_write_with_before_replace(&path, &json, fail_before_atomic_replace_for_tests)?;
    Ok(())
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub fn new_trigger_id() -> String {
    format!(
        "trigger-{}-{}",
        now_ms(),
        TERMINAL_TRIGGER_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
fn fail_before_atomic_replace_for_tests() -> io::Result<()> {
    FAIL_NEXT_ATOMIC_REPLACE.with(|fail| {
        if fail.replace(false) {
            Err(io::Error::other("injected failure before atomic replace"))
        } else {
            Ok(())
        }
    })
}

#[cfg(not(test))]
fn fail_before_atomic_replace_for_tests() -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
fn inject_atomic_replace_failure() {
    FAIL_NEXT_ATOMIC_REPLACE.with(|fail| fail.set(true));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LocalProcessSpec, TerminalTrigger, TerminalTriggerAction, TerminalTriggerDispatch,
        TerminalTriggerMatch, TerminalTriggerMatchMode, TerminalTriggerScope,
        TerminalTriggerTiming,
    };

    fn settings_path(directory: &Path) -> PathBuf {
        directory.join("settings.json")
    }

    fn trigger() -> TerminalTrigger {
        TerminalTrigger {
            id: "trigger-1".to_string(),
            name: "Ready".to_string(),
            description: Some("Respond when ready appears".to_string()),
            enabled: true,
            matcher: TerminalTriggerMatch {
                pattern: "READY".to_string(),
                mode: TerminalTriggerMatchMode::Literal,
                case_sensitive: true,
                whole_word: true,
            },
            action: TerminalTriggerAction::SendText {
                text: "continue".to_string(),
                append_enter: true,
            },
            timing: TerminalTriggerTiming {
                dispatch: TerminalTriggerDispatch::Immediate,
                delay_ms: 0,
                cooldown_ms: 500,
            },
            scope: TerminalTriggerScope::AllTerminals,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn snapshot() -> TerminalTriggersSnapshot {
        TerminalTriggersSnapshot {
            version: TERMINAL_TRIGGERS_SCHEMA_VERSION,
            triggers: vec![trigger()],
            updated_at: 1,
        }
    }

    #[test]
    fn missing_and_blank_files_load_as_empty_snapshots() {
        let directory = tempfile::tempdir().unwrap();
        let settings = settings_path(directory.path());

        assert!(load_snapshot(&settings).unwrap().triggers.is_empty());
        fs::write(terminal_triggers_path(&settings), " \n").unwrap();
        assert!(load_snapshot(&settings).unwrap().triggers.is_empty());
    }

    #[test]
    fn saves_and_loads_a_versioned_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let settings = settings_path(directory.path());
        let snapshot = snapshot();

        save_snapshot(&settings, &snapshot).unwrap();

        assert_eq!(load_snapshot(&settings).unwrap(), snapshot);
    }

    #[test]
    fn creates_a_missing_settings_directory() {
        let directory = tempfile::tempdir().unwrap();
        let settings = directory.path().join("nested").join("settings.json");

        save_snapshot(&settings, &snapshot()).unwrap();

        assert!(terminal_triggers_path(&settings).is_file());
    }

    #[test]
    fn corrupt_and_unsupported_files_are_not_rewritten() {
        let directory = tempfile::tempdir().unwrap();
        let settings = settings_path(directory.path());
        let path = terminal_triggers_path(&settings);
        let corrupt = b"{not-json";
        fs::write(&path, corrupt).unwrap();

        assert!(matches!(
            load_snapshot(&settings),
            Err(TerminalTriggerError::Parse(_))
        ));
        assert_eq!(fs::read(&path).unwrap(), corrupt);

        let unsupported = TerminalTriggersSnapshot {
            version: TERMINAL_TRIGGERS_SCHEMA_VERSION + 1,
            triggers: Vec::new(),
            updated_at: 1,
        };
        fs::write(&path, serde_json::to_vec(&unsupported).unwrap()).unwrap();
        assert!(matches!(
            load_snapshot(&settings),
            Err(TerminalTriggerError::UnsupportedSchema(_))
        ));
    }

    #[test]
    fn rejects_oversized_input_and_serialized_output() {
        let directory = tempfile::tempdir().unwrap();
        let settings = settings_path(directory.path());
        let path = terminal_triggers_path(&settings);
        fs::write(
            &path,
            vec![b'x'; MAX_TERMINAL_TRIGGERS_FILE_BYTES as usize + 1],
        )
        .unwrap();
        assert!(matches!(
            load_snapshot(&settings),
            Err(TerminalTriggerError::FileTooLarge)
        ));

        let mut oversized = snapshot();
        oversized.triggers[0].action = TerminalTriggerAction::LaunchLocalProcess {
            process: LocalProcessSpec::DirectProgram {
                executable: "program".to_string(),
                arguments: vec!["x".repeat(8_192); 64],
                working_directory: None,
            },
        };
        assert!(matches!(
            save_snapshot(&settings, &oversized),
            Err(TerminalTriggerError::FileTooLarge)
        ));
    }

    #[test]
    fn failed_atomic_replace_preserves_existing_file() {
        let directory = tempfile::tempdir().unwrap();
        let settings = settings_path(directory.path());
        let path = terminal_triggers_path(&settings);
        let original = snapshot();
        save_snapshot(&settings, &original).unwrap();
        let original_bytes = fs::read(&path).unwrap();
        let mut replacement = original;
        replacement.updated_at = 2;
        inject_atomic_replace_failure();

        assert!(save_snapshot(&settings, &replacement).is_err());
        assert_eq!(fs::read(path).unwrap(), original_bytes);
    }
}
