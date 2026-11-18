use std::{cell::RefCell, rc::Rc, time::Duration};

use gpui::{Context, Task};
use oxideterm_i18n::{I18n, Locale};

use super::SettingsSurfaceView;
use crate::{SettingsSearchResult, settings_search_results};

const SEARCH_FILTER_DELAY: Duration = Duration::from_millis(120);
type SearchCache = (Locale, Rc<[SettingsSearchResult]>);

#[derive(Default)]
pub(super) struct SettingsSearchFilter {
    observed_query: String,
    applied_query: String,
    cached: RefCell<Option<SearchCache>>,
    pending: Option<Task<()>>,
}

impl SettingsSearchFilter {
    pub(super) fn observed_query(&self) -> &str {
        &self.observed_query
    }

    pub(super) fn results(&self, i18n: &I18n) -> Rc<[SettingsSearchResult]> {
        let mut cache = self.cached.borrow_mut();
        if let Some((locale, results)) = cache.as_ref()
            && *locale == i18n.locale()
        {
            return results.clone();
        }
        let results: Rc<[_]> = if self.applied_query.is_empty() {
            Rc::from([])
        } else {
            settings_search_results(i18n, &self.applied_query).into()
        };
        *cache = Some((i18n.locale(), results.clone()));
        results
    }

    fn apply(&mut self, query: String) {
        self.applied_query = query;
        self.cached.get_mut().take();
    }
}

impl SettingsSurfaceView {
    pub(super) fn queue_search_filter(&mut self, query: String, cx: &mut Context<Self>) {
        let filter = &mut self.search_filter;
        filter.observed_query.clone_from(&query);
        // The view owns the only timer. Clearing the query cancels it immediately.
        filter.pending = None;
        if query.is_empty() {
            filter.apply(query);
            return;
        }
        filter.pending = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(SEARCH_FILTER_DELAY).await;
            let _ = view.update(cx, |view, cx| {
                view.search_filter.apply(query);
                cx.notify();
            });
        }));
    }
}
