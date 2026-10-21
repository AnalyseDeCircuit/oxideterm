use super::{SharedTerminalCommandHistory, TerminalAutosuggestCommandRecord, now_millis};
use parking_lot::Mutex;
use rusqlite::{Connection, params};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::Duration,
};
use zeroize::{Zeroize, Zeroizing};

const MAX_SAVED_RECORDS: usize = 10_000;

#[derive(Clone)]
pub(super) struct HistoryBinding {
    scope: String,
    store: Weak<StoreState>,
}

impl HistoryBinding {
    pub(super) fn enabled(&self) -> bool {
        self.store
            .upgrade()
            .is_some_and(|store| store.enabled.load(Ordering::Relaxed))
    }

    pub(super) fn changed(&self) {
        if let Some(store) = self.store.upgrade() {
            store.dirty.lock().insert(self.scope.clone());
            let _ = store.wake.try_send(());
        }
    }
}

struct ScopedHistory {
    label: String,
    history: SharedTerminalCommandHistory,
}

/// Scope creation and retirement share this lock so restore cannot recreate a retired entry.
#[derive(Default)]
struct HistoryRegistry {
    histories: HashMap<String, ScopedHistory>,
    removed_scope_prefixes: HashSet<String>,
}

struct StoreState {
    registry: Mutex<HistoryRegistry>,
    dirty: Mutex<HashSet<String>>,
    #[cfg(test)]
    restore_row_hook: Mutex<Option<Box<dyn FnOnce(&StoreState, &str) + Send>>>,
    clear_all: AtomicBool,
    loaded: AtomicBool,
    enabled: AtomicBool,
    retention_days: Mutex<u32>,
    stopping: AtomicBool,
    failed: AtomicBool,
    wake: mpsc::SyncSender<()>,
    updates: async_channel::Sender<()>,
}

/// The workspace owns the worker. History handles retain only weak persistence bindings.
pub struct TerminalHistoryStore {
    state: Arc<StoreState>,
    worker: Option<JoinHandle<()>>,
}

impl TerminalHistoryStore {
    pub fn new(path: PathBuf) -> std::io::Result<(Self, async_channel::Receiver<()>)> {
        let (wake, rx) = mpsc::sync_channel(1);
        let (updates, update_rx) = async_channel::bounded(1);
        let state = Arc::new(StoreState {
            registry: Mutex::new(HistoryRegistry::default()),
            dirty: Mutex::new(HashSet::new()),
            #[cfg(test)]
            restore_row_hook: Mutex::new(None),
            clear_all: AtomicBool::new(false),
            loaded: AtomicBool::new(false),
            enabled: AtomicBool::new(false),
            retention_days: Mutex::new(90),
            stopping: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            wake,
            updates,
        });
        let worker_state = state.clone();
        let worker = std::thread::Builder::new()
            .name("oxideterm-command-history".into())
            .spawn(move || run_worker(path, worker_state, rx))?;
        Ok((
            Self {
                state,
                worker: Some(worker),
            },
            update_rx,
        ))
    }

    pub fn configure(&self, enabled: bool, retention_days: u32) {
        self.state.enabled.store(enabled, Ordering::Relaxed);
        *self.state.retention_days.lock() = retention_days;
        let _ = self.state.wake.try_send(());
    }

    pub fn history(&self, scope: String, label: String) -> SharedTerminalCommandHistory {
        scoped_history(&self.state, scope, label)
    }

    pub fn histories(&self) -> Vec<(String, String, SharedTerminalCommandHistory)> {
        let mut histories = self
            .state
            .registry
            .lock()
            .histories
            .iter()
            .map(|(scope, entry)| (scope.clone(), entry.label.clone(), entry.history.clone()))
            .collect::<Vec<_>>();
        histories.sort_by(|left, right| left.0.cmp(&right.0));
        histories
    }

    pub fn clear(&self) {
        self.state.clear_all.store(true, Ordering::Relaxed);
        for (_, _, history) in self.histories() {
            history.clear();
        }
        let _ = self.state.wake.try_send(());
    }

    pub fn remove_scopes_with_prefix(&self, prefix: &str) {
        retire_scopes_with_prefix(&self.state, prefix);
    }

    pub fn failed(&self) -> bool {
        self.state.failed.load(Ordering::Relaxed)
    }
}

