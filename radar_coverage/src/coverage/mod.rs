use bevy::prelude::*;
use bevy::tasks::Task;
use crate::geo::LatLon;
use crate::io::Radar;
use crate::terrain::{TerrainManager, SRTM3_SIZE};
use crate::physics::los::TerrainProvider;
use crate::physics::radar_eq::max_detection_range;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CoverageTile {
    pub lat_idx: i32,
    pub lon_idx: i32,
    pub size: usize,
    pub data: Vec<u8>, // 0 = invisible, 1 = visible
    pub snr_margin: Vec<f32>, // Optional: detailed SNR margin
}

#[derive(Component)]
pub struct CoverageTask(pub Task<CoverageTile>);


use crate::physics::viewshed::Viewshed;

pub fn compute_coverage_tile(
    radar: Radar,
    // terrain_manager is not needed inside if we use precomputed viewshed, 
    // BUT we need it if we want to check target altitude relative to ground?
    // Actually, viewshed stores max horizon angle.
    // To check visibility of point P at altitude A_target:
    // 1. Calculate angle from Radar to P (considering A_target).
    // 2. If Angle_P > Horizon_Angle(P.lat, P.lon), then Visible.
    terrain_manager: Arc<TerrainManager>, // Needed for ground altitude of target
    viewshed: Arc<Viewshed>,
    lat_idx: i32,
    lon_idx: i32,
    target_rcs: f64,
    target_agl: f64,
    step_size: usize,
) -> CoverageTile {
    let full_size = SRTM3_SIZE; // 1201
    let size = (full_size + step_size - 1) / step_size; 
    
    let mut data = vec![0; size * size];
    let mut snr_margin = vec![0.0; size * size];

    let max_range = max_detection_range(&radar, target_rcs);

    // K factor for refraction curvature drop
    let k = 4.0/3.0; 
    let two_k_r = 2.0 * k * crate::geo::EARTH_RADIUS as f64;

    for y in 0..size {
        for x in 0..size {
            let orig_y = (y * step_size).min(full_size - 1);
            let orig_x = (x * step_size).min(full_size - 1);
            
            let pixel_lat = (lat_idx as f64 + 1.0) - (orig_y as f64 / (full_size - 1) as f64);
            let pixel_lon = (lon_idx as f64) + (orig_x as f64 / (full_size - 1) as f64);
            let target_loc = LatLon {
                latitude: pixel_lat,
                longitude: pixel_lon,
                altitude: 0.0, 
            };

            // Range check
            let (dist, _) = crate::physics::los::calculate_geodesic(radar.location, target_loc);
            if dist > max_range {
                continue;
            }

            // Get Horizon Angle from Viewshed
            // We need to look up in the viewshed grid.
            if let Some(horizon_angle) = viewshed.get_horizon_angle(target_loc) {
                // Get terrain height for target
                let ground_alt = terrain_manager.get_altitude(target_loc) as f64;
                let target_alt = ground_alt + target_agl;
                
                // Calculate Angle to Target
                // Drop due to curvature
                let curvature_drop = (dist * dist) / two_k_r;
                let height_diff = target_alt - radar.location.altitude - curvature_drop;
                
                // Angle to target
                let target_angle = if dist > 0.1 {
                     (height_diff / dist).atan() as f32
                } else {
                     std::f32::consts::FRAC_PI_2 // 90 deg (overhead/at radar)
                };

                if target_angle >= horizon_angle {
                    data[y * size + x] = 1;
                    // Margin: difference in degrees
                    snr_margin[y * size + x] = (target_angle - horizon_angle).to_degrees();
                } else {
                    data[y * size + x] = 2; // Shadowed
                }
            } else {
                 // Outside viewshed grid (should match max range check usually)
            }
        }
    }

    CoverageTile {
        lat_idx,
        lon_idx,
        size,
        data,
        snr_margin, 
    }
}
