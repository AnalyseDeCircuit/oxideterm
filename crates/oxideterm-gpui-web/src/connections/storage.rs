use super::{
    BleProfile, CatalogRow, Endpoint, PortableCatalogData, ProfileDetails,
    SessionManagerOpenTarget, SftpProfile, Workspace, known_service_label,
};
use gpui::Context;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/src/connection_catalog.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=readCatalog)]
    async fn read_catalog(scope: &str, defaults: String) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=writeCatalog)]
    async fn write_catalog(
        scope: &str,
        previous: String,
        proposed: String,
    ) -> Result<JsValue, JsValue>;
}

fn scope(demo: bool) -> &'static str {
    if demo { "demo" } else { "browser" }
}

// Serialize the saved connection, never the credential-bearing form, terminal, or device handle.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Protocol {
    Ssh(Endpoint),
    Sftp(SftpProfile),
    Ble(BleProfile),
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Profile {
    name: String,
    group: Option<String>,
    tags: Vec<String>,
    color: Option<String>,
    icon_background_color: Option<String>,
    details: ProfileDetails,
    protocol: Protocol,
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Snapshot {
    groups: BTreeSet<String>,
    profiles: BTreeMap<String, Profile>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    recent: BTreeMap<String, String>,
}

impl Snapshot {
    pub(crate) async fn load(demo: bool) -> Result<Self, ()> {
        let defaults = Self::capture(
            &super::catalog(demo),
            &HashMap::new(),
            None,
            &HashMap::new(),
        )?;
        let value = read_catalog(
            scope(demo),
            serde_json::to_string(&defaults).map_err(|_| ())?,
        )
        .await
        .map_err(|_| ())?;
        let data: Self = serde_json::from_str(&value.as_string().ok_or(())?).map_err(|_| ())?;
        for (id, p) in &data.profiles {
            let ble = matches!(p.protocol, Protocol::Ble(_));
            if id.is_empty() || super::is_ble_node(id) != ble || (!demo && !ble) {
                return Err(());
            }
        }
        Ok(data)
    }

    pub(crate) fn capture(
        data: &PortableCatalogData,
        details: &HashMap<String, ProfileDetails>,
        sftp: Option<&HashMap<String, SftpProfile>>,
        ble: &HashMap<String, BleProfile>,
    ) -> Result<Self, ()> {
        let mut profiles = BTreeMap::new();
        for row in &data.items {
            let id = row.id();
            let protocol = match &row.target {
                SessionManagerOpenTarget::Connection(_) => Protocol::Ssh(Endpoint {
                    host: row.host.clone(),
                    port: row.port.try_into().map_err(|_| ())?,
                    username: row.username.clone(),
                    path: String::new(),
                }),
                SessionManagerOpenTarget::StandaloneSftp(_) => {
                    Protocol::Sftp(sftp.and_then(|p| p.get(id)).cloned().ok_or(())?)
                }
                SessionManagerOpenTarget::Ble(_) => Protocol::Ble(ble.get(id).cloned().ok_or(())?),
                _ => return Err(()),
            };
            profiles.insert(
                id.into(),
                Profile {
                    name: row.name.clone(),
                    group: row.group.clone(),
                    tags: row.tags.clone(),
                    color: row.color.clone(),
                    icon_background_color: row.icon_background_color.clone(),
                    details: details.get(id).cloned().unwrap_or_default(),
                    protocol,
                },
            );
        }
        Ok(Self {
            groups: data.groups.iter().cloned().collect(),
            profiles,
            recent: data
                .items
                .iter()
                .filter_map(|row| Some((row.id().to_owned(), row.last_used.clone()?)))
                .collect(),
        })
    }

    pub(crate) fn restore(
        mut self,
        data: &mut PortableCatalogData,
        details: &mut HashMap<String, ProfileDetails>,
        mut sftp: Option<&mut HashMap<String, SftpProfile>>,
        ble: &mut HashMap<String, BleProfile>,
    ) {
        data.groups = self.groups.into_iter().collect();
        data.items.clear();
        for (id, p) in self.profiles {
            let target = match &p.protocol {
                Protocol::Ssh(_) => SessionManagerOpenTarget::Connection(id.clone()),
                Protocol::Sftp(_) => SessionManagerOpenTarget::StandaloneSftp(id.clone()),
                Protocol::Ble(_) => SessionManagerOpenTarget::Ble(id.clone()),
            };
            let mut row = CatalogRow::new(target, p.name);
            match p.protocol {
                Protocol::Ssh(ep) => apply_endpoint(&mut row, &ep),
                Protocol::Sftp(profile) => {
                    apply_endpoint(&mut row, &profile.primary);
                    if let Some(second) = &profile.secondary {
                        row.subtitle = format!("{} ↔ {}", row.subtitle, second.label());
                    }
                    if let Some(sftp) = sftp.as_deref_mut() {
                        sftp.insert(id.clone(), profile);
                    }
                }
                Protocol::Ble(profile) => {
                    row.host = profile.device_name.clone();
                    row.port = 0;
                    row.subtitle = profile
                        .config
                        .service_uuid
                        .map(known_service_label)
                        .unwrap_or_else(|| "BLE GATT".into());
                    ble.insert(id.clone(), profile);
                }
            }
            if let Some(icon) =
                oxideterm_gpui_connections::icons::session_icon_from_id(Some(&p.details.icon))
            {
                row.icon = icon;
            }
            row.last_used = self.recent.remove(&id);
            row.group = p.group;
            row.tags = p.tags;
            row.color = p.color;
            row.icon_background_color = p.icon_background_color;
            details.insert(id, p.details);
            data.items.push(row);
        }
    }
}

fn apply_endpoint(row: &mut CatalogRow, ep: &Endpoint) {
    row.host = ep.host.clone();
    row.port = ep.port.into();
    row.username = ep.username.clone();
    row.subtitle = ep.label();
}

pub(crate) struct CatalogStorage {
    saved: Option<Snapshot>,
    writing: bool,
    dirty: bool,
    conflict: bool,
    imports: usize,
}
impl CatalogStorage {
    pub(crate) fn new(saved: Option<Snapshot>) -> Self {
        Self {
            saved,
            writing: false,
            dirty: false,
            conflict: false,
            imports: 0,
        }
    }
    pub(super) fn imported(&mut self, count: usize) {
        self.imports += count;
    }
    pub(super) fn can_record_use(&self) -> bool {
        self.saved.is_some() && !self.conflict
    }
    pub(crate) fn unavailable(&self) -> bool {
        self.saved.is_none()
    }
}

impl Workspace {
    pub(super) fn persist_catalog(&mut self, cx: &mut Context<Self>) {
        if self.catalog_storage.writing {
            self.catalog_storage.dirty = true;
            return;
        }
        if self.catalog_storage.conflict {
            self.notice("sessionManager.browser_conflict", cx);
            return;
        }
        let Some(base) = &self.catalog_storage.saved else {
            self.notice("sessionManager.browser_load_failed", cx);
            return;
        };
        let Ok(next) = Snapshot::capture(
            &self.catalog_data,
            &self.profile_details,
            self.files.as_ref().map(|f| &f.sftp_profiles),
            &self.ble_profiles,
        ) else {
            self.notice("sessionManager.browser_not_saved", cx);
            return;
        };
        if &next == base {
            return;
        }
        let (Ok(previous), Ok(proposed)) =
            (serde_json::to_string(base), serde_json::to_string(&next))
        else {
            self.notice("sessionManager.browser_not_saved", cx);
            return;
        };
        self.catalog_storage.writing = true;
        let imports = std::mem::take(&mut self.catalog_storage.imports);
        let key = scope(self.demo);
        cx.spawn(async move |owner, cx| {
            let result = write_catalog(key, previous, proposed).await;
            let _ = owner.update(cx, |h, cx| {
                h.catalog_storage.writing = false;
                let dirty = std::mem::take(&mut h.catalog_storage.dirty);
                match result.ok().and_then(|v| v.as_bool()) {
                    Some(true) => {
                        h.catalog_storage.saved = Some(next);
                        if imports > 0 {
                            h.notifications_notice(
                                h.i18n
                                    .t("settings_view.connections.ssh_config.batch_import_success")
                                    .replace("{{count}}", &imports.to_string()),
                                cx,
                            );
                        }
                        if dirty {
                            h.persist_catalog(cx);
                        }
                    }
                    Some(false) => {
                        h.catalog_storage.conflict = true;
                        h.notice("sessionManager.browser_conflict", cx);
                    }
                    None => h.notice("sessionManager.browser_not_saved", cx),
                }
            });
        })
        .detach();
    }
}
