
use std::f64::consts::PI;

use serde::{Serialize, Deserialize};

pub const EARTH_RADIUS: f64 = 6378137.0;

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct LatLon {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64, // AMSL
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WebMercator {
    pub x: f64, 
    pub y: f64,
    pub altitude: f64, // AMSL
}

/// Convert Lat/Lon (WGS84) to Web Mercator (EPSG:3857)
/// Note: Web Mercator is not conformal at high latitudes and distorts scale. 
/// We use it for the grid/rendering projection.
pub fn latlon_to_webmercator(coord: LatLon) -> WebMercator {
    let x = coord.longitude * (PI / 180.0) * EARTH_RADIUS;
    let y = ((coord.latitude * PI / 360.0 + PI / 4.0).tan()).ln() * EARTH_RADIUS;
    WebMercator {
        x,
        y,
        altitude: coord.altitude,
    }
}

/// Convert Web Mercator (EPSG:3857) to Lat/Lon (WGS84)
pub fn webmercator_to_latlon(coord: WebMercator) -> LatLon {
    let longitude = (coord.x / EARTH_RADIUS) * (180.0 / PI);
    let latitude = (2.0 * (coord.y / EARTH_RADIUS).exp().atan() - PI / 2.0) * (180.0 / PI);
    LatLon {
        latitude,
        longitude,
        altitude: coord.altitude,
    }
}

pub fn get_scale_factor_at_lat(latitude: f64) -> f64 {
    1.0 / latitude.to_radians().cos()
}
