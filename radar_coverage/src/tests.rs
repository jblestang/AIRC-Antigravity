use crate::geo::LatLon;
use crate::physics::los::{calculate_geodesic, LosSystem, TerrainProvider};
use crate::physics::refraction::RefractionParams;
use crate::io::Radar;


struct MockTerrain {
    pub altitude: f64,
}

impl TerrainProvider for MockTerrain {
    fn get_altitude(&self, _loc: LatLon) -> f64 {
        self.altitude
    }
}

#[test]
fn test_geodesic_distance() {
    let p1 = LatLon { latitude: 0.0, longitude: 0.0, altitude: 0.0 };
    let p2 = LatLon { latitude: 1.0, longitude: 0.0, altitude: 0.0 };
    
    let (dist, bearing) = calculate_geodesic(p1, p2);
    
    // 1 degree latitude ~ 111km
    assert!((dist - 111319.0).abs() < 100.0);
    assert!((bearing - 0.0).abs() < 0.1); 
}

#[test]
fn test_los_flat_earth_blocked() {
    // Test if curvature blocks view on "flat" terrain (0m) at long distance
    let radar_display = Radar {
        name: "Test".to_string(),
        location: LatLon { latitude: 0.0, longitude: 0.0, altitude: 10.0 }, // 10m tower
        antenna_height_agl: 0.0,
        tx_power_w: 0.0, gain_dbi: 0.0, frequency_mhz: 0.0, system_loss_db: 0.0, snr_threshold_db: 0.0, azimuth_sector: None, elevation_sector: None
    };
    
    let target = LatLon { latitude: 1.0, longitude: 0.0, altitude: 0.0 }; // ~111km away
    // Target at 10m AGL
    let target_agl = 10.0;
    
    let terrain = MockTerrain { altitude: 0.0 };
    let los = LosSystem::new(RefractionParams { k_factor: 1.33 }); 
    
    let result = los.check_visibility(&radar_display, target, target_agl, &terrain);
    
    // Horizon for 10m + 10m is much less than 111km. Should be blocked.
    assert!(!result.is_visible);
    assert!(result.obstruction_dist_m.is_some());
}

#[test]
fn test_los_close_visible() {
    let radar_display = Radar {
        name: "Test".to_string(),
        location: LatLon { latitude: 0.0, longitude: 0.0, altitude: 10.0 }, 
        antenna_height_agl: 0.0,
        tx_power_w: 0.0, gain_dbi: 0.0, frequency_mhz: 0.0, system_loss_db: 0.0, snr_threshold_db: 0.0, azimuth_sector: None, elevation_sector: None
    };
    
    let target = LatLon { latitude: 0.0001, longitude: 0.0, altitude: 0.0 }; // Very close
    let target_agl = 10.0;
    
    let terrain = MockTerrain { altitude: 0.0 };
    let los = LosSystem::new(RefractionParams { k_factor: 1.33 }); 
    
    let result = los.check_visibility(&radar_display, target, target_agl, &terrain);
    
    assert!(result.is_visible);
}
