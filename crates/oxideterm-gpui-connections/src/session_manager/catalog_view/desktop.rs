// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::views::{ConnectionsDispatch, ConnectionsWindowRequest};

// The workspace attaches live statuses after from_store returns. Keep its concrete
// snapshot contract here; only display metadata crosses into the catalog entity.
pub type CatalogEnvironment =
    CatalogFrame<SessionManagerDisplayItem, oxideterm_forwarding::ForwardStatus>;

impl CatalogEnvironment {
    pub fn from_store(
        manager: &SessionManagerState,
        store: &oxideterm_connections::ConnectionStore,
        tunnels: Vec<oxideterm_forwarding::PersistedForward>,
        include_items: bool,
    ) -> Self {
        let group_counts = if include_items {
            crate::session_manager::groups::connection_counts_by_group(store, &tunnels)
        } else {
            HashMap::new()
        };
        let groups = session_group_tree(store, &tunnels);
        Self {
            items: if include_items {
                manager.display_catalog(store, tunnels)
            } else {
                Vec::new()
            },
            group_counts,
            groups,
            stored_groups: store.groups().to_vec(),
            ..Self::default()
        }
    }
}

impl From<oxideterm_forwarding::ForwardStatus> for TunnelDisplayStatus {
    fn from(status: oxideterm_forwarding::ForwardStatus) -> Self {
        use oxideterm_forwarding::ForwardStatus as Native;
        match status {
            Native::Active => Self::Active,
            Native::Starting => Self::Starting,
            Native::Suspended => Self::Suspended,
            Native::Stopped => Self::Stopped,
            Native::Error => Self::Error,
        }
    }
}

impl From<CatalogRequest> for ConnectionsWindowRequest {
    fn from(request: CatalogRequest) -> Self {
        match request {
            Request::Catalog(action) => Self::Catalog(action),
            Request::FocusSessionInput(field, event) => Self::FocusSessionInput(field, event),
            Request::DragInputSelection(event) => Self::DragInputSelection(event),
            Request::ClearComposition => Self::ClearComposition,
            Request::ToggleCatalogSidebarSearch => Self::ToggleCatalogSidebarSearch,
            Request::QueueTooltip { id, label, x, y } => Self::QueueTooltip { id, label, x, y },
            Request::ClearTooltip(id) => Self::ClearTooltip(id),
        }
    }
}
impl CatalogView {
    pub(crate) fn new(
        mount: CatalogMount,
        manager: Entity<SessionManagerState>,
        theme: ConnectionsEnvironmentSource,
        bindings: &SessionManagerViewBindings,
        dispatch: ConnectionsDispatch,
        cx: &mut Context<Self>,
    ) -> Self {
        let catalog = bindings.catalog.clone();
        Self::mount(
            mount,
            manager,
            theme,
            Rc::new(move |include_items, cx| {
                let frame = catalog(include_items, cx)?;
                Some(CatalogFrame {
                    items: frame.items.into_iter().map(CatalogRow::from).collect(),
                    tunnel_status: frame
                        .tunnel_status
                        .into_iter()
                        .map(|(id, status)| (id, status.into()))
                        .collect(),
                    groups: frame.groups,
                    group_counts: frame.group_counts,
                    stored_groups: frame.stored_groups,
                    ui_font: frame.ui_font,
                    chrome_divider: frame.chrome_divider,
                    sidebar_background: frame.sidebar_background,
                    sidebar_sort_anchor: frame.sidebar_sort_anchor,
                    horizontal_chrome_width: frame.horizontal_chrome_width,
                    has_background: frame.has_background,
                })
            }),
            bindings.ime.clone(),
            Rc::new(move |request, window, cx| dispatch(request.into(), window, cx)),
            cx,
        )
    }
}
