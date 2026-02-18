use crate::geo::LatLon;
use crate::terrain::TerrainManager;
use crate::io::Radar;
use std::collections::HashMap;
use std::sync::Arc;

/// Represents a dense grid of visibility data relative to a radar
pub struct Viewshed {
    pub origin: LatLon,
    pub radius_m: f64,
    pub cell_size_m: f64,
    pub width: usize,
    pub height: usize,
    /// Stores the maximum elevation angle (in radians) visible from the radar for each cell.
    /// If a target's elevation angle is < max_angle, it is shadowed.
    pub horizon_map: Vec<f32>, 
}

impl Viewshed {
    pub fn new(origin: LatLon, radius_m: f64, cell_size_m: f64) -> Self {
        let size = (radius_m * 2.0 / cell_size_m).ceil() as usize;
        Self {
            origin,
            radius_m,
            cell_size_m,
            width: size,
            height: size,
            horizon_map: vec![-std::f32::consts::FRAC_PI_2; size * size], // Initialize with -90 degrees (everything visible)
        }
    }

    /// Convert LatLon to local grid coordinates
    pub fn latlon_to_grid(&self, loc: LatLon) -> Option<(usize, usize)> {
        // Simple equirectangular projection for local grid (valid for short ranges < 500km)
        // Center is at width/2, height/2
        
        // 1 deg Lat ~= 111111 meters
        // 1 deg Lon ~= 111111 * cos(lat) meters
        
        let d_lat = loc.latitude - self.origin.latitude;
        let d_lon = loc.longitude - self.origin.longitude;
        
        let dy_m = d_lat * 111111.0;
        let dx_m = d_lon * 111111.0 * self.origin.latitude.to_radians().cos();
        
        let center_x = self.width as f64 / 2.0;
        let center_y = self.height as f64 / 2.0;
        
        let x = center_x + dx_m / self.cell_size_m;
        // In our grid, y increases upwards (North)
        let y = center_y + dy_m / self.cell_size_m;
        
        if x < 0.0 || x >= self.width as f64 || y < 0.0 || y >= self.height as f64 {
            return None;
        }
        
        Some((x as usize, y as usize))
    }

    pub fn get_horizon_angle(&self, loc: LatLon) -> Option<f32> {
        if let Some((x, y)) = self.latlon_to_grid(loc) {
            return Some(self.horizon_map[y * self.width + x]);
        }
        None
    }
}

use bevy::prelude::Component;

#[derive(Component)]
pub struct ViewshedProgress {
    pub current: Arc<AtomicU32>,
    pub total: u32,
}



use crate::physics::refraction::RefractionParams;
use crate::geo::EARTH_RADIUS;
use crate::physics::los::TerrainProvider;

use std::sync::atomic::{AtomicU32, Ordering};

