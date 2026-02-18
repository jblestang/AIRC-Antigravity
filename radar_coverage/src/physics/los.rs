use crate::geo::{LatLon, EARTH_RADIUS};
use crate::io::Radar;
use crate::physics::refraction::{effective_earth_radius, RefractionParams};

pub trait TerrainProvider {
    fn get_altitude(&self, loc: LatLon) -> f64;
}

#[derive(Debug, Clone, Copy)]
pub struct LosResult {
    pub is_visible: bool,
    pub margin_deg: f64,
    pub obstruction_dist_m: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
pub struct LosSystem {
    pub refraction: RefractionParams,
}

impl LosSystem {
    pub fn new(refraction: RefractionParams) -> Self {
        Self { refraction }
    }

    pub fn check_visibility<T: TerrainProvider>(
        &self,
        radar: &Radar,
        target_loc: LatLon,
        target_agl_m: f64,
        terrain: &T,
    ) -> LosResult {
        let r_eff = effective_earth_radius(self.refraction);
        
        // Calculate distance and bearing
        let (dist_m, _azimuth_deg) = calculate_geodesic(radar.location, target_loc);
        
        if dist_m < 1.0 {
            return LosResult { is_visible: true, margin_deg: 90.0, obstruction_dist_m: None };
        }

        // Sampling parameters
        let step_size_m = 100.0; // 100m steps for MVP (should be adaptive)
        let steps = (dist_m / step_size_m).ceil() as usize;

        let h_radar = radar.location.altitude + radar.antenna_height_agl;
        
        // Pre-calculate target effective parameters for final check
        // h_tgt_eff = h_tgt_amsl - d^2 / (2 * R_eff)
        let h_target_amsl = terrain.get_altitude(target_loc) + target_agl_m;
        // Wait, terrain.get_altitude might be slow if we query it for target separately.
        // It's fine.

        let mut max_angle = -std::f64::consts::FRAC_PI_2; // -90 degrees
        let mut obstruction_dist = None;

        // Effective Earth Radius Model
        // theta = atan( (h_eff(d) - h_radar) / d )
        // h_eff(d) = h_terrain(d) - d^2 / (2*R_eff)

        for i in 1..steps {
            let d = i as f64 * step_size_m;
            let ratio = d / dist_m;
            
            // Interpolate position
            // Simple linear approx for coordinates is okay for short ranges, 
            // but for "national scale" we should use geodesic direct problem.
            // For MVP, linear on Lat/Lon is acceptable approximation if far from poles.
            let lat = radar.location.latitude + (target_loc.latitude - radar.location.latitude) * ratio;
            let lon = radar.location.longitude + (target_loc.longitude - radar.location.longitude) * ratio;
            
            let pos = LatLon { latitude: lat, longitude: lon, altitude: 0.0 };
            let h_terr = terrain.get_altitude(pos);
            
            let h_eff = h_terr - (d * d) / (2.0 * r_eff);
            let angle = (h_eff - h_radar).atan2(d);

            if angle > max_angle {
                max_angle = angle;
                obstruction_dist = Some(d);
            }
        }

        // Check target visibility
        // Effective target height
        let h_tgt_eff = h_target_amsl - (dist_m * dist_m) / (2.0 * r_eff);
        let target_angle = (h_tgt_eff - h_radar).atan2(dist_m);

        let margin = target_angle - max_angle;
        
        // Epsilon for stability
        let epsilon = 1e-4;

        if margin > epsilon {
            LosResult {
                is_visible: true,
                margin_deg: margin.to_degrees(),
                obstruction_dist_m: None,
            }
        } else {
            LosResult {
                is_visible: false,
                margin_deg: margin.to_degrees(),
                obstruction_dist_m: obstruction_dist,
            }
        }
    }
}

// Helper for geodesic distance (Haversine or simple spherical)
// For MVP, spherical is enough.
pub fn calculate_geodesic(p1: LatLon, p2: LatLon) -> (f64, f64) {
    let lat1 = p1.latitude.to_radians();
    let lat2 = p2.latitude.to_radians();
    let dlat = (p2.latitude - p1.latitude).to_radians();
    let dlon = (p2.longitude - p1.longitude).to_radians();

    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    
    let dist = EARTH_RADIUS * c;
    
    // Bearing
    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    let bearing = y.atan2(x).to_degrees();

    (dist, bearing)
}
