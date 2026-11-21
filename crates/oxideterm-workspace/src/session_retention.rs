//! Monotonic, process-local retention for closed session records.
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    time::{Duration, Instant},
};

pub struct SessionRetention<K> {
    since: HashMap<K, Instant>,
}

impl<K> Default for SessionRetention<K> {
    fn default() -> Self {
        Self {
            since: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash + Clone> SessionRetention<K> {
    pub fn forget(&mut self, id: &K) {
        self.since.remove(id);
    }

    /// Only uninterrupted eligibility counts. Expired parents may remain blocked by children;
    /// their next opportunity is a child deadline or lifecycle event, never an immediate retry.
    pub fn reconcile(
        &mut self,
        eligible: HashSet<K>,
        duration: Duration,
        now: Instant,
    ) -> (HashSet<K>, Option<Instant>) {
        self.since.retain(|id, _| eligible.contains(id));
        let mut expired = HashSet::new();
        let mut next = None;
        for id in eligible {
            let start = self.since.entry(id.clone()).or_insert(now);
            let deadline = *start + duration;
            if deadline <= now {
                expired.insert(id);
            } else {
                next = Some(next.map_or(deadline, |old: Instant| old.min(deadline)));
            }
        }
        (expired, next)
    }
}
