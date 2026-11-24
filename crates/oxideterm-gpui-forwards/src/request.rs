use crate::data::{ForwardRule, ForwardStats, ForwardUpdate, NodeId};
use crate::{ForwardInput, PortDetectionViewState};
use gpui::{App, MouseDownEvent, MouseMoveEvent, SharedString, Window};
use oxideterm_gpui_ui::text_input::{TextInputAnchor, TextInputAnchorId, TextInputViewport};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::TabId;
use std::{collections::HashMap, ops::Range, rc::Rc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForwardingQuickAction {
    Jupyter,
    Tensorboard,
    Vscode,
}

#[derive(Debug)]
pub enum ForwardingRuntimeOperation {
    Save {
        rule: ForwardRule,
    },
    Create {
        rule: ForwardRule,
        check_health: bool,
    },
    Update {
        forward_id: String,
        update: ForwardUpdate,
    },
    Delete {
        forward_id: String,
    },
    Stop {
        forward_id: String,
    },
    Restart {
        forward_id: String,
    },
    Quick {
        action: ForwardingQuickAction,
        port: u16,
    },
}

#[derive(Clone, Default, Eq, PartialEq)]
pub struct ForwardingRuntimeSnapshot {
    pub rules: Vec<ForwardRule>,
    pub names: HashMap<String, String>,
    pub stats_by_forward_id: HashMap<String, ForwardStats>,
}

pub struct ForwardsFrame {
    pub tab_id: TabId,
    pub node_id: NodeId,
    pub node_ready: bool,
    pub can_save: bool,
    pub runtime: ForwardingRuntimeSnapshot,
    pub ports: PortDetectionViewState,
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub has_background: bool,
    pub ui_font: SharedString,
    pub cjk_font: SharedString,
    pub mono_font: SharedString,
}

pub struct ForwardsImeFrame {
    pub anchor: TextInputAnchorId,
    pub viewport: TextInputViewport,
    pub active_offset: Option<usize>,
    pub selection: Option<Range<usize>>,
    pub composition: Option<String>,
    pub caret_visible: bool,
    pub place_anchor: Rc<dyn Fn(TextInputAnchor)>,
}

pub enum ForwardsRequest {
    SelectPage,
    FocusInput(ForwardInput, MouseDownEvent),
    DragInput(MouseMoveEvent),
    Refresh,
    ScanPorts,
    DismissPort(u16),
    Create {
        rule: ForwardRule,
        start: bool,
        check_health: bool,
    },
    Operation {
        operation: ForwardingRuntimeOperation,
        message_key: &'static str,
        sync_saved: bool,
    },
}

#[derive(Clone)]
pub struct ForwardsViewBindings {
    pub frame: Rc<dyn Fn(&App) -> Option<ForwardsFrame>>,
    pub ime: Rc<dyn Fn(ForwardInput, &App) -> Option<ForwardsImeFrame>>,
    pub dispatch: Rc<dyn Fn(ForwardsRequest, Option<&mut Window>, &mut App)>,
}
