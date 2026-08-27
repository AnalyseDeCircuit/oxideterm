pub const DEFAULT_PIXEL_LIMIT: u32 = 16_777_216;
pub const DEFAULT_STORAGE_LIMIT_MB: u32 = 16;

/// Per-session authority for Kitty's local-file transmission modes.
#[derive(Clone)]
pub struct KittyFileTransmissionControl {
    inner: Arc<KittyFileTransmissionControlInner>,
}

struct KittyFileTransmissionControlInner {
    state: Mutex<KittyFileTransmissionState>,
}

enum KittyFileTransmissionState {
    AwaitingDecision { request_pending: bool },
    Denied,
    Trusted(KittyFileTransmissionSandbox),
}

struct KittyFileTransmissionSandbox {
    directory: TempDir,
    session_token: Zeroizing<String>,
}

impl KittyFileTransmissionControl {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(KittyFileTransmissionControlInner {
                state: Mutex::new(KittyFileTransmissionState::AwaitingDecision {
                    request_pending: false,
                }),
            }),
        }
    }

    pub fn take_authorization_request(&self) -> bool {
        let Ok(mut state) = self.inner.state.lock() else {
            return false;
        };
        let KittyFileTransmissionState::AwaitingDecision { request_pending } = &mut *state else {
            return false;
        };
        std::mem::take(request_pending)
    }

    pub fn authorize_for_session(&self) -> std::io::Result<PathBuf> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| std::io::Error::other("Kitty file transmission state is unavailable"))?;
        match &*state {
            KittyFileTransmissionState::Trusted(sandbox) => {
                return Ok(sandbox.directory.path().to_path_buf());
            }
            KittyFileTransmissionState::Denied => {
                // A denial is final for this session so stale UI actions cannot
                // revive a capability after the user rejected the request.
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Kitty file transmission was denied for this session",
                ));
            }
            KittyFileTransmissionState::AwaitingDecision { .. } => {}
        }

        // The random directory name is the session capability. It is never
        // persisted or logged, and TempDir removes the owned root on drop.
        let session_token = Zeroizing::new(Uuid::new_v4().simple().to_string());
        let directory_prefix =
            Zeroizing::new(format!("oxideterm-kitty-{}-", session_token.as_str()));
        let directory = tempfile::Builder::new()
            .prefix(directory_prefix.as_str())
            .tempdir()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
        }
        let path = directory.path().to_path_buf();
        *state = KittyFileTransmissionState::Trusted(KittyFileTransmissionSandbox {
            directory,
            session_token,
        });
        Ok(path)
    }

    pub fn deny_for_session(&self) {
        if let Ok(mut state) = self.inner.state.lock() {
            *state = KittyFileTransmissionState::Denied;
        }
    }

    fn note_authorization_request(&self) {
        let Ok(mut state) = self.inner.state.lock() else {
            return;
        };
        if let KittyFileTransmissionState::AwaitingDecision { request_pending } = &mut *state {
            *request_pending = true;
        }
    }

    fn authorized_root(&self) -> Option<PathBuf> {
        let state = self.inner.state.lock().ok()?;
        match &*state {
            KittyFileTransmissionState::Trusted(sandbox) => sandbox
                .directory
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(sandbox.session_token.as_str()))
                .then(|| sandbox.directory.path().to_path_buf()),
            KittyFileTransmissionState::AwaitingDecision { .. }
            | KittyFileTransmissionState::Denied => None,
        }
    }
}

impl Default for KittyFileTransmissionControl {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for KittyFileTransmissionControl {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let decision = self
            .inner
            .state
            .lock()
            .map(|state| match &*state {
                KittyFileTransmissionState::AwaitingDecision { .. } => "awaiting-decision",
                KittyFileTransmissionState::Denied => "denied",
                KittyFileTransmissionState::Trusted(_) => "trusted",
            })
            .unwrap_or("unavailable");
        formatter
            .debug_struct("KittyFileTransmissionControl")
            .field("decision", &decision)
            .field("sandbox", &"[redacted session capability]")
            .finish()
    }
}

