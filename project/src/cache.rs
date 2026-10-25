//! Process-local bounded LRU reuse of immutable compiled engines.
use crate::{expr::Params, Engine, Mode};
use std::{collections::VecDeque, rc::Rc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub compilations: u64,
    pub evictions: u64,
}
struct Entry {
    pattern: String,
    engine: Rc<Engine>,
}
/// Entries are bounded; callers can keep evicted engines alive through Rc.
/// The cache is local to one thread and includes static parameters and mode in
/// its identity. A hit neither parses the pattern nor allocates a new key.
pub struct EngineCache {
    capacity: usize,
    entries: VecDeque<Entry>,
    stats: CacheStats,
}
impl EngineCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: VecDeque::new(),
            stats: CacheStats::default(),
        }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn stats(&self) -> CacheStats {
        self.stats
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn get_or_compile(
        &mut self,
        pattern: &str,
        params: &Params,
        mode: Mode,
    ) -> Result<Rc<Engine>, String> {
        if let Some(index) = self.entries.iter().position(|e| {
            e.pattern == pattern && e.engine.mode == mode && &e.engine.program().params == params
        }) {
            let entry = self.entries.remove(index).unwrap();
            let engine = Rc::clone(&entry.engine);
            self.entries.push_back(entry);
            self.stats.hits = self.stats.hits.saturating_add(1);
            return Ok(engine);
        }
        self.stats.misses = self.stats.misses.saturating_add(1);
        let engine = Rc::new(Engine::new(pattern, params.clone(), mode)?);
        self.stats.compilations = self.stats.compilations.saturating_add(1);
        if self.capacity > 0 {
            if self.entries.len() == self.capacity {
                self.entries.pop_front();
                self.stats.evictions = self.stats.evictions.saturating_add(1);
            }
            self.entries.push_back(Entry {
                pattern: pattern.to_owned(),
                engine: Rc::clone(&engine),
            });
        }
        Ok(engine)
    }
}
