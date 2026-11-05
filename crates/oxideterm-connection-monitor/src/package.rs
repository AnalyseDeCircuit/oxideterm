use std::collections::{HashMap, HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::shell::shell_quote;

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
    Winget,
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
            PackageCommandCapability::Partial,
        ),
    };
    PackageCaptureCommand {
        command,
        capability,
    }
}

/// Rejects an identifier a shell would interpret, then double-quotes it.
///
/// The line reaches cmd.exe, PowerShell, or a POSIX shell, and double quotes do
/// not stop `$(...)`, backticks, or `%VAR%` expansion. Winget identifiers are
/// restricted to the characters below, so anything else is refused instead of
/// escaped: a host reporting a package id such as `Acme.$(notepad)` must not be
/// able to run a command when the user upgrades or uninstalls that package.
fn winget_argument(value: &str) -> Result<String, String> {
    let allowed = |ch: char| {
        ch.is_ascii_alphanumeric()
            || matches!(ch, '.' | '-' | '_' | '+' | '\\' | '{' | '}' | '@' | '/')
    };
    if value.is_empty() || !value.chars().all(allowed) {
        return Err("Package id contains characters a shell would interpret.".to_string());
    }
    Ok(format!("\"{value}\""))
}

/// Builds the command a terminal runs to upgrade one package in place.
///
/// Only managers whose upgrade verb is unambiguous are covered; the UI only
/// offers the action for the same set.
pub fn build_package_upgrade_command(
    os_type: &str,
    manager: &str,
    package_id: &str,
) -> Result<PackageInspectCommand, String> {
    let package_id = package_id.trim();
    if package_id.is_empty() {
        return Err("Package id is empty.".to_string());
    }
    let (command, capability) = match (package_os(os_type), manager.trim()) {
        (PackageOs::Windows, "winget") => (
            format!(
                "winget upgrade --id {} --accept-source-agreements --accept-package-agreements --disable-interactivity",
                winget_argument(package_id)?
            ),
            PackageCommandCapability::Partial,
        ),
        (_, manager) => return Err(format!("Upgrading through {manager} is not supported.")),
    };
    Ok(PackageInspectCommand {
        command,
        capability,
    })
}

/// Builds the uninstall command for packages the manager can remove by id.
pub fn build_package_uninstall_command(
    os_type: &str,
    manager: &str,
    package_id: &str,
) -> Result<PackageInspectCommand, String> {
    let package_id = package_id.trim();
    if package_id.is_empty() {
        return Err("Package id is empty.".to_string());
    }
    let (command, capability) = match (package_os(os_type), manager.trim()) {
        (PackageOs::Windows, "winget") => (
            format!(
                "winget uninstall --id {} --accept-source-agreements --disable-interactivity",
                winget_argument(package_id)?
            ),
            PackageCommandCapability::Partial,
        ),
        (_, manager) => return Err(format!("Uninstalling through {manager} is not supported.")),
    };
    Ok(PackageInspectCommand {
        command,
        capability,
    })
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
        (PackageOs::Windows, "winget") => (
            format!(
                "winget show {} --accept-source-agreements",
                winget_argument(package_name)?
            ),
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
        // Distinguish "no Packages section at all" from "the section was cut
        // off": only the latter has a start marker without its end marker.
        return if output.contains("===PACKAGES===") {
            ResourcePackageSnapshot {
                status: ResourcePackageStatus::Error {
                    message: "Package output was truncated.".to_string(),
                },
                managers: Vec::new(),
                entries: Vec::new(),
            }
        } else {
            ResourcePackageSnapshot::default()
        };
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

/// Produces a stable identity signature without treating sampled package metadata as row geometry.
pub fn package_row_signature(entry: &ResourcePackageEntry) -> u64 {
    let mut hasher = DefaultHasher::new();
    entry.id.hash(&mut hasher);
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
        PackageFilter::Winget => "sidebar.host_packages.filters.winget",
    }
}

/// The package manager a manager-specific filter selects, if any.
pub fn package_filter_manager(filter: PackageFilter) -> Option<&'static str> {
    match filter {
        PackageFilter::Apt => Some("apt"),
        PackageFilter::Dnf => Some("dnf"),
        PackageFilter::Yum => Some("yum"),
        PackageFilter::Pacman => Some("pacman"),
        PackageFilter::Brew => Some("brew"),
        PackageFilter::Winget => Some("winget"),
        PackageFilter::All
        | PackageFilter::Upgradable
        | PackageFilter::Installed
        | PackageFilter::Services => None,
    }
}

