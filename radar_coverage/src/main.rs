use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use bevy_egui::EguiPlugin;
use futures_lite::future;
use std::sync::Arc;
use std::path::PathBuf;

use radar_coverage::geo::LatLon;
use radar_coverage::io::Radar;
use radar_coverage::terrain::{TerrainManager, TerrainLoader};
use radar_coverage::physics::refraction::RefractionParams;
// use radar_coverage::render;
use radar_coverage::render::{create_terrain_mesh, create_coverage_texture};
use radar_coverage::ui::{MapController, map_control_system, ui_panel_system};
use radar_coverage::coverage::compute_coverage_tile; 
// use radar_coverage::physics::los::{LosSystem, TerrainProvider}; 
use radar_coverage::cache::{CoverageKey, CoverageMetrics, CoverageCache};
use std::time::Instant;

#[derive(Resource)]
struct TerrainResource(Arc<TerrainManager>);

#[derive(Component)]
struct CoverageChunk {
    lat_idx: i32,
    lon_idx: i32,
    target_agl: f32,
    radar_hash: u64,
    radar_unique_id: u64, // Stable ID (name hash) to identify ownership
}

#[derive(Component)]
struct CoverageTask(pub bevy::tasks::Task<radar_coverage::coverage::CoverageTile>);

#[derive(Component)]
struct ComputingCoverage {
    lat: i32,
    lon: i32,
    target_agl: f32, // Store AGL to reconstruct key correctly
    radar_hash: u64,
    radar_unique_id: u64,
}

// Marker for tasks that are running
#[derive(Component)]
struct TerrainLoadingTask {
    task: bevy::tasks::Task<Option<(i32, i32, Mesh)>>,
    lat: i32,
    lon: i32,
}

#[derive(Component)]
struct RadarViewshed(Arc<radar_coverage::physics::viewshed::Viewshed>);

use radar_coverage::physics::viewshed::compute_viewshed;
// use radar_coverage::physics::radar_eq::max_detection_range;
use radar_coverage::physics::viewshed::ViewshedProgress;

use std::sync::atomic::AtomicU32;

fn main() {
    let terrain_manager = TerrainManager::new(
        TerrainLoader::new(PathBuf::from("/Users/jean-baptiste/AIRC-Antigravity/radar_coverage/assets/")),
        50 // Cache 50 tiles
    );
    let terrain_arc = Arc::new(terrain_manager);

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin)
        .add_plugins(bevy::pbr::wireframe::WireframePlugin)
        .init_resource::<MapController>()
        .init_resource::<RefractionParams>()
        .init_resource::<radar_coverage::cache::CoverageCache>()
        .init_resource::<radar_coverage::cache::CoverageMetrics>()
        .insert_resource(TerrainResource(terrain_arc.clone()))
        // Load 3 radars at their specific locations
        .init_resource::<radar_coverage::cache::CoverageCache>()
        .init_resource::<radar_coverage::cache::CoverageMetrics>()
        .insert_resource(TerrainResource(terrain_arc.clone()))
        .add_systems(Startup, (setup, setup_radars))
        .add_systems(Update, (
            map_control_system,
            ui_panel_system, 
            simple_terrain_loader,
            update_radar_viewshed,
            handle_viewshed_tasks,
            handle_terrain_loading,
            draw_radar_gizmos,
            schedule_coverage_tasks,
            handle_coverage_tasks,
// renedr::update_mesh_visibility,
            // render::update_coverage_texture,
        ))
        .run();
}

