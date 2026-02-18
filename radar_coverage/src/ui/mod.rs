use bevy::prelude::*;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy_egui::{egui, EguiContexts};
use crate::geo::LatLon;
use crate::physics::refraction::RefractionParams;
use std::sync::atomic::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RCSProfile {
    StealthFighter, // 5G (0.1)
    Fighter,        // 4G (5.0)
    SmallAircraft,  // 2.0
    LargeAircraft,  // 50.0
    Ship,           // 5000.0
}

impl RCSProfile {
    pub fn value(&self) -> f64 {
        match self {
            RCSProfile::StealthFighter => 0.1,
            RCSProfile::Fighter => 5.0,
            RCSProfile::SmallAircraft => 2.0,
            RCSProfile::LargeAircraft => 50.0,
            RCSProfile::Ship => 5000.0,
        }
    }
    
    pub fn label(&self) -> &'static str {
        match self {
            RCSProfile::StealthFighter => "5G Stealth Fighter (0.1 m²)",
            RCSProfile::Fighter => "4G Fighter (5.0 m²)",
            RCSProfile::SmallAircraft => "Small Aircraft (2.0 m²)",
            RCSProfile::LargeAircraft => "Large Aircraft (50.0 m²)",
            RCSProfile::Ship => "Ship (5000.0 m²)",
        }
    }
}

#[derive(Resource)]
pub struct MapController {
    pub center: LatLon,
    pub zoom: f32, // Logarithmic zoom level? Or simple scale. Let's use scale (pixels per meter).
    pub move_speed: f32,
    pub show_coverage: bool,
    pub target_agl: f32,
    pub rcs_profile: RCSProfile,
}

impl Default for MapController {
    fn default() -> Self {
        Self {
            center: LatLon { latitude: 45.0, longitude: 5.0, altitude: 0.0 }, // France approx
            zoom: 100.0, // Matches 2000m altitude (200000 / 2000)
            move_speed: 1000.0,
            show_coverage: false,
            target_agl: 50.0,
            rcs_profile: RCSProfile::Fighter,
        }
    }
}

use bevy::window::PrimaryWindow;

pub fn map_control_system(
    mut controller: ResMut<MapController>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
    mut scroll_evr: EventReader<MouseWheel>,
    time: Res<Time>,
    mut query: Query<(&mut Transform, &GlobalTransform, &Camera)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut contexts: EguiContexts, 
) {
    // In Bevy 0.14, ctx_mut returns &Context directly (mostly)
    // In Bevy 0.14+, ctx_mut can panic if not ready. Use try_ctx_mut.
    let ctx = match contexts.try_ctx_mut() {
        Some(ctx) => ctx,
        None => return,
    };
    if ctx.wants_pointer_input() {
        return;
    }

    let (mut cam_transform, cam_global_transform, camera) = query.single_mut();
    
    // --- Zoom (Mouse Wheel) ---
    let mut scroll_line = 0.0;
    for ev in scroll_evr.read() {
        scroll_line += ev.y;
    }

    if scroll_line != 0.0 {
        // Zoom speed factor
        let zoom_sensitivity = 0.1;
        let scale = 1.0 - (scroll_line * zoom_sensitivity);
        
        // Clamp scale to prevent finding underground or too high
        // Calculate potential new Y
        let new_y = cam_transform.translation.y * scale;
        let clamped_y = new_y.clamp(100.0, 5_000_000.0);
        let actual_scale = clamped_y / cam_transform.translation.y;

        // Mouse centered zoom
        let mut zoom_center = Vec3::ZERO; // Default to map center if raycast fails
        let mut has_target = false;

        // Try to get window and cursor
        if let Some(window) = windows.get_single().ok() {
            if let Some(cursor_position) = window.cursor_position() {
               if let Ok(ray) = camera.viewport_to_world(cam_global_transform, cursor_position) {
                    // Intersect with Plane Y=0
                    // Ray: Origin + t * Dir
                    // O.y + t * D.y = 0 => t = -O.y / D.y
                    if ray.direction.y.abs() > 1e-6 {
                        let t = -ray.origin.y / ray.direction.y;
                        if t > 0.0 {
                            zoom_center = ray.origin + ray.direction * t;
                            has_target = true;
                        }
                    }
               }
            }
        }

        if has_target {
            // vector from target to camera
            let v = cam_transform.translation - zoom_center;
            // scale it
            let v_new = v * actual_scale;
            // new position
            cam_transform.translation = zoom_center + v_new;
        } else {
             // Fallback to center zoom (just Y scaling)
             cam_transform.translation.y = clamped_y;
        }
    }
    
    // Always update zoom based on current altitude
    controller.zoom = 200000.0 / cam_transform.translation.y;


    // --- Pan (Keyboard & Mouse Drag) ---
    let speed = controller.move_speed * time.delta_secs();
    // Scale pan speed with altitude (zoom out -> faster pan)
    let pan_speed = speed * (cam_transform.translation.y / 1000.0);
    
    let mut delta = Vec3::ZERO;
    
    // Keyboard
    if keyboard.pressed(KeyCode::ArrowUp) || keyboard.pressed(KeyCode::KeyW) {
        delta.z -= pan_speed;
    }
    if keyboard.pressed(KeyCode::ArrowDown) || keyboard.pressed(KeyCode::KeyS) {
        delta.z += pan_speed;
    }
    if keyboard.pressed(KeyCode::ArrowLeft) || keyboard.pressed(KeyCode::KeyA) {
        delta.x -= pan_speed;
    }
    if keyboard.pressed(KeyCode::ArrowRight) || keyboard.pressed(KeyCode::KeyD) {
        delta.x += pan_speed;
    }

    // Mouse Drag (Left or Right Button)
    if mouse_button.pressed(MouseButton::Left) || mouse_button.pressed(MouseButton::Right) {
        for ev in mouse_motion.read() {
            // Dragging mouse moves camera opposite to drag direction
            // X motion -> X pan
            // Y motion -> Z pan (since we are looking down-ish)
            
            // Adjust sensitivity based on altitude
            let drag_sensitivity = cam_transform.translation.y * 0.002;
            
            delta.x -= ev.delta.x * drag_sensitivity;
            delta.z -= ev.delta.y * drag_sensitivity;
        }
    } else {
         // Consume events even if not pressed to avoid accumulation? 
         // Actually better to consume them only if used? 
         // No, EventReader needs to be read.
         mouse_motion.clear();
    }

    // Update Camera Position
    cam_transform.translation += delta;
    
    // Update center in controller (purely informational/sync)
    // Inverse projection not strictly implemented, but we can update vaguely
    // controller.center = ... (requires keeping track of lat/lon ref)
}

