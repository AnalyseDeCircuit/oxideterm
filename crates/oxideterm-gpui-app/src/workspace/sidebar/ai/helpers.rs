fn ai_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

impl WorkspaceApp {
    fn cached_ai_markdown_document(
        &self,
        source: &str,
        options: &MarkdownOptions,
        cacheable: bool,
    ) -> AiCachedMarkdownDocument {
        if !cacheable {
            let document = markdown_parser::parse(source);
            let layout = MarkdownBlockLayout::from_document(&document, options);
            return AiCachedMarkdownDocument { document, layout };
        }

        if let Some(cached) = self.ai_markdown_cache.borrow().documents.get(source).cloned() {
            return cached;
        }

        let document = markdown_parser::parse(source);
        let layout = MarkdownBlockLayout::from_document(&document, options);
        let cached = AiCachedMarkdownDocument { document, layout };
        let mut cache = self.ai_markdown_cache.borrow_mut();
        if !cache.documents.contains_key(source) {
            cache.insertion_order.push_back(source.to_string());
        }
        cache.documents.insert(source.to_string(), cached.clone());

        while cache.documents.len() > AI_MARKDOWN_DOCUMENT_CACHE_MAX_ENTRIES {
            let Some(oldest) = cache.insertion_order.pop_front() else {
                break;
            };
            cache.documents.remove(&oldest);
        }

        cached
    }
}

fn time_label(timestamp_ms: i64) -> String {
    use chrono::{Local, TimeZone};

    Local
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .map(|time| time.format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".to_string())
}
