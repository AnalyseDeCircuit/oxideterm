use super::*;

/// `toolbar.overlayClosed` reason: the pane keeps its overlay membership and
/// the host opens the card again in this same event batch (focus handover),
/// so the plugin may keep per-pane card state instead of tearing it down.
const OVERLAY_CLOSED_REASON_REBOUND: &str = "rebound";
/// `toolbar.overlayClosed` reason: the pane genuinely left the overlay
/// (released the state, closed the card, or the pane closed), so the plugin
/// should drop any per-pane card state it kept.
const OVERLAY_CLOSED_REASON_RELEASED: &str = "released";

/// Overlay membership registered by the pane whose toolbar state declares
/// `overlay`. Several panes may hold membership at once; the single physical
/// card binds to whichever member is the active pane. The host owns the
/// binding transitions, the render gate, the card chrome, and the visibility
/// events; the plugin owns the card content and every semantic attached to
/// the state id.
#[derive(Clone)]
pub(in crate::workspace) struct PluginOverlayAnchor {
    pub plugin_id: String,
    pub action_id: String,
    pub pane_id: PaneId,
    pub window: Option<gpui::AnyWindowHandle>,
    /// Whether `toolbar.overlayOpened` is outstanding for this member, i.e.
    /// it currently holds the card. Only the bound member can hold the card,
    /// so at most one member is opened at a time; focus changes rebind the
    /// card through a closed/opened pair while losing focus to a non-member
    /// pane only hides the holder.
    pub opened: bool,
    /// Last visibility value delivered to the plugin; the poll only emits
    /// changes so the plugin never receives duplicate notifications.
    pub visible: bool,
    /// Whether a visibility poll loop was started for this member's
    /// generation. The single-flight guard in the spawn helper checks it so
    /// the member table never gets two concurrent loops.
    pub timer_active: bool,
    /// Monotonic counter captured at claim time. The visibility poll loop
    /// compares it against the workspace counter each tick and exits when a
    /// newer claim superseded it, so a release followed by a reclaim within
    /// 1s cannot leave two concurrent loops polling the same table.
    pub generation: u64,
}

/// Generic machinery for plugin-contributed terminal toolbar chips. The
/// manifest (`contributes.terminalToolbar`) is the discovery channel: the host
/// renders one chip per contributed action on every terminal pane, stores the
/// per-pane state id, and reports clicks by `plugin_id:action_id`. State ids
/// are opaque here; a state may declare `overlay`, which registers the pane
/// as an overlay member: the host keeps one card, binds it to the member that
/// is the active pane, gates it on focus, and forwards open/close and
/// visibility events while the plugin renders the content.
impl WorkspaceApp {
    /// Installed, enabled, and activatable as a process runtime. Mirrors the
    /// connection-plugin predicate; permission review blocks activation.
    pub(in crate::workspace) fn native_plugin_available(&self, plugin_id: &str, cx: &App) -> bool {
        self.plugin_entity
            .read(cx)
            .registry()
            .plugins()
            .iter()
            .any(|plugin| {
                plugin.manifest.id == plugin_id
                    && plugin_host::native_plugin_process_activatable(plugin)
            })
    }

    /// Resolves a contributed toolbar action definition by plugin and action
    /// id; a missing entry means the plugin no longer declares it.
    pub(in crate::workspace) fn plugin_toolbar_definition(
        &self,
        plugin_id: &str,
        action_id: &str,
        cx: &App,
    ) -> Option<plugin_host::NativePluginTerminalToolbarActionDef> {
        self.plugin_entity
            .read(cx)
            .registry()
            .contributions()
            .terminal_toolbar_actions
            .iter()
            .find(|entry| entry.plugin_id == plugin_id && entry.definition.id == action_id)
            .map(|entry| entry.definition.clone())
    }

    /// Current state id one pane reports for an action; `None` means the
    /// action's declared default state.
    pub(in crate::workspace) fn plugin_toolbar_state(
        &self,
        plugin_id: &str,
        action_id: &str,
        pane_id: PaneId,
    ) -> Option<&String> {
        self.plugin_toolbar_states
            .get(&(plugin_id.to_string(), action_id.to_string(), pane_id))
    }

    /// Stores the state one pane reports for a contributed action, runs the
    /// generic overlay lifecycle for `overlay` states, and re-pushes the pane
    /// specs. The state map is the authority; specs are a render mirror.
    pub(in crate::workspace) fn apply_plugin_toolbar_state(
        &mut self,
        plugin_id: &str,
        action_id: &str,
        state_id: &str,
        pane_id: PaneId,
        window_handle: Option<gpui::AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) {
        let Some(definition) = self.plugin_toolbar_definition(plugin_id, action_id, cx) else {
            return;
        };
        self.plugin_toolbar_states.insert(
            (plugin_id.to_string(), action_id.to_string(), pane_id),
            state_id.to_string(),
        );
        let overlay = definition
            .states
            .iter()
            .find(|state| state.id == state_id)
            .is_some_and(|state| state.overlay);
        self.apply_plugin_overlay_lifecycle(
            plugin_id,
            action_id,
            overlay,
            pane_id,
            window_handle,
            cx,
        );
        self.sync_plugin_toolbar_state(cx);
    }

