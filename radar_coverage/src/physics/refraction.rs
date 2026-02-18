use bevy::prelude::*;
use crate::geo::EARTH_RADIUS;

#[derive(Clone, Copy, Debug, Resource)]
pub struct RefractionParams {
    pub k_factor: f64,
}

impl Default for RefractionParams {
    fn default() -> Self {
        Self { k_factor: 4.0 / 3.0 }
    }
}

pub fn effective_earth_radius(params: RefractionParams) -> f64 {
    EARTH_RADIUS * params.k_factor
}