pub fn ui_panel_system(
    mut contexts: EguiContexts,
    mut refraction: ResMut<RefractionParams>,
    radars: Query<&crate::io::Radar>,
    mut controller: ResMut<MapController>,
    metrics: Res<crate::cache::CoverageMetrics>,
    computing_radars: Query<(&crate::io::Radar, &crate::physics::viewshed::ViewshedProgress)>,
) {
    let ctx = match contexts.try_ctx_mut() {
        Some(ctx) => ctx,
        None => return,
    };
    egui::Window::new("Radar Coverage Params").show(ctx, |ui| {
        ui.heading("Simulation Parameters");
        
        // Explicit dereference for ResMut
        ui.add(egui::Slider::new(&mut refraction.k_factor, 1.0..=2.0).text("K-Factor"));
        ui.checkbox(&mut controller.show_coverage, "Show Coverage");
        if controller.show_coverage {
            ui.horizontal(|ui| {
                if ui.button("<").clicked() {
                    controller.target_agl = (controller.target_agl - 50.0).max(10.0);
                }
                ui.add(egui::Slider::new(&mut controller.target_agl, 10.0..=2000.0).text("Target AGL (m)"));
                if ui.button(">").clicked() {
                    controller.target_agl = (controller.target_agl + 50.0).min(2000.0);
                }
            });
            
            ui.add_space(5.0);
            
            egui::ComboBox::from_label("Target RCS Profile")
                .selected_text(controller.rcs_profile.label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut controller.rcs_profile, RCSProfile::StealthFighter, RCSProfile::StealthFighter.label());
                    ui.selectable_value(&mut controller.rcs_profile, RCSProfile::Fighter, RCSProfile::Fighter.label());
                    ui.selectable_value(&mut controller.rcs_profile, RCSProfile::SmallAircraft, RCSProfile::SmallAircraft.label());
                    ui.selectable_value(&mut controller.rcs_profile, RCSProfile::LargeAircraft, RCSProfile::LargeAircraft.label());
                    ui.selectable_value(&mut controller.rcs_profile, RCSProfile::Ship, RCSProfile::Ship.label());
                });
        }
        
        ui.separator();
        ui.heading("Metrics");

        ui.label(format!("Tiles Computed: {}", metrics.tiles_computed));
        ui.label(format!("Cache Hits: {}", metrics.cache_hits));
        
        ui.separator();
        ui.heading("Radars");
        ui.label(format!("Loaded: {}", radars.iter().count()));
        
            for radar in radars.iter() {
                ui.collapsing(&radar.name, |ui| {
                    ui.label(format!("Freq: {:.1} MHz", radar.frequency_mhz));
                    ui.label(format!("Power: {:.1} W", radar.tx_power_w));
                });
            }

            ui.separator();
            ui.heading("Processing");
            // Check for active viewshed tasks
             for (radar, progress) in computing_radars.iter() {
                let current = progress.current.load(Ordering::Relaxed) as f32;
                let total = progress.total as f32;
                let percent = (current / total).clamp(0.0, 1.0);
                ui.label(format!("Computing Viewshed: {}", radar.name));
                ui.add(egui::ProgressBar::new(percent).show_percentage());
            }

            ui.separator();
            ui.label(format!("Zoom: {:.5}", controller.zoom));
        });
}
