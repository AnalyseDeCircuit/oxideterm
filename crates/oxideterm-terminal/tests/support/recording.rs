use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

pub struct AuditTestKeys;

// These multi-megabyte fixtures share the CI runner's CPU and disk budget.
// Acquire before setup so waiting for another fixture does not consume its deadlines.
pub static RECORDING_PRESSURE_LOCK: Mutex<()> = Mutex::new(());

impl oxideterm_audit::AuditKeyProvider for AuditTestKeys {
    fn load(&self, _: &str) -> Result<zeroize::Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
        Ok(zeroize::Zeroizing::new(vec![7; 32]))
    }
    fn create(&self, id: &str) -> Result<zeroize::Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
        self.load(id)
    }
}

pub struct PausedFiles {
    pub first: AtomicBool,
    pub entered: mpsc::Sender<()>,
    pub resume: Mutex<mpsc::Receiver<()>>,
}
impl oxideterm_audit::RecordingFiles for PausedFiles {
    fn write_chunk(
        &self,
        path: &std::path::Path,
        header: &[u8],
        encrypted: &[u8],
    ) -> std::io::Result<()> {
        if !self.first.swap(true, Ordering::AcqRel) {
            self.entered
                .send(())
                .map_err(|_| std::io::ErrorKind::BrokenPipe)?;
            self.resume
                .lock()
                .unwrap()
                .recv()
                .map_err(|_| std::io::ErrorKind::BrokenPipe)?;
        }
        oxideterm_audit::RecordingFiles::write_chunk(
            &oxideterm_audit::DurableRecordingFiles,
            path,
            header,
            encrypted,
        )
    }
}

pub fn read_finished(
    runtime: &tokio::runtime::Runtime,
    client: &oxideterm_audit::AuditClient,
) -> (
    oxideterm_audit::RecordingState,
    Vec<oxideterm_audit::StoredRecordingFrameKind>,
) {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(30);
    let recording =
        loop {
            let mut list = runtime.block_on(client.list_recordings(None, 10)).unwrap();
            if list.recordings.first().is_some_and(|recording| {
                recording.state != oxideterm_audit::RecordingState::InProgress
            }) {
                break list.recordings.remove(0);
            }
            assert!(Instant::now() < deadline, "recording did not settle");
            std::thread::sleep(Duration::from_millis(5));
        };
    let mut frames = Vec::new();
    let mut cursor = None;
    loop {
        let page = runtime
            .block_on(client.read_recording_page(recording.id.clone(), cursor, None, 16))
            .unwrap();
        frames.extend(
            page.chunks
                .into_iter()
                .flat_map(|chunk| chunk.frames)
                .map(|frame| frame.kind),
        );
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    (recording.state, frames)
}
