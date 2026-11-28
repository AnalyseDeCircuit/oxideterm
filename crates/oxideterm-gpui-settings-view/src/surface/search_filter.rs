#[cfg(not(target_family = "wasm"))]
use std::time::Duration;
use std::{cell::RefCell, rc::Rc};

#[cfg(not(target_family = "wasm"))]
use gpui::{Context, Task};
use oxideterm_i18n::{I18n, Locale};

#[cfg(not(target_family = "wasm"))]
use super::SettingsSurfaceView;
use crate::SettingsSearchResult;

#[cfg(not(target_family = "wasm"))]
const SEARCH_FILTER_DELAY: Duration = Duration::from_millis(120);
type SearchCache = (Locale, Rc<[SettingsSearchResult]>);

pub(super) struct SettingsSearchFilter {
    #[cfg(not(target_family = "wasm"))]
    observed_query: String,
    applied_query: String,
    search: Box<dyn Fn(&I18n, &str) -> Vec<SettingsSearchResult>>,
    cached: RefCell<Option<SearchCache>>,
    #[cfg(not(target_family = "wasm"))]
    pending: Option<Task<()>>,
}

impl Default for SettingsSearchFilter {
    fn default() -> Self {
        Self::new(crate::settings_search_results)
    }
}

impl SettingsSearchFilter {
    pub(super) fn new(search: impl Fn(&I18n, &str) -> Vec<SettingsSearchResult> + 'static) -> Self {
        Self {
            #[cfg(not(target_family = "wasm"))]
            observed_query: String::new(),
            #[cfg(not(target_family = "wasm"))]
            pending: None,
            applied_query: String::new(),
            search: Box::new(search),
            cached: Default::default(),
        }
    }
    #[cfg(not(target_family = "wasm"))]
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
            (self.search)(i18n, &self.applied_query).into()
        };
        *cache = Some((i18n.locale(), results.clone()));
        results
    }

    pub(super) fn apply(&mut self, query: String) {
        self.applied_query = query;
        self.cached.get_mut().take();
    }
}

#[cfg(not(target_family = "wasm"))]
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