/// The manager filter for a manager name reported by a snapshot.
pub fn package_filter_for_manager(manager: &str) -> Option<PackageFilter> {
    let manager = manager.trim();
    [
        PackageFilter::Apt,
        PackageFilter::Dnf,
        PackageFilter::Yum,
        PackageFilter::Pacman,
        PackageFilter::Brew,
        PackageFilter::Winget,
    ]
    .into_iter()
    .find(|filter| package_filter_manager(*filter) == Some(manager))
}

/// Base filters are always offered; manager filters follow the managers the
/// selected host reports, so a Linux host never shows `winget` and a Windows
/// host never shows `apt`.
pub fn package_filter_options(managers: &[ResourcePackageManager]) -> Vec<PackageFilter> {
    let mut filters = vec![
        PackageFilter::All,
        PackageFilter::Upgradable,
        PackageFilter::Installed,
        PackageFilter::Services,
    ];
    for manager in managers {
        if let Some(filter) = package_filter_for_manager(&manager.name)
            && !filters.contains(&filter)
        {
            filters.push(filter);
        }
    }
    filters
}

pub fn package_status_label_key(status: &str) -> &'static str {
    match status.trim().to_lowercase().as_str() {
        "upgradable" | "outdated" => "sidebar.host_packages.status.upgradable",
        "installed" => "sidebar.host_packages.status.installed",
        _ => "sidebar.host_packages.status.unknown",
    }
}

fn build_linux_package_snapshot_command() -> String {
    // Package snapshots share the user's SSH connection with terminals. Keep
    // them to manager detection plus cached/upgradable rows; full inventories
    // and service-owner mapping are explicit inspect/diagnostic work.
    concat!(
        "echo '===PACKAGES==='; ",
        "echo '__OXIDE_PACKAGE_CAPABILITY__\tpartial\tlinux_packages'; ",
        "oxide_pkg_any=0; ",
        "if command -v apt >/dev/null 2>&1 && command -v dpkg-query >/dev/null 2>&1; then ",
        "oxide_pkg_any=1; echo 'MANAGER\tapt\ttrue\tapt'; ",
        "apt list --upgradable 2>/dev/null | awk 'NR>1 && $0 !~ /^Listing/ { line=$0; split(line,a,\"/\"); name=a[1]; rest=a[2]; n=split(rest,b,\" \"); repo=b[1]; cand=b[2]; arch=b[3]; installed=\"\"; marker=\"upgradable from: \"; start=index(line,marker); if(start>0){ installed=substr(line,start+length(marker)); sub(/\\].*/,\"\",installed) } printf \"ROW\\t%s\\tapt\\t%s\\t%s\\t%s\\t%s\\tupgradable\\t\\t\\t\\tapt\\n\", name, installed, cand, arch, repo }'; ",
        "fi; ",
        "oxide_rpm_manager=''; if command -v dnf >/dev/null 2>&1 && command -v rpm >/dev/null 2>&1; then oxide_rpm_manager='dnf'; elif command -v yum >/dev/null 2>&1 && command -v rpm >/dev/null 2>&1; then oxide_rpm_manager='yum'; fi; ",
        "if [ -n \"$oxide_rpm_manager\" ]; then oxide_pkg_any=1; printf 'MANAGER\\t%s\\ttrue\\trpm\\n' \"$oxide_rpm_manager\"; ",
        "oxide_rpm_out=$($oxide_rpm_manager --cacheonly check-update -q 2>&1); oxide_rpm_status=$?; ",
        // dnf4 exits 100 when updates are listed and dnf5 exits 0; any other
        // status is a real failure (for example an empty cache-only metadata
        // store) that must reach the UI instead of looking like zero packages.
        "if [ \"$oxide_rpm_status\" -eq 0 ] || [ \"$oxide_rpm_status\" -eq 100 ]; then printf '%s\\n' \"$oxide_rpm_out\" | awk -v manager=\"$oxide_rpm_manager\" 'NF>=3 && $1 !~ /^(Last|Obsoleting|Security:)/ { arch=$1; sub(/^.*\\./,\"\",arch); name=$1; sub(\"\\\\.\" arch \"$\",\"\",name); printf \"ROW\\t%s\\t%s\\t\\t%s\\t%s\\t%s\\tupgradable\\t\\t\\t\\t%s\\n\", name, manager, $2, arch, $3, manager }'; ",
        "else printf '__OXIDE_PACKAGE_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_rpm_out\" | head -n 1 | tr '\\t' ' ')\"; fi; ",
        "fi; ",
        "if command -v pacman >/dev/null 2>&1; then oxide_pkg_any=1; echo 'MANAGER\tpacman\ttrue\tpacman'; ",
        "pacman -Qu 2>/dev/null | awk 'NF>=4 { printf \"ROW\\t%s\\tpacman\\t%s\\t%s\\t\\t\\tupgradable\\t\\t\\t\\tpacman\\n\", $1, $2, $4 }'; ",
        "fi; ",
        "if [ \"$oxide_pkg_any\" -eq 0 ]; then echo '__OXIDE_PACKAGE_UNAVAILABLE__'; fi; ",
        "echo '===PACKAGES_END==='"
    )
    .to_string()
}