// Optimized Viewshed Computation
pub fn compute_viewshed(
    radar: &Radar, 
    terrain: &TerrainManager, 
    max_range_m: f64, 
    k_factor: f32,
    progress: Option<Arc<AtomicU32>>
) -> Viewshed {
    let cell_size = 100.0; // 100m resolution
    let mut viewshed = Viewshed::new(radar.location, max_range_m, cell_size);
    
    let center_x = viewshed.width as isize / 2;
    let center_y = viewshed.height as isize / 2;
    // let radius_cells = (max_range_m / cell_size).ceil() as isize;
    
    // We compute 8 octants or sweep 360 degrees. 
    // Since we need to visit every cell, a perimeter walk + Bresenham to center is efficient.
    // Iterate perimeter of the square bounding box.
    
    let min_x = 0;
    let max_x = viewshed.width as isize - 1;
    let min_y = 0;
    let max_y = viewshed.height as isize - 1;

    // Ray casting function
    let cast_ray = |end_x: isize, end_y: isize, viewshed: &mut Viewshed| {
        let mut x = center_x;
        let mut y = center_y;
        
        let dx = (end_x - center_x).abs();
        let dy = (end_y - center_y).abs();
        let sx = if center_x < end_x { 1 } else { -1 };
        let sy = if center_y < end_y { 1 } else { -1 };
        let mut err = dx - dy;
        
        // Horizon tracking
        let mut max_angle = -std::f32::consts::FRAC_PI_2; // -90 deg
        
        loop {
            // Process current cell (x, y)
             if x >= 0 && x < viewshed.width as isize && y >= 0 && y < viewshed.height as isize {
                let idx = (y as usize) * viewshed.width + (x as usize);
                
                // Get ground altitude at this cell
                // Convert grid (x,y) back to lat/lon? Or direct query if we knew bounds?
                // For simplicity, let's reverse project.
                // dx_m = (x - center_x) * cell_size
                // dy_m = (y - center_y) * cell_size
                
                let dist_x = (x - center_x) as f64 * cell_size;
                let dist_y = (y - center_y) as f64 * cell_size;
                let dist_sq = dist_x*dist_x + dist_y*dist_y;
                let dist = dist_sq.sqrt();
                
                if dist > 0.0 && dist <= max_range_m {
                    // Reverse projection (approximate flat earth for small dlat/dlon, ok for lookup)
                    // d_lat = dy_m / 111111.0
                    // d_lon = dx_m / (111111.0 * cos(lat))
                    let d_lat = dist_y / 111111.0;
                    let d_lon = dist_x / (111111.0 * radar.location.latitude.to_radians().cos());
                    
                    let sample_loc = LatLon {
                        latitude: radar.location.latitude + d_lat,
                        longitude: radar.location.longitude + d_lon,
                        altitude: 0.0 
                    };
                    
                    let h_ground = terrain.get_altitude(sample_loc) as f32;
                    
                    // Effective Earth Radius Model
                    // drop = D^2 / (2 * k * R)
                    let k = k_factor as f64; 
                    let curvature_drop = (dist * dist) / (2.0 * k * EARTH_RADIUS as f64);
                    
                    // Angle calculation
                    // Angle = atan( (h_target - h_radar - drop) / dist )
                    // Here, h_target is the ground height (we are checking if ground shadows itself)
                    // Wait, we want the horizon angle *imposed* by this terrain point.
                    // The angle TO this ground point is:
                    
                    let height_diff = h_ground as f64 - radar.location.altitude - curvature_drop;
                    let angle = (height_diff / dist).atan() as f32;
                    
                    if angle > max_angle {
                        max_angle = angle;
                        // This point forms a new horizon
                        viewshed.horizon_map[idx] = max_angle;
                    } else {
                        // This point is below previous horizon, so the horizon remains the same
                        // (Ideally we store the *masking* angle, which is max_angle)
                        viewshed.horizon_map[idx] = max_angle;
                    }
                } else if dist == 0.0 {
                    // At radar
                     viewshed.horizon_map[idx] = -std::f32::consts::FRAC_PI_2;
                }
            }

            if x == end_x && y == end_y { break; }
            let e2 = 2 * err;
            if e2 > -dy { err -= dy; x += sx; }
            if e2 < dx { err += dx; y += sy; }
        }
    };
    
    // Perimeter Traversal
    // Top and Bottom
    let mut ray_count = 0;
    
    for x in min_x..=max_x {
        cast_ray(x, min_y, &mut viewshed); // Top edge (min_y)
        cast_ray(x, max_y, &mut viewshed); // Bottom edge
        ray_count += 2;
        if ray_count % 100 == 0 {
             if let Some(p) = &progress {
                 p.fetch_add(100, Ordering::Relaxed);
             }
        }
    }
    // Left and Right
    for y in min_y..=max_y {
         cast_ray(min_x, y, &mut viewshed); // Left edge
         cast_ray(max_x, y, &mut viewshed); // Right edge
         ray_count += 2;
         if ray_count % 100 == 0 {
             if let Some(p) = &progress {
                 p.fetch_add(100, Ordering::Relaxed);
             }
         }
    }

    viewshed
}
