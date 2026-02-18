use radar_coverage::terrain::TerrainLoader;
use std::path::PathBuf;

fn main() {
    let path = PathBuf::from("/Users/jean-baptiste/AIRC-Antigravity/radar_coverage/assets/");
    let loader = TerrainLoader::new(path);
    
    println!("Loading tile N45E005...");
    match loader.load_tile(45, 5) {
        Ok(tile) => {
            println!("Tile loaded! Size: {}", tile.size);
            
            // Sample radar location
            let lat = 45.1;
            let lon = 5.2;
            
             // Manual sample logic copy from TerrainTile::sample (simplified)
            let lat_deg = 45;
            let lon_deg = 5;
            let u = lon - lon_deg as f64;
            let v = (lat_deg as f64 + 1.0) - lat;
            
            println!("Sampling at Lat: {}, Lon: {} (u: {:.4}, v: {:.4})", lat, lon, u, v);
            let alt = tile.sample(u, v);
            println!("Altitude at Radar: {:.2} m", alt);
            
            // Check for non-zero data
            let nonzero_count = tile.data.iter().filter(|&&h| h != 0).count();
            println!("Non-zero samples: {} / {}", nonzero_count, tile.data.len());
            
            if nonzero_count == 0 {
                println!("WARNING: Tile contains only zeros!");
            }
        },
        Err(e) => {
            println!("Error loading tile: {:?}", e);
        }
    }
}
