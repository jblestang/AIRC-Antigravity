use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use crate::geo::LatLon;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

#[derive(Debug, Clone, Serialize, Deserialize, Component)]
pub struct Radar {
    pub name: String,
    pub location: LatLon,
    pub antenna_height_agl: f64, // meters
    pub tx_power_w: f64,         // Watts
    pub gain_dbi: f64,           // dBi
    pub frequency_mhz: f64,      // MHz
    pub system_loss_db: f64,     // dB
    pub snr_threshold_db: f64,   // dB
    pub azimuth_sector: Option<(f64, f64)>, // min/max degrees
    pub elevation_sector: Option<(f64, f64)>, // min/max degrees
}

#[derive(Resource, Default)]
pub struct RadarList(pub Vec<Radar>);

impl Radar {
    pub fn get_erps_w(&self) -> f64 {
        let gain_linear = 10.0f64.powf(self.gain_dbi / 10.0);
        let loss_linear = 10.0f64.powf(-self.system_loss_db / 10.0);
        self.tx_power_w * gain_linear * loss_linear
    }
}

pub fn load_radars_from_json(path: &str) -> anyhow::Result<Vec<Radar>> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let radars: Vec<Radar> = serde_json::from_reader(reader)?;
    Ok(radars)
}

pub fn compute_radar_set_hash(radars: &[Radar]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for radar in radars {
        radar.name.hash(&mut hasher);
        radar.location.latitude.to_bits().hash(&mut hasher);
        radar.location.longitude.to_bits().hash(&mut hasher);
        // Hash other critical parameters for coverage calculation
        radar.frequency_mhz.to_bits().hash(&mut hasher);
        radar.tx_power_w.to_bits().hash(&mut hasher);
        // ... add significant digits hashing if stability is an issue, 
        // to_bits is exact for identical floats.
    }
    hasher.finish()
}