fn setup_radars(mut commands: Commands, terrain_res: Res<TerrainResource>) {
    let definitions = vec![
        ("Lyon Mont Verdun", 45.8511, 4.7933, 626.0),
        ("Sainte Baume", 43.3164, 5.6835, 1148.0),
        ("Nice Mont Agel", 43.7411, 7.4208, 1151.0),
    ];

    let _terrain_manager = &terrain_res.0;

    for (name, lat, lon, altitude) in definitions {
        // Query ground altitude? We trust the definition for now or update it.
        // Actually, let's just spawn them.
        println!("Configuring {}: Lat {}, Lon {}, Alt {} m", name, lat, lon, altitude);

        commands.spawn(Radar {
            name: name.to_string(),
            location: LatLon { latitude: lat, longitude: lon, altitude: altitude + 20.0 }, // +20m mast
            antenna_height_agl: 0.0, // Already included in altitude
            tx_power_w: 150000.0, 
            gain_dbi: 42.0, 
            frequency_mhz: 3100.0, 
            system_loss_db: 3.0, 
            snr_threshold_db: 13.0, 
            azimuth_sector: None,
            elevation_sector: None,
        });
    }
}

fn setup(mut commands: Commands) {
    // Camera
    // Initial Camera Position based on MapController Default (Lat 45, Lon 5)
    // Scale = 111111.0
    let scale = 111111.0;
    let start_lat = 45.5;
    let start_lon = 5.5;
    let start_x = start_lon * scale;
    let start_z = -start_lat * scale;

    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            far: 4_000_000.0,
            near: 100.0, // Increase near plane to avoid Z-fighting at large scales
            ..default()
        }),
        Transform::from_xyz(start_x, 2000000.0, start_z) // Start high up
            .looking_at(Vec3::new(start_x, 0.0, start_z), Vec3::NEG_Z),
    ));

    // Light
    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_4)),
    ));
    
    commands.insert_resource(bevy::pbr::wireframe::WireframeConfig {
        global: false, // Only draw on entities with Wireframe component
        default_color: Color::WHITE,
    });
}

// Simple system to trigger loading of a 3x3 grid around start
fn simple_terrain_loader(
    mut commands: Commands,
    terrain_res: Res<TerrainResource>,
    loading_tasks: Query<&TerrainLoadingTask>,
    existing_chunks: Query<&radar_coverage::terrain::TerrainChunk>,
) {
    let center_lat = 45;
    let center_lon = 5;
    let radius = 5; // Reduced from 10 to 5 (11x11 = 121 tiles)

    // Spawn Loop
    let task_pool = AsyncComputeTaskPool::get();
    
    // Fixed step for simplified visualization and large scale
    let step = 16; // Low detail to handle 400+ tiles

    for dlat in -radius..=radius {
        for dlon in -radius..=radius {
            let lat = center_lat + dlat;
            let lon = center_lon + dlon;

            // Check if exists
            if existing_chunks.iter().any(|c| c.lat_idx == lat && c.lon_idx == lon) {
                continue;
            }
            // Check if loading
            if loading_tasks.iter().any(|t| t.lat == lat && t.lon == lon) {
                continue;
            }

            let terrain_manager = terrain_res.0.clone();
            let task = task_pool.spawn(async move {
                let tile_res = terrain_manager.get_tile(lat, lon);
                if let Ok(tile) = tile_res {
                    // Use Vertex Colors
                    let mesh = create_terrain_mesh(&tile, step);
                    return Some((lat, lon, mesh));
                }
                None
            });

            commands.spawn(TerrainLoadingTask {
                task,
                lat,
                lon,
            });
        }
    }
}

