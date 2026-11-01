pub const DEFAULT_PIXEL_LIMIT: u32 = 16_777_216;
pub const DEFAULT_STORAGE_LIMIT_MB: u32 = 256;
// Growing the cache must not grow protocol buffers or codec workspaces with it.
const MAX_TRANSFER_MB: u32 = 64;
const MAX_DECODE_BYTES: usize = 64 * 1024 * 1024;
const MAX_DECODE_WORKSPACE_BYTES: usize = 256 * 1024 * 1024;
const MAX_GRAPHICS_METADATA_MB: u32 = 16;

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
    Trusted,
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

    pub fn authorize_for_session(&self) -> std::io::Result<()> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| std::io::Error::other("Kitty file authorization is unavailable"))?;
        *state = KittyFileTransmissionState::Trusted;
        Ok(())
    }

    pub fn deny_for_session(&self) {
        if let Ok(mut state) = self.inner.state.lock() {
            *state = KittyFileTransmissionState::Denied;
        }
    }

    pub fn revoke(&self) {
        if let Ok(mut state) = self.inner.state.lock() {
            *state = KittyFileTransmissionState::AwaitingDecision {
                request_pending: false,
            };
        }
    }

    pub fn request_authorization(&self) {
        self.note_authorization_request();
    }

    fn note_authorization_request(&self) {
        if let Ok(mut state) = self.inner.state.lock()
            && let KittyFileTransmissionState::AwaitingDecision { request_pending } = &mut *state
        {
            *request_pending = true;
        }
    }

    fn is_authorized(&self) -> bool {
        self.inner
            .state
            .lock()
            .is_ok_and(|state| matches!(*state, KittyFileTransmissionState::Trusted))
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
                KittyFileTransmissionState::Trusted => "trusted",
            })
            .unwrap_or("unavailable");
        formatter
            .debug_struct("KittyFileTransmissionControl")
            .field("decision", &decision)
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

#[derive(Clone, Debug)]
pub struct TerminalImageData {
    pub id: TerminalImageId,
    pub protocol: TerminalImageProtocol,
    pub version: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
    /// Shares the admission with every snapshot and renderer retaining these pixels.
    pub memory: Option<ImageMemoryLease>,
    pub frames: Vec<TerminalImageFrame>,
    pub animation: TerminalImageAnimationState,
    pub name: Option<String>,
}

// Admission ownership does not change the pixels or protocol state of a snapshot.
impl PartialEq for TerminalImageData {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.protocol == other.protocol
            && self.version == other.version
            && self.width == other.width
            && self.height == other.height
            && self.rgba == other.rgba
            && self.frames == other.frames
            && self.animation == other.animation
            && self.name == other.name
    }
}
impl Eq for TerminalImageData {}

#[derive(Clone, Debug)]
pub struct TerminalImageFrame {
    pub rgba: Arc<[u8]>,
    pub memory: Option<ImageMemoryLease>,
    pub delay_ms_numerator: u32,
    pub delay_ms_denominator: u32,
    pub gapless: bool,
}

