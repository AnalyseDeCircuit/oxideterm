use super::*;
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonVariant, button_focus_visible, button_with},
    confirm::{ModalFooterKeyAction, modal_footer_key_action},
    modal,
    motion::MotionDuration,
};
use oxideterm_ssh_launch::config::{
    ConfigEndpoint, ConfigLimit, MAX_CONFIG_BYTES, preview_endpoints,
};
use std::collections::HashSet;

struct Host {
    endpoint: ConfigEndpoint,
    selected: bool,
    imported: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ImportFocus {
    Row(usize),
    SelectAll,
    Close,
    Submit,
}

struct Preview {
    focus: Option<ImportFocus>,
    rows: Vec<Host>,
    skipped: usize,
    scroll: UniformListScrollHandle,
    return_focus: Option<FocusHandle>,
}

#[derive(Default)]
pub(crate) struct ConnectionImports {
    picker: Option<Task<()>>,
    preview: Option<Preview>,
}

impl ConnectionImports {
    pub(crate) fn is_open(&self) -> bool {
        self.preview.is_some()
    }
}

impl Workspace {
    pub(crate) fn open_ssh_import(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if !self.demo || self.imports.is_open() {
            return;
        }
        let file = crate::browser_file::pick(MAX_CONFIG_BYTES as u32, "");
        self.imports.picker = Some(cx.spawn_in(w, async move |owner, cx| {
            let result = file.await.and_then(|bytes| {
                let Some(bytes) = bytes else {
                    return Ok(None);
                };
                let source = std::str::from_utf8(&bytes)
                    .map_err(|_| "sessionManager.batchAdd.file_read_failed")?;
                preview_endpoints(source)
                    .map(Some)
                    .map_err(|limit| match limit {
                        ConfigLimit::Bytes => "settings_view.connections.ssh_config.file_limit",
                        ConfigLimit::Hosts => "settings_view.connections.ssh_config.host_limit",
                    })
            });
            let _ = owner.update_in(cx, |h, w, cx| {
                let preview = match result {
                    Ok(Some(preview)) => preview,
                    Ok(None) => return,
                    Err(key) => {
                        h.notice(key, cx);
                        return;
                    }
                };
                let names = h
                    .catalog_data
                    .items
                    .iter()
                    .map(|r| r.name.to_ascii_lowercase())
                    .collect::<HashSet<_>>();
                h.imports.preview = Some(Preview {
                    focus: None,
                    skipped: preview.skipped,
                    scroll: Default::default(),
                    return_focus: w.focused(cx),
                    rows: preview
                        .hosts
                        .into_iter()
                        .map(|endpoint| {
                            let imported = names.contains(&endpoint.alias.to_ascii_lowercase());
                            Host {
                                endpoint,
                                selected: !imported,
                                imported,
                            }
                        })
                        .collect(),
                });
                w.focus(&h.focus, cx);
                cx.notify();
            });
        }));
    }

    pub(crate) fn close_import(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(preview) = self.imports.preview.take() {
            w.focus(preview.return_focus.as_ref().unwrap_or(&self.focus), cx);
            cx.notify();
        }
    }

    fn import_hosts(&mut self, single: Option<usize>, cx: &mut Context<Self>) {
        let Some(preview) = &mut self.imports.preview else {
            return;
        };
        let mut names = self
            .catalog_data
            .items
            .iter()
            .map(|r| r.name.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        let mut count = 0;
        for (i, row) in preview.rows.iter_mut().enumerate() {
            if row.imported || !single.map_or(row.selected, |target| target == i) {
                continue;
            }
            row.selected = false;
            row.imported = true;
            let ep = &row.endpoint;
            if !names.insert(ep.alias.to_ascii_lowercase()) {
                continue;
            }
            let mut saved = CatalogRow::new(
                SessionManagerOpenTarget::Connection(format!("profile-{}", uuid::Uuid::new_v4())),
                ep.alias.clone(),
            );
            saved.subtitle = ep.address();
            saved.host = ep.host.clone();
            saved.username = ep.user.clone().unwrap_or_default();
            saved.port = u32::from(ep.port);
            self.catalog_data.upsert_row(saved);
            count += 1;
        }
        if count > 0 {
            self.catalog_storage.imported(count);
            self.refresh_catalog(cx);
        }
    }

    pub(crate) fn import_key(&mut self, key: &Keystroke, w: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = &mut self.imports.preview else {
            return;
        };
        let mut choices: Vec<_> = p
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| !r.imported)
            .map(|(i, _)| ImportFocus::Row(i))
            .collect();
        if !choices.is_empty() {
            choices.push(ImportFocus::SelectAll);
        }
        choices.push(ImportFocus::Close);
        if p.rows.iter().any(|r| r.selected) {
            choices.push(ImportFocus::Submit);
        }
        let k = match key.key.as_str() {
            "up" => "left",
            "down" => "right",
            k => k,
        };
        match modal_footer_key_action(
            k,
            key.modifiers.shift,
            &choices,
            p.focus.filter(|f| choices.contains(f)),
            ImportFocus::Close,
        ) {
            Some(ModalFooterKeyAction::Focus(f)) => {
                p.focus = Some(f);
                if let ImportFocus::Row(i) = f {
                    p.scroll.scroll_to_item(i, ScrollStrategy::Nearest);
                }
            }
            Some(ModalFooterKeyAction::Activate(ImportFocus::Row(i))) => {
                p.rows[i].selected = !p.rows[i].selected
            }
            Some(ModalFooterKeyAction::Activate(ImportFocus::SelectAll)) => {
                let select = p.rows.iter().any(|r| !r.imported && !r.selected);
                for r in &mut p.rows {
                    r.selected = !r.imported && select;
                }
            }
            Some(ModalFooterKeyAction::Activate(ImportFocus::Submit)) => {
                p.focus = Some(ImportFocus::Close);
                self.import_hosts(None, cx);
            }
            Some(
                ModalFooterKeyAction::Cancel | ModalFooterKeyAction::Activate(ImportFocus::Close),
            ) => self.close_import(w, cx),
            None => return,
        }
        cx.notify();
    }

