use env_logger::Builder;
use exrs::binance_f::api::*;
use exrs::binance_f::userstream::*;
use exrs::binance_f::websockets::*;
use exrs::binance_f::ws_model::FuturesWebsocketEvent;
use log::{debug, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{env, fs};
use tokio::sync::mpsc;

pub mod config;
pub mod strategies;
pub mod util;
use strategies::avellaneda_stoikov::AvellanedaStoikov;

#[actix_rt::main]
async fn main() {
    println!("main started: {:?}", chrono::prelude::Local::now());
    Builder::new().parse_default_env().init();
    let args: Vec<String> = env::args().collect();
    let file = fs::File::open(&args[1]).expect("file should open read only");
    let config: config::Config = serde_json::from_reader(file).expect("file shoud be proper json");
    let sub = String::from(format!(
        "{}{}{}",
        config.base_asset.clone().to_lowercase(),
        config.quote_asset.clone().to_lowercase(),
        "@bookTicker"
    ));
    println!("trading to: {:?}", sub);

    let (tx, rx): (
        mpsc::Sender<FuturesWebsocketEvent>,
        mpsc::Receiver<FuturesWebsocketEvent>,
    ) = mpsc::channel(1024);

    let account_keep_running = AtomicBool::new(true);
    let account_tx = tx.clone();
    let c = config.clone();
    actix_rt::spawn(async move {
        let userstream: FuturesUserStream = BinanceF::new(c.api_key, None);
        match userstream.start().await {
            Ok(answer) => {
                debug!("listen_key: {}", &answer.listen_key);
                let mut account_ws: FuturesWebSockets<FuturesWebsocketEvent> =
                    FuturesWebSockets::new(account_tx);

                let listen_key = answer.listen_key.clone();

                actix_rt::spawn(async move {
                    loop {
                        let res = userstream.keep_alive(&listen_key).await;
                        println!("Send keep_alive: {:?}", res);
                        tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
                    }
                });

                account_ws.connect(&answer.listen_key).await.unwrap();
                while let Err(e) = account_ws.event_loop(&account_keep_running).await {
                    warn!("account_ws Error: {}, starting reconnect...", e);

                    account_ws.connect(&answer.listen_key).await.unwrap();
                }
            }
            Err(_e) => panic!("Not able to start an User Stream (Check your API_KEY"),
        }
    });

    let book_keep_running = AtomicBool::new(true);
    actix_rt::spawn(async move {
        let book_tx = tx.clone();
        let mut book_ws: FuturesWebSockets<FuturesWebsocketEvent> = FuturesWebSockets::new(book_tx);
        book_ws.connect(&sub).await.unwrap();

        while let Err(e) = book_ws.event_loop(&book_keep_running).await {
            warn!("book_ws Error: {}, starting reconnect...", e);

            book_ws.connect(&sub).await.unwrap();
        }
    });

    let mut strategy = AvellanedaStoikov::new(config);
    strategy.run_forever(rx).await;
}
