use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub api_key: Option<String>,
    pub secret_key: Option<String>,
    pub base_asset: String,
    pub quote_asset: String,
    pub order_qty: f64,
    pub tick_size: f64,
    pub n_spreads: usize,
    pub estimate_window: u64,
    pub period: u64,
    pub sigma_tick_period: usize,
    pub gamma: f64,
    pub sigma_multiplier: f64,
    pub stoploss: f64,
    pub stoploss_sleep: u64,
    pub stopprofit: f64,
    pub trailing_stop: f64,
    pub q_max: f64,
}
