use super::*;
use futures::StreamExt;
use oxideterm_plugin_manifest::{NativePluginRegistryEntry as Entry, catalog};
use std::collections::HashSet;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, Request, RequestInit, Response};

const PAGE_SIZE: usize = 10;

pub(crate) struct Catalog {
    entries: Option<Vec<Entry>>,
    request: Option<Task<()>>,
    failed: bool,
    tag: Option<String>,
    expanded: HashSet<String>,
    sort: PluginMarketplaceSort,
    pub(crate) page: usize,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            entries: None,
            request: None,
            failed: false,
            tag: None,
            expanded: Default::default(),
            sort: PluginMarketplaceSort::Name,
            page: 0,
        }
    }
}

impl Catalog {
    pub(crate) fn cancel(&mut self) {
        self.request.take();
    }
    fn sort_entries(&mut self) {
        if let Some(entries) = &mut self.entries {
            catalog::sort_entries(entries, |e| match self.sort {
                PluginMarketplaceSort::Name => None,
                PluginMarketplaceSort::RecentUpdates => e.latest_release_at.as_deref(),
                PluginMarketplaceSort::NewestListings => e.listed_at.as_deref(),
            });
        }
    }
}

async fn fetch_catalog() -> Result<Vec<Entry>, ()> {
    struct Fetch(AbortController);
    impl Drop for Fetch {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let fetch = Fetch(AbortController::new().map_err(|_| ())?);
    let opts = RequestInit::new();
    opts.set_signal(Some(&fetch.0.signal()));
    opts.set_credentials(web_sys::RequestCredentials::Omit);
    opts.set_referrer_policy(web_sys::ReferrerPolicy::NoReferrer);
    opts.set_redirect(web_sys::RequestRedirect::Error);
    let req =
        Request::new_with_str_and_init(catalog::OFFICIAL_NATIVE_PLUGIN_REGISTRY_V2_URL, &opts)
            .map_err(|_| ())?;
    let resp: Response = JsFuture::from(web_sys::window().ok_or(())?.fetch_with_request(&req))
        .await
        .map_err(|_| ())?
        .dyn_into()
        .map_err(|_| ())?;
    if !resp.ok() {
        return Err(());
    }
    let mut reader = wasm_streams::ReadableStream::from_raw(resp.body().ok_or(())?).into_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = reader.next().await {
        let chunk = chunk
            .map_err(|_| ())?
            .dyn_into::<js_sys::Uint8Array>()
            .map_err(|_| ())?;
        if bytes.len() as u64 + u64::from(chunk.length()) > catalog::MAX_CATALOG_BYTES {
            return Err(());
        }
        let offset = bytes.len();
        bytes.resize(offset + chunk.length() as usize, 0);
        chunk.copy_to(&mut bytes[offset..]);
    }
    catalog::read_summaries(&bytes).ok_or(())
}

impl Workspace {
    pub(crate) fn ensure_plugin_catalog(&mut self, cx: &mut Context<Self>) {
        if self.pages.catalog.entries.is_none() && !self.pages.catalog.failed {
            self.refresh_plugin_catalog(cx);
        }
    }
    fn refresh_plugin_catalog(&mut self, cx: &mut Context<Self>) {
        let c = &mut self.pages.catalog;
        if c.request.is_some() {
            return;
        }
        c.failed = false;
        c.request = Some(cx.spawn(async |owner, cx| {
            let result = fetch_catalog().await;
            let _ = owner.update(cx, |h, cx| {
                let c = &mut h.pages.catalog;
                c.request = None;
                match result {
                    Ok(entries) => {
                        c.expanded.retain(|id| entries.iter().any(|e| &e.id == id));
                        if c.tag.is_some()
                            && !entries.iter().any(|e| {
                                catalog::tag_matches(
                                    e.tags.as_deref().unwrap_or_default(),
                                    c.tag.as_deref(),
                                )
                            })
                        {
                            c.tag = None;
                        }
                        c.entries = Some(entries);
                        c.sort_entries();
                    }
                    Err(()) => c.failed = true,
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(crate) fn render_plugin_manager_page(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        let active_tab = self.pages.plugin_tab;
        let installed_label = self.i18n.t("plugin.tab_installed");
        let installed_badge = "0".to_owned();
        let marketplace_label = self.i18n.t("plugin.tab_marketplace");
        let marketplace_badge: Option<String> = None;
        let tab_width = oxideterm_gpui_ui::estimate_segmented_control_width(
            &t,
            PLUGIN_MANAGER_TAB_BAR_WIDTH,
            540.0,
            true,
            [
                (installed_label.as_str(), Some(installed_badge.as_str())),
                (marketplace_label.as_str(), marketplace_badge.as_deref()),
            ],
        );

        let make_tab =
            |tab: PluginManagerTab, icon: LucideIcon, label: String, badge: Option<String>| {
                let active = active_tab == tab;
                let content = div()
                    .w_full()
                    .py(px(2.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(8.0))
                    .child(glyph(
                        icon,
                        16.0,
                        if active {
                            theme.accent
                        } else {
                            theme.text_muted
                        },
                    ))
                    .child(label)
                    .when_some(badge, |c, badge| {
                        c.child(
                            div()
                                .ml(px(4.0))
                                .rounded(px(t.radii.sm))
                                .border_1()
                                .border_color(if active {
                                    rgb(theme.accent)
                                } else {
                                    rgba((theme.border << 8) | 0x80)
                                })
                                .bg(if active {
                                    rgba((theme.accent << 8) | 0x1a)
                                } else {
                                    rgb(theme.bg_panel)
                                })
                                .px(px(6.0))
                                .py(px(2.0))
                                .text_size(px(t.metrics.ui_text_xs))
                                .text_color(rgb(if active {
                                    theme.accent
                                } else {
                                    theme.text_muted
                                }))
                                .child(badge),
                        )
                    });
                segmented_control_item_content(&t, active, content.into_any_element())
                    .font_weight(FontWeight::MEDIUM)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |h, _, _, cx| {
                            if h.pages.plugin_tab != tab {
                                let prev_idx = usize::from(
                                    h.pages.plugin_tab == PluginManagerTab::Marketplace,
                                );
                                let next_idx = usize::from(tab == PluginManagerTab::Marketplace);
                                h.pages.plugin_previous_tab = h.pages.plugin_tab;
                                h.pages.plugin_tab = tab;
                                h.begin_segmented_transition(
                                    "web-plugin-switcher",
                                    prev_idx,
                                    next_idx,
                                    cx,
                                );
                            }
                            if tab == PluginManagerTab::Marketplace {
                                h.ensure_plugin_catalog(cx);
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .into_any_element()
            };

        let active_idx = usize::from(active_tab == PluginManagerTab::Marketplace);
        let prev_idx = self
            .segmented_motion
            .previous_index_for("web-plugin-switcher", active_idx)
            .unwrap_or_else(|| {
                usize::from(self.pages.plugin_previous_tab == PluginManagerTab::Marketplace)
            });
        let switcher = segmented_control(
            &t,
            "web-plugin-switcher",
            SegmentedControlOptions::new(active_idx, prev_idx, 2)
                .user_transition_active(
                    self.segmented_motion
                        .is_active_for("web-plugin-switcher", active_idx),
                )
                .compact(tab_width),
            vec![
                make_tab(
                    PluginManagerTab::Installed,
                    LucideIcon::Puzzle,
                    installed_label,
                    Some(installed_badge),
                ),
                make_tab(
                    PluginManagerTab::Marketplace,
                    LucideIcon::Network,
                    marketplace_label,
                    marketplace_badge,
                ),
            ],
        )
        .into_any_element();

        let installed = active_tab == PluginManagerTab::Installed;
        let header = self.page_header_bar(
            self.i18n.t("plugin.manager_title"),
            Some(self.i18n.t("plugin.browser_catalog_description")),
            switcher,
        );
        let c = &self.pages.catalog;
        let mut toolbar = div().w_full().flex().flex_wrap().items_center().gap(px(8.));
        if !installed {
            toolbar = toolbar
                .child(self.plugin_action_button(
                    LucideIcon::ExternalLink,
                    self.i18n.t("plugin.marketplace_repository"),
                    |_, _, cx| {
                        cx.open_url(OFFICIAL_PLUGIN_MARKETPLACE_HOME);
                        cx.stop_propagation();
                    },
                ))
                .child(self.plugin_action_button(
                    LucideIcon::RefreshCw,
                    self.i18n.t("plugin.refresh"),
                    cx.listener(|h, _, _, cx| h.refresh_plugin_catalog(cx)),
                ));
        }
        let content = if installed {
            oxideterm_gpui_ui::empty_state(
                &t,
                glyph(LucideIcon::Puzzle, 32., theme.text_muted),
                self.i18n.t("plugin.footer").replace("{{count}}", "0"),
                None,
                None,
            )
            .into_any_element()
        } else {
            let mut body = div().w_full().flex().flex_col().gap(px(12.)).child(toolbar);
            if c.request.is_some() {
                body = body.child(self.i18n.t("plugin.loading_marketplace"));
            }
            if c.failed {
                body = body.child(
                    div()
                        .text_color(rgb(theme.error))
                        .child(self.i18n.t("plugin.marketplace_load_error")),
                );
            }
            if let Some(entries) = &c.entries {
                body = body.child(self.catalog_list(entries, cx));
            }
            body.into_any_element()
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(header)
            .child(
                div()
                    .id("web-plugin-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(t.metrics.settings_content_padding))
                    .child(fade_in(
                        &t,
                        ("web-plugin-body", active_idx),
                        div().child(content),
                        MotionDuration::Micro,
                    )),
            )
            .into_any_element()
    }

    fn catalog_list(&self, entries: &[Entry], cx: &Context<Self>) -> AnyElement {
        let t = &self.tokens;
        let c = &self.pages.catalog;
        let search = self.plugin_search();
        let query = search.read(cx).text().trim().to_lowercase();
        let visible = entries
            .iter()
            .filter(|e| {
                catalog::entry_matches(e, &query)
                    && catalog::tag_matches(e.tags.as_deref().unwrap_or_default(), c.tag.as_deref())
            })
            .collect::<Vec<_>>();
        let pages = visible.len().div_ceil(PAGE_SIZE).max(1);
        let page = c.page.min(pages - 1);
        let mut tags = div().flex().flex_wrap().gap_1().child(self.plugin_tag_pill(
            self.i18n.t("plugin.marketplace_all"),
            c.tag.is_none(),
            cx.listener(|h, _, _, cx| {
                h.pages.catalog.tag = None;
                h.pages.catalog.page = 0;
                cx.notify();
            }),
        ));
        for tag in catalog::filter_tags(
            entries
                .iter()
                .flat_map(|e| e.tags.iter().flatten().map(String::as_str)),
        ) {
            let label = PLUGIN_TAGS
                .iter()
                .find(|(id, _)| *id == tag)
                .map_or_else(|| tag.clone(), |(_, k)| self.i18n.t(k));
            tags = tags.child(self.plugin_tag_pill(
                label,
                c.tag.as_ref() == Some(&tag),
                cx.listener(move |h, _, _, cx| {
                    h.pages.catalog.tag = Some(tag.clone());
                    h.pages.catalog.page = 0;
                    cx.notify();
                }),
            ));
        }
        let sort = self.plugin_action_button(
            LucideIcon::ArrowDownAZ,
            self.i18n.t("plugin.marketplace_sort").replace(
                "{{order}}",
                &self.i18n.t(plugin_marketplace_sort_label(c.sort)),
            ),
            cx.listener(|h, _, _, cx| {
                let c = &mut h.pages.catalog;
                c.sort = match c.sort {
                    PluginMarketplaceSort::Name => PluginMarketplaceSort::RecentUpdates,
                    PluginMarketplaceSort::RecentUpdates => PluginMarketplaceSort::NewestListings,
                    PluginMarketplaceSort::NewestListings => PluginMarketplaceSort::Name,
                };
                c.sort_entries();
                c.page = 0;
                cx.notify();
            }),
        );
        let mut list = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(div().min_w(px(160.)).flex_1().child(search.clone()))
                    .child(sort),
            )
            .child(tags);
        for e in visible.iter().skip(page * PAGE_SIZE).take(PAGE_SIZE) {
            let id = e.id.clone();
            let expanded = c.expanded.contains(&id);
            let heading = div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(e.name.clone()),
                )
                .child(
                    div()
                        .text_color(rgb(t.ui.text_muted))
                        .child(format!("v{}", e.version)),
                )
                .when_some(e.author.as_ref(), |r, a| {
                    r.child(
                        div()
                            .text_size(px(t.metrics.ui_text_xs))
                            .text_color(rgb(t.ui.text_muted))
                            .child(self.i18n.t("plugin.by_author").replace("{{author}}", a)),
                    )
                });
            let toggle = self.plugin_action_button(
                if expanded {
                    LucideIcon::ChevronDown
                } else {
                    LucideIcon::ChevronRight
                },
                self.i18n.t(if expanded {
                    "plugin.hide_details"
                } else {
                    "plugin.show_details"
                }),
                cx.listener(move |h, _, _, cx| {
                    if !h.pages.catalog.expanded.remove(&id) {
                        h.pages.catalog.expanded.insert(id.clone());
                    }
                    cx.notify();
                }),
            );
            let row = div()
                .flex()
                .flex_col()
                .gap_1()
                .py_2()
                .border_b_1()
                .border_color(rgb(t.ui.border))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(heading)
                        .child(toggle),
                )
                .when_some(e.description.as_ref(), |r, d| {
                    r.child(div().text_color(rgb(t.ui.text_muted)).child(d.clone()))
                })
                .when(expanded, |r| {
                    r.child(
                        div()
                            .text_size(px(t.metrics.ui_text_xs))
                            .child(e.id.clone()),
                    )
                    .children(
                        e.capabilities_summary
                            .iter()
                            .flatten()
                            .cloned()
                            .map(|line| div().child(line)),
                    )
                });
            list = list.child(row);
        }
        if visible.is_empty() {
            list = list.child(self.i18n.t(if entries.is_empty() {
                "plugin.marketplace_empty"
            } else {
                "plugin.no_search_results"
            }));
        } else {
            let start = page * PAGE_SIZE;
            let range = self
                .i18n
                .t("plugin.page_range")
                .replace("{{start}}", &(start + 1).to_string())
                .replace(
                    "{{end}}",
                    &(start + PAGE_SIZE).min(visible.len()).to_string(),
                )
                .replace("{{total}}", &visible.len().to_string());
            list = list.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(range)
                    .when(page > 0, |row| {
                        row.child(self.plugin_action_button(
                            LucideIcon::ChevronLeft,
                            self.i18n.t("plugin.previous_page"),
                            cx.listener(move |h, _, _, cx| {
                                h.pages.catalog.page = page - 1;
                                cx.notify();
                            }),
                        ))
                    })
                    .when(page + 1 < pages, |row| {
                        row.child(self.plugin_action_button(
                            LucideIcon::ChevronRight,
                            self.i18n.t("plugin.next_page"),
                            cx.listener(move |h, _, _, cx| {
                                h.pages.catalog.page = page + 1;
                                cx.notify();
                            }),
                        ))
                    }),
            );
        }
        list.into_any_element()
    }
}