impl Drop for TerminalHistoryStore {
    fn drop(&mut self) {
        self.state.stopping.store(true, Ordering::Relaxed);
        let _ = self.state.wake.try_send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn scoped_history(
    store: &Arc<StoreState>,
    scope: String,
    label: String,
) -> SharedTerminalCommandHistory {
    let mut registry = store.registry.lock();
    scoped_history_locked(&mut registry, store, scope, label)
}

fn scoped_history_locked(
    registry: &mut HistoryRegistry,
    store: &Arc<StoreState>,
    scope: String,
    label: String,
) -> SharedTerminalCommandHistory {
    let entry = registry.histories.entry(scope.clone()).or_insert_with(|| {
        let history = SharedTerminalCommandHistory::default();
        {
            let mut state = history.state.lock();
            state.persistence = Some(HistoryBinding {
                scope,
                store: Arc::downgrade(store),
            });
            state.restored = store.loaded.load(Ordering::Relaxed);
        }
        ScopedHistory {
            label: label.clone(),
            history,
        }
    });
    if !label.is_empty() {
        entry.label = label;
    }
    entry.history.clone()
}

fn open_database(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|_| rusqlite::Error::InvalidPath(path.to_owned()))?;
    }
    // SQLite also uses this mode for its journal files. Existing files are restricted as well.
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)
            .and_then(|file| file.set_permissions(std::fs::Permissions::from_mode(0o600)))
            .map_err(|_| rusqlite::Error::InvalidPath(path.to_owned()))?;
    }
    let db = Connection::open(path)?;
    db.busy_timeout(Duration::from_secs(1))?;
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.pragma_update(None, "secure_delete", true)?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS commands(
        scope TEXT NOT NULL, id TEXT NOT NULL, label TEXT NOT NULL, command TEXT NOT NULL,
        started_at INTEGER NOT NULL, finished_at INTEGER NOT NULL, PRIMARY KEY(scope, id));
        CREATE INDEX IF NOT EXISTS commands_recency ON commands(finished_at DESC);",
    )?;
    Ok(db)
}

fn prune(db: &Connection, retention_days: u32) -> rusqlite::Result<Vec<(String, String)>> {
    let cutoff = now_millis().saturating_sub(u64::from(retention_days) * 86_400_000);
    db.prepare(
        "DELETE FROM commands WHERE (?1 != 0 AND finished_at < ?2) OR rowid IN (
        SELECT rowid FROM commands ORDER BY finished_at DESC, id DESC LIMIT -1 OFFSET ?3)
        RETURNING scope, id",
    )?
    .query_map(params![retention_days, cutoff, MAX_SAVED_RECORDS], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?
    .collect()
}

fn forget_pruned(store: &StoreState, removed: Vec<(String, String)>) {
    let mut by_scope: HashMap<String, HashSet<String>> = HashMap::new();
    for (scope, id) in removed {
        by_scope.entry(scope).or_default().insert(id);
    }
    for (scope, ids) in by_scope {
        let history = store
            .registry
            .lock()
            .histories
            .get(&scope)
            .map(|entry| entry.history.clone());
        if let Some(history) = history {
            let mut state = history.state.lock();
            state.records.retain_mut(|record| {
                if ids.contains(&record.command_id) {
                    record.command.zeroize();
                    false
                } else {
                    true
                }
            });
            state.persistent_ids.retain(|id| !ids.contains(id));
        }
    }
}

fn clear_scope_history(history: &SharedTerminalCommandHistory) {
    let mut state = history.state.lock();
    for record in &mut state.records {
        record.command.zeroize();
    }
    state.records.clear();
    state.persistent_ids.clear();
    for mut command in state.removed_before_restore.drain() {
        command.zeroize();
    }
    state.restore_cleared = true;
}

fn retire_scopes_with_prefix(store: &StoreState, prefix: &str) {
    let histories = {
        let mut registry = store.registry.lock();
        registry.removed_scope_prefixes.insert(prefix.to_string());
        let mut histories = Vec::new();
        registry.histories.retain(|scope, entry| {
            if scope.starts_with(prefix) {
                histories.push(entry.history.clone());
                false
            } else {
                true
            }
        });
        histories
    };
    for history in histories {
        clear_scope_history(&history);
    }
    store
        .dirty
        .lock()
        .retain(|scope| !scope.starts_with(prefix));
    let _ = store.wake.try_send(());
}