    /// Registers or releases the pane's overlay membership for the reported
    /// state. Membership is per pane and never preempts another pane: the
    /// card binding follows the active pane instead. A claim opens the card
    /// in place only when the claiming pane is already the bound one;
    /// otherwise the pane only joins the table and the visibility poll opens
    /// the card when focus arrives. The runtime is started before
    /// `toolbar.overlayOpened` so the plugin process can receive it, and
    /// stopped only after `toolbar.overlayClosed` so a closing plugin still
    /// sees the event.
    fn apply_plugin_overlay_lifecycle(
        &mut self,
        plugin_id: &str,
        action_id: &str,
        overlay: bool,
        pane_id: PaneId,
        window_handle: Option<gpui::AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) {
        if overlay {
            // One overlay action per pane: re-reporting the member's own
            // action keeps its entry and generation; a different action
            // replaces it.
            let reclaims = self
                .plugin_overlay_members
                .get(&pane_id)
                .is_some_and(|member| {
                    member.plugin_id == plugin_id && member.action_id == action_id
                });
            let replaced = if reclaims {
                None
            } else {
                self.plugin_overlay_members.remove(&pane_id)
            };
            if let Some(previous) = &replaced {
                // The replaced action loses its membership; demote its chip
                // to the declared default so the state mirror and the member
                // table agree. A definition removed by a plugin update has
                // nothing to demote.
                if let Some(definition) =
                    self.plugin_toolbar_definition(&previous.plugin_id, &previous.action_id, cx)
                {
                    self.plugin_toolbar_states.insert(
                        (
                            previous.plugin_id.clone(),
                            previous.action_id.clone(),
                            pane_id,
                        ),
                        definition.default_state,
                    );
                }
                // The replaced member may still hold the card (its pane kept
                // focus, or it stayed opened while hidden): close it before
                // the new member can take the binding, keeping open/close
                // paired per pane.
                if previous.opened {
                    self.dispatch_native_plugin_event(
                        previous.plugin_id.clone(),
                        plugin_host::NATIVE_PLUGIN_TOOLBAR_OVERLAY_CLOSED_EVENT,
                        serde_json::json!({
                            "actionId": previous.action_id,
                            "paneId": pane_id.0,
                            "reason": OVERLAY_CLOSED_REASON_RELEASED,
                        }),
                        cx,
                    );
                }
            }
            if !reclaims {
                // A fresh generation retires the previous poll loop: it
                // observes the mismatch and exits, so a release followed by a
                // quick reclaim cannot accumulate concurrent loops. The new
                // loop is spawned below through the single-flight guard.
                self.plugin_overlay_generation += 1;
                self.plugin_overlay_members.insert(
                    pane_id,
                    PluginOverlayAnchor {
                        plugin_id: plugin_id.to_string(),
                        action_id: action_id.to_string(),
                        pane_id,
                        window: window_handle,
                        opened: false,
                        visible: false,
                        timer_active: false,
                        generation: self.plugin_overlay_generation,
                    },
                );
            }
            self.sync_plugin_toolbar_runtime(plugin_id, cx);
            if self.active_pane_id(cx) == Some(pane_id) {
                // The new member owns the card now: hand it over from the
                // previous holder in closed-then-opened order, mirroring the
                // poll's rebind.
                self.bind_plugin_overlay(pane_id, cx);
            }
            if let Some(previous) = &replaced {
                // The replaced plugin's runtime may stop only after its
                // overlay closed (dispatched above); otherwise a runtime still
                // engaged by another pane keeps polling for a pane that no
                // longer owns the card.
                if previous.plugin_id != plugin_id {
                    self.sync_plugin_toolbar_runtime(&previous.plugin_id, cx);
                }
            }
            self.ensure_plugin_overlay_visibility_timer(cx);
        } else if self
            .plugin_overlay_members
            .get(&pane_id)
            .is_some_and(|member| member.plugin_id == plugin_id && member.action_id == action_id)
        {
            // Only the member action itself can release its pane's
            // membership; a different action leaving overlay must not touch
            // it. A member that never held the card needs no closed event.
            if let Some(member) = self.plugin_overlay_members.remove(&pane_id) {
                if member.opened {
                    self.dispatch_native_plugin_event(
                        member.plugin_id,
                        plugin_host::NATIVE_PLUGIN_TOOLBAR_OVERLAY_CLOSED_EVENT,
                        serde_json::json!({
                            "actionId": member.action_id,
                            "paneId": pane_id.0,
                            "reason": OVERLAY_CLOSED_REASON_RELEASED,
                        }),
                        cx,
                    );
                }
            }
            self.sync_plugin_toolbar_runtime(plugin_id, cx);
        } else {
            self.sync_plugin_toolbar_runtime(plugin_id, cx);
        }
    }

    /// Moves the single card to `pane_id`: closes the member that currently
    /// holds it (if another one does) and opens the new holder. Members that
    /// never held the card receive no events, so open/close stays strictly
    /// paired per pane. The closed→opened pair is dispatched in the same
    /// batch, so a plugin that keeps per-pane state can treat the handover as
    /// a rebind rather than a teardown.
    fn bind_plugin_overlay(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        let previous_owner = self
            .plugin_overlay_members
            .iter()
            .find(|(owner_pane, member)| member.opened && **owner_pane != pane_id)
            .map(|(owner_pane, _)| *owner_pane);
        if let Some(owner_pane) = previous_owner {
            self.close_plugin_overlay_member(owner_pane, OVERLAY_CLOSED_REASON_REBOUND, cx);
        }
        if let Some(member) = self.plugin_overlay_members.get_mut(&pane_id) {
            if !member.opened {
                member.opened = true;
                let plugin_id = member.plugin_id.clone();
                let payload =
                    serde_json::json!({ "actionId": member.action_id, "paneId": pane_id.0 });
                self.dispatch_native_plugin_event(
                    plugin_id,
                    plugin_host::NATIVE_PLUGIN_TOOLBAR_OVERLAY_OPENED_EVENT,
                    payload,
                    cx,
                );
            }
        }
    }

