use std::path::PathBuf;
use std::fs::File;
use std::io::Read;
use anyhow::{Result, Context};
use crate::geo::LatLon;
use crate::physics::los::TerrainProvider;
use std::sync::{Arc, Mutex};
use lru::LruCache;
use std::num::NonZeroUsize;

pub const SRTM3_SIZE: usize = 1201;
pub const SRTM1_SIZE: usize = 3601;

#[derive(Debug, Clone)]
pub struct TerrainTile {
    pub latitude: i32,
    pub longitude: i32,
    pub size: usize,
    pub data: Vec<i16>, // Row-major, big-endian parsed
}

use bevy::prelude::Component;

#[derive(Component)]
pub struct TerrainChunk {
    pub lat_idx: i32,
    pub lon_idx: i32,
    pub lod_step: usize,
}

impl TerrainTile {
    /// Returns altitude in meters at specific local coordinates (0.0 to 1.0)
    /// where (0,0) is top-left (NW) and (1,1) is bottom-right (SE)
    pub fn sample(&self, u: f64, v: f64) -> f64 {
        let max_idx = (self.size - 1) as f64;
        let x = u * max_idx;
        let y = v * max_idx;

        // Bilinear interpolation
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(self.size - 1);
        let y1 = (y0 + 1).min(self.size - 1);

        let tx = x - x0 as f64;
        let ty = y - y0 as f64;

        let h00 = self.get_height(x0, y0) as f64;
        let h10 = self.get_height(x1, y0) as f64;
        let h01 = self.get_height(x0, y1) as f64;
        let h11 = self.get_height(x1, y1) as f64;

        let h0 = h00 * (1.0 - tx) + h10 * tx;
        let h1 = h01 * (1.0 - tx) + h11 * tx;

        h0 * (1.0 - ty) + h1 * ty
    }


    #[inline(always)]
    pub fn get_height(&self, x: usize, y: usize) -> i16 {
        self.data[y * self.size + x]
    }

    pub fn get_max_height(&self, x: usize, y: usize, step: usize) -> i16 {
        let mut max_h = i16::MIN;
        let x_end = (x + step).min(self.size);
        let y_end = (y + step).min(self.size);

        for SampleY in y..y_end {
            for SampleX in x..x_end {
                let h = self.data[SampleY * self.size + SampleX];
                if h > max_h {
                    max_h = h;
                }
            }
        }
        if max_h == i16::MIN { 0 } else { max_h }
    }
}

pub struct TerrainLoader {
    pub assets_path: PathBuf,
}

impl TerrainLoader {
    pub fn new(assets_path: PathBuf) -> Self {
        Self { assets_path }
    }

    pub fn load_tile(&self, lat: i32, lon: i32) -> Result<TerrainTile> {
        let filename = format!("{}{:02}{}{:03}.hgt", 
            if lat >= 0 { "N" } else { "S" }, lat.abs(),
            if lon >= 0 { "E" } else { "W" }, lon.abs()
        );
        let path = self.assets_path.join(&filename);

        if !path.exists() {
            // Fallback: Return flat tile at 0m
            return Ok(TerrainTile {
                latitude: lat,
                longitude: lon,
                size: SRTM3_SIZE,
                data: vec![0; SRTM3_SIZE * SRTM3_SIZE],
            });
        }

        let mut file = File::open(&path).with_context(|| format!("Failed to open {:?}", path))?;
        let metadata = file.metadata()?;
        let size = match metadata.len() {
            2884802 => SRTM3_SIZE,
            25934402 => SRTM1_SIZE,
            len => anyhow::bail!("Unknown HGT file size: {}", len),
        };

        let mut buffer = Vec::with_capacity(size * size * 2);
        file.read_to_end(&mut buffer)?;

        let data: Vec<i16> = buffer
            .chunks_exact(2)
            .map(|chunk| i16::from_be_bytes([chunk[0], chunk[1]]))
            .collect();

        Ok(TerrainTile {
            latitude: lat,
            longitude: lon,
            size,
            data,
        })
    }
}

pub struct TerrainManager {
    loader: TerrainLoader,
    cache: Arc<Mutex<LruCache<(i32, i32), Arc<TerrainTile>>>>,
}

impl TerrainManager {
    pub fn new(loader: TerrainLoader, cache_capacity: usize) -> Self {
        Self {
            loader,
            cache: Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(cache_capacity).unwrap()))),
        }
    }

    pub fn get_tile(&self, lat: i32, lon: i32) -> Result<Arc<TerrainTile>> {
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(tile) = cache.get(&(lat, lon)) {
                return Ok(tile.clone());
            }
        }

        let tile = self.loader.load_tile(lat, lon)?;
        let tile_arc = Arc::new(tile);

        let mut cache = self.cache.lock().unwrap();
        cache.put((lat, lon), tile_arc.clone());
        
        Ok(tile_arc)
    }
}

impl TerrainProvider for TerrainManager {
    fn get_altitude(&self, loc: LatLon) -> f64 {
        let lat_deg = loc.latitude.floor() as i32;
        let lon_deg = loc.longitude.floor() as i32;
        
        // In a real implementation we would handle error gracefully or return 0.0
        match self.get_tile(lat_deg, lon_deg) {
            Ok(tile) => {
                let u = loc.longitude - lon_deg as f64;
                // SRTM is top-down, v=0 is top (North). 
                // Latitude increases North.
                // Within a tile N34, rows go from 35.0 (idx 0) to 34.0 (idx 1200).
                // So v should be (Lat_top - lat).
                // Lat_top = lat_deg + 1.
                let v = (lat_deg as f64 + 1.0) - loc.latitude;
                tile.sample(u, v)
            },
            Err(_) => 0.0,
        }
    }
}