fn restore(db: &Connection, store: &Arc<StoreState>) -> rusqlite::Result<()> {
    let mut statement = db.prepare(
        "SELECT scope, id, label, command, started_at, finished_at
        FROM commands ORDER BY finished_at, id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(2)?,
            TerminalAutosuggestCommandRecord {
                command_id: row.get(1)?,
                command: row.get(3)?,
                started_at: row.get(4)?,
                finished_at: row.get(5)?,
            },
        ))
    })?;
    for row in rows {
        let (scope, label, mut record) = row?;
        #[cfg(test)]
        if let Some(hook) = store.restore_row_hook.lock().take() {
            hook(store, &scope);
        }
        let history = {
            let mut registry = store.registry.lock();
            if registry
                .removed_scope_prefixes
                .iter()
                .any(|prefix| scope.starts_with(prefix))
            {
                None
            } else {
                Some(scoped_history_locked(&mut registry, store, scope, label))
            }
        };
        let Some(history) = history else {
            record.command.zeroize();
            continue;
        };
        let mut state = history.state.lock();
        if !store.clear_all.load(Ordering::Relaxed)
            && !state.restore_cleared
            && !state.removed_before_restore.contains(&record.command)
            && !state.persistent_ids.contains(&record.command_id)
        {
            state.persistent_ids.insert(record.command_id.clone());
            state.records.push(record);
        } else {
            record.command.zeroize();
        }
    }
    store.loaded.store(true, Ordering::Relaxed);
    let histories = store
        .registry
        .lock()
        .histories
        .values()
        .map(|entry| entry.history.clone())
        .collect::<Vec<_>>();
    for history in histories {
        let mut state = history.state.lock();
        state.records.sort_by_key(|record| record.finished_at);
        super::trim_autosuggest_records(&mut state.records);
        state.restored = true;
        for mut command in state.removed_before_restore.drain() {
            command.zeroize();
        }
    }
    Ok(())
}