impl PartialEq for TerminalImageFrame {
    fn eq(&self, other: &Self) -> bool {
        self.rgba == other.rgba
            && self.delay_ms_numerator == other.delay_ms_numerator
            && self.delay_ms_denominator == other.delay_ms_denominator
            && self.gapless == other.gapless
    }
}
impl Eq for TerminalImageFrame {}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TerminalImageAnimationState {
    pub running: bool,
    pub loading: bool,
    pub current_frame: usize,
    pub loop_limit: Option<u32>,
    pub control_serial: u64,
    pub seek: Option<usize>,
    pub reset_loops: bool,
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
    pub kitty: Option<KittyPlacement>,
    pub sixel: Option<SixelPlacement>,
    pub alternate_screen: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SixelPlacement {
    pub serial: u64,
    pub fixed: bool,
    pub clip_top: usize,
    pub clip_bottom: usize,
    pub clip_left: usize,
    pub clip_right: usize,
    pub column_offset: i64,
    /// Opaque Sixel backgrounds are resolved against the terminal palette in snapshots.
    pub background: Option<alacritty_terminal::vte::ansi::Color>,
}

impl SixelPlacement {
    pub fn background_rgb(&self) -> Option<[u8; 3]> {
        match self.background {
            Some(alacritty_terminal::vte::ansi::Color::Spec(rgb)) => Some([rgb.r, rgb.g, rgb.b]),
            _ => None,
        }
    }
}

/// Protocol placement identity is independent of the shared pixel resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KittyPlacement {
    pub serial: u64,
    pub id: u32,
    pub offset_x: u32,
    pub offset_y: u32,
    pub scaled: bool,
    pub virtual_placement: bool,
    pub parent: Option<(TerminalImageId, u32, i32, i32)>,
    pub clip_top: usize,
    pub clip_bottom: usize,
    pub image_id: u32,
    pub column_offset: i64,
    pub hidden: bool,
    pub unicode: Option<KittyPlaceholderRegion>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KittyPlaceholderRegion {
    pub row: usize,
    pub col: usize,
    pub cols: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalGraphicsEvent {
    ImageReady(TerminalImageData),
    ImageUpdated(TerminalImageData),
    Place(TerminalImagePlacement),
    Delete { id: Option<TerminalImageId> },
    DeletePlacements { id: Option<TerminalImageId> },
    DeletePlacement { serial: u64 },
    Screen { alternate: bool },
    MoveCursor { cols: usize, rows: usize },
    SixelCursor { rows: usize, column: usize },
    Batch(Vec<TerminalGraphicsEvent>),
    Respond(Vec<u8>),
    Error(String),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GraphicsAdvance {
    pub terminal_bytes: Vec<u8>,
    pub events: Vec<TerminalGraphicsEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalGraphicsSegment<'a> {
    Terminal(std::borrow::Cow<'a, [u8]>),
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
    #[error("ENOPARENT:Parent placement not found")]
    ParentNotFound,
    #[error("ECYCLE:Relative placement cycle")]
    PlacementCycle,
    #[error("ETOODEEP:Relative placement depth exceeded")]
    PlacementDepth,
    #[error("EINVAL:Invalid graphics command")]
    InvalidCommand,
    #[error("ENOENT:Image or placement not found")]
    ImageNotFound,
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
    SyncStart(Vec<u8>),
    Osc(Vec<u8>),
    OscEsc(Vec<u8>),
    Dcs(Vec<u8>),
    DcsEsc(Vec<u8>),
    Apc(Vec<u8>),
    ApcEsc(Vec<u8>),
    DiscardString { osc: bool, escape: bool },
}

pub struct GraphicsIngress {
    options: GraphicsOptions,
    state: ParserState,
    utf8_remaining: u8,
    next_image_id: u64,
    kitty_chunks: HashMap<u64, KittyChunkAssembly>,
    kitty_images: HashMap<TerminalImageId, TerminalImageData>,
    kitty_numbers: HashMap<(bool, u32), Vec<TerminalImageId>>,
    kitty_wire_ids: HashMap<(bool, u32), TerminalImageId>,
    kitty_placements: Vec<TerminalImagePlacement>,
    sixel_placements: Vec<TerminalImagePlacement>,
    sixel_state: Option<Box<SixelState>>,
    next_placement_serial: u64,
    grid_events: Option<alacritty_terminal::term::graphics::GraphicsGridEvents>,
    alternate_screen: bool,
    placeholder_rows: HashMap<(bool, i32), Vec<IndexedPlaceholder>>,
    placeholder_rendered: HashMap<u64, TerminalImagePlacement>,
    placeholders_changed: bool,
    placeholder_index_incomplete: bool,
    image_order: VecDeque<(TerminalImageId, usize)>,
    storage_bytes: usize,
    sync_buffer: Option<Zeroizing<Vec<u8>>>,
    sync_deadline: Option<std::time::Instant>,
}

struct KittyChunkAssembly {
    params: HashMap<String, String>,
    encoded: Vec<u8>,
}
