use std::env;
use std::fs;
use chrono::NaiveDateTime;

use exrs::okex_v5::api::*;
use exrs::okex_v5::config::*;
use exrs::okex_v5::market::*;

#[actix_rt::main]
async fn main() {
    // Intend for downloading historical data for backtesting strategy
    let args: Vec<String> = env::args().collect();
    let file = fs::File::open(&args[1]).expect("Error in opening config file");
    let config: rainmaker::config::OkexConfig =
        serde_json::from_reader(file).expect("Expect a proper json file");
    let api_config = Config::new(config.is_testnet);
    let market_client: Market = Okex::new(
        config.api_key.clone(),
        config.secret_key.clone(),
        config.passphrase.clone(),
        &api_config
    );
    let symbol = String::from("BTC-USDT");
    let start_time = NaiveDateTime::parse_from_str("2023-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    // let end_time = NaiveDateTime::parse_from_str("2023-01-01 01:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    println!("{:?}", start_time.timestamp_millis());
    let response 
        = market_client.get_candles(
            symbol, 
            None, 
            Some(start_time.timestamp_millis()), 
            None,
            // Some(end_time.timestamp_millis()), 
            None
    ).await;
    println!("{:?}", response);
}

// 1672534800000
// 1676680920000
// 1676674980000
// 1676674920000 Friday, February 17, 2023 11:02:00 PM
// 1676669040000 Friday, February 17, 2023 9:24:00 PM
// 1676668980000 Friday, February 17, 2023 9:23:00 PM