impl PartialEq for KittyFileTransmissionControl {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for KittyFileTransmissionControl {}

#[derive(Clone, Debug)]
pub struct GraphicsOptions {
    pub enabled: bool,
    pub sixel: bool,
    pub iterm2_inline: bool,
    pub kitty: bool,
    pub pixel_limit: u32,
    pub storage_limit_mb: u32,
    pub show_placeholder: bool,
    pub kitty_file_transmission: KittyFileTransmissionControl,
}

impl PartialEq for GraphicsOptions {
    fn eq(&self, other: &Self) -> bool {
        // The session capability is runtime state, not a graphics preference.
        self.enabled == other.enabled
            && self.sixel == other.sixel
            && self.iterm2_inline == other.iterm2_inline
            && self.kitty == other.kitty
            && self.pixel_limit == other.pixel_limit
            && self.storage_limit_mb == other.storage_limit_mb
            && self.show_placeholder == other.show_placeholder
    }
}

impl Eq for GraphicsOptions {}

impl Default for GraphicsOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            sixel: true,
            iterm2_inline: true,
            kitty: true,
            pixel_limit: DEFAULT_PIXEL_LIMIT,
            storage_limit_mb: DEFAULT_STORAGE_LIMIT_MB,
            show_placeholder: true,
            kitty_file_transmission: KittyFileTransmissionControl::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerminalImageId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalImageProtocol {
    Iterm2,
    Kitty,
    Sixel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalImageData {
    pub id: TerminalImageId,
    pub protocol: TerminalImageProtocol,
    pub version: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
    pub frames: Vec<TerminalImageFrame>,
    pub animation: TerminalImageAnimationState,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalImageFrame {
    pub rgba: Arc<[u8]>,
    pub delay_ms_numerator: u32,
    pub delay_ms_denominator: u32,
    pub gapless: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalImageAnimationState {
    pub running: bool,
    pub loading: bool,
    pub current_frame: usize,
    pub loop_limit: Option<u32>,
}

impl Default for TerminalImageAnimationState {
    fn default() -> Self {
        Self {
            running: false,
            loading: false,
            current_frame: 0,
            loop_limit: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalImagePlacement {
    pub id: TerminalImageId,
    pub protocol: TerminalImageProtocol,
    pub line: i32,
    pub row: usize,
    pub col: usize,
    pub cols: usize,
    pub rows: usize,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub z_index: i32,
    pub placeholder: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalGraphicsEvent {
    ImageReady(TerminalImageData),
    ImageUpdated(TerminalImageData),
    Place(TerminalImagePlacement),
    Delete { id: Option<TerminalImageId> },
    Respond(Vec<u8>),
    Error(String),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GraphicsAdvance {
    pub terminal_bytes: Vec<u8>,
    pub events: Vec<TerminalGraphicsEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalGraphicsSegment {
    Terminal(Vec<u8>),
    Event(TerminalGraphicsEvent),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphicsCursor {
    pub line: i32,
    pub row: usize,
    pub col: usize,
    pub cols: usize,
    pub rows: usize,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl GraphicsCursor {
    pub fn image_cells(self, pixel_width: u32, pixel_height: u32) -> (usize, usize) {
        let cell_width = u32::from(self.cell_width).max(1);
        let cell_height = u32::from(self.cell_height).max(1);
        let cols = pixel_width.div_ceil(cell_width).max(1) as usize;
        let rows = pixel_height.div_ceil(cell_height).max(1) as usize;
        (cols.min(self.cols.max(1)), rows.min(self.rows.max(1)))
    }
}

#[derive(Debug, Error)]
pub enum GraphicsError {
    #[error("image is larger than the configured pixel limit")]
    PixelLimitExceeded,
    #[error("invalid base64 image payload")]
    InvalidBase64,
    #[error("unsupported image payload")]
    UnsupportedImage,
    #[error("Kitty local file transmission is disabled")]
    LocalFileTransmissionDisabled,
    #[error("Kitty local file transmission path is not allowed")]
    InvalidLocalFileTransmissionPath,
    #[error("Kitty local file transmission could not read the approved file")]
    LocalFileTransmissionAccessFailed,
    #[error("image payload is larger than the configured storage limit")]
    StorageLimitExceeded,
    #[error("{0}")]
    Decode(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ParserState {
    Ground,
    Esc,
    Osc(Vec<u8>),
    OscEsc(Vec<u8>),
    Dcs(Vec<u8>),
    DcsEsc(Vec<u8>),
    Apc(Vec<u8>),
    ApcEsc(Vec<u8>),
}

pub struct GraphicsIngress {
    options: GraphicsOptions,
    state: ParserState,
    next_image_id: u64,
    kitty_chunks: HashMap<u64, KittyChunkAssembly>,
    kitty_images: HashMap<TerminalImageId, TerminalImageData>,
}

struct KittyChunkAssembly {
    params: HashMap<String, String>,
    encoded: Vec<u8>,
}
