use zeroize::Zeroizing;

/// Association data and endpoint are encrypted together in SQLite.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct RecordingDetails {
    pub session_id: String,
    pub transport_id: Option<String>,
    pub consumer_id: Option<String>,
    pub operation_id: Option<String>,
    pub endpoint: Option<Zeroizing<String>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingState {
    InProgress,
    Finished,
    Interrupted,
    Gaps,
    Expired,
}

pub struct RecordingSummary {
    pub id: String,
    pub instance_id: String,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub state: RecordingState,
    pub has_expired_content: bool,
    pub details: RecordingDetails,
}

pub struct RecordingListPage {
    pub recordings: Vec<RecordingSummary>,
    pub next_cursor: Option<(i64, String)>,
}

pub enum RecordingFrameKind<'a> {
    Output(&'a [u8]),
    Resize { columns: u16, rows: u16 },
    Gap { lost_bytes: Option<u64> },
}

pub struct RecordingFrame<'a> {
    pub occurred_at_ms: i64,
    pub kind: RecordingFrameKind<'a>,
}

pub enum StoredRecordingFrameKind {
    Output(Zeroizing<Vec<u8>>),
    Resize { columns: u16, rows: u16 },
    Gap { lost_bytes: Option<u64> },
}

pub struct StoredRecordingFrame {
    pub occurred_at_ms: i64,
    pub kind: StoredRecordingFrameKind,
}

pub struct RecordingChunk {
    pub sequence: i64,
    pub frames: Vec<StoredRecordingFrame>,
}

pub struct RecordingPage {
    pub chunks: Vec<RecordingChunk>,
    pub expired_sequences: Vec<i64>,
    pub state: RecordingState,
    pub ended_at_ms: Option<i64>,
    pub next_cursor: Option<i64>,
}
