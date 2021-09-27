extern crate rainmaker;
use env_logger::Builder;
use exrs::binance_f::api::*;
use exrs::binance_f::userstream::*;
use exrs::binance_f::websockets::*;
use exrs::binance_f::ws_model::FuturesWebsocketEvent;
use log::{debug, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{env, fs};
use tokio::sync::mpsc;

use rainmaker::strategies::cross_exchange_arbitrage;

#[actix_rt::main]
async fn main() {
    
}
