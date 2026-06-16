use std::collections::{HashMap, HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceFilesystemEntry {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub device: String,
    pub fs_type: String,
    pub size_bytes: String,
    pub used_bytes: String,
    pub available_bytes: String,
    pub used_percent: String,
    pub inode_total: String,
    pub inode_used: String,
    pub inode_available: String,
    pub inode_percent: String,
    pub read_only: bool,
    pub options: String,
    pub item_type: String,
    pub source: String,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilesystemCommandCapability {
    #[default]
    Unknown,
    Full,
    Partial,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceFilesystemStatus {
    #[default]
    Unknown,
    Available {
        capability: FilesystemCommandCapability,
        platform: String,
    },
    Unavailable,
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceFilesystemSnapshot {
    pub status: ResourceFilesystemStatus,
    pub entries: Vec<ResourceFilesystemEntry>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FilesystemEntrySeverity {
    #[default]
    Normal,
    Warning,
    Critical,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FilesystemFilter {
    #[default]
    All,
    Attention,
    Mounts,
    ReadOnly,
    HighUsage,
    InodePressure,
    InodeHotspots,
    LargeItems,
    Blocks,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemCaptureCommand {
    pub command: String,
    pub capability: FilesystemCommandCapability,
}

const FILESYSTEM_UNAVAILABLE_MARKER: &str = "__OXIDE_FILESYSTEM_UNAVAILABLE__";
const FILESYSTEM_ERROR_MARKER: &str = "__OXIDE_FILESYSTEM_ERROR__";
const FILESYSTEM_CAPABILITY_MARKER: &str = "__OXIDE_FILESYSTEM_CAPABILITY__";

pub fn build_filesystem_snapshot_command(os_type: &str) -> FilesystemCaptureCommand {
    let (command, capability) = match filesystem_os(os_type) {
        FilesystemOs::Windows => (
            build_windows_filesystem_snapshot_command(),
            FilesystemCommandCapability::Partial,
        ),
        FilesystemOs::MacOs => (
            build_macos_filesystem_snapshot_command(),
            FilesystemCommandCapability::Partial,
        ),
        FilesystemOs::Bsd => (
            build_bsd_filesystem_snapshot_command(),
            FilesystemCommandCapability::Partial,
        ),
        FilesystemOs::Linux | FilesystemOs::Unknown => (
            build_linux_filesystem_snapshot_command(),
            FilesystemCommandCapability::Full,
        ),
    };
    FilesystemCaptureCommand {
        command,
        capability,
    }
}

pub fn build_filesystem_diagnostic_command(os_type: &str) -> String {
    match filesystem_os(os_type) {
        FilesystemOs::Windows => concat!(
            "powershell -NoProfile -ExecutionPolicy Bypass -Command \"",
            "Get-Volume | Format-Table -AutoSize; ",
            "Get-PSDrive -PSProvider FileSystem | Format-Table -AutoSize; ",
            "Get-ChildItem -Path C:\\ -Force -ErrorAction SilentlyContinue | ",
            "Sort-Object Length -Descending | Select-Object -First 40 FullName,Length | Format-Table -AutoSize",
            "\""
        )
        .to_string(),
        FilesystemOs::MacOs => concat!(
            "df -h; df -ih; mount; diskutil list; ",
            "du -x -h -d 2 / 2>/dev/null | sort -hr | head -n 80; ",
            "find -x / -type f -size +100M -exec stat -f '%z %N' {} \\; 2>/dev/null | sort -nr | head -n 40"
        )
        .to_string(),
        FilesystemOs::Bsd => concat!(
            "df -h; df -ih; mount; ",
            "du -x -h -d 2 / 2>/dev/null | sort -hr | head -n 80; ",
            "find -x / -type f -size +100M -exec stat -f '%z %N' {} \\; 2>/dev/null | sort -nr | head -n 40"
        )
        .to_string(),
        FilesystemOs::Linux | FilesystemOs::Unknown => concat!(
            "df -hT; df -ih; findmnt -rn -o TARGET,SOURCE,FSTYPE,OPTIONS 2>/dev/null; ",
            "lsblk -f; du -xhd2 / 2>/dev/null | sort -hr | head -n 80; ",
            "find / -xdev -type f -size +100M -printf '%s %p\\n' 2>/dev/null | sort -nr | head -n 40"
        )
        .to_string(),
    }
}

pub fn parse_filesystem_snapshot(output: &str) -> ResourceFilesystemSnapshot {
    let Some(section) = extract_section(output, "FILESYSTEMS") else {
        return ResourceFilesystemSnapshot::default();
    };

    let mut entries = Vec::new();
    let mut capability = FilesystemCommandCapability::Unknown;
    let mut platform = "unknown".to_string();
    let mut inode_rows = HashMap::new();
    let mut mount_rows = HashMap::new();

    for line in section
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.trim().is_empty())
    {
        if line == FILESYSTEM_UNAVAILABLE_MARKER {
            return ResourceFilesystemSnapshot {
                status: ResourceFilesystemStatus::Unavailable,
                entries: Vec::new(),
            };
        }
        if let Some(message) = line.strip_prefix(FILESYSTEM_ERROR_MARKER) {
            return ResourceFilesystemSnapshot {
                status: ResourceFilesystemStatus::Error {
                    message: clean_marker_message(message, "Filesystem command failed."),
                },
                entries: Vec::new(),
            };
        }
        if let Some((next_capability, next_platform)) = parse_filesystem_capability_line(line) {
            capability = next_capability;
            platform = next_platform;
            continue;
        }
        if let Some(inode) = parse_inode_line(line) {
            inode_rows.insert((inode.device.clone(), inode.path.clone()), inode);
            continue;
        }
        if let Some(mount) = parse_findmnt_line(line).or_else(|| parse_mount_line(line)) {
            mount_rows.insert(mount.path.clone(), mount);
            continue;
        }
        if let Some(entry) = parse_filesystem_row_line(line)
            .or_else(|| parse_df_line(line))
            .or_else(|| parse_hotspot_line(line))
            .or_else(|| parse_inode_hotspot_line(line))
            .or_else(|| parse_count_hotspot_line(line))
            .or_else(|| parse_lsblk_line(line))
            .or_else(|| parse_windows_volume_line(line))
        {
            entries.push(entry);
        }
    }

    for entry in &mut entries {
        if entry.kind == "mount" {
            if let Some(inode) = inode_rows
                .get(&(entry.device.clone(), entry.path.clone()))
                .or_else(|| inode_rows.values().find(|inode| inode.path == entry.path))
            {
                entry.inode_total = inode.total.clone();
                entry.inode_used = inode.used.clone();
                entry.inode_available = inode.available.clone();
                entry.inode_percent = inode.percent.clone();
            }
            if let Some(mount) = mount_rows.get(&entry.path) {
                if entry.device.is_empty() {
                    entry.device = mount.device.clone();
                }
                if entry.fs_type.is_empty() {
                    entry.fs_type = mount.fs_type.clone();
                }
                entry.options = mount.options.clone();
                entry.read_only = mount.read_only;
            }
        }
    }

    dedupe_and_sort_filesystem_entries(&mut entries);
    ResourceFilesystemSnapshot {
        status: ResourceFilesystemStatus::Available {
            capability,
            platform,
        },
        entries,
    }
}

pub fn visible_filesystem_rows(
    entries: &[ResourceFilesystemEntry],
    query: &str,
    filter: FilesystemFilter,
) -> Vec<ResourceFilesystemEntry> {
    let query = query.trim().to_lowercase();
    entries
        .iter()
        .filter(|entry| filesystem_matches_filter(entry, filter))
        .filter(|entry| query.is_empty() || filesystem_matches_query(entry, &query))
        .cloned()
        .collect()
}

pub fn filesystem_row_signature(entry: &ResourceFilesystemEntry) -> u64 {
    let mut hasher = DefaultHasher::new();
    entry.id.hash(&mut hasher);
    entry.kind.hash(&mut hasher);
    entry.path.hash(&mut hasher);
    entry.device.hash(&mut hasher);
    entry.fs_type.hash(&mut hasher);
    entry.size_bytes.hash(&mut hasher);
    entry.used_bytes.hash(&mut hasher);
    entry.available_bytes.hash(&mut hasher);
    entry.used_percent.hash(&mut hasher);
    entry.inode_total.hash(&mut hasher);
    entry.inode_used.hash(&mut hasher);
    entry.inode_available.hash(&mut hasher);
    entry.inode_percent.hash(&mut hasher);
    entry.read_only.hash(&mut hasher);
    entry.options.hash(&mut hasher);
    entry.item_type.hash(&mut hasher);
    entry.source.hash(&mut hasher);
    entry.detail.hash(&mut hasher);
    hasher.finish()
}

pub fn filesystem_filter_label_key(filter: FilesystemFilter) -> &'static str {
    match filter {
        FilesystemFilter::All => "sidebar.host_filesystems.filters.all",
        FilesystemFilter::Attention => "sidebar.host_filesystems.filters.attention",
        FilesystemFilter::Mounts => "sidebar.host_filesystems.filters.mounts",
        FilesystemFilter::ReadOnly => "sidebar.host_filesystems.filters.read_only",
        FilesystemFilter::HighUsage => "sidebar.host_filesystems.filters.high_usage",
        FilesystemFilter::InodePressure => "sidebar.host_filesystems.filters.inode_pressure",
        FilesystemFilter::InodeHotspots => "sidebar.host_filesystems.filters.inode_hotspots",
        FilesystemFilter::LargeItems => "sidebar.host_filesystems.filters.large_items",
        FilesystemFilter::Blocks => "sidebar.host_filesystems.filters.blocks",
    }
}

pub fn filesystem_kind_label_key(kind: &str) -> &'static str {
    match kind.trim().to_lowercase().as_str() {
        "mount" => "sidebar.host_filesystems.kinds.mount",
        "large_dir" => "sidebar.host_filesystems.kinds.large_dir",
        "large_file" => "sidebar.host_filesystems.kinds.large_file",
        "inode_dir" => "sidebar.host_filesystems.kinds.inode_dir",
        "count_dir" => "sidebar.host_filesystems.kinds.count_dir",
        "block" => "sidebar.host_filesystems.kinds.block",
        _ => "sidebar.host_filesystems.kinds.unknown",
    }
}

pub fn filesystem_read_only_label_key(read_only: bool) -> &'static str {
    if read_only {
        "sidebar.host_filesystems.read_only.yes"
    } else {
        "sidebar.host_filesystems.read_only.no"
    }
}

pub fn filesystem_entry_severity(entry: &ResourceFilesystemEntry) -> FilesystemEntrySeverity {
    let usage_percent = parse_percent(&entry.used_percent);
    let inode_percent = parse_percent(&entry.inode_percent);
    let available_bytes = parse_u64(&entry.available_bytes);
    let total_bytes = parse_u64(&entry.size_bytes);
    let item_bytes = parse_u64(&entry.size_bytes);
    let inode_count = parse_u64(&entry.inode_used);

    if entry.kind == "mount" {
        if usage_percent >= 95
            || inode_percent >= 95
            || (total_bytes >= 1024 * 1024 * 1024
                && available_bytes > 0
                && available_bytes < 512 * 1024 * 1024)
        {
            return FilesystemEntrySeverity::Critical;
        }
        if usage_percent >= 85
            || inode_percent >= 85
            || entry.read_only
            || (total_bytes >= 1024 * 1024 * 1024
                && available_bytes > 0
                && available_bytes < 1024 * 1024 * 1024)
        {
            return FilesystemEntrySeverity::Warning;
        }
    }

    if entry.kind == "large_dir" || entry.kind == "large_file" {
        if item_bytes >= 50 * 1024 * 1024 * 1024 {
            return FilesystemEntrySeverity::Critical;
        }
        if item_bytes >= 10 * 1024 * 1024 * 1024 {
            return FilesystemEntrySeverity::Warning;
        }
    }

    if entry.kind == "inode_dir" || entry.kind == "count_dir" {
        if inode_count >= 100_000 {
            return FilesystemEntrySeverity::Critical;
        }
        if inode_count >= 10_000 {
            return FilesystemEntrySeverity::Warning;
        }
    }

    FilesystemEntrySeverity::Normal
}

pub fn filesystem_attention_label_keys(entry: &ResourceFilesystemEntry) -> Vec<&'static str> {
    let mut keys = Vec::new();
    let usage_percent = parse_percent(&entry.used_percent);
    let inode_percent = parse_percent(&entry.inode_percent);
    let available_bytes = parse_u64(&entry.available_bytes);
    let total_bytes = parse_u64(&entry.size_bytes);

    if entry.kind == "mount" {
        if usage_percent >= 95 {
            keys.push("sidebar.host_filesystems.attention.critical_usage");
        } else if usage_percent >= 85 {
            keys.push("sidebar.host_filesystems.attention.high_usage");
        }
        if inode_percent >= 95 {
            keys.push("sidebar.host_filesystems.attention.critical_inode");
        } else if inode_percent >= 85 {
            keys.push("sidebar.host_filesystems.attention.inode_pressure");
        }
        if total_bytes >= 1024 * 1024 * 1024
            && available_bytes > 0
            && available_bytes < 1024 * 1024 * 1024
        {
            keys.push("sidebar.host_filesystems.attention.low_free_space");
        }
        if entry.read_only {
            keys.push("sidebar.host_filesystems.attention.read_only");
        }
    }

    if entry.kind == "large_dir" || entry.kind == "large_file" {
        keys.push("sidebar.host_filesystems.attention.large_item");
    }
    if entry.kind == "inode_dir" {
        keys.push("sidebar.host_filesystems.attention.inode_hotspot");
    }
    if entry.kind == "count_dir" {
        keys.push("sidebar.host_filesystems.attention.file_count_hotspot");
    }
    keys
}

fn build_linux_filesystem_snapshot_command() -> String {
    concat!(
        "echo '===FILESYSTEMS==='; ",
        "if command -v df >/dev/null 2>&1; then ",
        "echo '__OXIDE_FILESYSTEM_CAPABILITY__\tfull\tlinux_df'; ",
        "df -PTB1 2>/dev/null | awk 'NR>1 { mount=$7; for (i=8;i<=NF;i++) mount=mount\" \"$i; pct=$6; gsub(/%/,\"\",pct); printf \"DF\\t%s\\t%s\\t%s\\t%s\\t%s\\t%s\\t%s\\n\", $1,$2,$3,$4,$5,pct,mount }'; ",
        "df -Pi 2>/dev/null | awk 'NR>1 { mount=$6; for (i=7;i<=NF;i++) mount=mount\" \"$i; pct=$5; gsub(/%/,\"\",pct); printf \"INODE\\t%s\\t%s\\t%s\\t%s\\t%s\\t%s\\n\", $1,$2,$3,$4,pct,mount }'; ",
        "else echo '__OXIDE_FILESYSTEM_UNAVAILABLE__'; fi; ",
        "if command -v findmnt >/dev/null 2>&1; then findmnt -rnP -o TARGET,SOURCE,FSTYPE,OPTIONS 2>/dev/null | sed 's/^/FINDMNT\\t/'; fi; ",
        "if command -v lsblk >/dev/null 2>&1; then lsblk -b -P -o NAME,TYPE,FSTYPE,SIZE,MOUNTPOINTS,MODEL 2>/dev/null | sed 's/^/LSBLK\\t/'; fi; ",
        "if command -v du >/dev/null 2>&1; then du -x -B1 -d 2 / 2>/dev/null | sort -nr | head -n 80 | awk '{ size=$1; $1=\"\"; sub(/^ /,\"\"); printf \"HOTSPOT\\tlarge_dir\\t%s\\t%s\\t/\\tdu\\n\", size, $0 }'; fi; ",
        "if command -v find >/dev/null 2>&1; then find / -xdev -type f -size +100M -printf '%s\\t%p\\n' 2>/dev/null | sort -nr | head -n 40 | awk -F '\\t' '{ printf \"HOTSPOT\\tlarge_file\\t%s\\t%s\\t/\\tfind\\n\", $1, $2 }'; fi; ",
        "if command -v find >/dev/null 2>&1; then find / -xdev -printf '%h\\n' 2>/dev/null | sort | uniq -c | sort -nr | head -n 50 | awk '{ count=$1; $1=\"\"; sub(/^ /,\"\"); printf \"INODEHOTSPOT\\t%s\\t%s\\t/\\tfind\\n\", count, $0 }'; fi; ",
        "echo '===FILESYSTEMS_END==='"
    )
    .to_string()
}

fn build_macos_filesystem_snapshot_command() -> String {
    concat!(
        "echo '===FILESYSTEMS==='; ",
        "if command -v df >/dev/null 2>&1; then ",
        "echo '__OXIDE_FILESYSTEM_CAPABILITY__\tpartial\tmacos_df'; ",
        "df -kP 2>/dev/null | awk 'NR>1 { mount=$6; for (i=7;i<=NF;i++) mount=mount\" \"$i; printf \"DF\\t%s\\t\\t%s\\t%s\\t%s\\t%s\\t%s\\n\", $1,$2*1024,$3*1024,$4*1024,$5,mount }'; ",
        "df -iP 2>/dev/null | awk 'NR>1 { mount=$6; for (i=7;i<=NF;i++) mount=mount\" \"$i; pct=$5; gsub(/%/,\"\",pct); printf \"INODE\\t%s\\t%s\\t%s\\t%s\\t%s\\t%s\\n\", $1,$2,$3,$4,pct,mount }'; ",
        "else echo '__OXIDE_FILESYSTEM_UNAVAILABLE__'; fi; ",
        "mount 2>/dev/null | sed 's/^/MOUNT\\t/'; ",
        "du -x -k -d 2 / 2>/dev/null | sort -nr | head -n 80 | awk '{ size=$1*1024; $1=\"\"; sub(/^ /,\"\"); printf \"HOTSPOT\\tlarge_dir\\t%.0f\\t%s\\t/\\tdu\\n\", size, $0 }'; ",
        "find -x / -type f -size +100M -exec stat -f '%z\\t%N' {} \\; 2>/dev/null | sort -nr | head -n 40 | awk -F '\\t' '{ printf \"HOTSPOT\\tlarge_file\\t%s\\t%s\\t/\\tfind\\n\", $1, $2 }'; ",
        "find -x / -print 2>/dev/null | sed 's#[^/][^/]*$##; s#/$##; s#^$#/#' | sort | uniq -c | sort -nr | head -n 50 | awk '{ count=$1; $1=\"\"; sub(/^ /,\"\"); printf \"INODEHOTSPOT\\t%s\\t%s\\t/\\tfind\\n\", count, $0 }'; ",
        "echo '===FILESYSTEMS_END==='"
    )
    .to_string()
}

fn build_bsd_filesystem_snapshot_command() -> String {
    concat!(
        "echo '===FILESYSTEMS==='; ",
        "if command -v df >/dev/null 2>&1; then ",
        "echo '__OXIDE_FILESYSTEM_CAPABILITY__\tpartial\tbsd_df'; ",
        "df -kP 2>/dev/null | awk 'NR>1 { mount=$6; for (i=7;i<=NF;i++) mount=mount\" \"$i; printf \"DF\\t%s\\t\\t%s\\t%s\\t%s\\t%s\\t%s\\n\", $1,$2*1024,$3*1024,$4*1024,$5,mount }'; ",
        "df -iP 2>/dev/null | awk 'NR>1 { mount=$6; for (i=7;i<=NF;i++) mount=mount\" \"$i; pct=$5; gsub(/%/,\"\",pct); printf \"INODE\\t%s\\t%s\\t%s\\t%s\\t%s\\t%s\\n\", $1,$2,$3,$4,pct,mount }'; ",
        "else echo '__OXIDE_FILESYSTEM_UNAVAILABLE__'; fi; ",
        "mount 2>/dev/null | sed 's/^/MOUNT\\t/'; ",
        "du -x -k -d 2 / 2>/dev/null | sort -nr | head -n 80 | awk '{ size=$1*1024; $1=\"\"; sub(/^ /,\"\"); printf \"HOTSPOT\\tlarge_dir\\t%.0f\\t%s\\t/\\tdu\\n\", size, $0 }'; ",
        "find -x / -type f -size +100M -exec stat -f '%z\\t%N' {} \\; 2>/dev/null | sort -nr | head -n 40 | awk -F '\\t' '{ printf \"HOTSPOT\\tlarge_file\\t%s\\t%s\\t/\\tfind\\n\", $1, $2 }'; ",
        "find -x / -print 2>/dev/null | sed 's#[^/][^/]*$##; s#/$##; s#^$#/#' | sort | uniq -c | sort -nr | head -n 50 | awk '{ count=$1; $1=\"\"; sub(/^ /,\"\"); printf \"INODEHOTSPOT\\t%s\\t%s\\t/\\tfind\\n\", count, $0 }'; ",
        "echo '===FILESYSTEMS_END==='"
    )
    .to_string()
}

fn build_windows_filesystem_snapshot_command() -> String {
    // Windows has no Unix inode concept, so file-count hotspots model the same
    // operational risk without pretending that NTFS exposes inode pressure.
    concat!(
        "powershell -NoProfile -ExecutionPolicy Bypass -Command \"",
        "Write-Output '===FILESYSTEMS===';",
        "Write-Output ('__OXIDE_FILESYSTEM_CAPABILITY__'+[char]9+'partial'+[char]9+'windows_powershell');",
        "$volumeError=$null;$psdriveError=$null;",
        "try{",
        "Get-Volume|ForEach-Object{",
        "$path=if($_.DriveLetter){$_.DriveLetter+':\\\\'}else{$_.Path};",
        "$size=[int64]($_.Size);$free=[int64]($_.SizeRemaining);$used=$size-$free;",
        "$pct=if($size -gt 0){[math]::Round(($used*100.0)/$size,1)}else{0};",
        "$ro=if($_.OperationalStatus -contains 'Read-only'){'true'}else{'false'};",
        "Write-Output ('WINVOL'+[char]9+$path+[char]9+$_.UniqueId+[char]9+$_.FileSystem+[char]9+$size+[char]9+$used+[char]9+$free+[char]9+$pct+[char]9+$ro+[char]9+$_.HealthStatus)",
        "};",
        "}catch{$volumeError=$_.Exception.Message};",
        "try{",
        "Get-PSDrive -PSProvider FileSystem|ForEach-Object{",
        "$root=$_.Root;$used=[int64]($_.Used);$free=[int64]($_.Free);$size=$used+$free;$pct=if($size -gt 0){[math]::Round(($used*100.0)/$size,1)}else{0};",
        "Write-Output ('WINVOL'+[char]9+$root+[char]9+$_.Name+[char]9+''+[char]9+$size+[char]9+$used+[char]9+$free+[char]9+$pct+[char]9+'false'+[char]9+'PSDrive')",
        "}",
        "}catch{$psdriveError=$_.Exception.Message};",
        "if($volumeError -and $psdriveError){Write-Output ('__OXIDE_FILESYSTEM_ERROR__'+[char]9+$psdriveError)};",
        "$roots=@();",
        "try{Get-Volume|Where-Object{$_.DriveLetter}|ForEach-Object{$roots+=($_.DriveLetter+':\\')}}catch{};",
        "if($roots.Count -eq 0){try{Get-PSDrive -PSProvider FileSystem|ForEach-Object{$roots+=$_.Root}}catch{}};",
        "foreach($root in ($roots|Select-Object -Unique)){",
        "try{",
        "Get-ChildItem -LiteralPath $root -Force -Directory -ErrorAction SilentlyContinue|ForEach-Object{",
        "$dir=$_.FullName;$size=[int64]0;$count=[int64]0;",
        "Get-ChildItem -LiteralPath $dir -Force -Recurse -File -ErrorAction SilentlyContinue|ForEach-Object{$size+=[int64]$_.Length;$count+=1};",
        "Write-Output ('HOTSPOT'+[char]9+'large_dir'+[char]9+$size+[char]9+$dir+[char]9+$root+[char]9+'powershell');",
        "Write-Output ('COUNTHOTSPOT'+[char]9+$count+[char]9+$dir+[char]9+$root+[char]9+'powershell')",
        "};",
        "Get-ChildItem -LiteralPath $root -Force -Recurse -File -ErrorAction SilentlyContinue|Where-Object{$_.Length -ge 104857600}|Sort-Object Length -Descending|Select-Object -First 40|ForEach-Object{",
        "Write-Output ('HOTSPOT'+[char]9+'large_file'+[char]9+[int64]$_.Length+[char]9+$_.FullName+[char]9+$root+[char]9+'powershell')",
        "}",
        "}catch{}",
        "};",
        "Write-Output '===FILESYSTEMS_END==='",
        "\""
    )
    .to_string()
}

fn parse_filesystem_capability_line(line: &str) -> Option<(FilesystemCommandCapability, String)> {
    let payload = line.strip_prefix(FILESYSTEM_CAPABILITY_MARKER)?;
    let parts = payload
        .trim_start_matches('\t')
        .split('\t')
        .collect::<Vec<_>>();
    let capability = match parts.first().copied().unwrap_or("unknown") {
        "full" => FilesystemCommandCapability::Full,
        "partial" => FilesystemCommandCapability::Partial,
        _ => FilesystemCommandCapability::Unknown,
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

fn parse_filesystem_row_line(line: &str) -> Option<ResourceFilesystemEntry> {
    let payload = line.strip_prefix("ROW\t")?;
    let parts = payload.splitn(18, '\t').collect::<Vec<_>>();
    if parts.len() != 18 {
        return None;
    }
    Some(ResourceFilesystemEntry {
        id: clean(parts[0]),
        kind: clean(parts[1]),
        path: clean(parts[2]),
        device: clean(parts[3]),
        fs_type: clean(parts[4]),
        size_bytes: clean(parts[5]),
        used_bytes: clean(parts[6]),
        available_bytes: clean(parts[7]),
        used_percent: clean_percent(parts[8]),
        inode_total: clean(parts[9]),
        inode_used: clean(parts[10]),
        inode_available: clean(parts[11]),
        inode_percent: clean_percent(parts[12]),
        read_only: parse_bool(parts[13]),
        options: clean(parts[14]),
        item_type: clean(parts[15]),
        source: clean(parts[16]),
        detail: clean(parts[17]),
    })
}

fn parse_df_line(line: &str) -> Option<ResourceFilesystemEntry> {
    let payload = line.strip_prefix("DF\t")?;
    let parts = payload.splitn(7, '\t').collect::<Vec<_>>();
    if parts.len() != 7 {
        return None;
    }
    let path = clean(parts[6]);
    let device = clean(parts[0]);
    Some(ResourceFilesystemEntry {
        id: format!("mount:{path}"),
        kind: "mount".to_string(),
        path,
        device,
        fs_type: clean(parts[1]),
        size_bytes: clean_number(parts[2]),
        used_bytes: clean_number(parts[3]),
        available_bytes: clean_number(parts[4]),
        used_percent: clean_percent(parts[5]),
        inode_total: String::new(),
        inode_used: String::new(),
        inode_available: String::new(),
        inode_percent: String::new(),
        read_only: false,
        options: String::new(),
        item_type: "mount".to_string(),
        source: "df".to_string(),
        detail: String::new(),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FilesystemInodeRow {
    device: String,
    path: String,
    total: String,
    used: String,
    available: String,
    percent: String,
}

fn parse_inode_line(line: &str) -> Option<FilesystemInodeRow> {
    let payload = line.strip_prefix("INODE\t")?;
    let parts = payload.splitn(6, '\t').collect::<Vec<_>>();
    if parts.len() != 6 {
        return None;
    }
    Some(FilesystemInodeRow {
        device: clean(parts[0]),
        total: clean_number(parts[1]),
        used: clean_number(parts[2]),
        available: clean_number(parts[3]),
        percent: clean_percent(parts[4]),
        path: clean(parts[5]),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FilesystemMountRow {
    path: String,
    device: String,
    fs_type: String,
    options: String,
    read_only: bool,
}

fn parse_findmnt_line(line: &str) -> Option<FilesystemMountRow> {
    let payload = line.strip_prefix("FINDMNT\t")?;
    let properties = parse_key_value_properties(payload);
    let path = properties.get("TARGET")?.to_string();
    let options = properties.get("OPTIONS").cloned().unwrap_or_default();
    Some(FilesystemMountRow {
        path,
        device: properties.get("SOURCE").cloned().unwrap_or_default(),
        fs_type: properties.get("FSTYPE").cloned().unwrap_or_default(),
        read_only: mount_options_are_read_only(&options),
        options,
    })
}

fn parse_mount_line(line: &str) -> Option<FilesystemMountRow> {
    let payload = line.strip_prefix("MOUNT\t")?;
    let (device, tail) = payload.split_once(" on ")?;
    let (path, rest) = tail.split_once(" (")?;
    let options = rest.trim_end_matches(')').to_string();
    let fs_type = options.split(',').next().map(clean).unwrap_or_default();
    Some(FilesystemMountRow {
        path: clean(path),
        device: clean(device),
        fs_type,
        read_only: mount_options_are_read_only(&options),
        options,
    })
}

fn parse_hotspot_line(line: &str) -> Option<ResourceFilesystemEntry> {
    let payload = line.strip_prefix("HOTSPOT\t")?;
    let parts = payload.splitn(5, '\t').collect::<Vec<_>>();
    if parts.len() != 5 {
        return None;
    }
    let kind = clean(parts[0]);
    let path = clean(parts[2]);
    Some(ResourceFilesystemEntry {
        id: format!("{kind}:{path}"),
        kind: kind.clone(),
        path,
        device: String::new(),
        fs_type: String::new(),
        size_bytes: clean_number(parts[1]),
        used_bytes: clean_number(parts[1]),
        available_bytes: String::new(),
        used_percent: String::new(),
        inode_total: String::new(),
        inode_used: String::new(),
        inode_available: String::new(),
        inode_percent: String::new(),
        read_only: false,
        options: String::new(),
        item_type: if kind == "large_file" {
            "file".to_string()
        } else {
            "directory".to_string()
        },
        source: clean(parts[4]),
        detail: clean(parts[3]),
    })
}

fn parse_inode_hotspot_line(line: &str) -> Option<ResourceFilesystemEntry> {
    let payload = line.strip_prefix("INODEHOTSPOT\t")?;
    let parts = payload.splitn(4, '\t').collect::<Vec<_>>();
    if parts.len() != 4 {
        return None;
    }
    let path = clean(parts[1]);
    Some(ResourceFilesystemEntry {
        id: format!("inode_dir:{path}"),
        kind: "inode_dir".to_string(),
        path,
        device: String::new(),
        fs_type: String::new(),
        size_bytes: String::new(),
        used_bytes: String::new(),
        available_bytes: String::new(),
        used_percent: String::new(),
        inode_total: String::new(),
        inode_used: clean_number(parts[0]),
        inode_available: String::new(),
        inode_percent: String::new(),
        read_only: false,
        options: String::new(),
        item_type: "directory".to_string(),
        source: clean(parts[3]),
        detail: clean(parts[2]),
    })
}

fn parse_count_hotspot_line(line: &str) -> Option<ResourceFilesystemEntry> {
    let payload = line.strip_prefix("COUNTHOTSPOT\t")?;
    let parts = payload.splitn(4, '\t').collect::<Vec<_>>();
    if parts.len() != 4 {
        return None;
    }
    let path = clean(parts[1]);
    Some(ResourceFilesystemEntry {
        id: format!("count_dir:{path}"),
        kind: "count_dir".to_string(),
        path,
        device: String::new(),
        fs_type: String::new(),
        size_bytes: String::new(),
        used_bytes: String::new(),
        available_bytes: String::new(),
        used_percent: String::new(),
        inode_total: String::new(),
        inode_used: clean_number(parts[0]),
        inode_available: String::new(),
        inode_percent: String::new(),
        read_only: false,
        options: String::new(),
        item_type: "directory".to_string(),
        source: clean(parts[3]),
        detail: clean(parts[2]),
    })
}

fn parse_lsblk_line(line: &str) -> Option<ResourceFilesystemEntry> {
    let payload = line.strip_prefix("LSBLK\t")?;
    let properties = parse_key_value_properties(payload);
    let name = properties.get("NAME")?.to_string();
    let mountpoint = properties
        .get("MOUNTPOINTS")
        .cloned()
        .unwrap_or_default()
        .replace("\\n", ", ");
    Some(ResourceFilesystemEntry {
        id: format!("block:{name}:{mountpoint}"),
        kind: "block".to_string(),
        path: if mountpoint.trim().is_empty() {
            name.clone()
        } else {
            mountpoint
        },
        device: name,
        fs_type: properties.get("FSTYPE").cloned().unwrap_or_default(),
        size_bytes: properties.get("SIZE").cloned().unwrap_or_default(),
        used_bytes: String::new(),
        available_bytes: String::new(),
        used_percent: String::new(),
        inode_total: String::new(),
        inode_used: String::new(),
        inode_available: String::new(),
        inode_percent: String::new(),
        read_only: false,
        options: String::new(),
        item_type: properties.get("TYPE").cloned().unwrap_or_default(),
        source: "lsblk".to_string(),
        detail: properties.get("MODEL").cloned().unwrap_or_default(),
    })
}

fn parse_windows_volume_line(line: &str) -> Option<ResourceFilesystemEntry> {
    let payload = line.strip_prefix("WINVOL\t")?;
    let parts = payload.splitn(9, '\t').collect::<Vec<_>>();
    if parts.len() != 9 {
        return None;
    }
    let path = clean(parts[0]);
    Some(ResourceFilesystemEntry {
        id: format!("mount:{path}"),
        kind: "mount".to_string(),
        path,
        device: clean(parts[1]),
        fs_type: clean(parts[2]),
        size_bytes: clean_number(parts[3]),
        used_bytes: clean_number(parts[4]),
        available_bytes: clean_number(parts[5]),
        used_percent: clean_percent(parts[6]),
        inode_total: String::new(),
        inode_used: String::new(),
        inode_available: String::new(),
        inode_percent: String::new(),
        read_only: parse_bool(parts[7]),
        options: String::new(),
        item_type: "volume".to_string(),
        source: "windows_powershell".to_string(),
        detail: clean(parts[8]),
    })
}

fn dedupe_and_sort_filesystem_entries(entries: &mut Vec<ResourceFilesystemEntry>) {
    let mut seen = HashSet::new();
    entries.retain(|entry| {
        seen.insert((
            entry.kind.clone(),
            entry.path.clone(),
            entry.device.clone(),
            entry.source.clone(),
        ))
    });
    entries.sort_by(|left, right| {
        filesystem_kind_rank(&left.kind)
            .cmp(&filesystem_kind_rank(&right.kind))
            .then(parse_percent(&right.used_percent).cmp(&parse_percent(&left.used_percent)))
            .then(parse_u64(&right.size_bytes).cmp(&parse_u64(&left.size_bytes)))
            .then(left.path.cmp(&right.path))
    });
}

fn filesystem_matches_filter(entry: &ResourceFilesystemEntry, filter: FilesystemFilter) -> bool {
    match filter {
        FilesystemFilter::All => true,
        FilesystemFilter::Attention => {
            filesystem_entry_severity(entry) != FilesystemEntrySeverity::Normal
        }
        FilesystemFilter::Mounts => entry.kind == "mount",
        FilesystemFilter::ReadOnly => entry.kind == "mount" && entry.read_only,
        FilesystemFilter::HighUsage => {
            entry.kind == "mount" && parse_percent(&entry.used_percent) >= 85
        }
        FilesystemFilter::InodePressure => {
            entry.kind == "mount" && parse_percent(&entry.inode_percent) >= 85
        }
        FilesystemFilter::InodeHotspots => entry.kind == "inode_dir" || entry.kind == "count_dir",
        FilesystemFilter::LargeItems => entry.kind == "large_dir" || entry.kind == "large_file",
        FilesystemFilter::Blocks => entry.kind == "block",
    }
}

fn filesystem_matches_query(entry: &ResourceFilesystemEntry, query: &str) -> bool {
    [
        entry.kind.as_str(),
        entry.path.as_str(),
        entry.device.as_str(),
        entry.fs_type.as_str(),
        entry.size_bytes.as_str(),
        entry.used_bytes.as_str(),
        entry.available_bytes.as_str(),
        entry.inode_used.as_str(),
        entry.used_percent.as_str(),
        entry.inode_percent.as_str(),
        entry.options.as_str(),
        entry.item_type.as_str(),
        entry.source.as_str(),
        entry.detail.as_str(),
    ]
    .iter()
    .any(|value| value.to_lowercase().contains(query))
}

fn filesystem_kind_rank(kind: &str) -> u8 {
    match kind {
        "mount" => 0,
        "large_dir" => 1,
        "large_file" => 2,
        "inode_dir" => 3,
        "count_dir" => 4,
        "block" => 5,
        _ => 6,
    }
}

fn parse_key_value_properties(payload: &str) -> HashMap<String, String> {
    let mut properties = HashMap::new();
    let mut rest = payload.trim();
    while let Some((key, tail)) = rest.split_once('=') {
        let key = key
            .split_whitespace()
            .last()
            .unwrap_or_default()
            .trim()
            .to_string();
        let tail = tail.trim_start();
        if let Some(after_quote) = tail.strip_prefix('"') {
            let mut value = String::new();
            let mut escaped = false;
            let mut end_index = after_quote.len();
            for (index, ch) in after_quote.char_indices() {
                if escaped {
                    value.push(ch);
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == '"' {
                    end_index = index + ch.len_utf8();
                    break;
                }
                value.push(ch);
            }
            properties.insert(key, value);
            rest = after_quote
                .get(end_index..)
                .unwrap_or_default()
                .trim_start();
        } else {
            let value = tail.split_whitespace().next().unwrap_or_default();
            properties.insert(key, clean(value));
            rest = tail.get(value.len()..).unwrap_or_default().trim_start();
        }
    }
    properties
}

fn mount_options_are_read_only(options: &str) -> bool {
    options
        .split(',')
        .any(|option| matches!(option.trim().to_lowercase().as_str(), "ro" | "read-only"))
}

fn parse_percent(value: &str) -> u32 {
    clean_percent(value)
        .split('.')
        .next()
        .unwrap_or_default()
        .parse::<u32>()
        .unwrap_or(0)
}

fn parse_u64(value: &str) -> u64 {
    clean_number(value).parse::<u64>().unwrap_or(0)
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "ro" | "read-only"
    )
}

fn clean(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

fn clean_number(value: &str) -> String {
    clean(value).replace(',', "")
}

fn clean_percent(value: &str) -> String {
    clean_number(value).trim_end_matches('%').to_string()
}

fn clean_marker_message(message: &str, fallback: &str) -> String {
    let cleaned = message.trim_start_matches('\t').trim();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned.to_string()
    }
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
enum FilesystemOs {
    Linux,
    MacOs,
    Bsd,
    Windows,
    Unknown,
}

fn filesystem_os(os_type: &str) -> FilesystemOs {
    match os_type {
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => {
            FilesystemOs::Linux
        }
        "macOS" | "macos" | "Darwin" => FilesystemOs::MacOs,
        "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => FilesystemOs::Bsd,
        "Windows" | "windows" => FilesystemOs::Windows,
        _ => FilesystemOs::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_mounts_inodes_findmnt_and_lsblk() {
        let output = concat!(
            "===FILESYSTEMS===\n",
            "__OXIDE_FILESYSTEM_CAPABILITY__\tfull\tlinux_df\n",
            "DF\t/dev/sda1\text4\t10737418240\t9663676416\t1073741824\t90\t/\n",
            "INODE\t/dev/sda1\t1000\t900\t100\t90\t/\n",
            "FINDMNT\tTARGET=\"/\" SOURCE=\"/dev/sda1\" FSTYPE=\"ext4\" OPTIONS=\"rw,relatime\"\n",
            "LSBLK\tNAME=\"sda1\" TYPE=\"part\" FSTYPE=\"ext4\" SIZE=\"10737418240\" MOUNTPOINTS=\"/\" MODEL=\"Fast Disk\"\n",
            "===FILESYSTEMS_END===\n"
        );

        let snapshot = parse_filesystem_snapshot(output);
        let mounts = visible_filesystem_rows(&snapshot.entries, "/", FilesystemFilter::Mounts);
        let high_usage =
            visible_filesystem_rows(&snapshot.entries, "", FilesystemFilter::HighUsage);

        assert_eq!(
            snapshot.status,
            ResourceFilesystemStatus::Available {
                capability: FilesystemCommandCapability::Full,
                platform: "linux_df".to_string(),
            }
        );
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].used_percent, "90");
        assert_eq!(mounts[0].inode_percent, "90");
        assert!(!mounts[0].read_only);
        assert_eq!(high_usage.len(), 1);
        assert!(snapshot.entries.iter().any(|entry| entry.kind == "block"));
    }

    #[test]
    fn parses_read_only_and_large_item_rows() {
        let output = concat!(
            "===FILESYSTEMS===\n",
            "__OXIDE_FILESYSTEM_CAPABILITY__\tpartial\tfixture\n",
            "DF\t/dev/sdb1\txfs\t2000\t1000\t1000\t50\t/data archive\n",
            "FINDMNT\tTARGET=\"/data archive\" SOURCE=\"/dev/sdb1\" FSTYPE=\"xfs\" OPTIONS=\"ro,noatime\"\n",
            "HOTSPOT\tlarge_dir\t1500\t/data archive/logs\t/data archive\tdu\n",
            "HOTSPOT\tlarge_file\t1200\t/data archive/big file.bin\t/data archive\tfind\n",
            "INODEHOTSPOT\t12000\t/data archive/cache\t/data archive\tfind\n",
            "===FILESYSTEMS_END===\n"
        );

        let snapshot = parse_filesystem_snapshot(output);
        let read_only = visible_filesystem_rows(&snapshot.entries, "", FilesystemFilter::ReadOnly);
        let large =
            visible_filesystem_rows(&snapshot.entries, "big file", FilesystemFilter::LargeItems);
        let inode_hotspots =
            visible_filesystem_rows(&snapshot.entries, "cache", FilesystemFilter::InodeHotspots);
        let attention = visible_filesystem_rows(&snapshot.entries, "", FilesystemFilter::Attention);

        assert_eq!(read_only.len(), 1);
        assert_eq!(read_only[0].path, "/data archive");
        assert_eq!(large.len(), 1);
        assert_eq!(large[0].kind, "large_file");
        assert_eq!(large[0].path, "/data archive/big file.bin");
        assert_eq!(inode_hotspots.len(), 1);
        assert_eq!(inode_hotspots[0].inode_used, "12000");
        assert!(attention.iter().any(|entry| entry.kind == "inode_dir"));
        assert_eq!(
            filesystem_entry_severity(&read_only[0]),
            FilesystemEntrySeverity::Warning
        );
        assert!(
            filesystem_attention_label_keys(&read_only[0])
                .contains(&"sidebar.host_filesystems.attention.read_only")
        );
    }

    #[test]
    fn parses_macos_mount_and_windows_volume_rows() {
        let mac = concat!(
            "===FILESYSTEMS===\n",
            "__OXIDE_FILESYSTEM_CAPABILITY__\tpartial\tmacos_df\n",
            "DF\t/dev/disk3s1\t\t1000\t500\t500\t50\t/System/Volumes/Data\n",
            "MOUNT\t/dev/disk3s1 on /System/Volumes/Data (apfs, local, read-only, journaled)\n",
            "===FILESYSTEMS_END===\n"
        );
        let windows = concat!(
            "===FILESYSTEMS===\n",
            "__OXIDE_FILESYSTEM_CAPABILITY__\tpartial\twindows_powershell\n",
            "WINVOL\tC:\\\t\\\\?\\Volume{abc}\\\tNTFS\t1000\t700\t300\t70\tfalse\tHealthy\n",
            "HOTSPOT\tlarge_dir\t53687091200\tC:\\logs\tC:\\\tpowershell\n",
            "COUNTHOTSPOT\t25000\tC:\\logs\tC:\\\tpowershell\n",
            "===FILESYSTEMS_END===\n"
        );

        let mac_snapshot = parse_filesystem_snapshot(mac);
        let windows_snapshot = parse_filesystem_snapshot(windows);

        assert!(mac_snapshot.entries[0].read_only);
        assert_eq!(mac_snapshot.entries[0].fs_type, "apfs");
        assert_eq!(windows_snapshot.entries[0].path, "C:\\");
        assert_eq!(windows_snapshot.entries[0].used_percent, "70");

        let count_hotspots = visible_filesystem_rows(
            &windows_snapshot.entries,
            "logs",
            FilesystemFilter::InodeHotspots,
        );
        assert_eq!(count_hotspots.len(), 1);
        assert_eq!(count_hotspots[0].kind, "count_dir");
        assert_eq!(count_hotspots[0].inode_used, "25000");
        assert_eq!(
            filesystem_entry_severity(&count_hotspots[0]),
            FilesystemEntrySeverity::Warning
        );
        assert!(
            filesystem_attention_label_keys(&count_hotspots[0])
                .contains(&"sidebar.host_filesystems.attention.file_count_hotspot")
        );
    }

    #[test]
    fn normalized_rows_search_all_fields() {
        let output = concat!(
            "===FILESYSTEMS===\n",
            "__OXIDE_FILESYSTEM_CAPABILITY__\tpartial\tfixture\n",
            "ROW\tmount:/mnt/cache\tmount\t/mnt/cache\tserver:/cache\tnfs4\t1000\t900\t100\t90\t10\t9\t1\t90\ttrue\tro,vers=4\tmount\tfixture\tremote cache\n",
            "===FILESYSTEMS_END===\n"
        );

        let snapshot = parse_filesystem_snapshot(output);
        let rows =
            visible_filesystem_rows(&snapshot.entries, "remote cache", FilesystemFilter::All);
        let inode = visible_filesystem_rows(&snapshot.entries, "", FilesystemFilter::InodePressure);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].device, "server:/cache");
        assert_eq!(inode.len(), 1);
    }

    #[test]
    fn filesystem_commands_keep_platforms_separate() {
        let linux = build_filesystem_snapshot_command("Linux");
        let mac = build_filesystem_snapshot_command("macOS");
        let bsd = build_filesystem_snapshot_command("FreeBSD");
        let windows = build_filesystem_snapshot_command("Windows");

        assert!(linux.command.contains("df -PTB1"));
        assert!(linux.command.contains("-d 2 /"));
        assert!(linux.command.contains("INODEHOTSPOT"));
        assert_eq!(linux.capability, FilesystemCommandCapability::Full);
        assert!(mac.command.contains("df -kP"));
        assert_eq!(mac.capability, FilesystemCommandCapability::Partial);
        assert!(bsd.command.contains("df -kP"));
        assert_eq!(bsd.capability, FilesystemCommandCapability::Partial);
        assert!(windows.command.contains("Get-Volume"));
        assert!(windows.command.contains("COUNTHOTSPOT"));
        assert!(windows.command.contains("Get-ChildItem"));
        assert_eq!(windows.capability, FilesystemCommandCapability::Partial);
    }
}