    /// Dispatches `toolbar.overlayClosed` for the card holder and clears its
    /// open and visibility bookkeeping so a later rebind starts a fresh event
    /// pair. The member keeps its overlay membership and chip state.
    /// `reason` tells the plugin whether the card may be rebound in this same
    /// event batch (`rebound`) or the pane genuinely left the overlay.
    fn close_plugin_overlay_member(
        &mut self,
        pane_id: PaneId,
        reason: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(member) = self.plugin_overlay_members.get_mut(&pane_id) else {
            return;
        };
        member.opened = false;
        member.visible = false;
        let plugin_id = member.plugin_id.clone();
        let payload = serde_json::json!({
            "actionId": member.action_id,
            "paneId": pane_id.0,
            "reason": reason,
        });
        self.dispatch_native_plugin_event(
            plugin_id,
            plugin_host::NATIVE_PLUGIN_TOOLBAR_OVERLAY_CLOSED_EVENT,
            payload,
            cx,
        );
    }

    /// Starts or stops a toolbar plugin's runtime from the authoritative
    /// state map: engaged means at least one pane reports a state other than
    /// the action's declared default. Entries whose definition disappeared
    /// (plugin update) are ignored, matching the chip render fallback.
    fn sync_plugin_toolbar_runtime(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        let engaged =
            self.plugin_toolbar_states
                .iter()
                .any(|((entry_plugin, action_id, _), state)| {
                    entry_plugin == plugin_id
                        && self
                            .plugin_toolbar_definition(entry_plugin, action_id, cx)
                            .is_some_and(|definition| state != &definition.default_state)
                });
        let resolver = self.native_plugin_host_api_resolver(cx);
        self.plugin_entity.update(cx, |plugins, _cx| {
            if engaged {
                let _ = plugins.start_runtime_plugin_activation(plugin_id, resolver);
            } else {
                plugins.stop_runtime_plugin(plugin_id);
            }
        });
    }

    /// Poll-side mirror of the render gate for the member holding the card.
    /// The card only paints in the member window, and `member.window` is
    /// written only at claim time: after the pane moves to another window the
    /// gate fails there while the pane can still be the active one, so the
    /// poll must also require the member to point at the window that renders
    /// it. Otherwise the plugin keeps polling the device for a card the user
    /// cannot see.
    fn plugin_overlay_member_visible(&self, pane_id: PaneId, cx: &App) -> bool {
        let Some(member) = self.plugin_overlay_members.get(&pane_id) else {
            return false;
        };
        if self.active_pane_id(cx) != Some(member.pane_id) {
            return false;
        }
        let Some(member_window) = member.window else {
            return false;
        };
        let Some(main) = self
            .window_registry
            .handle_for_role(window_registry::WindowRole::Main)
        else {
            return false;
        };
        member_window.window_id() == main.window_id()
    }

    /// Single-flight 1s poll that keeps the card bound to the active pane's
    /// membership: when the bound member changes it reports the handover to
    /// the plugins as `toolbar.overlayClosed` then `toolbar.overlayOpened`,
    /// and while a member holds the card it reports focus or window changes as
    /// `toolbar.overlayVisibilityChanged`. Losing focus to a non-member pane
    /// only hides the card, so the holder keeps its open state. The loop owns
    /// no business semantics and self-terminates once the member table is
    /// empty or a newer claim superseded its generation, so at most one loop
    /// is live for the table.
    fn ensure_plugin_overlay_visibility_timer(&mut self, cx: &mut Context<Self>) {
        // The loop serves the whole member table; only the newest claim's
        // member carries it, so the single-flight guard lives there and older
        // loops retire on their generation mismatch.
        let generation = self.plugin_overlay_generation;
        let Some(member) = self
            .plugin_overlay_members
            .values_mut()
            .find(|member| member.generation == generation)
        else {
            return;
        };
        if member.timer_active {
            return;
        }
        member.timer_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                // Background-executor timer (as in the settings watch loop) so
                // the cadence is drivable by test virtual clocks.
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
                let Ok(keep) = this.update(cx, |workspace, cx| {
                    if workspace.plugin_overlay_members.is_empty() {
                        return false;
                    }
                    if workspace.plugin_overlay_generation != generation {
                        // A newer claim owns the loop now; its own loop serves
                        // the table, so retire this one instead of polling the
                        // replacement membership from two loops.
                        return false;
                    }
                    let bound = workspace
                        .active_pane_id(cx)
                        .filter(|pane| workspace.plugin_overlay_members.contains_key(pane));
                    let holder = workspace
                        .plugin_overlay_members
                        .iter()
                        .find(|(_, member)| member.opened)
                        .map(|(pane, _)| *pane);
                    // A bound member takes the card from the previous holder;
                    // an unbound table (focus on a non-member pane) keeps the
                    // holder so the handover stays closed/opened-paired.
                    if let Some(bound_pane) = bound {
                        if bound != holder {
                            workspace.bind_plugin_overlay(bound_pane, cx);
                        }
                    }
                    // Mirror the render gate (window match included) for the
                    // holder: a member whose window moved away, or a holder
                    // that lost focus to a non-member pane, reports invisible.
                    let Some(holder) = workspace
                        .plugin_overlay_members
                        .iter()
                        .find(|(_, member)| member.opened)
                        .map(|(pane, _)| *pane)
                    else {
                        return true;
                    };
                    let visible = workspace.plugin_overlay_member_visible(holder, cx);
                    let changed = workspace
                        .plugin_overlay_members
                        .get(&holder)
                        .is_some_and(|member| member.visible != visible);
                    if changed {
                        let member = &workspace.plugin_overlay_members[&holder];
                        let plugin_id = member.plugin_id.clone();
                        let payload = serde_json::json!({
                            "actionId": member.action_id,
                            "paneId": member.pane_id.0,
                            "visible": visible,
                        });
                        if let Some(member) = workspace.plugin_overlay_members.get_mut(&holder) {
                            member.visible = visible;
                        }
                        workspace.dispatch_native_plugin_event(
                            plugin_id,
                            plugin_host::NATIVE_PLUGIN_TOOLBAR_OVERLAY_VISIBILITY_CHANGED_EVENT,
                            payload,
                            cx,
                        );
                        cx.notify();
                    }
                    true
                }) else {
                    break;
                };
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }

