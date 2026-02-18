use bevy::prelude::*;
use lru::LruCache;
use std::sync::{Arc, Mutex};
use std::num::NonZeroUsize;
use crate::coverage::CoverageTile;

#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub struct CoverageKey {
    pub lat: i32,
    pub lon: i32,
    pub target_agl_m: i16, 
    pub radar_hash: u64,
    // We might want to include refraction params hash here too
    // For MVP, assuming constant refraction or clearing cache on change is simpler
}

#[derive(Resource, Default)]
pub struct CoverageMetrics {
    pub tiles_computed: u32,
    pub cache_hits: u32,
    pub last_compute_time_ms: u64,
}

#[derive(Resource)]
pub struct CoverageCache {
    cache: Arc<Mutex<LruCache<CoverageKey, Arc<CoverageTile>>>>,
}

impl Default for CoverageCache {
    fn default() -> Self {
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(100).unwrap()))),
        }
    }
}

impl CoverageCache {
    pub fn get(&self, key: &CoverageKey) -> Option<Arc<CoverageTile>> {
        let mut cache = self.cache.lock().unwrap();
        cache.get(key).cloned()
    }

    pub fn insert(&self, key: CoverageKey, tile: Arc<CoverageTile>) {
        let mut cache = self.cache.lock().unwrap();
        cache.put(key, tile);
    }
    
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }
}
