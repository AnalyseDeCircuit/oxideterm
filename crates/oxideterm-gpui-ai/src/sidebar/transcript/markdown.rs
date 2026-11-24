struct AiMarkdownProjection {
    #[cfg(not(target_family = "wasm"))]
    source: String,
    document: MarkdownDocument,
}

struct AiCachedMarkdownDocument {
    projection: Arc<AiMarkdownProjection>,
    layout: MarkdownBlockLayout,
}

impl AiAssistantSidebarView {
    fn cached_ai_markdown_document(
        &self,
        source: &str,
        options: &MarkdownOptions,
        cacheable: bool,
        cx: &App,
    ) -> AiCachedMarkdownDocument {
        #[cfg(not(target_family = "wasm"))]
        let projection = {
            use std::hash::{Hash, Hasher};
            let ai = self.state.read(cx);
            let conversation = ai
                .conversation_state()
                .active_conversation_id
                .as_deref()
                .unwrap_or_default();
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            source.hash(&mut hash);
            let key = format!("markdown:{:016x}", hash.finish());
            let store = ai.history.store.as_ref();
            cacheable
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
                })
        };
        #[cfg(target_family = "wasm")]
        let projection = {
            let _ = (cacheable, cx);
            Arc::new(AiMarkdownProjection {
                document: markdown_parser::parse(source),
            })
        };
        // Layout belongs to this visible render, so it cannot retain an evicted document.
        let layout = MarkdownBlockLayout::from_document(&projection.document, options);
        AiCachedMarkdownDocument { projection, layout }
    }
}

pub fn time_label(timestamp_ms: i64, today_label: &str, yesterday_label: &str) -> String {
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
