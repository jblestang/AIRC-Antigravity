use criterion::{black_box, criterion_group, criterion_main, Criterion};
use radar_coverage::coverage::compute_coverage_tile;
use radar_coverage::terrain::{TerrainManager, TerrainLoader};
use radar_coverage::physics::los::{LosSystem};
use radar_coverage::physics::refraction::RefractionParams;
use radar_coverage::io::Radar;
use radar_coverage::geo::LatLon;
use std::path::PathBuf;
use std::sync::Arc;

fn coverage_benchmark(c: &mut Criterion) {
    // Setup - Mock simplistic terrain manager if possible, or load real assets
    // For benchmark stability, loading real assets is risky if path changes. 
    // Ideally we mock TerrainProvider. But for MVP we use real one with fallback.
    
    let loader = TerrainLoader::new(PathBuf::from("assets"));
    let terrain_manager = Arc::new(TerrainManager::new(loader, 10));
    
    let radar = Radar {
        name: "Bench Radar".to_string(),
        location: LatLon { latitude: 45.0, longitude: 5.0, altitude: 200.0 },
        antenna_height_agl: 10.0,
        tx_power_w: 1000.0,
        gain_dbi: 30.0,
        frequency_mhz: 3000.0,
        system_loss_db: 2.0,
        snr_threshold_db: 10.0,
        azimuth_sector: None,
        elevation_sector: None,
    };
    
    let los = LosSystem::new(RefractionParams { k_factor: 1.33 });

    c.bench_function("compute_coverage_tile", |b| {
        b.iter(|| {
            compute_coverage_tile(
                black_box(radar.clone()),
                black_box(terrain_manager.clone()),
                black_box(los),
                black_box(45),
                black_box(5),
                black_box(1.0),
                black_box(50.0),
                black_box(1), // Full resolution
            )
        })
    });
}

criterion_group!(benches, coverage_benchmark);
criterion_main!(benches);
