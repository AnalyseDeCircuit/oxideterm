use gpui::{div, prelude::*, px, rgb};
use oxideterm_gpui_ui::icons::LucideIcon;
use oxideterm_workspace::{
    CommandPaletteMode as PaletteMode, command_palette_match, parse_command_palette_query,
};
pub const COMMAND_PALETTE_MIN_WIDTH: f32 = 560.0;
pub const COMMAND_PALETTE_MAX_WIDTH: f32 = 960.0;
pub const COMPACT_PALETTE_MIN_WIDTH: f32 = 360.0;
pub const COMPACT_PALETTE_MAX_WIDTH: f32 = 480.0;
pub const COMMAND_PALETTE_CONNECTION_PREVIEW_HEIGHT: f32 = 80.0;
pub const COMMAND_PALETTE_TOP_RATIO: f32 = 0.14;
pub const COMMAND_PALETTE_VIEWPORT_HEIGHT_RATIO: f32 = 0.58;
pub const COMPACT_PALETTE_VIEWPORT_HEIGHT_RATIO: f32 = 0.45;
pub const COMMAND_PALETTE_LIST_MAX_HEIGHT: f32 = 640.0; // Up to 20 rows (20 * 32px) on large displays.
pub const COMPACT_PALETTE_LIST_MAX_HEIGHT: f32 = 448.0; // Up to 14 rows (14 * 32px) for compact pickers.
pub const COMMAND_PALETTE_LIST_PADDING_Y: f32 = 4.0;
pub const COMMAND_PALETTE_INPUT_HEIGHT: f32 = 40.0; // Tauri CommandInput h-10.
pub const COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT: f32 = 32.0; // Tauri CommandItem py-1.5 + text-sm line height.
pub const COMMAND_PALETTE_VIRTUAL_OVERSCAN: usize = 8; // Browser CommandList keeps a small DOM buffer around the viewport.
pub const COMMAND_PALETTE_ICON_SLOT: f32 = 16.0; // Tauri CommandInput/CommandItem h-4 w-4 icons.
pub const COMMAND_PALETTE_ITEM_GAP: f32 = 10.0; // Tauri CommandItem gap-2.5.
pub const COMMAND_PALETTE_SELECTED_ALPHA: u32 = 0x26; // Tauri accent/15.
pub const COMMAND_PALETTE_MODE_BADGE_ALPHA: u32 = 0x33; // Tauri accent/20.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaletteSection {
    QuickConnect,
    Recent,
    Commands,
    Sessions,
    Connections,
    Transfers,
    Reconnects,
    #[allow(dead_code)]
    Plugins,
    Help,
}

#[derive(Clone)]
pub struct PaletteItem<A> {
    pub id: String,
    pub label: String,
    pub section: PaletteSection,
    pub icon: LucideIcon,
    pub detail: Option<String>,
    pub shortcut: Option<String>,
    pub count: Option<String>,
    pub active: bool,
    pub value: String,
    pub action: A,
    pub disabled: bool,
}

#[derive(Clone)]
pub struct RankedItem<A> {
    pub item: PaletteItem<A>,
    pub score: f32,
    pub highlights: Vec<usize>,
}

#[derive(Clone)]
pub enum CommandPaletteVirtualRow<A> {
    Heading {
        section: PaletteSection,
        show_separator: bool,
    },
    Item {
        ranked: RankedItem<A>,
        item_index: usize,
    },
    Empty,
}
pub fn command_palette_scroll_child_index<A>(
    ranked_items: &[RankedItem<A>],
    selected_index: usize,
) -> Option<usize> {
    let mut previous_section = None;
    let mut child_index = 0;
    for (item_index, ranked) in ranked_items.iter().enumerate() {
        if previous_section != Some(ranked.item.section) {
            previous_section = Some(ranked.item.section);
            child_index += 1;
        }
        if item_index == selected_index {
            return Some(child_index);
        }
        child_index += 1;
    }
    None
}

#[derive(Clone, Copy)]
pub enum PaletteStep {
    Forward(usize),
    Backward(usize),
    First,
    Last,
}

pub fn resolve_command_palette_selected_index<A>(
    ranked_items: &[RankedItem<A>],
    selected_index: usize,
) -> Option<usize> {
    if ranked_items.is_empty() {
        return None;
    }
    if let Some(item) = ranked_items.get(selected_index)
        && !item.item.disabled
    {
        return Some(selected_index);
    }
    ranked_items.iter().position(|ranked| !ranked.item.disabled)
}

