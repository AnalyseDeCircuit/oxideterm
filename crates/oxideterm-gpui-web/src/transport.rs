use futures::channel::mpsc::UnboundedSender;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{Event, MessageEvent, WebSocket};
use zeroize::Zeroizing;

mod session;
pub use session::GatewaySessionState;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GatewayRequest {
    Open {
        request: u64,
        node: String,
        session: u64,
    },
    Input {
        session: u64,
        bytes: Zeroizing<Vec<u8>>,
    },
    Resize {
        session: u64,
        cols: u16,
        rows: u16,
    },
    Unsubscribe {
        session: u64,
    },
    Copy {
        transfer: u64,
        node: String,
        from: String,
        to: String,
    },
    Cancel {
        request: u64,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GatewayEvent {
    #[serde(skip)]
    DemoDirectoryChanged { session: u64 },
    Connected,
    Disconnected,
    Output {
        session: u64,
        bytes: Zeroizing<Vec<u8>>,
    },
    Rejected {
        request: u64,
    },
    Copied {
        transfer: u64,
    },
}

pub trait WebTransport {
    fn send(&mut self, request: GatewayRequest) -> Result<(), ()>;
    fn directory(&self, _session: u64) -> Option<&str> { None }
    fn change_directory(&mut self, _session: u64, _path: &str) -> Result<(), &'static str> {
        Err("sftp.toolbar.terminal_directory_unavailable")
    }
}

pub type Events = UnboundedSender<GatewayEvent>;

pub struct WebSocketTransport {
    socket: WebSocket,
    _message: Closure<dyn FnMut(MessageEvent)>,
    _open: Closure<dyn FnMut(Event)>,
    _closed: Closure<dyn FnMut(Event)>,
}

impl WebSocketTransport {
    pub fn connect(url: &str, events: Events) -> Result<Self, ()> {
        if !url.starts_with("wss://") {
            return Err(());
        }
        let socket = WebSocket::new(url).map_err(|_| ())?;
        let tx = events.clone();
        let _open = Closure::new(move |_: Event| {
            let _ = tx.unbounded_send(GatewayEvent::Connected);
        });
        let failed = Rc::new(std::cell::Cell::new(false));
        let flag = failed.clone();
        let tx = events.clone();
        let _closed = Closure::new(move |_: Event| {
            if !flag.replace(true) {
                let _ = tx.unbounded_send(GatewayEvent::Disconnected);
            }
        });
        let peer = socket.clone();
        let _message = Closure::new(move |msg: MessageEvent| {
            if failed.get() {
                return;
            }
            let event = msg
                .data()
                .as_string()
                .map(Zeroizing::new)
                .and_then(|text| serde_json::from_str::<GatewayEvent>(&text).ok());
            match event {
                Some(
                    event @ (GatewayEvent::Output { .. }
                    | GatewayEvent::Rejected { .. }
                    | GatewayEvent::Copied { .. }),
                ) => {
                    let _ = events.unbounded_send(event);
                }
                _ => {
                    let _ = peer.close_with_code_and_reason(4000, "Invalid gateway message");
                }
            }
        });
        socket.set_onopen(Some(_open.as_ref().unchecked_ref()));
        socket.set_onmessage(Some(_message.as_ref().unchecked_ref()));
        socket.set_onerror(Some(_closed.as_ref().unchecked_ref()));
        socket.set_onclose(Some(_closed.as_ref().unchecked_ref()));
        Ok(Self {
            socket,
            _open,
            _closed,
            _message,
        })
    }
}

impl WebTransport for WebSocketTransport {
    fn send(&mut self, request: GatewayRequest) -> Result<(), ()> {
        if self.socket.ready_state() != WebSocket::OPEN {
            return Err(());
        }
        let text = Zeroizing::new(serde_json::to_string(&request).map_err(|_| ())?);
        self.socket.send_with_str(&text).map_err(|_| ())
    }
}
impl Drop for WebSocketTransport {
    fn drop(&mut self) {
        self.socket.set_onopen(None);
        self.socket.set_onerror(None);
        self.socket.set_onclose(None);
        self.socket.set_onmessage(None);
        let _ = self.socket.close();
    }
}
