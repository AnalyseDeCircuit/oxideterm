pub(in crate::workspace) fn ai_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

impl WorkspaceApp {
    pub(in crate::workspace) fn cached_ai_markdown_document(
        &self,
        source: &str,
        options: &MarkdownOptions,
        cacheable: bool,
        cx: &App,
    ) -> AiCachedMarkdownDocument {
        use std::hash::{Hash, Hasher};
        let ai = self.ai_entity.read(cx);
        let conversation = ai
            .conversation_state()
            .active_conversation_id
            .as_deref()
            .unwrap_or_default();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hash);
        let key = format!("markdown:{:016x}", hash.finish());
        let store = ai.history.store.as_ref();
        let projection = cacheable
            .then(|| {
                store.and_then(|store| {
                    store.cached_render::<AiMarkdownProjection>(conversation, &key)
                })
            })
            .flatten()
            .filter(|projection| projection.source == source)
            .unwrap_or_else(|| {
                let projection = Arc::new(AiMarkdownProjection {
                    source: source.to_owned(),
                    document: markdown_parser::parse(source),
                });
                if cacheable {
                    if let Some(store) = store {
                        let bytes = std::mem::size_of::<AiMarkdownProjection>()
                            + projection.source.capacity()
                            + projection.document.retained_bytes();
                        store.cache_render(conversation, &key, projection.clone(), bytes);
                    }
                }
                projection
            });
        // Layout belongs to this visible render, so it cannot retain an evicted document.
        let layout = MarkdownBlockLayout::from_document(&projection.document, options);
        AiCachedMarkdownDocument { projection, layout }
    }
}

pub(in crate::workspace) fn time_label(
    timestamp_ms: i64,
    today_label: &str,
    yesterday_label: &str,
) -> String {
    use chrono::{Local, TimeZone};

    Local
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .map(|time| {
            format_local_time_label(
                time.naive_local(),
                Local::now().naive_local(),
                today_label,
                yesterday_label,
            )
        })
        .unwrap_or_else(|| "--:--".to_string())
}

fn format_local_time_label(
    time: chrono::NaiveDateTime,
    now: chrono::NaiveDateTime,
    today_label: &str,
    yesterday_label: &str,
) -> String {
    use chrono::Datelike;

    let clock = time.format("%H:%M");
    match now.date().signed_duration_since(time.date()).num_days() {
        0 => format!("{today_label} {clock}"),
        1 => format!("{yesterday_label} {clock}"),
        _ if time.year() == now.year() => time.format("%m-%d %H:%M").to_string(),
        _ => time.format("%Y-%m-%d %H:%M").to_string(),
    }
}

#[cfg(test)]
mod time_label_tests {
    use super::format_local_time_label;
    use chrono::NaiveDate;

    fn local_time(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
    ) -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(year, month, day)
            .expect("valid date")
            .and_hms_opt(hour, minute, 0)
            .expect("valid time")
    }

    #[test]
    fn ai_time_labels_include_relative_or_calendar_dates() {
        let now = local_time(2026, 7, 29, 16, 0);

        assert_eq!(
            format_local_time_label(local_time(2026, 7, 29, 8, 5), now, "Today", "Yesterday"),
            "Today 08:05"
        );
        assert_eq!(
            format_local_time_label(local_time(2026, 7, 28, 23, 17), now, "Today", "Yesterday"),
            "Yesterday 23:17"
        );
        assert_eq!(
            format_local_time_label(local_time(2026, 7, 20, 9, 30), now, "Today", "Yesterday"),
            "07-20 09:30"
        );
        assert_eq!(
            format_local_time_label(local_time(2025, 12, 31, 23, 59), now, "Today", "Yesterday"),
            "2025-12-31 23:59"
        );
    }
}
