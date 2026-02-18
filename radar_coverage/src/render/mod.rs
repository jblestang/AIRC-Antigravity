use bevy::prelude::*;

use bevy::render::render_resource::PrimitiveTopology;
use bevy::render::render_asset::RenderAssetUsages;
use crate::terrain::TerrainTile;

pub fn create_terrain_mesh(tile: &TerrainTile, step: usize) -> Mesh {
    let size = tile.size;
    
    // Ensure step is at least 1
    let step = if step == 0 { 1 } else { step };
    
    // Grid size depends on step
    // Original: (size-1) cells wide
    // New: (size-1)/step cells wide?
    // We iterate 0..size-1 by step.
    // e.g. size=1201, step=10. 0, 10, 20 .. 1200.
    
    let mut positions = Vec::new(); // Use default capacity to avoid over-allocation guess
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new(); // Vertex Colors

    // Min/Max height for simple gradient normalization (approximate for Alps)
    let min_h = 0.0;
    let max_h = 4800.0; // Mont Blanc approx

    for y in (0..(size - 1)).step_by(step) {
        for x in (0..(size - 1)).step_by(step) {
            // Get heights for the 4 corners of the quad (sampled at step intervals)
            // Use get_max_height to preserve peaks when downsampling
            let x0 = x;
            let y0 = y;
            let x1 = (x + step).min(size - 1);
            let y1 = (y + step).min(size - 1);
            
            let h00 = tile.get_max_height(x0, y0, step) as f32;
            let h10 = tile.get_max_height(x1, y0, step) as f32; // Note: this might overlap? 
            // Ideally max pooling should be centered or block based. 
            // Current Approach: 
            // Vertex 00 covers area [x0, x0+step] x [y0, y0+step]
            // Vertex 10 covers area [x1, x1+step] ...
            
            // Actually, for a continuous mesh, we want vertices to align.
            // If we use max of block, adjacent quads might differ? 
            // shared vertices must have same height.
            // Vertices are at x0, y0. 
            // The height at vertex (x0, y0) should be representative.
            // If we use max of surrounding, we need a consistent definition.
            
            // Let's define height at (x,y) as max of block centered at x,y? 
            // Or max of block [x, x+step]?
            
            // User said "take the max of the surrounding cells".
            // Let's interpret: Height at vertex (x, y) = Max of tile.data covering roughly (x-step/2, y-step/2) to (x+step/2, y+step/2).
            // But strict grid is easier:
            // Let's just use max of the block [x, y] to [x+step, y+step] as the height for the "cell" and use flat shading?            
            // Wait, we are generating 4 vertices per quad for flat shading.
            // p00, p10, p01, p11.
            // p00 corresponding to u0, v0. 
            
            // If we want the *entire quad* to represent the peak, maybe we should sample max of the *whole quad area* and set all 4 vertices to that?
            // "re-scaled ... take the max of the surrounding cells".
            
            // If I set h00 = max(block 00), h10 = max(block 10)... 
            // Block 10 is the neighbor.
            // This preserves peaks at vertices.
            
            let h00 = tile.get_max_height(x0, y0, step) as f32;
            let h10 = tile.get_max_height(x1, y0, step) as f32;
            let h01 = tile.get_max_height(x0, y1, step) as f32;
            let h11 = tile.get_max_height(x1, y1, step) as f32;

            // Normalize heights for color
            let c00 = get_color(h00, min_h, max_h);
            let c10 = get_color(h10, min_h, max_h);
            let c01 = get_color(h01, min_h, max_h);
            let c11 = get_color(h11, min_h, max_h);
            
            // Coordinates
            let u0 = x0 as f32 / (size - 1) as f32;
            let v0 = y0 as f32 / (size - 1) as f32;
            let u1 = x1 as f32 / (size - 1) as f32;
            let v1 = y1 as f32 / (size - 1) as f32;

            // Scale height for visualization. 
            // In a real app we'd use a vertex shader or proper transform.
            let h_scale = 1.0; // Use 1.0 here, scale via Transform
            
            let p00 = [u0, h00 * h_scale, v0];
            let p10 = [u1, h10 * h_scale, v0];
            let p01 = [u0, h01 * h_scale, v1];
            let p11 = [u1, h11 * h_scale, v1];

            // Triangle 1: (0,0) -> (1,1) -> (1,0) (Top-Left, Bottom-Right, Top-Right) - CCW?
            // Bevy uses CCW winding for front face.
            // Let's do: 00 -> 01 -> 11 And 00 -> 11 -> 10
            
            // Tri 1: 00, 01, 11
            positions.push(p00); uvs.push([u0, v0]); normals.push([0.0, 1.0, 0.0]); colors.push(c00);
            positions.push(p01); uvs.push([u0, v1]); normals.push([0.0, 1.0, 0.0]); colors.push(c01);
            positions.push(p11); uvs.push([u1, v1]); normals.push([0.0, 1.0, 0.0]); colors.push(c11);
            
            // Tri 2: 00, 11, 10
            positions.push(p00); uvs.push([u0, v0]); normals.push([0.0, 1.0, 0.0]); colors.push(c00);
            positions.push(p11); uvs.push([u1, v1]); normals.push([0.0, 1.0, 0.0]); colors.push(c11);
            positions.push(p10); uvs.push([u1, v0]); normals.push([0.0, 1.0, 0.0]); colors.push(c10);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals); // Will be overwritten by compute_flat_normals
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    // No indices needed for flat shaded list
    
    mesh.compute_flat_normals();
    
    mesh
}

fn get_color(h: f32, min: f32, max: f32) -> [f32; 4] {
    let t = ((h - min) / (max - min)).clamp(0.0, 1.0);
    // Simple gradient: Blue(Low) -> Green -> Brown -> White(High)
    if t < 0.2 {
        // Water/Low -> Green
        // Lerp Blue (0,0,1) to Green (0,1,0)
        let local_t = t / 0.2;
        [0.0, local_t, 1.0 - local_t, 1.0]
    } else if t < 0.5 {
        // Green -> Brown
        let local_t = (t - 0.2) / 0.3;
        [local_t * 0.6, 1.0 - local_t * 0.4, 0.0, 1.0] // Approx
    } else {
        // Brown -> White
        let local_t = (t - 0.5) / 0.5;
        [0.6 + local_t * 0.4, 0.6 + local_t * 0.4, local_t, 1.0]
    }
}

use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use crate::coverage::CoverageTile;

pub fn create_coverage_texture(tile: &CoverageTile) -> Image {
    let size = tile.size;
    let mut pixels = Vec::with_capacity(size * size * 4);
    
    // Create RGBA texture
    // Green = Visible, Red = Invisible (or Transparent)
    // For overlay, we want visible to be Green transparent, Invisible to be Red transparent or hidden.
    
    for y in 0..size {
        for x in 0..size {
            let idx = y * size + x;
            if tile.data[idx] == 1 {
                // Visible - Green
                pixels.push(0);   // R
                pixels.push(255); // G
                pixels.push(0);   // B
                pixels.push(100); // A (Semi-transparent)
            } else if tile.data[idx] == 2 {
                // Shadowed - Dark Red
                pixels.push(128);
                pixels.push(0);
                pixels.push(0);
                pixels.push(120); // A (Slightly more opaque)
            } else {
                // Out of range (0) - Transparent
                pixels.push(0);
                pixels.push(0);
                pixels.push(0);
                pixels.push(0); // Invisible
            }
        }
    }

    Image::new(
        Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        bevy::render::render_asset::RenderAssetUsages::RENDER_WORLD, 
    )
}