fn build_macos_package_snapshot_command() -> String {
    // Homebrew can be slow on large installations. Avoid automatic full
    // inventory and services scans; users can inspect a selected formula.
    concat!(
        "echo '===PACKAGES==='; ",
        "if command -v brew >/dev/null 2>&1; then ",
        "echo '__OXIDE_PACKAGE_CAPABILITY__\tpartial\tmacos_brew'; ",
        "echo 'MANAGER\tbrew\ttrue\tbrew'; ",
        "HOMEBREW_NO_AUTO_UPDATE=1 brew outdated --verbose 2>/dev/null | awk '{ name=$1; installed=\"\"; candidate=\"\"; if (match($0, /\\([^)]*\\)/)) installed=substr($0,RSTART+1,RLENGTH-2); if (index($0,\" < \")>0) candidate=$NF; printf \"ROW\\t%s\\tbrew\\t%s\\t%s\\t\\t\\toutdated\\t\\t\\t\\tbrew\\n\", name, installed, candidate }'; ",
        "else echo '__OXIDE_PACKAGE_UNAVAILABLE__'; fi; ",
        "echo '===PACKAGES_END==='"
    )
    .to_string()
}

fn build_bsd_package_snapshot_command() -> String {
    // Keep BSD package snapshots to update checks. Full pkg query output can be
    // large over SSH and should be requested explicitly later.
    concat!(
        "echo '===PACKAGES==='; ",
        "if command -v pkg >/dev/null 2>&1; then ",
        "echo '__OXIDE_PACKAGE_CAPABILITY__\tpartial\tbsd_pkg'; ",
        "echo 'MANAGER\tpkg\ttrue\tpkg'; ",
        "pkg version -v 2>/dev/null | awk 'NF>=3 && $2 ~ /[<>]/ { printf \"ROW\\t%s\\tpkg\\t%s\\t\\t\\t\\tupgradable\\t\\t\\t\\tpkg\\n\", $1, $3 }'; ",
        "else echo '__OXIDE_PACKAGE_UNAVAILABLE__'; fi; ",
        "echo '===PACKAGES_END==='"
    )
    .to_string()
}