    /// The member the card is bound to: the overlay member in the active
    /// pane. The binding is derived from focus and never stored, so a focus
    /// change rebinds the render immediately while the visibility poll
    /// reports the handover to the plugins.
    fn plugin_overlay_bound(&self, cx: &App) -> Option<&PluginOverlayAnchor> {
        let pane_id = self.active_pane_id(cx)?;
        self.plugin_overlay_members.get(&pane_id)
    }

    /// Whether the overlay card may render in this window; the call site
    /// additionally requires the window to be focused, so an unfocused
    /// member window shows nothing.
    pub(in crate::workspace) fn plugin_overlay_targets_window(
        &self,
        window: gpui::AnyWindowHandle,
        cx: &App,
    ) -> bool {
        self.plugin_overlay_bound(cx)
            .is_some_and(|member| member.window == Some(window))
    }

    /// The card renders only while a member holds the active pane; a closed
    /// pane loses its membership, so the card never follows focus onto another
    /// pane.
    pub(in crate::workspace) fn plugin_overlay_visible(&self, cx: &App) -> bool {
        self.plugin_overlay_bound(cx)
            .is_some_and(|member| self.tab_host.read(cx).panes().contains_key(&member.pane_id))
    }

    /// Bottom-right overlay card rendering the bound member's `overlayTab`
    /// runtime view. Renders nothing until the activated plugin registers its
    /// view; content refresh is the plugin's responsibility.
    pub(in crate::workspace) fn render_plugin_overlay_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(bound) = self.plugin_overlay_bound(cx).cloned() else {
            return div().into_any_element();
        };
        let Some(definition) =
            self.plugin_toolbar_definition(&bound.plugin_id, &bound.action_id, cx)
        else {
            return div().into_any_element();
        };
        let Some(tab_id) = definition.overlay_tab.as_deref() else {
            return div().into_any_element();
        };
        let Some(mut view) = self
            .plugin_entity
            .read(cx)
            .registry()
            .contributions()
            .runtime_tab_view(&bound.plugin_id, tab_id)
        else {
            return div().into_any_element();
        };
        super::plugin_ui::localize_native_plugin_schema(&mut view.schema, self.i18n.locale());
        let title = view
            .schema
            .title
            .clone()
            .unwrap_or_else(|| view.title.clone());
        let body = self.render_native_plugin_declarative_schema(
            &bound.plugin_id,
            "tab",
            &view.tab_id,
            &view.schema,
            cx,
        );
        div()
            .absolute()
            .bottom(px(12.0))
            .right(px(12.0))
            .w(px(340.0))
            .max_h(px(420.0))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .shadow_lg()
            // Panel clicks must not fall through to the terminal surface and
            // steal focus; the schema body keeps its own blur/select handling.
            .on_mouse_down(MouseButton::Left, |_, _, cx: &mut App| {
                cx.stop_propagation();
            })
            .on_scroll(cx.listener(|this, _event, _window, cx| {
                this.close_native_plugin_select(cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .px(px(10.0))
                    .py(px(8.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(title),
                    )
                    .child(
                        div()
                            .id("plugin-overlay-close")
                            .cursor_pointer()
                            .px(px(4.0))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.text_muted))
                            .child("×")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    // Closing the card equals the toolbar chip
                                    // cycling to the default state: it releases
                                    // the bound member and re-derives the
                                    // runtime.
                                    let Some(bound) = this.plugin_overlay_bound(cx).cloned() else {
                                        return;
                                    };
                                    let Some(definition) = this.plugin_toolbar_definition(
                                        &bound.plugin_id,
                                        &bound.action_id,
                                        cx,
                                    ) else {
                                        return;
                                    };
                                    this.apply_plugin_toolbar_state(
                                        &bound.plugin_id,
                                        &bound.action_id,
                                        &definition.default_state,
                                        bound.pane_id,
                                        bound.window,
                                        cx,
                                    );
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    // The card's height is content-driven (no definite height),
                    // so the body must size to its content; a flex_1 basis of
                    // zero would collapse it to an empty window frame.
                    .max_h(px(370.0))
                    .overflow_y_scrollbar()
                    .px(px(10.0))
                    .pb(px(10.0))
                    .child(body),
            )
            .into_any_element()
    }

    /// Removes every toolbar state entry owned by a closed pane, releases the
    /// overlay membership it held, and re-derives the runtime of every
    /// affected plugin. Reconnecting opens a fresh pane that seeds the default
    /// state.
    pub(in crate::workspace) fn plugin_toolbar_pane_closed(
        &mut self,
        pane_id: &PaneId,
        cx: &mut Context<Self>,
    ) {
        let mut affected = Vec::new();
        self.plugin_toolbar_states
            .retain(|(plugin_id, _, entry_pane), _| {
                if entry_pane == pane_id {
                    if !affected.contains(plugin_id) {
                        affected.push(plugin_id.clone());
                    }
                    false
                } else {
                    true
                }
            });
        // A closed pane cannot hold the card: only its holder needs the closed
        // event, and the remaining members rebind through the visibility poll.
        if let Some(member) = self.plugin_overlay_members.remove(pane_id) {
            if member.opened {
                self.dispatch_native_plugin_event(
                    member.plugin_id,
                    plugin_host::NATIVE_PLUGIN_TOOLBAR_OVERLAY_CLOSED_EVENT,
                    serde_json::json!({
                        "actionId": member.action_id,
                        "paneId": pane_id.0,
                        "reason": OVERLAY_CLOSED_REASON_RELEASED,
                    }),
                    cx,
                );
            }
        }
        for plugin_id in affected {
            self.sync_plugin_toolbar_runtime(&plugin_id, cx);
        }
        self.sync_plugin_toolbar_state(cx);
    }

    /// Locale bundles for the plugins contributing toolbar actions, keyed by
    /// plugin id. Rebuilt on every sync so labels follow locale switches and
    /// package updates without a cache to invalidate.
    fn plugin_toolbar_locale_bundles(
        &self,
        contributions: &[plugin_host::NativePluginTerminalToolbarActionContribution],
        cx: &App,
    ) -> HashMap<String, HashMap<String, String>> {
        let plugin_entity = self.plugin_entity.read(cx);
        let tag = plugin_ui::native_plugin_locale_tag(self.i18n.locale());
        let mut bundles = HashMap::new();
        for entry in contributions {
            if bundles.contains_key(&entry.plugin_id) {
                continue;
            }
            let bundle = plugin_entity
                .registry()
                .plugins()
                .iter()
                .find(|plugin| plugin.manifest.id == entry.plugin_id)
                .and_then(|plugin| {
                    let locales_dir = plugin.manifest.locales.as_deref()?;
                    Some(load_plugin_locale_bundle(
                        &plugin.install_dir,
                        locales_dir,
                        tag,
                    ))
                })
                .unwrap_or_default();
            bundles.insert(entry.plugin_id.clone(), bundle);
        }
        bundles
    }

    /// Re-pushes every pane's contributed chip specs (state mirror + plugin
    /// availability). Also re-runs after plugin runtime state changes and
    /// locale switches so chips follow install/disable/activation failures and
    /// label changes without persistence.
    pub(in crate::workspace) fn sync_plugin_toolbar_state(&mut self, cx: &mut Context<Self>) {
        let contributions = self
            .plugin_entity
            .read(cx)
            .registry()
            .contributions()
            .terminal_toolbar_actions
            .clone();
        let bundles = self.plugin_toolbar_locale_bundles(&contributions, cx);
        let panes = self
            .tab_host
            .read(cx)
            .panes()
            .iter()
            .map(|(pane_id, pane)| (*pane_id, pane.clone()))
            .collect::<Vec<_>>();
        for (pane_id, pane) in panes {
            self.push_plugin_toolbar_specs(pane_id, &pane, &contributions, &bundles, cx);
        }
        cx.notify();
    }

    /// Pushes the chip specs for one pane; used when a pane registers so the
    /// new pane renders the contributed default states immediately.
    pub(in crate::workspace) fn push_plugin_toolbar_specs_for_pane(
        &self,
        pane_id: PaneId,
        pane: &gpui::Entity<TerminalPane>,
        cx: &mut Context<Self>,
    ) {
        let contributions = self
            .plugin_entity
            .read(cx)
            .registry()
            .contributions()
            .terminal_toolbar_actions
            .clone();
        let bundles = self.plugin_toolbar_locale_bundles(&contributions, cx);
        self.push_plugin_toolbar_specs(pane_id, pane, &contributions, &bundles, cx);
    }

    fn push_plugin_toolbar_specs(
        &self,
        pane_id: PaneId,
        pane: &gpui::Entity<TerminalPane>,
        contributions: &[plugin_host::NativePluginTerminalToolbarActionContribution],
        bundles: &HashMap<String, HashMap<String, String>>,
        cx: &mut Context<Self>,
    ) {
        let specs = contributions
            .iter()
            .map(|entry| {
                let def = &entry.definition;
                let available = self.native_plugin_available(&entry.plugin_id, cx);
                let bundle = bundles.get(&entry.plugin_id);
                // A missing key shows the raw key, mirroring the host i18n
                // fallback for untranslated labels.
                let label = |key: &str| {
                    bundle
                        .and_then(|bundle| bundle.get(key))
                        .cloned()
                        .unwrap_or_else(|| key.to_string())
                };
                let state_id = self
                    .plugin_toolbar_state(&entry.plugin_id, &def.id, pane_id)
                    .cloned()
                    .unwrap_or_else(|| def.default_state.clone());
                // Validation guarantees `default_state` is declared, so the
                // fallback only covers a stored state a plugin update removed.
                let state_label_key = def
                    .states
                    .iter()
                    .find(|state| state.id == state_id)
                    .or_else(|| {
                        def.states
                            .iter()
                            .find(|state| state.id == def.default_state)
                    })
                    .map(|state| state.label_key.as_str())
                    .unwrap_or(def.label_key.as_str());
                TerminalToolbarActionSpec {
                    id: format!("{}:{}", entry.plugin_id, def.id).into(),
                    label: format!("{} {}", label(&def.label_key), label(state_label_key)),
                    enabled: available,
                    active: state_id != def.default_state,
                    disabled_hint: if available {
                        None
                    } else {
                        def.disabled_hint_key.as_ref().map(|key| label(key))
                    },
                }
            })
            .collect::<Vec<_>>();
        pane.update(cx, |pane, cx| pane.set_toolbar_actions(specs, cx));
    }
}

/// Reads one plugin's locale bundle for the current app locale. The chip must
/// render while the plugin runtime is stopped, so the host reads the package
/// files directly instead of asking the plugin process; a missing tag file
/// falls back to English and an absent bundle leaves raw keys visible.
fn load_plugin_locale_bundle(
    install_dir: &Path,
    locales_dir: &str,
    tag: &str,
) -> HashMap<String, String> {
    let dir = install_dir.join(locales_dir);
    for candidate in [tag, "en"] {
        let Ok(text) = fs::read_to_string(dir.join(format!("{candidate}.json"))) else {
            continue;
        };
        return parse_plugin_locale_bundle(&text);
    }
    HashMap::new()
}

fn parse_plugin_locale_bundle(text: &str) -> HashMap<String, String> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .map(|object| {
            object
                .into_iter()
                .filter_map(|(key, value)| value.as_str().map(|text| (key, text.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_plugin_locale_bundle_prefers_current_tag_and_falls_back_to_english() {
        let dir = tempfile::tempdir().unwrap();
        let locales = dir.path().join("locales");
        std::fs::create_dir(&locales).unwrap();
        std::fs::write(
            locales.join("zh-CN.json"),
            r#"{"toolbarButton":"演示模式","toolbarModeOff":"关闭"}"#,
        )
        .unwrap();
        std::fs::write(
            locales.join("en.json"),
            r#"{"toolbarButton":"Demo Mode","toolbarModeOff":"Off"}"#,
        )
        .unwrap();

        let zh = load_plugin_locale_bundle(dir.path(), "locales", "zh-CN");
        assert_eq!(
            zh.get("toolbarButton").map(String::as_str),
            Some("演示模式")
        );
        assert_eq!(zh.get("toolbarModeOff").map(String::as_str), Some("关闭"));

        let missing_tag = load_plugin_locale_bundle(dir.path(), "locales", "fr-FR");
        assert_eq!(
            missing_tag.get("toolbarButton").map(String::as_str),
            Some("Demo Mode")
        );

        assert!(load_plugin_locale_bundle(dir.path(), "absent", "zh-CN").is_empty());
    }
}

#[cfg(test)]
mod overlay_lifecycle_tests {
    use super::*;
    use crate::workspace::session_manager::tests::isolated_workspace_fixture;
    use gpui::TestAppContext;
    use std::time::Duration;

    /// WorkspaceApp owns process-wide storage, so run the overlay state
    /// machine against an isolated portable fixture directory like the other
    /// workspace-level tests. The shell window registers as Main, which the
    /// visibility poll compares against the member window.
    fn overlay_test_workspace(
        cx: &mut TestAppContext,
    ) -> Option<(Entity<WorkspaceApp>, gpui::AnyWindowHandle)> {
        let _fixture_dir = isolated_workspace_fixture(cx, "OXIDETERM_PLUGIN_OVERLAY_TEST_DIR")?;
        let settings_path = default_settings_path();
        let mut settings = SettingsStore::load_from_path(settings_path).unwrap();
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.settings_mut().onboarding_completed = true;
        settings.save().unwrap();
        // The claim path spawns the visibility poll loop on the workspace.
        cx.executor().allow_parking();
        let (shell, _window_cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        let window = workspace.update(cx, |workspace, _cx| {
            workspace
                .window_registry
                .handle_for_role(window_registry::WindowRole::Main)
                .expect("the shell window registers as main")
        });
        Some((workspace, window))
    }

    /// Installs one tab with three panes so focus can move between overlay
    /// members and a non-member pane; the first pane starts active.
    fn overlay_test_panes(
        workspace: &Entity<WorkspaceApp>,
        cx: &mut TestAppContext,
    ) -> [PaneId; 3] {
        let panes = [PaneId(1), PaneId(2), PaneId(3)];
        workspace.update(cx, |ws, cx| {
            ws.tab_host.update(cx, |host, _cx| {
                host.insert_and_select_main_tab(Tab {
                    id: TabId(1),
                    kind: TabKind::LocalTerminal,
                    title: "overlay-test".into(),
                    title_source: TabTitleSource::Static,
                    root_pane: Some(PaneNode::leaf(panes[0], TerminalSessionId(1))),
                    active_pane_id: Some(panes[0]),
                });
                assert!(host.split_pane(
                    TabId(1),
                    panes[0],
                    PaneId(10),
                    SplitDirection::Horizontal,
                    panes[1],
                    TerminalSessionId(2),
                ));
                assert!(host.split_pane(
                    TabId(1),
                    panes[1],
                    PaneId(11),
                    SplitDirection::Horizontal,
                    panes[2],
                    TerminalSessionId(3),
                ));
                assert!(host.set_active_pane(Some(TabId(1)), panes[0]));
            });
        });
        panes
    }

    #[gpui::test]
    fn second_member_claim_coexists_and_only_the_bound_pane_opens(cx: &mut TestAppContext) {
        let Some((workspace, window)) = overlay_test_workspace(cx) else {
            return;
        };
        let [first, second, _third] = overlay_test_panes(&workspace, cx);
        workspace.update(cx, |ws, cx| {
            // `first` is the active pane, so its claim binds and opens.
            ws.apply_plugin_overlay_lifecycle("p", "a1", true, first, Some(window), cx);
            let member = ws
                .plugin_overlay_members
                .get(&first)
                .expect("first claim registers a member");
            assert_eq!(
                (member.plugin_id.as_str(), member.action_id.as_str()),
                ("p", "a1")
            );
            assert!(member.opened);
            assert!(member.timer_active);

            // H1 (rewritten): a second pane's claim coexists with the first
            // instead of preempting it. The inactive pane only joins the
            // table; no other member is touched and no card opens for it.
            ws.apply_plugin_overlay_lifecycle("p", "a2", true, second, Some(window), cx);
            assert_eq!(ws.plugin_overlay_members.len(), 2);
            let first = ws
                .plugin_overlay_members
                .get(&first)
                .expect("first keeps membership");
            assert!(first.opened, "the bound member keeps the card");
            let second = &ws.plugin_overlay_members[&second];
            assert!(!second.opened, "an inactive claim must not open the card");
            assert_eq!(second.generation, ws.plugin_overlay_generation);
            assert!(
                second.timer_active,
                "the newest claim carries the poll loop"
            );
        });
    }

    #[gpui::test]
    fn same_pane_second_action_replaces_member_and_rebinds_in_place(cx: &mut TestAppContext) {
        let Some((workspace, window)) = overlay_test_workspace(cx) else {
            return;
        };
        let [first, ..] = overlay_test_panes(&workspace, cx);
        workspace.update(cx, |ws, cx| {
            ws.apply_plugin_overlay_lifecycle("p", "a1", true, first, Some(window), cx);
            let (first_generation, first_opened) = {
                let member = &ws.plugin_overlay_members[&first];
                (member.generation, member.opened)
            };
            assert!(first_opened);

            // H1: one overlay action per pane. The second action replaces the
            // member in place (the bound pane keeps the card) with a newer
            // generation; the table still holds exactly one entry.
            ws.apply_plugin_overlay_lifecycle("p", "a2", true, first, Some(window), cx);
            let member = ws
                .plugin_overlay_members
                .get(&first)
                .expect("replacement member");
            assert_eq!(member.action_id, "a2");
            assert!(
                member.opened,
                "the bound pane's card moved to the new action"
            );
            assert!(member.generation > first_generation);
            assert_eq!(ws.plugin_overlay_members.len(), 1);

            // Re-reporting the member's own action keeps its generation.
            let generation = member.generation;
            ws.apply_plugin_overlay_lifecycle("p", "a2", true, first, Some(window), cx);
            assert_eq!(ws.plugin_overlay_members[&first].generation, generation);

            // A non-member action leaving overlay must not release the member.
            ws.apply_plugin_overlay_lifecycle("p", "a1", false, first, Some(window), cx);
            assert_eq!(ws.plugin_overlay_members[&first].action_id, "a2");
        });
    }

    #[gpui::test]
    fn release_drops_only_the_releasing_member(cx: &mut TestAppContext) {
        let Some((workspace, window)) = overlay_test_workspace(cx) else {
            return;
        };
        let [first, second, _third] = overlay_test_panes(&workspace, cx);
        workspace.update(cx, |ws, cx| {
            ws.apply_plugin_overlay_lifecycle("p", "a1", true, first, Some(window), cx);
            ws.apply_plugin_overlay_lifecycle("p", "a2", true, second, Some(window), cx);
            // Releasing the bound member drops only its entry; the other
            // member keeps its membership and chip state.
            ws.apply_plugin_overlay_lifecycle("p", "a1", false, first, Some(window), cx);
            assert!(!ws.plugin_overlay_members.contains_key(&first));
            assert!(ws.plugin_overlay_members.contains_key(&second));
            // Releasing the never-bound member empties the table.
            ws.apply_plugin_overlay_lifecycle("p", "a2", false, second, Some(window), cx);
            assert!(ws.plugin_overlay_members.is_empty());
        });
    }

    #[gpui::test]
    fn release_then_reclaim_starts_a_new_generation(cx: &mut TestAppContext) {
        let Some((workspace, window)) = overlay_test_workspace(cx) else {
            return;
        };
        let [first, ..] = overlay_test_panes(&workspace, cx);
        workspace.update(cx, |ws, cx| {
            ws.apply_plugin_overlay_lifecycle("p", "a1", true, first, Some(window), cx);
            let generation = ws.plugin_overlay_members[&first].generation;
            // The claim path spawned exactly one poll loop for this generation.
            assert!(ws.plugin_overlay_members[&first].timer_active);

            // Release: the member action itself leaves the overlay state.
            ws.apply_plugin_overlay_lifecycle("p", "a1", false, first, Some(window), cx);
            assert!(ws.plugin_overlay_members.is_empty());

            // H2: a reclaim right after release must carry a newer generation
            // so the retired poll loop cannot serve the replacement membership.
            ws.apply_plugin_overlay_lifecycle("p", "a1", true, first, Some(window), cx);
            let member = ws
                .plugin_overlay_members
                .get(&first)
                .expect("reclaim registers");
            assert!(member.generation > generation);
            assert!(member.timer_active);
        });
    }

    #[gpui::test]
    fn pane_close_releases_membership_and_state_entries(cx: &mut TestAppContext) {
        let Some((workspace, window)) = overlay_test_workspace(cx) else {
            return;
        };
        let [first, second, _third] = overlay_test_panes(&workspace, cx);
        workspace.update(cx, |ws, cx| {
            ws.apply_plugin_overlay_lifecycle("p", "a1", true, first, Some(window), cx);
            ws.apply_plugin_overlay_lifecycle("p", "a2", true, second, Some(window), cx);
            ws.plugin_toolbar_states.insert(
                ("p".to_string(), "a1".to_string(), first),
                "overlay".to_string(),
            );

            ws.plugin_toolbar_pane_closed(&first, cx);
            assert!(!ws.plugin_overlay_members.contains_key(&first));
            assert!(
                ws.plugin_overlay_members.contains_key(&second),
                "closing one pane must not drop other members"
            );
            assert!(ws.plugin_toolbar_states.is_empty());
        });
    }

    #[gpui::test]
    fn poll_hands_the_card_between_members_on_focus_change(cx: &mut TestAppContext) {
        let Some((workspace, window)) = overlay_test_workspace(cx) else {
            return;
        };
        let [first, second, _third] = overlay_test_panes(&workspace, cx);
        workspace.update(cx, |ws, cx| {
            ws.apply_plugin_overlay_lifecycle("p", "a1", true, first, Some(window), cx);
            ws.apply_plugin_overlay_lifecycle("p", "a2", true, second, Some(window), cx);
        });
        // One tick first: the active member holds the card and reports visible.
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        workspace.read_with(cx, |ws, _cx| {
            assert!(ws.plugin_overlay_members[&first].visible);
            assert!(!ws.plugin_overlay_members[&second].opened);
        });
        // Focus moves to the second member; the poll rebinds on its next tick.
        workspace.update(cx, |ws, cx| {
            ws.tab_host.update(cx, |host, _cx| {
                assert!(host.set_active_pane(Some(TabId(1)), second));
            });
        });
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        workspace.read_with(cx, |ws, _cx| {
            // The handover closed the previous holder and opened the new one:
            // `opened` records the outstanding `overlayOpened` per member, so
            // the flag flips exactly where the event was dispatched.
            let old_holder = &ws.plugin_overlay_members[&first];
            assert!(!old_holder.opened, "the old holder receives overlayClosed");
            assert!(
                !old_holder.visible,
                "closing resets the visibility mirror after it was visible"
            );
            assert!(
                ws.plugin_overlay_members.contains_key(&first),
                "the old holder keeps its overlay membership"
            );
            let new_holder = &ws.plugin_overlay_members[&second];
            assert!(
                new_holder.opened,
                "the newly bound member receives overlayOpened"
            );
            assert!(
                new_holder.visible,
                "the focused main-window member is visible"
            );
        });
    }

    #[gpui::test]
    fn poll_reports_visibility_and_focus_loss_only_hides(cx: &mut TestAppContext) {
        let Some((workspace, window)) = overlay_test_workspace(cx) else {
            return;
        };
        let [first, _second, third] = overlay_test_panes(&workspace, cx);
        workspace.update(cx, |ws, cx| {
            ws.apply_plugin_overlay_lifecycle("p", "a1", true, first, Some(window), cx);
        });
        // The card renders for the focused member in the main window: the
        // first tick reports visible=true through the mirror.
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        workspace.read_with(cx, |ws, _cx| {
            let member = &ws.plugin_overlay_members[&first];
            assert!(member.opened);
            assert!(member.visible, "the focused main-window member is visible");
        });
        // Focus on a non-member pane hides the card but does not close it:
        // the holder keeps its open state and reports invisible.
        workspace.update(cx, |ws, cx| {
            ws.tab_host.update(cx, |host, _cx| {
                assert!(host.set_active_pane(Some(TabId(1)), third));
            });
        });
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        workspace.read_with(cx, |ws, _cx| {
            let member = &ws.plugin_overlay_members[&first];
            assert!(
                member.opened,
                "losing focus to a non-member pane must not close the card"
            );
            assert!(!member.visible, "the hidden holder reports invisible");
        });
        // The holder stays the single opened member while unbound; no other
        // entry takes the card.
        workspace.read_with(cx, |ws, _cx| {
            assert_eq!(
                ws.plugin_overlay_members
                    .values()
                    .filter(|member| member.opened)
                    .count(),
                1
            );
        });
    }
}