fn handle_terrain_loading(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut TerrainLoadingTask)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let scale = 111111.0;
    let target_step = 16; // Consistent with loader

    for (entity, mut task) in &mut tasks {
        if let Some(result) = future::block_on(future::poll_once(&mut task.task)) {
            // Task finished
            if let Some((lat, lon, mesh)) = result {
                let mesh_handle = meshes.add(mesh);
                // Use default material with vertex colors enabled
                let material_handle = materials.add(StandardMaterial {
                    base_color: Color::WHITE, // Tint
                    perceptual_roughness: 1.0, 
                    // Vertex colors are used if attribute is present and no texture?
                    // Bevy StandardMaterial supports vertex colors.
                    ..default()
                });
    
                let pos_x = (lon as f32) * scale;
                // N45 means 45..46. HGT row 0 is North (46). 
                // We want Mesh (0,0) [North-West] to be at Lat 46 (Z = -46).
                // Mesh Z goes 0..1 (scaled to 1 degree).
                // So if we spawn at Z = -46 * scale, then v=0 is at -46. v=1 is at -45.
                // lat is 45. -(lat+1) = -46.
                let pos_z = -((lat + 1) as f32) * scale;
    
                commands.spawn((
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(material_handle),
                    Transform::from_xyz(
                        pos_x, 
                        0.0, 
                        pos_z 
                    ).with_scale(Vec3::new(scale, 1.0, scale)), // Scale Y was 2.0? Reset to 1.0 for true height.
                    radar_coverage::terrain::TerrainChunk { lat_idx: lat, lon_idx: lon, lod_step: target_step }, 
                ));
            }
            
            // Remove task
            commands.entity(entity).despawn();
        }
    }
}
fn draw_radar_gizmos(
    mut gizmos: Gizmos,
    radars: Query<&Radar>,
    controller: Res<MapController>,
) {
    if !controller.show_coverage {
        return;
    }

    let scale = 111111.0;
    
    for radar in radars.iter() {
        let x = radar.location.longitude as f32 * scale;
        let z = -(radar.location.latitude as f32 * scale);
        let y = radar.location.altitude as f32; // AMSL

        // Draw a red sphere at radar location
        gizmos.sphere(
            Vec3::new(x, y, z),
            2000.0, // 2km radius
            Color::srgb(1.0, 0.0, 0.0),
        ).resolution(32);
        
        // Also draw a line to the ground?
        gizmos.line(
            Vec3::new(x, y, z),
            Vec3::new(x, 0.0, z),
            Color::srgb(1.0, 0.0, 0.0),
        );
    }
}

fn update_radar_viewshed(
    mut commands: Commands,
    terrain_res: Res<TerrainResource>,
    radars: Query<(Entity, &Radar, Option<&RadarViewshed>, Option<&ComputingViewshedTask>)>,
    refraction: Res<RefractionParams>,
    mut last_k: Local<f32>,
) {
    let task_pool = AsyncComputeTaskPool::get();
    
    // We update last_k just to track it, but we don't trigger re-computation
    if (*last_k - refraction.k_factor as f32).abs() > 0.1 {
        *last_k = refraction.k_factor as f32;
    }

    for (entity, radar, viewshed_opt, computing_task) in radars.iter() {
        // Only compute if viewshed is missing (startup) AND not already computing
        if viewshed_opt.is_none() && computing_task.is_none() {
             println!("Computing Viewshed for Radar: {:?} (K={:.2})", radar.name, refraction.k_factor);
            let terrain_manager = terrain_res.0.clone();
            let radar_clone = radar.clone();
            
            // Calculate approx total rays for progress
            // width = ceil(range*2 / cell)
            // perimeter = width*4
            let range: f64 = 470000.0;
            let cell: f64 = 100.0;
            let width = (range * 2.0 / cell).ceil() as u32;
            let total_rays = width * 4; 
            
            let progress = Arc::new(AtomicU32::new(0));
            let progress_clone = progress.clone();
            // Use current K factor for initial computation
            let k = refraction.k_factor as f32;

            // Spawn async task for viewshed computation
            let task = task_pool.spawn(async move {
                let start = Instant::now();
                let viewshed = compute_viewshed(&radar_clone, &terrain_manager, range, k, Some(progress_clone));
                println!("Viewshed computed in {:.2?}", start.elapsed());
                viewshed
            });
            
            commands.entity(entity)
                .insert(ComputingViewshedTask(task))
                .insert(ViewshedProgress { current: progress, total: total_rays });
        }
    }
}

#[derive(Component)]
struct ComputingViewshedTask(bevy::tasks::Task<radar_coverage::physics::viewshed::Viewshed>);

