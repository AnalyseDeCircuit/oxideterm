// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::views::ConnectionsWindowRequest as Request;
use gpui::{Context, KeyDownEvent};
use oxideterm_gpui_ui::{
    confirm::{ModalFooterBodyInputKeyAction as KeyAction, modal_footer_body_input_key_action},
    motion::ExitPhase,
};

#[derive(Clone, Copy)]
pub enum OxideKeyScope {
    Active,
    Import,
    Export,
}

impl SessionManagerState {
    pub fn handle_oxide_key(
        &mut self,
        event: &KeyDownEvent,
        scope: OxideKeyScope,
        cx: &mut Context<Self>,
    ) -> (bool, Option<Request>) {
        let import = match scope {
            OxideKeyScope::Import => true,
            OxideKeyScope::Export => false,
            OxideKeyScope::Active => {
                let modifiers = event.keystroke.modifiers;
                if modifiers.platform || modifiers.control {
                    return (false, None);
                }
                self.oxide_import_dialog.is_some()
            }
        };
        let (actions, inputs, footer) = if import {
            let Some(dialog) = self.oxide_import_dialog.as_ref() else {
                return (false, None);
            };
            if dialog.presence.phase() == ExitPhase::Exiting {
                return (true, None);
            }
            if dialog.busy {
                return (false, None);
            }
            (
                oxide_import_footer_actions(dialog),
                oxide_import_footer_body_inputs(dialog),
                dialog.focused_footer_action,
            )
        } else {
            let Some(dialog) = self.oxide_export_dialog.as_ref() else {
                return (false, None);
            };
            if dialog.presence.phase() == ExitPhase::Exiting {
                return (true, None);
            }
            if dialog.busy {
                return (false, None);
            }
            (
                vec![
                    OxideDialogFooterAction::Cancel,
                    OxideDialogFooterAction::Primary,
                ],
                oxide_export_footer_body_inputs(dialog),
                dialog.focused_footer_action,
            )
        };
        let Some(fallback) = actions.first().copied() else {
            return (false, None);
        };
        let focused = self.focused_input.filter(|input| inputs.contains(input));
        let key = modal_footer_body_input_key_action(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            &actions,
            footer,
            inputs,
            focused,
            fallback,
            None,
        );
        let action = match key {
            None => return (false, None),
            Some(KeyAction::Cancel)
            | Some(KeyAction::Activate(OxideDialogFooterAction::Cancel)) => {
                self.dismiss_oxide(import, cx);
                None
            }
            Some(KeyAction::FocusInput(input)) => {
                self.focus_oxide_footer(import, None, Some(input), cx);
                Some(Request::ClearComposition)
            }
            Some(KeyAction::FocusFooter(footer)) => {
                self.focus_oxide_footer(import, Some(footer), None, cx);
                Some(Request::ClearComposition)
            }
            Some(KeyAction::Activate(footer)) if import => self
                .activate_oxide_import_footer(footer, cx)
                .map(Request::Oxide),
            Some(KeyAction::Activate(OxideDialogFooterAction::Primary)) => self
                .oxide_export_dialog
                .as_ref()
                .is_some_and(oxide_export_has_selected_content)
                .then_some(Request::Oxide(OxideDialogAction::Export)),
            Some(KeyAction::Activate(OxideDialogFooterAction::Secondary)) => None,
        };
        (true, action)
    }

    pub fn activate_oxide_import_footer(
        &mut self,
        action: OxideDialogFooterAction,
        cx: &mut Context<Self>,
    ) -> Option<OxideDialogAction> {
        let dialog = self.oxide_import_dialog.as_mut()?;
        match action {
            OxideDialogFooterAction::Cancel => self.dismiss_oxide(true, cx),
            OxideDialogFooterAction::Secondary if dialog.preview.is_some() => {
                dialog.preview = None;
                dialog.result_summary = None;
                dialog.focused_footer_action = Some(OxideDialogFooterAction::Secondary);
                cx.notify();
            }
            OxideDialogFooterAction::Secondary => return Some(OxideDialogAction::PickImportFile),
            OxideDialogFooterAction::Primary if dialog.result.is_some() => {
                self.dismiss_oxide(true, cx)
            }
            OxideDialogFooterAction::Primary if dialog.preview.is_some() => {
                return oxide_import_has_selected_content(dialog)
                    .then_some(OxideDialogAction::ApplyImport);
            }
            OxideDialogFooterAction::Primary => {
                return (!dialog.password.is_empty()).then_some(OxideDialogAction::PreviewImport);
            }
        }
        None
    }

    pub(super) fn dismiss_oxide(&mut self, import: bool, cx: &mut Context<Self>) {
        if import {
            self.oxide_import_dialog = None;
        } else {
            self.oxide_export_dialog = None;
        }
        self.focused_input = None;
        cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
        cx.notify();
    }

    fn focus_oxide_footer(
        &mut self,
        import: bool,
        footer: Option<OxideDialogFooterAction>,
        input: Option<SessionManagerInput>,
        cx: &mut Context<Self>,
    ) {
        self.focused_input = input;
        if import {
            if let Some(dialog) = self.oxide_import_dialog.as_mut() {
                dialog.focused_footer_action = footer;
            }
        } else if let Some(dialog) = self.oxide_export_dialog.as_mut() {
            dialog.focused_footer_action = footer;
        }
        cx.notify();
    }
}
