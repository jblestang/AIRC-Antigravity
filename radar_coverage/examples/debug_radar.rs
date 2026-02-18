use radar_coverage::terrain::{TerrainManager, TerrainLoader};
use radar_coverage::io::Radar;
use radar_coverage::geo::LatLon;
use radar_coverage::physics::los::{LosSystem, calculate_geodesic, TerrainProvider};
use radar_coverage::physics::refraction::RefractionParams;
use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    let path = PathBuf::from("/Users/jean-baptiste/AIRC-Antigravity/radar_coverage/assets/");
    let loader = TerrainLoader::new(path);
    let terrain = Arc::new(TerrainManager::new(loader, 10));

    let radar = Radar {
        name: "Demo Radar".to_string(),
        location: LatLon { latitude: 45.1, longitude: 5.2, altitude: 200.0 }, // 200m AMSL (approx 100m underground)
        antenna_height_agl: 10.0,
        tx_power_w: 50000.0,
        gain_dbi: 35.0,
        frequency_mhz: 3000.0,
        system_loss_db: 2.0,
        snr_threshold_db: 10.0,
        azimuth_sector: None,
        elevation_sector: None,
    };

    println!("Radar params: Alt={}", radar.location.altitude);
    
    // Check altitude at radar
    let g_alt = terrain.get_altitude(radar.location);
    println!("Ground Altitude at Radar: {:.2} m", g_alt);

    let los = LosSystem::new(RefractionParams::default());
    
    // Check target 5km away
    // 45.1, 5.2 + delta
    let target_loc = LatLon { latitude: 45.1, longitude: 5.26, altitude: 0.0 }; // ~5km East
    let (dist, _) = calculate_geodesic(radar.location, target_loc);
    println!("Target dist: {:.2} km", dist / 1000.0);
    
    let res = los.check_visibility(&radar, target_loc, 10.0, &*terrain); // 10m AGL
    println!("Visibility (10m AGL): {:?}", res);
}