pub fn next_enabled_palette_index<A>(
    ranked_items: &[RankedItem<A>],
    current: usize,
    step: PaletteStep,
) -> Option<usize> {
    if ranked_items.is_empty() {
        return None;
    }
    let last = ranked_items.len() - 1;
    let current = current.min(last);
    match step {
        PaletteStep::First => ranked_items.iter().position(|ranked| !ranked.item.disabled),
        PaletteStep::Last => ranked_items
            .iter()
            .rposition(|ranked| !ranked.item.disabled),
        PaletteStep::Forward(delta) => {
            let start = current.saturating_add(delta).min(last);
            (start..=last)
                .find(|&idx| !ranked_items[idx].item.disabled)
                .or_else(|| {
                    ((current + 1)..start)
                        .rev()
                        .find(|&idx| !ranked_items[idx].item.disabled)
                })
        }
        PaletteStep::Backward(delta) => {
            let start = current.saturating_sub(delta);
            (0..=start)
                .rev()
                .find(|&idx| !ranked_items[idx].item.disabled)
                .or_else(|| ((start + 1)..current).find(|&idx| !ranked_items[idx].item.disabled))
        }
    }
}

pub fn compact_palette_panel_width(viewport_width: f32) -> f32 {
    (viewport_width * 0.36)
        .round()
        .clamp(COMPACT_PALETTE_MIN_WIDTH, COMPACT_PALETTE_MAX_WIDTH)
        .min((viewport_width - 16.0).max(0.0))
}

pub fn command_palette_list_max_height(
    viewport_height: f32,
    palette_top: f32,
    preview_height: f32,
    compact: bool,
) -> f32 {
    let (ratio, cap) = if compact {
        (
            COMPACT_PALETTE_VIEWPORT_HEIGHT_RATIO,
            COMPACT_PALETTE_LIST_MAX_HEIGHT,
        )
    } else {
        (
            COMMAND_PALETTE_VIEWPORT_HEIGHT_RATIO,
            COMMAND_PALETTE_LIST_MAX_HEIGHT,
        )
    };
    let proportional_target = viewport_height * ratio;
    let available = (viewport_height
        - palette_top
        - COMMAND_PALETTE_INPUT_HEIGHT
        - preview_height
        - COMMAND_PALETTE_LIST_PADDING_Y * 2.0
        - 48.0)
        .max(0.0);
    snap_command_palette_list_height(proportional_target.min(available), cap)
}

pub fn snap_command_palette_list_height(available_height: f32, max_height: f32) -> f32 {
    let capped = available_height.max(0.0).min(max_height);
    let whole_rows = (capped / COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT).floor();
    whole_rows * COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT
}

pub fn command_palette_virtual_rows<A>(
    ranked_items: Vec<RankedItem<A>>,
) -> Vec<CommandPaletteVirtualRow<A>> {
    if ranked_items.is_empty() {
        return vec![CommandPaletteVirtualRow::Empty];
    }

    let mut rows = Vec::new();
    let mut previous_section = None;
    for (index, ranked) in ranked_items.into_iter().enumerate() {
        if previous_section != Some(ranked.item.section) {
            let show_separator = previous_section.is_some();
            previous_section = Some(ranked.item.section);
            rows.push(CommandPaletteVirtualRow::Heading {
                section: ranked.item.section,
                show_separator,
            });
        }
        rows.push(CommandPaletteVirtualRow::Item {
            ranked,
            item_index: index,
        });
    }
    rows
}
pub fn command_palette_panel_width(viewport_width: f32) -> f32 {
    (viewport_width * 0.65)
        .clamp(COMMAND_PALETTE_MIN_WIDTH, COMMAND_PALETTE_MAX_WIDTH)
        .min((viewport_width - 32.0).max(0.0))
}

pub fn command_palette_connection_preview<A>(
    tokens: &oxideterm_theme::ThemeTokens,
    item: &PaletteItem<A>,
) -> gpui::Stateful<gpui::Div> {
    // Keep the list stationary while keyboard selection changes. Long names wrap
    // and scroll here instead of being truncated again in the detail view.
    div()
        .id((
            gpui::ElementId::from("command-palette-connection-preview"),
            item.id.clone(),
        ))
        .w_full()
        .h(px(COMMAND_PALETTE_CONNECTION_PREVIEW_HEIGHT))
        .overflow_y_scroll()
        .border_t_1()
        .border_color(rgb(tokens.ui.border))
        .px(px(12.0))
        .py(px(8.0))
        .text_size(px(12.0))
        .line_height(px(18.0))
        .whitespace_normal()
        .child(
            div()
                .w_full()
                .text_color(rgb(tokens.ui.text))
                .child(item.label.clone()),
        )
        .when_some(item.detail.as_ref(), |preview, detail| {
            preview.child(
                div()
                    .w_full()
                    .text_color(rgb(tokens.ui.text_muted))
                    .child(detail.clone()),
            )
        })
}

pub fn rank_palette_section<A>(items: Vec<PaletteItem<A>>, query: &str) -> Vec<RankedItem<A>> {
    let mut ranked = items
        .into_iter()
        .filter_map(|item| rank_palette_item(item, query))
        .collect::<Vec<_>>();
    if !query.is_empty() {
        sort_palette_results(&mut ranked);
    }
    ranked
}

