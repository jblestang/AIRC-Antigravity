use std::f64::consts::PI;
use crate::io::Radar;

const C_LIGHT: f64 = 299_792_458.0;
const BOLTZMANN: f64 = 1.380649e-23;
const REF_TEMP: f64 = 290.0;

// Default noise parameters if not specified (Rec. ITU-R P.372)
// Assuming modest bandwidth and noise figure for generic radar
const DEFAULT_BANDWIDTH_HZ: f64 = 1_000_000.0; // 1 MHz
const DEFAULT_NOISE_FIGURE_DB: f64 = 3.0;

pub fn calculate_wavelength(freq_mhz: f64) -> f64 {
    C_LIGHT / (freq_mhz * 1e6)
}

pub fn calculate_noise_power_w(bandwidth_hz: Option<f64>, noise_figure_db: Option<f64>) -> f64 {
    let b = bandwidth_hz.unwrap_or(DEFAULT_BANDWIDTH_HZ);
    let nf = noise_figure_db.unwrap_or(DEFAULT_NOISE_FIGURE_DB);
    let nf_lin = 10.0f64.powf(nf / 10.0);
    BOLTZMANN * REF_TEMP * b * nf_lin
}

pub fn calculate_received_power(radar: &Radar, dist_m: f64, rcs_sqm: f64) -> f64 {
    if dist_m <= 0.0 { return f64::INFINITY; }
    
    let wavelength = calculate_wavelength(radar.frequency_mhz);
    let g_lin = 10.0f64.powf(radar.gain_dbi / 10.0);
    let l_sys_lin = 10.0f64.powf(radar.system_loss_db / 10.0);
    
    // Monostatic radar equation: Pr = (Pt * G^2 * lambda^2 * sigma) / ((4pi)^3 * R^4 * L)
    let numerator = radar.tx_power_w * g_lin.powi(2) * wavelength.powi(2) * rcs_sqm;
    let denominator = (4.0 * PI).powi(3) * dist_m.powi(4) * l_sys_lin;
    
    numerator / denominator
}

pub fn calculate_snr_db(radar: &Radar, dist_m: f64, rcs_sqm: f64) -> f64 {
    let pr = calculate_received_power(radar, dist_m, rcs_sqm);
    let noise = calculate_noise_power_w(None, None); // Using defaults for MVP
    
    10.0 * (pr / noise).log10()
}

pub fn max_detection_range(radar: &Radar, rcs_sqm: f64) -> f64 {
    let wavelength = calculate_wavelength(radar.frequency_mhz);
    let g_lin = 10.0f64.powf(radar.gain_dbi / 10.0);
    let l_sys_lin = 10.0f64.powf(radar.system_loss_db / 10.0);
    let snr_min_lin = 10.0f64.powf(radar.snr_threshold_db / 10.0);
    let noise = calculate_noise_power_w(None, None);
    
    // R = [ (Pt * G^2 * lambda^2 * sigma) / ((4pi)^3 * L * N * SNR_min) ] ^ (1/4)
    let numerator = radar.tx_power_w * g_lin.powi(2) * wavelength.powi(2) * rcs_sqm;
    let denominator = (4.0 * PI).powi(3) * l_sys_lin * noise * snr_min_lin;
    
    (numerator / denominator).powf(0.25)
}
