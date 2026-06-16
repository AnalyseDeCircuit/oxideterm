use std::collections::{HashMap, HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePackageEntry {
    pub id: String,
    pub name: String,
    pub manager: String,
    pub installed_version: String,
    pub candidate_version: String,
    pub arch: String,
    pub repository: String,
    pub status: String,
    pub summary: String,
    pub service_units: Vec<String>,
    pub owner_paths: Vec<String>,
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePackageManager {
    pub name: String,
    pub available: bool,
    pub source: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageCommandCapability {
    #[default]
    Unknown,
    Full,
    Partial,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourcePackageStatus {
    #[default]
    Unknown,
    Available {
        capability: PackageCommandCapability,
        platform: String,
    },
    Unavailable,
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePackageSnapshot {
    pub status: ResourcePackageStatus,
    pub managers: Vec<ResourcePackageManager>,
    pub entries: Vec<ResourcePackageEntry>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PackageFilter {
    #[default]
    All,
    Upgradable,
    Installed,
    Services,
    Apt,
    Dnf,
    Yum,
    Pacman,
    Brew,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageCaptureCommand {
    pub command: String,
    pub capability: PackageCommandCapability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageInspectCommand {
    pub command: String,
    pub capability: PackageCommandCapability,
}

const PACKAGE_UNAVAILABLE_MARKER: &str = "__OXIDE_PACKAGE_UNAVAILABLE__";
const PACKAGE_ERROR_MARKER: &str = "__OXIDE_PACKAGE_ERROR__";
const PACKAGE_CAPABILITY_MARKER: &str = "__OXIDE_PACKAGE_CAPABILITY__";

pub fn build_package_snapshot_command(os_type: &str) -> PackageCaptureCommand {
    let (command, capability) = match package_os(os_type) {
        PackageOs::MacOs => (
            build_macos_package_snapshot_command(),
            PackageCommandCapability::Partial,
        ),
        PackageOs::Linux | PackageOs::Unknown => (
            build_linux_package_snapshot_command(),
            PackageCommandCapability::Partial,
        ),
        PackageOs::Bsd => (
            build_bsd_package_snapshot_command(),
            PackageCommandCapability::Partial,
        ),
        PackageOs::Windows => (
            build_windows_package_snapshot_command(),
            PackageCommandCapability::Unknown,
        ),
    };
    PackageCaptureCommand {
        command,
        capability,
    }
}

pub fn build_package_inspect_command(
    os_type: &str,
    manager: &str,
    package_name: &str,
) -> Result<PackageInspectCommand, String> {
    let package_name = package_name.trim();
    if package_name.is_empty() {
        return Err("Package name is empty.".to_string());
    }
    let quoted = shell_quote(package_name);
    let (command, capability) = match (package_os(os_type), manager.trim()) {
        (PackageOs::MacOs, "brew") => (
            format!("HOMEBREW_NO_AUTO_UPDATE=1 brew info {quoted}"),
            PackageCommandCapability::Partial,
        ),
        (PackageOs::Linux | PackageOs::Unknown, "apt") => (
            format!("apt show {quoted} 2>/dev/null || dpkg -s {quoted}"),
            PackageCommandCapability::Partial,
        ),
        (PackageOs::Linux | PackageOs::Unknown, "dnf") => (
            format!("dnf --cacheonly info {quoted} || rpm -qi {quoted}"),
            PackageCommandCapability::Partial,
        ),
        (PackageOs::Linux | PackageOs::Unknown, "yum") => (
            format!("yum --cacheonly info {quoted} || rpm -qi {quoted}"),
            PackageCommandCapability::Partial,
        ),
        (PackageOs::Linux | PackageOs::Unknown, "pacman") => (
            format!("pacman -Qi {quoted}"),
            PackageCommandCapability::Partial,
        ),
        _ => {
            return Err(format!(
                "Package inspection is not supported for manager '{}'.",
                manager.trim()
            ));
        }
    };
    Ok(PackageInspectCommand {
        command,
        capability,
    })
}

pub fn parse_package_snapshot(output: &str) -> ResourcePackageSnapshot {
    let Some(section) = extract_section(output, "PACKAGES") else {
        return ResourcePackageSnapshot::default();
    };

    let mut entries = Vec::new();
    let mut managers = Vec::new();
    let mut owners = Vec::new();
    let mut capability = PackageCommandCapability::Unknown;
    let mut platform = "unknown".to_string();

    for line in section
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.trim().is_empty())
    {
        if line == PACKAGE_UNAVAILABLE_MARKER {
            return ResourcePackageSnapshot {
                status: ResourcePackageStatus::Unavailable,
                managers: Vec::new(),
                entries: Vec::new(),
            };
        }
        if let Some(message) = line.strip_prefix(PACKAGE_ERROR_MARKER) {
            return ResourcePackageSnapshot {
                status: ResourcePackageStatus::Error {
                    message: clean_marker_message(message, "Package command failed."),
                },
                managers: Vec::new(),
                entries: Vec::new(),
            };
        }
        if let Some((next_capability, next_platform)) = parse_package_capability_line(line) {
            capability = next_capability;
            platform = next_platform;
            continue;
        }
        if let Some(manager) = parse_package_manager_line(line) {
            managers.push(manager);
            continue;
        }
        if let Some(owner) = parse_package_owner_line(line) {
            owners.push(owner);
            continue;
        }
        if let Some(entry) = parse_package_row_line(line) {
            entries.push(entry);
        }
    }

    merge_package_entries(&mut entries);
    attach_package_owners(&mut entries, &owners);
    dedupe_and_sort_package_managers(&mut managers);
    dedupe_and_sort_package_entries(&mut entries);

    ResourcePackageSnapshot {
        status: ResourcePackageStatus::Available {
            capability,
            platform,
        },
        managers,
        entries,
    }
}

pub fn visible_package_rows(
    entries: &[ResourcePackageEntry],
    query: &str,
    filter: PackageFilter,
) -> Vec<ResourcePackageEntry> {
    let query = query.trim().to_lowercase();
    entries
        .iter()
        .filter(|entry| package_matches_filter(entry, filter))
        .filter(|entry| query.is_empty() || package_matches_query(entry, &query))
        .cloned()
        .collect()
}

pub fn package_row_signature(entry: &ResourcePackageEntry) -> u64 {
    let mut hasher = DefaultHasher::new();
    entry.id.hash(&mut hasher);
    entry.name.hash(&mut hasher);
    entry.manager.hash(&mut hasher);
    entry.installed_version.hash(&mut hasher);
    entry.candidate_version.hash(&mut hasher);
    entry.arch.hash(&mut hasher);
    entry.repository.hash(&mut hasher);
    entry.status.hash(&mut hasher);
    entry.summary.hash(&mut hasher);
    entry.service_units.hash(&mut hasher);
    entry.owner_paths.hash(&mut hasher);
    entry.source.hash(&mut hasher);
    hasher.finish()
}

pub fn package_filter_label_key(filter: PackageFilter) -> &'static str {
    match filter {
        PackageFilter::All => "sidebar.host_packages.filters.all",
        PackageFilter::Upgradable => "sidebar.host_packages.filters.upgradable",
        PackageFilter::Installed => "sidebar.host_packages.filters.installed",
        PackageFilter::Services => "sidebar.host_packages.filters.services",
        PackageFilter::Apt => "sidebar.host_packages.filters.apt",
        PackageFilter::Dnf => "sidebar.host_packages.filters.dnf",
        PackageFilter::Yum => "sidebar.host_packages.filters.yum",
        PackageFilter::Pacman => "sidebar.host_packages.filters.pacman",
        PackageFilter::Brew => "sidebar.host_packages.filters.brew",
    }
}

pub fn package_status_label_key(status: &str) -> &'static str {
    match status.trim().to_lowercase().as_str() {
        "upgradable" | "outdated" => "sidebar.host_packages.status.upgradable",
        "installed" => "sidebar.host_packages.status.installed",
        _ => "sidebar.host_packages.status.unknown",
    }
}

fn build_linux_package_snapshot_command() -> String {
    concat!(
        "echo '===PACKAGES==='; ",
        "echo '__OXIDE_PACKAGE_CAPABILITY__\tpartial\tlinux_packages'; ",
        "oxide_pkg_any=0; ",
        "if command -v apt >/dev/null 2>&1 && command -v dpkg-query >/dev/null 2>&1; then ",
        "oxide_pkg_any=1; echo 'MANAGER\tapt\ttrue\tapt'; ",
        "apt list --upgradable 2>/dev/null | awk 'NR>1 && $0 !~ /^Listing/ { line=$0; split(line,a,\"/\"); name=a[1]; rest=a[2]; n=split(rest,b,\" \"); repo=b[1]; cand=b[2]; arch=b[3]; installed=\"\"; marker=\"upgradable from: \"; start=index(line,marker); if(start>0){ installed=substr(line,start+length(marker)); sub(/\\].*/,\"\",installed) } printf \"ROW\\t%s\\tapt\\t%s\\t%s\\t%s\\t%s\\tupgradable\\t\\t\\t\\tapt\\n\", name, installed, cand, arch, repo }'; ",
        "dpkg-query -W -f='ROW\\t${binary:Package}\\tapt\\t${Version}\\t\\t${Architecture}\\t\\tinstalled\\t\\t\\t\\tdpkg\\n' 2>/dev/null; ",
        "if command -v systemctl >/dev/null 2>&1; then for oxide_unit in /etc/systemd/system/*.service /usr/lib/systemd/system/*.service /lib/systemd/system/*.service; do [ -e \"$oxide_unit\" ] || continue; oxide_owner=$(dpkg -S \"$oxide_unit\" 2>/dev/null | head -n 1 | cut -d: -f1 | cut -d, -f1); [ -n \"$oxide_owner\" ] && printf 'OWNER\\tapt\\t%s\\t%s\\t%s\\tdpkg\\n' \"$oxide_owner\" \"$(basename \"$oxide_unit\")\" \"$oxide_unit\"; done; fi; ",
        "fi; ",
        "oxide_rpm_manager=''; if command -v dnf >/dev/null 2>&1 && command -v rpm >/dev/null 2>&1; then oxide_rpm_manager='dnf'; elif command -v yum >/dev/null 2>&1 && command -v rpm >/dev/null 2>&1; then oxide_rpm_manager='yum'; fi; ",
        "if [ -n \"$oxide_rpm_manager\" ]; then oxide_pkg_any=1; printf 'MANAGER\\t%s\\ttrue\\trpm\\n' \"$oxide_rpm_manager\"; ",
        "$oxide_rpm_manager --cacheonly check-update -q 2>/dev/null | awk -v manager=\"$oxide_rpm_manager\" 'NF>=3 && $1 !~ /^(Last|Obsoleting|Security:)/ { arch=$1; sub(/^.*\\./,\"\",arch); name=$1; sub(\"\\\\.\" arch \"$\",\"\",name); printf \"ROW\\t%s\\t%s\\t\\t%s\\t%s\\t%s\\tupgradable\\t\\t\\t\\t%s\\n\", name, manager, $2, arch, $3, manager }'; ",
        "rpm -qa --qf 'ROW\\t%{NAME}\\t'\"$oxide_rpm_manager\"'\\t%{VERSION}-%{RELEASE}\\t\\t%{ARCH}\\t\\tinstalled\\t%{SUMMARY}\\t\\t\\trpm\\n' 2>/dev/null; ",
        "if command -v systemctl >/dev/null 2>&1; then for oxide_unit in /etc/systemd/system/*.service /usr/lib/systemd/system/*.service /lib/systemd/system/*.service; do [ -e \"$oxide_unit\" ] || continue; oxide_owner=$(rpm -qf --qf '%{NAME}\\n' \"$oxide_unit\" 2>/dev/null | head -n 1); [ -n \"$oxide_owner\" ] && printf 'OWNER\\t%s\\t%s\\t%s\\t%s\\trpm\\n' \"$oxide_rpm_manager\" \"$oxide_owner\" \"$(basename \"$oxide_unit\")\" \"$oxide_unit\"; done; fi; ",
        "fi; ",
        "if command -v pacman >/dev/null 2>&1; then oxide_pkg_any=1; echo 'MANAGER\tpacman\ttrue\tpacman'; ",
        "pacman -Qu 2>/dev/null | awk 'NF>=4 { printf \"ROW\\t%s\\tpacman\\t%s\\t%s\\t\\t\\tupgradable\\t\\t\\t\\tpacman\\n\", $1, $2, $4 }'; ",
        "pacman -Q 2>/dev/null | awk 'NF>=2 { printf \"ROW\\t%s\\tpacman\\t%s\\t\\t\\t\\tinstalled\\t\\t\\t\\tpacman\\n\", $1, $2 }'; ",
        "if command -v systemctl >/dev/null 2>&1; then for oxide_unit in /etc/systemd/system/*.service /usr/lib/systemd/system/*.service /lib/systemd/system/*.service; do [ -e \"$oxide_unit\" ] || continue; oxide_owner=$(pacman -Qoq \"$oxide_unit\" 2>/dev/null | head -n 1); [ -n \"$oxide_owner\" ] && printf 'OWNER\\tpacman\\t%s\\t%s\\t%s\\tpacman\\n' \"$oxide_owner\" \"$(basename \"$oxide_unit\")\" \"$oxide_unit\"; done; fi; ",
        "fi; ",
        "if [ \"$oxide_pkg_any\" -eq 0 ]; then echo '__OXIDE_PACKAGE_UNAVAILABLE__'; fi; ",
        "echo '===PACKAGES_END==='"
    )
    .to_string()
}

fn build_macos_package_snapshot_command() -> String {
    concat!(
        "echo '===PACKAGES==='; ",
        "if command -v brew >/dev/null 2>&1; then ",
        "echo '__OXIDE_PACKAGE_CAPABILITY__\tpartial\tmacos_brew'; ",
        "echo 'MANAGER\tbrew\ttrue\tbrew'; ",
        "HOMEBREW_NO_AUTO_UPDATE=1 brew outdated --verbose 2>/dev/null | awk '{ name=$1; installed=\"\"; candidate=\"\"; if (match($0, /\\([^)]*\\)/)) installed=substr($0,RSTART+1,RLENGTH-2); if (index($0,\" < \")>0) candidate=$NF; printf \"ROW\\t%s\\tbrew\\t%s\\t%s\\t\\t\\toutdated\\t\\t\\t\\tbrew\\n\", name, installed, candidate }'; ",
        "HOMEBREW_NO_AUTO_UPDATE=1 brew list --versions 2>/dev/null | awk 'NF>=1 { name=$1; $1=\"\"; sub(/^ /,\"\"); printf \"ROW\\t%s\\tbrew\\t%s\\t\\t\\t\\tinstalled\\t\\t\\t\\tbrew\\n\", name, $0 }'; ",
        "HOMEBREW_NO_AUTO_UPDATE=1 brew services list 2>/dev/null | awk 'NR>1 && NF>=1 { path=(NF>=4 ? $4 : \"\"); printf \"OWNER\\tbrew\\t%s\\t%s\\t%s\\tbrew\\n\", $1, $1, path }'; ",
        "else echo '__OXIDE_PACKAGE_UNAVAILABLE__'; fi; ",
        "echo '===PACKAGES_END==='"
    )
    .to_string()
}

fn build_bsd_package_snapshot_command() -> String {
    concat!(
        "echo '===PACKAGES==='; ",
        "if command -v pkg >/dev/null 2>&1; then ",
        "echo '__OXIDE_PACKAGE_CAPABILITY__\tpartial\tbsd_pkg'; ",
        "echo 'MANAGER\tpkg\ttrue\tpkg'; ",
        "pkg version -v 2>/dev/null | awk 'NF>=3 && $2 ~ /[<>]/ { printf \"ROW\\t%s\\tpkg\\t%s\\t\\t\\t\\tupgradable\\t\\t\\t\\tpkg\\n\", $1, $3 }'; ",
        "pkg query 'ROW\\t%n\\tpkg\\t%v\\t\\t%q\\t%R\\tinstalled\\t%c\\t\\t\\tpkg' 2>/dev/null; ",
        "else echo '__OXIDE_PACKAGE_UNAVAILABLE__'; fi; ",
        "echo '===PACKAGES_END==='"
    )
    .to_string()
}

fn build_windows_package_snapshot_command() -> String {
    concat!(
        "powershell -NoProfile -ExecutionPolicy Bypass -Command \"",
        "Write-Output '===PACKAGES===';",
        "Write-Output ('__OXIDE_PACKAGE_CAPABILITY__'+[char]9+'unknown'+[char]9+'windows_unsupported');",
        "Write-Output '__OXIDE_PACKAGE_UNAVAILABLE__';",
        "Write-Output '===PACKAGES_END==='",
        "\""
    )
    .to_string()
}

fn parse_package_capability_line(line: &str) -> Option<(PackageCommandCapability, String)> {
    let payload = line.strip_prefix(PACKAGE_CAPABILITY_MARKER)?;
    let parts = payload
        .trim_start_matches('\t')
        .split('\t')
        .collect::<Vec<_>>();
    let capability = match parts.first().copied().unwrap_or("unknown") {
        "full" => PackageCommandCapability::Full,
        "partial" => PackageCommandCapability::Partial,
        _ => PackageCommandCapability::Unknown,
    };
    Some((
        capability,
        parts
            .get(1)
            .copied()
            .unwrap_or("unknown")
            .trim()
            .to_string(),
    ))
}

fn parse_package_manager_line(line: &str) -> Option<ResourcePackageManager> {
    let payload = line.strip_prefix("MANAGER\t")?;
    let parts = payload.splitn(3, '\t').collect::<Vec<_>>();
    if parts.len() != 3 {
        return None;
    }
    Some(ResourcePackageManager {
        name: clean(parts[0]),
        available: parse_bool(parts[1]),
        source: clean(parts[2]),
    })
}

fn parse_package_row_line(line: &str) -> Option<ResourcePackageEntry> {
    let payload = line.strip_prefix("ROW\t")?;
    let parts = payload.splitn(11, '\t').collect::<Vec<_>>();
    if parts.len() != 11 {
        return None;
    }
    let name = clean(parts[0]);
    let manager = clean(parts[1]);
    let arch = clean(parts[4]);
    Some(ResourcePackageEntry {
        id: package_entry_id(&manager, &name, &arch),
        name,
        manager,
        installed_version: clean(parts[2]),
        candidate_version: clean(parts[3]),
        arch,
        repository: clean(parts[5]),
        status: normalize_package_status(parts[6]),
        summary: clean(parts[7]),
        service_units: split_list(parts[8]),
        owner_paths: split_list(parts[9]),
        source: clean(parts[10]),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PackageOwnerRow {
    manager: String,
    name: String,
    service_unit: String,
    owner_path: String,
    source: String,
}

fn parse_package_owner_line(line: &str) -> Option<PackageOwnerRow> {
    let payload = line.strip_prefix("OWNER\t")?;
    let parts = payload.splitn(5, '\t').collect::<Vec<_>>();
    if parts.len() != 5 {
        return None;
    }
    Some(PackageOwnerRow {
        manager: clean(parts[0]),
        name: clean(parts[1]),
        service_unit: clean(parts[2]),
        owner_path: clean(parts[3]),
        source: clean(parts[4]),
    })
}

fn merge_package_entries(entries: &mut Vec<ResourcePackageEntry>) {
    let mut merged: HashMap<String, ResourcePackageEntry> = HashMap::new();
    for entry in entries.drain(..) {
        let id = entry.id.clone();
        merged
            .entry(id)
            .and_modify(|current| merge_package_entry(current, &entry))
            .or_insert(entry);
    }
    entries.extend(merged.into_values());
}

fn merge_package_entry(current: &mut ResourcePackageEntry, next: &ResourcePackageEntry) {
    if current.installed_version.is_empty() {
        current.installed_version = next.installed_version.clone();
    }
    if current.candidate_version.is_empty() {
        current.candidate_version = next.candidate_version.clone();
    }
    if current.repository.is_empty() {
        current.repository = next.repository.clone();
    }
    if current.summary.is_empty() {
        current.summary = next.summary.clone();
    }
    if current.source.is_empty() {
        current.source = next.source.clone();
    }
    if package_is_upgradable(next) {
        current.status = next.status.clone();
        if !next.candidate_version.is_empty() {
            current.candidate_version = next.candidate_version.clone();
        }
    }
    append_unique_values(&mut current.service_units, &next.service_units);
    append_unique_values(&mut current.owner_paths, &next.owner_paths);
}

fn attach_package_owners(entries: &mut [ResourcePackageEntry], owners: &[PackageOwnerRow]) {
    for owner in owners {
        if owner.name.is_empty() || owner.manager.is_empty() {
            continue;
        }
        if let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.manager == owner.manager && entry.name == owner.name)
        {
            if !owner.service_unit.is_empty() {
                append_unique_value(&mut entry.service_units, owner.service_unit.clone());
            }
            if !owner.owner_path.is_empty() {
                append_unique_value(&mut entry.owner_paths, owner.owner_path.clone());
            }
            if entry.source.is_empty() {
                entry.source = owner.source.clone();
            }
        }
    }
}

fn dedupe_and_sort_package_managers(managers: &mut Vec<ResourcePackageManager>) {
    let mut seen = HashSet::new();
    managers.retain(|manager| seen.insert(manager.name.clone()));
    managers.sort_by(|left, right| left.name.cmp(&right.name));
}

fn dedupe_and_sort_package_entries(entries: &mut Vec<ResourcePackageEntry>) {
    entries.sort_by(|left, right| {
        package_status_rank(&left.status)
            .cmp(&package_status_rank(&right.status))
            .then(left.manager.cmp(&right.manager))
            .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then(left.arch.cmp(&right.arch))
    });
}

fn package_matches_filter(entry: &ResourcePackageEntry, filter: PackageFilter) -> bool {
    match filter {
        PackageFilter::All => true,
        PackageFilter::Upgradable => package_is_upgradable(entry),
        PackageFilter::Installed => entry.status == "installed",
        PackageFilter::Services => !entry.service_units.is_empty() || !entry.owner_paths.is_empty(),
        PackageFilter::Apt => entry.manager == "apt",
        PackageFilter::Dnf => entry.manager == "dnf",
        PackageFilter::Yum => entry.manager == "yum",
        PackageFilter::Pacman => entry.manager == "pacman",
        PackageFilter::Brew => entry.manager == "brew",
    }
}

fn package_matches_query(entry: &ResourcePackageEntry, query: &str) -> bool {
    [
        entry.name.as_str(),
        entry.manager.as_str(),
        entry.installed_version.as_str(),
        entry.candidate_version.as_str(),
        entry.arch.as_str(),
        entry.repository.as_str(),
        entry.status.as_str(),
        entry.summary.as_str(),
        entry.source.as_str(),
    ]
    .iter()
    .any(|value| value.to_lowercase().contains(query))
        || entry
            .service_units
            .iter()
            .chain(entry.owner_paths.iter())
            .any(|value| value.to_lowercase().contains(query))
}

fn package_entry_id(manager: &str, name: &str, arch: &str) -> String {
    if arch.trim().is_empty() {
        format!("{manager}:{name}")
    } else {
        format!("{manager}:{name}:{arch}")
    }
}

fn package_is_upgradable(entry: &ResourcePackageEntry) -> bool {
    matches!(entry.status.as_str(), "upgradable" | "outdated")
}

fn package_status_rank(status: &str) -> u8 {
    if matches!(status, "upgradable" | "outdated") {
        0
    } else {
        1
    }
}

fn normalize_package_status(status: &str) -> String {
    match clean(status).to_lowercase().as_str() {
        "upgradable" | "outdated" => "upgradable".to_string(),
        "installed" => "installed".to_string(),
        other if !other.is_empty() => other.to_string(),
        _ => "unknown".to_string(),
    }
}

fn append_unique_values(target: &mut Vec<String>, values: &[String]) {
    for value in values {
        append_unique_value(target, value.clone());
    }
}

fn append_unique_value(target: &mut Vec<String>, value: String) {
    if !value.trim().is_empty() && !target.iter().any(|existing| existing == &value) {
        target.push(value);
    }
}

fn split_list(value: &str) -> Vec<String> {
    clean(value)
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "available"
    )
}

fn clean(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

fn clean_marker_message(message: &str, fallback: &str) -> String {
    let cleaned = message.trim_start_matches('\t').trim();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned.to_string()
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn extract_section<'a>(output: &'a str, name: &str) -> Option<&'a str> {
    let start = format!("==={name}===");
    let end = format!("==={name}_END===");
    let after_start = output.split_once(&start)?.1;
    Some(
        after_start
            .split_once(&end)
            .map_or(after_start, |(section, _)| section),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PackageOs {
    Linux,
    MacOs,
    Bsd,
    Windows,
    Unknown,
}

fn package_os(os_type: &str) -> PackageOs {
    match os_type {
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => PackageOs::Linux,
        "macOS" | "macos" | "Darwin" => PackageOs::MacOs,
        "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => PackageOs::Bsd,
        "Windows" | "windows" => PackageOs::Windows,
        _ => PackageOs::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_merges_apt_rows_with_service_owner() {
        let output = concat!(
            "===PACKAGES===\n",
            "__OXIDE_PACKAGE_CAPABILITY__\tpartial\tlinux_packages\n",
            "MANAGER\tapt\ttrue\tapt\n",
            "ROW\topenssh-server\tapt\t1:9.6p1-3\t1:9.6p1-4\tamd64\tjammy-updates\tupgradable\t\t\t\tapt\n",
            "ROW\topenssh-server\tapt\t1:9.6p1-3\t\tamd64\t\tinstalled\tOpenSSH server\t\t\tdpkg\n",
            "OWNER\tapt\topenssh-server\tssh.service\t/usr/lib/systemd/system/ssh.service\tdpkg\n",
            "===PACKAGES_END===\n"
        );

        let snapshot = parse_package_snapshot(output);
        let rows = visible_package_rows(&snapshot.entries, "ssh.service", PackageFilter::Services);

        assert_eq!(
            snapshot.status,
            ResourcePackageStatus::Available {
                capability: PackageCommandCapability::Partial,
                platform: "linux_packages".to_string(),
            }
        );
        assert_eq!(snapshot.managers.len(), 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "upgradable");
        assert_eq!(rows[0].candidate_version, "1:9.6p1-4");
        assert_eq!(rows[0].summary, "OpenSSH server");
        assert_eq!(rows[0].service_units, vec!["ssh.service"]);
    }

    #[test]
    fn parses_brew_rows_without_auto_update_dependency() {
        let output = concat!(
            "===PACKAGES===\n",
            "__OXIDE_PACKAGE_CAPABILITY__\tpartial\tmacos_brew\n",
            "MANAGER\tbrew\ttrue\tbrew\n",
            "ROW\topenssl@3\tbrew\t3.3.1\t3.3.2\t\t\toutdated\t\t\t\tbrew\n",
            "ROW\topenssl@3\tbrew\t3.3.1\t\t\t\tinstalled\t\t\t\tbrew\n",
            "OWNER\tbrew\tpostgresql@16\tpostgresql@16\t/Users/me/Library/LaunchAgents/homebrew.mxcl.postgresql@16.plist\tbrew\n",
            "ROW\tpostgresql@16\tbrew\t16.3\t\t\t\tinstalled\tPostgreSQL\t\t\tbrew\n",
            "===PACKAGES_END===\n"
        );

        let snapshot = parse_package_snapshot(output);
        let outdated = visible_package_rows(&snapshot.entries, "openssl", PackageFilter::Brew);
        let services =
            visible_package_rows(&snapshot.entries, "LaunchAgents", PackageFilter::Services);

        assert_eq!(outdated.len(), 1);
        assert_eq!(outdated[0].status, "upgradable");
        assert_eq!(outdated[0].candidate_version, "3.3.2");
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "postgresql@16");
    }

    #[test]
    fn package_commands_keep_managers_read_only() {
        let linux = build_package_snapshot_command("Linux");
        let mac = build_package_snapshot_command("macOS");
        let windows = build_package_snapshot_command("Windows");

        assert!(linux.command.contains("apt list --upgradable"));
        assert!(linux.command.contains("--cacheonly check-update"));
        assert!(linux.command.contains("pacman -Qu"));
        assert!(!linux.command.contains(" upgrade"));
        assert!(mac.command.contains("HOMEBREW_NO_AUTO_UPDATE=1"));
        assert!(!mac.command.contains("brew update"));
        assert!(windows.command.contains("__OXIDE_PACKAGE_UNAVAILABLE__"));
    }

    #[test]
    fn inspect_commands_quote_package_names() {
        let apt = build_package_inspect_command("Linux", "apt", "weird'name").unwrap();
        let brew = build_package_inspect_command("macOS", "brew", "openssl@3").unwrap();

        assert!(apt.command.contains("'weird'\"'\"'name'"));
        assert!(brew.command.contains("HOMEBREW_NO_AUTO_UPDATE=1 brew info"));
        assert!(build_package_inspect_command("Linux", "apt", " ").is_err());
    }
}
