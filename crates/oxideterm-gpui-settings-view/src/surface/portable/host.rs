use super::*;
use crate::{
    known_hosts_page::{KnownHostsRequest, PortableKnownHostEntry, PortableKnownHostsSnapshot},
    proxy_page::{NetworkProxyTestSnapshot, ProxyRequest},
};
use oxideterm_settings::SettingsUpstreamProxyConfig;

pub enum PortableKnownHostsAction {
    Refresh,
    Delete(PortableKnownHostEntry),
}

/// The host owns this one probe. An entered password moves into the request;
/// with no draft the host may resolve the profile’s saved credential.
pub struct PortableProxyProbe {
    pub id: u64,
    pub proxy: SettingsUpstreamProxyConfig,
    pub host: String,
    pub port: u16,
    pub password: Option<zeroize::Zeroizing<String>>,
}

#[derive(Default)]
pub(super) struct KnownHosts {
    pub snapshot: PortableKnownHostsSnapshot,
    pub revision: u64,
    pub query: String,
    pub selected: Option<PortableKnownHostEntry>,
    pub scroll: ScrollHandle,
    pub handler: Option<Rc<dyn Fn(PortableKnownHostsAction, &mut App)>>,
}

pub(super) struct ProxyProbe {
    pub form: NetworkProxyTestSnapshot,
    pub handler: Option<Rc<dyn Fn(PortableProxyProbe, &mut App)>>,
    next_id: u64,
    last: Option<(u64, SettingsUpstreamProxyConfig)>,
    pub password_profile: Option<SettingsUpstreamProxyConfig>,
}

impl Default for ProxyProbe {
    fn default() -> Self {
        Self {
            form: NetworkProxyTestSnapshot {
                test_host: String::new(),
                test_port: "22".into(),
                test_pending: false,
                test_result: None,
            },
            handler: None,
            next_id: 0,
            last: None,
            password_profile: None,
        }
    }
}

impl ProxyProbe {
    pub fn clear_result(&mut self) {
        self.last = None;
        self.form.test_pending = false;
        self.form.test_result = None;
    }
}

impl PageState {
    pub(super) fn proxy_profile_changed(&mut self) {
        let profile = self.settings.network.upstream_proxy.as_ref();
        if self
            .probe
            .last
            .as_ref()
            .is_some_and(|(_, last)| Some(last) != profile)
        {
            self.probe.clear_result();
        }
        if self
            .probe
            .password_profile
            .as_ref()
            .is_some_and(|last| Some(last) != profile)
        {
            self.clear_proxy_password();
        }
    }

    pub(super) fn clear_proxy_password(&mut self) {
        drop(self.fields.take_password());
        self.probe.password_profile = None;
    }
}

impl PortableSettingsSurface {
    /// Supply gateway rows, including their loading or error state. Without an
    /// action handler the list remains searchable and fingerprints can be copied.
    pub fn set_known_hosts(&self, snapshot: PortableKnownHostsSnapshot, cx: &mut App) {
        self.state.update(cx, |s, cx| {
            s.known_hosts.snapshot = snapshot;
            s.known_hosts.selected = None;
            s.known_hosts.revision += 1;
            cx.notify();
        });
    }

    /// Complete refreshes and confirmed deletions with `set_known_hosts`.
    pub fn on_known_hosts_action(
        &self,
        handler: impl Fn(PortableKnownHostsAction, &mut App) + 'static,
        cx: &mut App,
    ) {
        self.state.update(cx, |s, cx| {
            s.known_hosts.handler = Some(Rc::new(handler));
            cx.notify();
        });
    }

    /// Enables the test button. Return elapsed milliseconds or a displayable error
    /// using the request ID; edits to the route invalidate its previous result.
    pub fn on_proxy_probe(
        &self,
        handler: impl Fn(PortableProxyProbe, &mut App) + 'static,
        cx: &mut App,
    ) {
        self.state.update(cx, |s, cx| {
            s.probe.handler = Some(Rc::new(handler));
            cx.notify();
        });
    }

    pub fn set_proxy_probe_result(&self, id: u64, result: Result<u128, String>, cx: &mut App) {
        self.state.update(cx, |s, cx| {
            if s.probe.form.test_pending
                && s.probe.last.as_ref().is_some_and(|(last, _)| *last == id)
            {
                s.probe.form.test_pending = false;
                s.probe.form.test_result = Some(result);
                cx.notify();
            }
        });
    }
}

impl PageState {
    pub(super) fn known_hosts_action(
        &mut self,
        request: KnownHostsRequest,
        cx: &mut Context<Self>,
    ) {
        let hosts = &mut self.known_hosts;
        let Some(handler) = hosts.handler.clone() else {
            return;
        };
        if hosts.snapshot.loading {
            return;
        }
        let action = match request {
            KnownHostsRequest::Refresh => PortableKnownHostsAction::Refresh,
            KnownHostsRequest::Select(entry) => {
                hosts.selected =
                    entry.filter(|e| e.can_remove && hosts.snapshot.entries.contains(e));
                cx.notify();
                return;
            }
            KnownHostsRequest::Delete(entry) if hosts.selected.as_ref() == Some(&entry) => {
                PortableKnownHostsAction::Delete(entry)
            }
            KnownHostsRequest::Delete(_) => return,
        };
        hosts.selected = None;
        hosts.snapshot.loading = true;
        hosts.snapshot.error = None;
        // Release the page borrow before calling a host that may reply synchronously.
        cx.defer(move |cx| handler(action, cx));
        cx.notify();
    }

    pub(super) fn proxy_action(&mut self, request: ProxyRequest, cx: &mut Context<Self>) {
        if let Some(edit) = request.preference_edit() {
            self.fields.blur();
            edit(&mut self.settings);
            self.changed(cx);
        } else if matches!(request, ProxyRequest::Test) {
            let probe = &mut self.probe;
            let (Some(handler), Some(proxy), Ok(port)) = (
                probe.handler.clone(),
                self.settings.network.upstream_proxy.clone(),
                probe.form.test_port.trim().parse(),
            ) else {
                return;
            };
            let host = probe.form.test_host.trim();
            if probe.form.test_pending || host.is_empty() {
                return;
            }
            probe.next_id += 1;
            probe.last = Some((probe.next_id, proxy.clone()));
            let password = self.fields.take_password();
            probe.password_profile = None;
            let request = PortableProxyProbe {
                id: probe.next_id,
                proxy,
                host: host.into(),
                port,
                password: (!password.is_empty()).then_some(password),
            };
            probe.form.test_pending = true;
            probe.form.test_result = None;
            cx.defer(move |cx| handler(request, cx));
            cx.notify();
        }
    }
}
