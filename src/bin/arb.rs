extern crate rainmaker;
use env_logger::Builder;
use exrs::binance_f::ws_model::FuturesWebsocketEvent as BinanceWSEvent;
use exrs::huobi::ws_model::WebsocketEvent as HuobiWSEvent;
use exrs::binance_f::websockets::FuturesWebSockets;
use exrs::huobi::websockets::WebSockets;

use log::{debug, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{env, fs};
use tokio::sync::mpsc;
use serde::Deserialize;
use rainmaker::strategies::cross_exchange_arbitrage;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Event {
    HuobiWS(HuobiWSEvent),
    BinanceWSt(BinanceWSEvent),
}

#[actix_rt::main]
async fn main() {
    Builder::new().parse_default_env().init();
    let (tx, mut rx): (
        mpsc::Sender<Event>,
        mpsc::Receiver<Event>,
    ) = mpsc::channel(1024);

    let hb_tx = tx.clone();
    let keep_running = AtomicBool::new(true);
    let bbo_req = r#"{"sub": "market.btcusdt.bbo","id": "id1"}"#;
    let mut hb_web_socket: WebSockets<Event> = WebSockets::new(hb_tx);
    actix_rt::spawn(async move {
        hb_web_socket.connect("ws").await.unwrap();
        hb_web_socket.subscribe_request(bbo_req).await.unwrap();
        if let Err(e) = hb_web_socket.event_loop(&keep_running).await {
            println!("Error: {}", e);
        }
    });

    let ba_tx = tx.clone();
    let keep_running = AtomicBool::new(true);
    let book_ticker: String = "btcusdt@bookTicker".to_string();
    let mut ba_web_socket: FuturesWebSockets<Event> = FuturesWebSockets::new(ba_tx);
    actix_rt::spawn(async move {
        ba_web_socket.connect(&book_ticker).await.unwrap(); // check error
        if let Err(e) = ba_web_socket.event_loop(&keep_running).await {
            println!("Error: {}", e);
        }
    });

    loop {
        let event = rx.recv().await.unwrap();
        println!("event: {:?}", event);
        actix_rt::task::yield_now().await;
    }

}   