    pub(crate) fn render_import(&self, w: &Window, cx: &Context<Self>) -> Option<AnyElement> {
        let preview = self.imports.preview.as_ref()?;
        let t = &self.tokens;
        let selected = preview.rows.iter().filter(|r| r.selected).count();
        let select = preview.rows.iter().any(|r| !r.selected && !r.imported);
        let body = if preview.rows.is_empty() {
            div()
                .p(px(16.))
                .child(self.i18n.t("settings_view.connections.ssh_config.no_hosts"))
                .into_any_element()
        } else {
            uniform_list(
                "web-import-hosts",
                preview.rows.len(),
                cx.processor(|h, range: std::ops::Range<usize>, _, cx| {
                    let Some(p) = &h.imports.preview else {
                        return Vec::new();
                    };
                    range
                        .filter_map(|i| {
                            let r = p.rows.get(i)?;
                            let row = oxideterm_gpui_settings_view::ssh_import::host_row(
                                &h.tokens,
                                &h.i18n,
                                r.endpoint.alias.clone(),
                                r.endpoint.address(),
                                r.selected,
                                r.imported,
                                cx.listener(move |h, _, _, cx| {
                                    if let Some(r) =
                                        h.imports.preview.as_mut().and_then(|p| p.rows.get_mut(i))
                                        && !r.imported
                                    {
                                        r.selected = !r.selected;
                                        cx.notify();
                                    }
                                }),
                                cx.listener(move |h, _, _, cx| h.import_hosts(Some(i), cx)),
                            );
                            Some(button_focus_visible(
                                &h.tokens,
                                div().child(row),
                                p.focus == Some(ImportFocus::Row(i)),
                            ))
                        })
                        .collect()
                }),
            )
            .track_scroll(&preview.scroll)
            .flex_1()
            .min_h_0()
            .w_full()
            .into_any_element()
        };
        let options = ButtonOptions::default();
        let button = |label, opts, focus| {
            button_focus_visible(t, button_with(t, label, opts), preview.focus == Some(focus))
        };
        let dialog = modal::dialog_content(t)
            .flex()
            .flex_col()
            .w((w.viewport_size().width - px(20.)).clamp(px(1.), px(720.)))
            .h((w.viewport_size().height - px(20.)).clamp(px(1.), px(560.)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(modal::modal_header(
                t,
                self.i18n.t("settings_view.connections.ssh_config.title"),
                self.i18n
                    .t("settings_view.connections.ssh_config.demo_preview"),
            ))
            .when(preview.skipped != 0, |d| {
                d.child(
                    div().px(px(16.)).child(
                        self.i18n
                            .t("settings_view.connections.ssh_config.skipped_invalid")
                            .replace("{{count}}", &preview.skipped.to_string()),
                    ),
                )
            })
            .child(body)
            .child(
                modal::dialog_footer(t)
                    .when(preview.rows.iter().any(|r| !r.imported), |f| {
                        f.child(
                            button(
                                self.i18n.t(if select {
                                    "settings_view.connections.ssh_config.select_all"
                                } else {
                                    "settings_view.connections.ssh_config.deselect_all"
                                }),
                                options,
                                ImportFocus::SelectAll,
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |h, _, _, cx| {
                                    if let Some(p) = &mut h.imports.preview {
                                        for r in &mut p.rows {
                                            r.selected = !r.imported && select;
                                        }
                                        cx.notify();
                                    }
                                }),
                            ),
                        )
                    })
                    .child(
                        button(
                            self.i18n.t("settings_view.connections.ssh_config.close"),
                            options,
                            ImportFocus::Close,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|h, _, w, cx| h.close_import(w, cx)),
                        ),
                    )
                    .child(
                        button(
                            self.i18n
                                .t("settings_view.connections.ssh_config.import_selected")
                                .replace("{{count}}", &selected.to_string()),
                            ButtonOptions {
                                variant: ButtonVariant::Default,
                                disabled: selected == 0,
                                ..options
                            },
                            ImportFocus::Submit,
                        )
                        .when(selected > 0, |b| {
                            b.on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|h, _, _, cx| h.import_hosts(None, cx)),
                            )
                        }),
                    ),
            );
        Some(modal::staged_dialog_overlay(
            t,
            "web-import-config",
            modal::TauriBackdropRole::Dialog,
            modal::staged_overlay_root().on_mouse_down(
                MouseButton::Left,
                cx.listener(|h, _, w, cx| h.close_import(w, cx)),
            ),
            dialog,
            8.,
            MotionDuration::Control,
            true,
        ))
    }
}