pub fn palette_detail_highlights(detail: &str, raw_query: &str) -> Vec<usize> {
    let (_, query) = parse_command_palette_query(raw_query);
    let mut highlights = Vec::new();
    for term in query.split_whitespace() {
        if let Some(matched) = command_palette_match(detail, detail, term) {
            if matched.score >= 1.0 {
                highlights.extend(matched.highlights);
            }
        }
    }
    highlights.sort_unstable();
    highlights.dedup();
    highlights
}

pub fn sort_palette_results<A>(ranked: &mut [RankedItem<A>]) {
    ranked.sort_by(|left, right| {
        right.score.total_cmp(&left.score).then_with(|| {
            let left_active = left.item.section == PaletteSection::Sessions;
            let right_active = right.item.section == PaletteSection::Sessions;
            right_active.cmp(&left_active)
        })
    });
}

pub fn rank_palette_item<A>(item: PaletteItem<A>, query: &str) -> Option<RankedItem<A>> {
    let palette_match = command_palette_match(&item.label, &item.value, query)?;
    Some(RankedItem {
        item,
        score: palette_match.score,
        highlights: palette_match.highlights,
    })
}
pub fn section_label_key(section: PaletteSection) -> &'static str {
    match section {
        PaletteSection::QuickConnect => "command_palette.quick_connect",
        PaletteSection::Recent => "command_palette.section_recent",
        PaletteSection::Commands => "command_palette.section_commands",
        PaletteSection::Sessions => "command_palette.section_sessions",
        PaletteSection::Connections => "command_palette.section_connections",
        PaletteSection::Transfers => "sftp.queue.title",
        PaletteSection::Reconnects => "sidebar.background_tasks.reconnecting",
        PaletteSection::Plugins => "command_palette.section_plugins",
        PaletteSection::Help => "command_palette.section_help",
    }
}

pub fn command_palette_placeholder_key(mode: PaletteMode) -> &'static str {
    match mode {
        PaletteMode::All => "command_palette.placeholder",
        PaletteMode::Commands => "command_palette.placeholder_commands",
        PaletteMode::Sessions => "command_palette.placeholder_sessions",
        PaletteMode::Connections => "command_palette.placeholder_connections",
    }
}

/// Combine provider sections with MRU and quick-connect results before applying the global limit.
pub fn rank_commands<A: Clone>(
    mode: PaletteMode,
    query: &str,
    quick_connect: Option<PaletteItem<A>>,
    mru: &[String],
    mut command_items: Vec<PaletteItem<A>>,
    mut session_items: Vec<PaletteItem<A>>,
    mut connection_items: Vec<PaletteItem<A>>,
    mut plugin_items: Vec<PaletteItem<A>>,
    mut help_items: Vec<PaletteItem<A>>,
) -> Vec<RankedItem<A>> {
    use std::collections::{HashMap, HashSet};
    let mut ranked = Vec::new();

    if mode == PaletteMode::All {
        if let Some(item) = quick_connect {
            ranked.push(RankedItem {
                item,
                score: 4.0,
                highlights: Vec::new(),
            });
        }
    }

    let mut recent_ids = HashSet::new();
    if mode == PaletteMode::All && query.is_empty() {
        let mut by_id = HashMap::<String, PaletteItem<A>>::new();
        for item in command_items
            .iter()
            .chain(session_items.iter())
            .chain(connection_items.iter())
            .chain(plugin_items.iter())
            .chain(help_items.iter())
        {
            by_id.insert(item.id.clone(), item.clone());
        }
        for id in mru.iter().take(5) {
            if let Some(mut item) = by_id.get(id).cloned() {
                recent_ids.insert(item.id.clone());
                item.section = PaletteSection::Recent;
                ranked.push(RankedItem {
                    item,
                    score: 1.0,
                    highlights: Vec::new(),
                });
            }
        }
        if !recent_ids.is_empty() {
            command_items.retain(|item| !recent_ids.contains(&item.id));
            session_items.retain(|item| !recent_ids.contains(&item.id));
            connection_items.retain(|item| !recent_ids.contains(&item.id));
            plugin_items.retain(|item| !recent_ids.contains(&item.id));
            help_items.retain(|item| !recent_ids.contains(&item.id));
        }
    }

    if matches!(mode, PaletteMode::All | PaletteMode::Commands) {
        ranked.extend(rank_palette_section(command_items, &query));
    }
    if matches!(mode, PaletteMode::All | PaletteMode::Sessions) {
        ranked.extend(rank_palette_section(session_items, &query));
    }
    if matches!(mode, PaletteMode::All | PaletteMode::Connections) {
        ranked.extend(rank_palette_section(connection_items, &query));
    }
    if matches!(mode, PaletteMode::All | PaletteMode::Commands) {
        ranked.extend(rank_palette_section(plugin_items, &query));
        ranked.extend(rank_palette_section(help_items, &query));
    }

    if !query.trim().is_empty() {
        sort_palette_results(&mut ranked);
    }
    ranked.truncate(80);
    ranked
}
