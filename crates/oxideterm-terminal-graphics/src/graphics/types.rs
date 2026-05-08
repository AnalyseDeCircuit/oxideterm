pub const DEFAULT_PIXEL_LIMIT: u32 = 16_777_216;
pub const DEFAULT_STORAGE_LIMIT_MB: u32 = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsOptions {
    pub enabled: bool,
    pub sixel: bool,
    pub iterm2_inline: bool,
    pub kitty: bool,
    pub pixel_limit: u32,
    pub storage_limit_mb: u32,
    pub show_placeholder: bool,
}

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
    pub name: Option<String>,
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
    pub z_index: i32,
    pub placeholder: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalGraphicsEvent {
    ImageReady(TerminalImageData),
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
    #[error("invalid image path payload")]
    InvalidPath,
    #[error("{0}")]
    Io(String),
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
}

struct KittyChunkAssembly {
    params: HashMap<String, String>,
    encoded: Vec<u8>,
}
