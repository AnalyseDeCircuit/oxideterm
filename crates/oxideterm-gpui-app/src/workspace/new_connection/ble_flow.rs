// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Hands a validated BLE form to the workspace connection runtime.

use gpui::{Context, Window};

use super::form_state::NewConnectionSubmitAction;
use crate::workspace::WorkspaceApp;

pub(in crate::workspace) use oxideterm_gpui_connections::ble::{
    BleConnectionWorkerResult, BleOpenRequest, BlePendingOpen, BleStandaloneTarget, BleTraceTarget,
};

impl WorkspaceApp {
    pub(super) fn submit_ble_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let save = self.connections.flow.update(cx, |flow, cx| {
            flow.submit_ble(&mut self.connection_store, &self.i18n, action, cx)
        });
        if let Some(open) = self.apply_profile_submission(save, cx) {
            self.start_ble_open_flow(open, cx);
            cx.notify();
        }
    }
}
