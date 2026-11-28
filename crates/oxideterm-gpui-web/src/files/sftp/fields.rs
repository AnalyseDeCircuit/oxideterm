use super::*;
use oxideterm_gpui_editor::line_edit::{FieldEvent, LineField};
use std::collections::HashMap;

pub(super) struct InputFields {
    pub(super) views: HashMap<SftpInput, Entity<LineField>>,
    bindings: Vec<Subscription>,
}

impl InputFields {
    pub(super) fn new(tokens: ThemeTokens, cx: &mut App) -> Self {
        let inputs = [
            SftpInput::LocalPath,
            SftpInput::RemotePath,
            SftpInput::LocalFilter,
            SftpInput::RemoteFilter,
            SftpInput::DialogValue,
        ];
        let views = inputs
            .into_iter()
            .map(|input| {
                (
                    input,
                    cx.new(|cx| LineField::new(tokens, String::new(), true, cx)),
                )
            })
            .collect();
        Self {
            views,
            bindings: Vec::new(),
        }
    }
}

impl Browser {
    pub(super) fn bind_input_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.fields.bindings.is_empty() {
            return;
        }
        self.fields
            .bindings
            .push(cx.subscribe_in(&self.state, window, |b, state, e, w, cx| {
                if !matches!(e, SftpWorkspaceEvent::InputFocusChanged) {
                    return;
                }
                if let Some(input) = state.read(cx).focused_input() {
                    let field = &b.fields.views[&input];
                    let value = state.read(cx).input_value(input).to_owned();
                    field.update(cx, |f, cx| f.sync_value(&value, cx));
                    w.focus(&field.focus_handle(cx), cx);
                } else if b.field_focused(w, cx) {
                    w.focus(&b.focus, cx);
                }
            }));
        for (&input, field) in &self.fields.views {
            self.fields.bindings.push(cx.subscribe_in(
                field,
                window,
                move |b, field, event, w, cx| {
                    if b.state.read(cx).focused_input() != Some(input) {
                        return;
                    }
                    match event {
                        FieldEvent::Changed => {
                            let load = b.state.update(cx, |s, cx| {
                                field.read(cx).text().clone_into(s.input_value_mut(input));
                                cx.notify();
                                s.refresh_path_completion(input, cx)
                            });
                            if let Some(load) = load {
                                b.complete_path(load, cx);
                            }
                        }
                        FieldEvent::Submit | FieldEvent::Cancel => {
                            let key = if matches!(event, FieldEvent::Submit) {
                                "enter"
                            } else {
                                "escape"
                            };
                            b.handle_sftp_key(&Keystroke::parse(key).unwrap(), None, w, cx);
                        }
                    }
                },
            ));
        }
    }

    pub(crate) fn field_focused(&self, w: &Window, cx: &App) -> bool {
        self.fields
            .views
            .values()
            .any(|f| f.focus_handle(cx).is_focused(w))
    }

    fn complete_path(&mut self, load: SftpCompletionLoad, cx: &mut Context<Self>) {
        if load.pane == SftpPane::Local && self.device.folder.is_some() {
            self.complete_device_path(load, cx);
            return;
        }
        let SftpCompletionLoad {
            pane,
            generation,
            remote_id,
            parent_path,
        } = load;
        let result = self.fs(pane).sftp_list(&parent_path).map(|files| {
            files
                .into_iter()
                .map(sftp_path_completion_candidate)
                .collect()
        });
        let message = match pane {
            SftpPane::Local => SftpWorkerResult::PairPrimaryPathCompletion {
                generation,
                remote_id,
                parent_path,
                result,
            },
            SftpPane::Remote => SftpWorkerResult::RemotePathCompletion {
                generation,
                remote_id,
                parent_path,
                result,
            },
        };
        let _ = self.state.read(cx).worker_sender().send(message);
    }
}