fn save_dirty(db: &mut Connection, store: &StoreState) -> rusqlite::Result<()> {
    let scopes = std::mem::take(&mut *store.dirty.lock());
    let removed_scope_prefixes = {
        let mut registry = store.registry.lock();
        std::mem::take(&mut registry.removed_scope_prefixes)
    };
    if scopes.is_empty() && removed_scope_prefixes.is_empty() {
        return Ok(());
    }
    let result = (|| {
        let tx = db.transaction()?;
        for prefix in &removed_scope_prefixes {
            // Match literal prefixes, including scopes not loaded into memory yet.
            tx.execute(
                "DELETE FROM commands WHERE substr(scope, 1, length(?1)) = ?1",
                [prefix],
            )?;
        }
        for scope in &scopes {
            let (label, history) = {
                let registry = store.registry.lock();
                let Some(entry) = registry.histories.get(scope) else {
                    continue;
                };
                (entry.label.clone(), entry.history.clone())
            };
            let records = {
                let state = history.state.lock();
                state
                    .records
                    .iter()
                    .filter(|record| state.persistent_ids.contains(&record.command_id))
                    .map(|record| {
                        (
                            record.command_id.clone(),
                            Zeroizing::new(record.command.clone()),
                            record.started_at,
                            record.finished_at,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            // Compare IDs in the worker so unchanged history is not rewritten for each command.
            let existing = tx
                .prepare("SELECT id FROM commands WHERE scope = ?1")?
                .query_map([scope], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<HashSet<_>>>()?;
            let retained = records
                .iter()
                .map(|record| record.0.as_str())
                .collect::<HashSet<_>>();
            for id in existing.iter().filter(|id| !retained.contains(id.as_str())) {
                tx.execute(
                    "DELETE FROM commands WHERE scope = ?1 AND id = ?2",
                    params![scope, id],
                )?;
            }
            for (id, command, started_at, finished_at) in &records {
                if !existing.contains(id) {
                    tx.execute(
                        "INSERT INTO commands VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![scope, id, label, command.as_str(), started_at, finished_at],
                    )?;
                }
            }
        }
        let removed = prune(&tx, *store.retention_days.lock())?;
        tx.commit()?;
        forget_pruned(store, removed);
        Ok(())
    })();
    if result.is_err() {
        store.dirty.lock().extend(scopes);
        store
            .registry
            .lock()
            .removed_scope_prefixes
            .extend(removed_scope_prefixes);
    }
    result
}

fn run_worker(path: PathBuf, store: Arc<StoreState>, wake: mpsc::Receiver<()>) {
    let mut database = None;
    while wake.recv().is_ok() {
        // A single wake token coalesces changes without queuing copies of command text.
        if !store.stopping.load(Ordering::Relaxed) {
            let _ = wake.recv_timeout(Duration::from_millis(50));
        }
        let should_open = store.enabled.load(Ordering::Relaxed)
            || path.is_file()
            || !store.dirty.lock().is_empty()
            || !store.registry.lock().removed_scope_prefixes.is_empty()
            || store.clear_all.load(Ordering::Relaxed);
        if database.is_none() && !should_open {
            if store.stopping.load(Ordering::Relaxed) {
                break;
            }
            continue;
        }
        let result = (|| -> rusqlite::Result<()> {
            if database.is_none() && should_open {
                let db = open_database(&path)?;
                let removed = prune(&db, *store.retention_days.lock())?;
                forget_pruned(&store, removed);
                restore(&db, &store)?;
                database = Some(db);
            }
            if let Some(db) = database.as_mut() {
                if store.clear_all.swap(false, Ordering::Relaxed) {
                    if let Err(error) = db.execute("DELETE FROM commands", []) {
                        store.clear_all.store(true, Ordering::Relaxed);
                        return Err(error);
                    }
                }
                save_dirty(db, &store)?;
                let removed = prune(db, *store.retention_days.lock())?;
                forget_pruned(&store, removed);
            }
            Ok(())
        })();
        // Database errors can contain SQL values; only an operation status crosses into the UI.
        store.failed.store(result.is_err(), Ordering::Relaxed);
        let _ = store.updates.try_send(());
        if store.stopping.load(Ordering::Relaxed) {
            // A final mutation may arrive while the current transaction is in flight.
            if result.is_err()
                || (store.dirty.lock().is_empty()
                    && store.registry.lock().removed_scope_prefixes.is_empty()
                    && !store.clear_all.load(Ordering::Relaxed))
            {
                break;
            }
            let _ = store.wake.try_send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detached_store() -> TerminalHistoryStore {
        let (wake, _) = mpsc::sync_channel(1);
        let (updates, _) = async_channel::bounded(1);
        TerminalHistoryStore {
            state: Arc::new(StoreState {
                registry: Mutex::new(HistoryRegistry::default()),
                dirty: Mutex::new(HashSet::new()),
                restore_row_hook: Mutex::new(None),
                clear_all: AtomicBool::new(false),
                loaded: AtomicBool::new(false),
                enabled: AtomicBool::new(true),
                retention_days: Mutex::new(90),
                stopping: AtomicBool::new(false),
                failed: AtomicBool::new(false),
                wake,
                updates,
            }),
            worker: None,
        }
    }

    async fn updated(rx: &async_channel::Receiver<()>) {
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
    }

    #[test]
    fn restore_does_not_resurrect_scopes_retired_while_rows_are_loading() {
        let directory = tempfile::tempdir().unwrap();
        let mut db = open_database(&directory.path().join("history.sqlite")).unwrap();
        let timestamp = now_millis();
        for scope in ["retired/old", "retired/new", "retired-other"] {
            db.execute(
                "INSERT INTO commands VALUES (?, ?, ?, ?, ?, ?)",
                params![
                    scope,
                    "command-id",
                    "Endpoint",
                    "saved command",
                    timestamp,
                    timestamp
                ],
            )
            .unwrap();
        }
        let store = Arc::new(detached_store());
        *store.state.restore_row_hook.lock() = Some(Box::new(|store, _| {
            retire_scopes_with_prefix(store, "retired/");
        }));

        restore(&db, &store.state).unwrap();
        save_dirty(&mut db, &store.state).unwrap();

        assert_eq!(
            store
                .histories()
                .into_iter()
                .flat_map(|(scope, _, history)| {
                    history
                        .records()
                        .into_iter()
                        .map(move |record| (scope.clone(), record.command))
                })
                .collect::<Vec<_>>(),
            [("retired-other".into(), "saved command".into())]
        );
        assert_eq!(
            db.prepare("SELECT scope, command FROM commands ORDER BY scope")
                .unwrap()
                .query_map([], |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?
                )))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap(),
            [("retired-other".into(), "saved command".into())]
        );
    }

    #[tokio::test]
    async fn history_survives_restart_verbatim_and_removal_persists() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.sqlite");
        let (store, updates) = TerminalHistoryStore::new(path.clone()).unwrap();
        let first = store.history("saved-a".into(), "Production".into());
        first.record("before enabling");
        store.configure(false, 90);
        assert!(!path.exists());
        store.configure(true, 90);
        updated(&updates).await;
        let command = "curl  -H 'Authorization: Bearer test-secret' 'https://example.com/a b'";
        first.record(command);
        first.set_paused(true);
        first.record("paused command");
        first.set_paused(false);
        let second = store.history("saved-b".into(), "Staging".into());
        second.record("printf 'two  spaces'\nprintf next");
        drop(store);

        let db = Connection::open(&path).unwrap();
        let rows = db
            .prepare("SELECT scope, command FROM commands ORDER BY scope")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                ("saved-a".into(), command.into()),
                ("saved-b".into(), "printf 'two  spaces'\nprintf next".into())
            ]
        );
        drop(db);

        let (store, updates) = TerminalHistoryStore::new(path.clone()).unwrap();
        let first = store.history("saved-a".into(), "Renamed".into());
        store.configure(false, 90);
        updated(&updates).await;
        assert_eq!(
            first
                .records()
                .iter()
                .map(|record| record.command.as_str())
                .collect::<Vec<_>>(),
            [command]
        );
        first.record("memory only while disabled");
        assert!(first.remove(command));
        drop(store);

        let (store, updates) = TerminalHistoryStore::new(path).unwrap();
        store.configure(true, 90);
        updated(&updates).await;
        assert_eq!(
            store.history("saved-a".into(), "Renamed".into()).records(),
            vec![]
        );
        assert_eq!(
            store
                .history("saved-b".into(), "Staging".into())
                .records()
                .iter()
                .map(|record| record.command.as_str())
                .collect::<Vec<_>>(),
            ["printf 'two  spaces'\nprintf next"]
        );
    }

    #[tokio::test]
    async fn clear_while_disabled_removes_history_not_yet_loaded() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.sqlite");
        let (store, updates) = TerminalHistoryStore::new(path.clone()).unwrap();
        store.configure(true, 90);
        updated(&updates).await;
        store
            .history("saved-a".into(), "Host".into())
            .record("old command");
        drop(store);
        let (store, _) = TerminalHistoryStore::new(path.clone()).unwrap();
        store.clear();
        drop(store);
        let (store, updates) = TerminalHistoryStore::new(path).unwrap();
        store.configure(true, 90);
        updated(&updates).await;
        assert_eq!(
            store
                .histories()
                .into_iter()
                .flat_map(|(_, _, history)| history.records())
                .collect::<Vec<_>>(),
            vec![]
        );
    }

    #[tokio::test]
    async fn scope_removal_scrubs_memory_and_persists_without_affecting_other_scopes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.sqlite");
        let (store, updates) = TerminalHistoryStore::new(path.clone()).unwrap();
        store.configure(true, 90);
        updated(&updates).await;
        let replaced = store.history("saved_%/old".into(), "Old endpoint".into());
        replaced.record("replaced command");
        let current = store.history("saved_%/new".into(), "Current endpoint".into());
        current.record("current command");
        let retained = store.history("saved-xx/old".into(), "Retained endpoint".into());
        retained.record("retained command");
        updated(&updates).await;

        store.remove_scopes_with_prefix("saved_%/");

        assert!(replaced.records().is_empty());
        assert!(current.records().is_empty());
        drop(store);

        let (store, updates) = TerminalHistoryStore::new(path).unwrap();
        store.configure(true, 90);
        updated(&updates).await;
        assert_eq!(
            store
                .histories()
                .into_iter()
                .flat_map(|(scope, _, history)| {
                    history
                        .records()
                        .into_iter()
                        .map(move |record| (scope.clone(), record.command))
                })
                .collect::<Vec<_>>(),
            [("saved-xx/old".into(), "retained command".into())]
        );
    }

    #[tokio::test]
    async fn retention_prunes_saved_commands_and_database_errors_are_observable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.sqlite");
        let db = open_database(&path).unwrap();
        db.execute(
            "INSERT INTO commands VALUES ('host', 'old', 'Host', 'expired command', 1, 1)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO commands VALUES ('host', 'new', 'Host', 'recent command', ?1, ?1)",
            [now_millis()],
        )
        .unwrap();
        drop(db);
        let (store, updates) = TerminalHistoryStore::new(path).unwrap();
        store.configure(true, 30);
        updated(&updates).await;
        assert_eq!(
            store
                .history("host".into(), "Host".into())
                .records()
                .iter()
                .map(|record| record.command.as_str())
                .collect::<Vec<_>>(),
            ["recent command"]
        );
        drop(store);

        let invalid_path = directory.path().join("not-a-database");
        std::fs::write(&invalid_path, b"invalid SQLite content").unwrap();
        let (store, updates) = TerminalHistoryStore::new(invalid_path).unwrap();
        let history = store.history("host".into(), "Host".into());
        store.configure(true, 90);
        updated(&updates).await;
        assert!(store.failed());
        history.record("available in memory");
        assert_eq!(
            history
                .records()
                .iter()
                .map(|record| record.command.as_str())
                .collect::<Vec<_>>(),
            ["available in memory"]
        );
    }
}