fn build_windows_package_snapshot_command() -> String {
    // `winget list` prints a localized fixed-width table with no JSON output.
    // The separator is one dash run, so column starts come from the header
    // labels, measured in display columns: winget pads CJK names by glyph
    // width, and slicing by .NET character index would split the next column.
    concat!(
        "powershell -NoProfile -ExecutionPolicy Bypass -Command \"",
        "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8;",
        "Write-Output '===PACKAGES===';",
        "function Oxide-W([int]$c){if(($c -ge 0x1100 -and $c -le 0x115F) -or ($c -ge 0x2E80 -and $c -le 0x303E) -or ($c -ge 0x3041 -and $c -le 0x33FF) -or ($c -ge 0x3400 -and $c -le 0x4DBF) -or ($c -ge 0x4E00 -and $c -le 0x9FFF) -or ($c -ge 0xA000 -and $c -le 0xA4CF) -or ($c -ge 0xAC00 -and $c -le 0xD7A3) -or ($c -ge 0xF900 -and $c -le 0xFAFF) -or ($c -ge 0xFE30 -and $c -le 0xFE4F) -or ($c -ge 0xFF00 -and $c -le 0xFF60) -or ($c -ge 0xFFE0 -and $c -le 0xFFE6)){2}else{1}};",
        "function Oxide-S([string]$s,[int]$a,[int]$w){$p=0;$o='';foreach($ch in $s.ToCharArray()){$cw=Oxide-W([int]$ch);if($p -ge $a -and ($p+$cw) -le ($a+$w)){$o+=$ch};$p+=$cw;if($p -ge ($a+$w)){break}};$o.Trim()};",
        "if(-not (Get-Command winget -ErrorAction SilentlyContinue)){Write-Output '__OXIDE_PACKAGE_UNAVAILABLE__';}else{",
        "Write-Output ('__OXIDE_PACKAGE_CAPABILITY__'+[char]9+'partial'+[char]9+'windows_winget');",
        "Write-Output ('MANAGER'+[char]9+'winget'+[char]9+'true'+[char]9+'winget');",
        "$lines=@(& winget list --accept-source-agreements --disable-interactivity 2>$null);",
        "$sep=-1;for($i=0;$i -lt $lines.Count;$i++){if($lines[$i] -match '^-{20,}\\s*$'){$sep=$i;break}};",
        "if($sep -gt 0){",
        // Localized headers put single spaces inside one label (`장치 ID`), so
        // only a gap of two or more display columns separates columns. A
        // layout that does not yield exactly the five known columns is
        // reported as unavailable instead of being sliced at wrong offsets.
        "$header=$lines[$sep-1];$starts=@();$p=0;$gap=0;",
        "foreach($ch in $header.ToCharArray()){$cw=Oxide-W([int]$ch);if($ch -eq ' '){$gap+=$cw}else{if($starts.Count -eq 0 -or $gap -ge 2){$starts+=$p};$gap=0};$p+=$cw};",
        // winget omits the Available column when nothing is upgradable, so a
        // four-column table still lists installed packages.
        "if($starts.Count -eq 4 -or $starts.Count -eq 5){",
        "$hasAvailable=$starts.Count -eq 5;",
        "for($i=$sep+1;$i -lt $lines.Count;$i++){",
        "$line=$lines[$i];if($line.Trim().Length -eq 0){continue};",
        "$cells=@();",
        "for($c=0;$c -lt $starts.Count;$c++){$a=$starts[$c];$w=if($c+1 -lt $starts.Count){$starts[$c+1]-$a}else{1000};$cells+=Oxide-S $line $a $w};",
        "$name=$cells[0];$id=$cells[1];$version=$cells[2];$available='';if($hasAvailable -and $cells.Count -ge 4){$available=$cells[3]};",
        "$status='installed';if($available.Length -gt 0){$status='upgradable'};",
        "Write-Output ('ROW'+[char]9+$name+[char]9+'winget'+[char]9+$version+[char]9+$available+[char]9+''+[char]9+$id+[char]9+$status+[char]9+''+[char]9+''+[char]9+''+[char]9+'windows_winget')",
        "}}else{Write-Output ('__OXIDE_PACKAGE_ERROR__'+[char]9+'winget_list_unavailable')}",
        "}else{Write-Output ('__OXIDE_PACKAGE_ERROR__'+[char]9+'winget_list_unavailable')}};",
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
        PackageFilter::Winget => entry.manager == "winget",
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

/// Returns the section body, or `None` when the output stops before the end
/// marker. Every generated package command prints that marker last, so a
/// missing one means the capture was truncated or killed and the rows it did
/// return are incomplete.
fn extract_section<'a>(output: &'a str, name: &str) -> Option<&'a str> {
    let start = format!("==={name}===");
    let end = format!("==={name}_END===");
    let after_start = output.split_once(&start)?.1;
    after_start.split_once(&end).map(|(section, _)| section)
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
    fn package_snapshots_merge_versions_and_attach_service_owners() {
        for (platform, manager, output, expected, searches) in [
            (
                "linux_packages",
                "apt",
                concat!(
                    "===PACKAGES===\n",
                    "__OXIDE_PACKAGE_CAPABILITY__\tpartial\tlinux_packages\n",
                    "MANAGER\tapt\ttrue\tapt\n",
                    "ROW\topenssh-server\tapt\t1:9.6p1-3\t1:9.6p1-4\tamd64\tjammy-updates\tupgradable\t\t\t\tapt\n",
                    "ROW\topenssh-server\tapt\t1:9.6p1-3\t\tamd64\t\tinstalled\tOpenSSH server\t\t\tdpkg\n",
                    "OWNER\tapt\topenssh-server\tssh.service\t/usr/lib/systemd/system/ssh.service\tdpkg\n",
                    "===PACKAGES_END===\n"
                ),
                vec![(
                    "openssh-server",
                    "upgradable",
                    "1:9.6p1-4",
                    "OpenSSH server",
                    vec!["ssh.service"],
                )],
                vec![(
                    "ssh.service",
                    PackageFilter::Services,
                    vec!["openssh-server"],
                )],
            ),
            (
                "macos_brew",
                "brew",
                concat!(
                    "===PACKAGES===\n",
                    "__OXIDE_PACKAGE_CAPABILITY__\tpartial\tmacos_brew\n",
                    "MANAGER\tbrew\ttrue\tbrew\n",
                    "ROW\topenssl@3\tbrew\t3.3.1\t3.3.2\t\t\toutdated\t\t\t\tbrew\n",
                    "ROW\topenssl@3\tbrew\t3.3.1\t\t\t\tinstalled\t\t\t\tbrew\n",
                    "OWNER\tbrew\tpostgresql@16\tpostgresql@16\t/Users/me/Library/LaunchAgents/homebrew.mxcl.postgresql@16.plist\tbrew\n",
                    "ROW\tpostgresql@16\tbrew\t16.3\t\t\t\tinstalled\tPostgreSQL\t\t\tbrew\n",
                    "===PACKAGES_END===\n"
                ),
                vec![
                    ("openssl@3", "upgradable", "3.3.2", "", vec![]),
                    (
                        "postgresql@16",
                        "installed",
                        "",
                        "PostgreSQL",
                        vec!["postgresql@16"],
                    ),
                ],
                vec![
                    ("openssl", PackageFilter::Brew, vec!["openssl@3"]),
                    (
                        "LaunchAgents",
                        PackageFilter::Services,
                        vec!["postgresql@16"],
                    ),
                ],
            ),
            (
                "windows_winget",
                "winget",
                concat!(
                    "===PACKAGES===\n",
                    "__OXIDE_PACKAGE_CAPABILITY__\tpartial\twindows_winget\n",
                    "MANAGER\twinget\ttrue\twinget\n",
                    "ROW\tCMake\twinget\t4.4.4\t\t\tKitware.CMake\tinstalled\t\t\t\twindows_winget\n",
                    "ROW\tPowerShell\twinget\t7.6.8.0\t7.6.9.0\t\tMicrosoft.PowerShell\tupgradable\t\t\t\twindows_winget\n",
                    "===PACKAGES_END===\n"
                ),
                vec![
                    ("PowerShell", "upgradable", "7.6.9.0", "", vec![]),
                    ("CMake", "installed", "", "", vec![]),
                ],
                vec![("powershell", PackageFilter::Winget, vec!["PowerShell"])],
            ),
        ] {
            let snapshot = parse_package_snapshot(output);
            assert_eq!(
                snapshot.status,
                ResourcePackageStatus::Available {
                    capability: PackageCommandCapability::Partial,
                    platform: platform.into(),
                }
            );
            assert_eq!(
                snapshot
                    .managers
                    .iter()
                    .map(|entry| (entry.name.as_str(), entry.available))
                    .collect::<Vec<_>>(),
                [(manager, true)]
            );
            assert_eq!(
                snapshot
                    .entries
                    .iter()
                    .map(|entry| (
                        entry.name.as_str(),
                        entry.status.as_str(),
                        entry.candidate_version.as_str(),
                        entry.summary.as_str(),
                        entry
                            .service_units
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>(),
                    ))
                    .collect::<Vec<_>>(),
                expected,
                "{platform}"
            );
            for (query, filter, names) in searches {
                let rows = visible_package_rows(&snapshot.entries, query, filter);
                assert_eq!(
                    rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
                    names,
                    "{platform}: {query}"
                );
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn rpm_check_update_failures_are_reported_instead_of_zero_rows() {
        let command = build_package_snapshot_command("Linux");
        let fixture = "rpm() { return 0; }; dnf() { \
            if [ \"$DNF_MODE\" = updates ]; then \
            printf '%s\\n' 'Last metadata expiration check: 0:00:01 ago' \
            'kernel.x86_64 5.14.0-427.el9 rhel9-baseos'; return 100; fi; \
            if [ \"$DNF_MODE\" = empty ]; then return 0; fi; \
            echo 'Cache-only enabled but no cache for repo baseos' >&2; return 1; \
        }; ";
        let run = |mode: &str| {
            let output = std::process::Command::new("sh")
                .args(["-c", &format!("{fixture}{}", command.command)])
                .env("DNF_MODE", mode)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            parse_package_snapshot(&String::from_utf8(output.stdout).unwrap())
        };

        let updates = run("updates");
        assert!(matches!(
            updates.status,
            ResourcePackageStatus::Available { .. }
        ));
        // The generated command also probes the host's real apt/pacman, so
        // only the fixture's dnf rows are asserted.
        fn dnf_rows(snapshot: &ResourcePackageSnapshot) -> Vec<(&str, &str, &str, &str, &str)> {
            snapshot
                .entries
                .iter()
                .filter(|entry| entry.manager == "dnf")
                .map(|entry| {
                    (
                        entry.name.as_str(),
                        entry.candidate_version.as_str(),
                        entry.arch.as_str(),
                        entry.repository.as_str(),
                        entry.status.as_str(),
                    )
                })
                .collect()
        }
        assert_eq!(
            dnf_rows(&updates),
            [(
                "kernel",
                "5.14.0-427.el9",
                "x86_64",
                "rhel9-baseos",
                "upgradable"
            )]
        );

        let empty = run("empty");
        assert!(matches!(
            empty.status,
            ResourcePackageStatus::Available { .. }
        ));
        assert_eq!(
            empty
                .managers
                .iter()
                .filter(|manager| manager.name == "dnf")
                .map(|manager| manager.name.as_str())
                .collect::<Vec<_>>(),
            ["dnf"]
        );
        assert!(dnf_rows(&empty).is_empty());

        let failed = run("failed");
        assert_eq!(
            failed.status,
            ResourcePackageStatus::Error {
                message: "Cache-only enabled but no cache for repo baseos".to_string()
            }
        );
    }

    #[test]
    fn package_commands_keep_managers_read_only() {
        let linux = build_package_snapshot_command("Linux");
        let mac = build_package_snapshot_command("macOS");
        let bsd = build_package_snapshot_command("FreeBSD");
        let windows = build_package_snapshot_command("Windows");

        assert!(linux.command.contains("apt list --upgradable"));
        assert!(linux.command.contains("--cacheonly check-update"));
        assert!(linux.command.contains("pacman -Qu"));
        assert!(!linux.command.contains("dpkg-query -W"));
        assert!(!linux.command.contains("rpm -qa"));
        assert!(!linux.command.contains("pacman -Q 2"));
        assert!(!linux.command.contains("OWNER\\t"));
        assert!(!linux.command.contains(" upgrade"));
        assert!(mac.command.contains("HOMEBREW_NO_AUTO_UPDATE=1"));
        assert!(mac.command.contains("brew outdated"));
        assert!(!mac.command.contains("brew list --versions"));
        assert!(!mac.command.contains("brew services list"));
        assert!(!mac.command.contains("brew update"));
        assert!(bsd.command.contains("pkg version -v"));
        assert!(!bsd.command.contains("pkg query"));
        assert!(windows.command.contains("winget list"));
        assert!(!windows.command.contains("winget upgrade"));
        assert!(!windows.command.contains("winget uninstall"));
        assert!(!windows.command.contains("winget install"));
    }

    #[test]
    fn package_filter_options_follow_reported_managers() {
        let managers = |names: &[&str]| {
            names
                .iter()
                .map(|name| ResourcePackageManager {
                    name: (*name).to_string(),
                    available: true,
                    source: (*name).to_string(),
                })
                .collect::<Vec<_>>()
        };
        let base = vec![
            PackageFilter::All,
            PackageFilter::Upgradable,
            PackageFilter::Installed,
            PackageFilter::Services,
        ];
        assert_eq!(package_filter_options(&managers(&[])), base);
        assert_eq!(
            package_filter_options(&managers(&["apt", "winget"])),
            [
                base.clone(),
                vec![PackageFilter::Apt, PackageFilter::Winget]
            ]
            .concat()
        );
        assert_eq!(
            package_filter_options(&managers(&["brew"])),
            [base, vec![PackageFilter::Brew]].concat()
        );
    }

    #[test]
    fn truncated_package_output_is_reported_instead_of_half_a_list() {
        let truncated = "===PACKAGES===\nROW\tApp\twinget\t1.0\t\t\tAcme.App\tinstalled\n";
        let snapshot = parse_package_snapshot(truncated);

        assert!(matches!(
            snapshot.status,
            ResourcePackageStatus::Error { .. }
        ));
        assert!(snapshot.entries.is_empty());
        // Output with no Packages section at all stays "no data", not an error.
        assert!(matches!(
            parse_package_snapshot("something else").status,
            ResourcePackageStatus::Unknown
        ));
    }

    #[test]
    fn uninstall_commands_are_id_bound_and_winget_only() {
        let winget =
            build_package_uninstall_command("Windows", "winget", "Microsoft.PowerShell").unwrap();

        assert_eq!(
            winget.command,
            "winget uninstall --id \"Microsoft.PowerShell\" --accept-source-agreements --disable-interactivity"
        );
        assert_eq!(winget.capability, PackageCommandCapability::Partial);
        assert!(build_package_uninstall_command("Windows", "winget", "bad\"name").is_err());
        // Double quotes do not stop expansion, so shell syntax is refused.
        for hostile in [
            "Acme.$(notepad)",
            "Acme.`whoami`",
            "Acme.%PATH%",
            "Acme.; rm -rf /",
        ] {
            assert!(
                build_package_upgrade_command("Windows", "winget", hostile).is_err(),
                "{hostile}"
            );
            assert!(
                build_package_uninstall_command("Windows", "winget", hostile).is_err(),
                "{hostile}"
            );
            assert!(
                build_package_inspect_command("Windows", "winget", hostile).is_err(),
                "{hostile}"
            );
        }
        assert!(build_package_uninstall_command("Windows", "winget", " ").is_err());
        assert!(build_package_uninstall_command("Linux", "apt", "curl").is_err());
    }

    #[test]
    fn inspect_commands_quote_package_names() {
        let apt = build_package_inspect_command("Linux", "apt", "weird'name").unwrap();
        let brew = build_package_inspect_command("macOS", "brew", "openssl@3").unwrap();
        let winget =
            build_package_inspect_command("Windows", "winget", "Microsoft.PowerShell").unwrap();

        assert!(apt.command.contains("'weird'\"'\"'name'"));
        assert!(brew.command.contains("HOMEBREW_NO_AUTO_UPDATE=1 brew info"));
        assert_eq!(
            winget.command,
            "winget show \"Microsoft.PowerShell\" --accept-source-agreements"
        );
        assert!(build_package_inspect_command("Windows", "winget", "bad\"name").is_err());
        assert!(build_package_inspect_command("Linux", "apt", " ").is_err());
    }
}