fn handle_viewshed_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut ComputingViewshedTask)>,
) {
     for (entity, mut task) in &mut tasks {
        if let Some(viewshed) = future::block_on(future::poll_once(&mut task.0)) {
            commands.entity(entity)
                .insert(RadarViewshed(Arc::new(viewshed)))
                .remove::<ComputingViewshedTask>()
                .remove::<ViewshedProgress>();
            println!("Viewshed applied to entity {:?}", entity);
        }
    }
}

fn schedule_coverage_tasks(
    mut commands: Commands,
    controller: Res<MapController>,
    terrain_res: Res<TerrainResource>,
    cache: Res<CoverageCache>,
    radars: Query<(&Radar, Option<&RadarViewshed>)>,
    mut metrics: ResMut<CoverageMetrics>,
    // Queries to check if task already exists or chunk already loaded
    computing: Query<&ComputingCoverage>, 
    existing_chunks: Query<&CoverageChunk>,
    // Existing coverage chunks to check for stale AGL/RCS
    coverage_chunks: Query<(Entity, &CoverageChunk)>,
) {
    if !controller.show_coverage {
        return;
    }

    let center_lat = 45;
    let center_lon = 5;
    let radius = 5; 

    // For each radar
    for (radar, viewshed_opt) in radars.iter() {
        // If no viewshed yet, skip coverage computation for this radar
        let viewshed = match viewshed_opt {
            Some(v) => v.0.clone(),
            None => continue, 
        };

        // Compute hash for this radar conf (including AGL and RCS)
        let target_agl = controller.target_agl;
        let target_rcs = controller.rcs_profile.value();
        
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        radar.name.hash(&mut hasher);
        radar.location.latitude.to_bits().hash(&mut hasher);
        radar.location.longitude.to_bits().hash(&mut hasher);
        radar.location.altitude.to_bits().hash(&mut hasher);
        target_agl.to_bits().hash(&mut hasher);
        target_rcs.to_bits().hash(&mut hasher);
        // Add other params that affect coverage
        radar.frequency_mhz.to_bits().hash(&mut hasher);
        radar.tx_power_w.to_bits().hash(&mut hasher);
        radar.gain_dbi.to_bits().hash(&mut hasher);
        
        let radar_hash = hasher.finish();

        // Compute Stable ID (Name only)
        let mut stable_hasher = std::collections::hash_map::DefaultHasher::new();
        radar.name.hash(&mut stable_hasher);
        let radar_unique_id = stable_hasher.finish();

        // 1. Check existing chunks for stale data
        for (entity, chunk) in coverage_chunks.iter() {
            // Check if this chunk belongs to THIS radar
            if chunk.radar_unique_id == radar_unique_id {
                // If hash mismatch, it's stale (old parameters) -> Despawn
                if chunk.radar_hash != radar_hash {
                    commands.entity(entity).despawn_recursive();
                }
            }
        }

        for dlat in -radius..=radius {
            for dlon in -radius..=radius {
                let lat = center_lat + dlat;
                let lon = center_lon + dlon;

                // Check overlap with ANY existing chunk for THIS radar hash
                if existing_chunks.iter().any(|c| c.lat_idx == lat && c.lon_idx == lon && c.radar_hash == radar_hash) {
                    continue;
                }
                
                // Check if computing
                if computing.iter().any(|c| c.lat == lat && c.lon == lon && c.radar_hash == radar_hash) {
                    continue;
                }

                // Check Cache
                let key = CoverageKey { 
                    lat, 
                    lon, 
                    target_agl_m: target_agl as i16,
                    radar_hash 
                };
                
                if let Some(cached_tile) = cache.get(&key) {
                    // Spawn from Cache
                    metrics.cache_hits += 1;
                    
                    let task_pool = AsyncComputeTaskPool::get();
                    let cached_tile_clone = cached_tile.clone();
                    let task = task_pool.spawn(async move {
                        (*cached_tile_clone).clone() 
                    });
                        
                    commands.spawn((
                        CoverageTask(task),
                        ComputingCoverage { lat, lon, target_agl, radar_hash, radar_unique_id }
                    ));
                } else {
                    // Trigger Computation with Viewshed
                    let task_pool = AsyncComputeTaskPool::get();
                    let terrain_manager = terrain_res.0.clone();
                    let radar_clone = radar.clone();
                    let viewshed_clone = viewshed.clone();
                    
                    let step_size = 2; // Higher resolution.
                    
                    let task = task_pool.spawn(async move {
                        let result = compute_coverage_tile(
                            radar_clone,
                            terrain_manager, 
                            viewshed_clone,
                            lat, 
                            lon, 
                            target_rcs, 
                            target_agl as f64, 
                            step_size
                        );
                        result
                    });
                    
                    commands.spawn((
                        CoverageTask(task),
                        ComputingCoverage { lat, lon, target_agl, radar_hash, radar_unique_id }
                    ));
                }
            }
        }
    }
}
fn handle_coverage_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut CoverageTask, &ComputingCoverage)>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    cache: Res<CoverageCache>,
    mut metrics: ResMut<CoverageMetrics>,
    coverage_chunks: Query<(Entity, &CoverageChunk)>,
    controller: Res<MapController>,
) {
    if !controller.show_coverage {
        // Despawn all coverage if toggled off
        for (e, _) in coverage_chunks.iter() {
           commands.entity(e).despawn_recursive();
        }
        // Cancel tasks? 
        for (e, _, _) in tasks.iter() {
            commands.entity(e).despawn_recursive();
        }
        return;
    }

    for (entity, mut task, comp) in &mut tasks {
        if let Some(coverage_tile) = future::block_on(future::poll_once(&mut task.0)) {
            // Task finished
            metrics.tiles_computed += 1;
            
            // Insert into Cache using the radar_hash from the computing component
            // Insert into Cache using the radar_hash and AGL from the computing component
            let key = CoverageKey {
                lat: coverage_tile.lat_idx,
                lon: coverage_tile.lon_idx,
                target_agl_m: comp.target_agl as i16,
                radar_hash: comp.radar_hash,
            };
            
            cache.insert(key, Arc::new(coverage_tile.clone()));

            let image = create_coverage_texture(&coverage_tile);
            let texture_handle = images.add(image);
            
            let scale = 111111.0; 
            
            // Coverage Mesh:
            // Center X = lon*scale + scale/2.
            // Center Z = -((lat+1)*scale) + scale/2.
            
            let center_x = (coverage_tile.lon_idx as f32) * scale + scale/2.0;
            let center_z = -((coverage_tile.lat_idx + 1) as f32) * scale + scale/2.0;

            // Prevent Z-fighting by adding a small offset based on radar hash
            let hash_offset = (comp.radar_hash % 100) as f32 * 5.0; 
            let altitude = 5000.0 + hash_offset;

            let plane = Mesh::from(Rectangle::new(scale, scale));
            let mesh_handle = meshes.add(plane);
            let mat_handle = materials.add(StandardMaterial {
                base_color_texture: Some(texture_handle),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                cull_mode: None, // Double sided
                depth_bias: 1.0, // Help with z-fighting
                ..default()
            });

            commands.spawn((
                Mesh3d(mesh_handle),
                MeshMaterial3d(mat_handle),
                Transform::from_xyz(center_x, altitude, center_z) 
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                CoverageChunk { 
                    lat_idx: coverage_tile.lat_idx, 
                    lon_idx: coverage_tile.lon_idx,
                    target_agl: comp.target_agl,
                    radar_hash: comp.radar_hash,
                    radar_unique_id: comp.radar_unique_id,
                }
            ));
            
            // Remove task
            commands.entity(entity).despawn();
        }
    }
}
