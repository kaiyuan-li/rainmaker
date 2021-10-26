extern crate rainmaker;
use env_logger::Builder;
use exrs::okex_v5::websockets::*;
use exrs::okex_v5::ws_model::WebsocketEvent;
use log::{debug, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{env, fs};
use tokio::sync::mpsc;

use rainmaker::strategies::avellaneda_stoikov_okex::AvellanedaStoikov;

#[actix_rt::main]
async fn main() {
    println!("main started: {:?}", chrono::prelude::Local::now());
    Builder::new().parse_default_env().init();
    let args: Vec<String> = env::args().collect();
    let file = fs::File::open(&args[1]).expect("file should open read only");
    let config: rainmaker::config::OkexConfig =
        serde_json::from_reader(file).expect("file shoud be proper json");
    let sub = r#"{"op": "subscribe","args": [{"channel": "tickers","instId": "SHIB-USDT-SWAP"}]}"#;
    println!("trading to: {:?}", sub);

    let (tx, rx): (mpsc::Sender<WebsocketEvent>, mpsc::Receiver<WebsocketEvent>) =
        mpsc::channel(1024);

    let private_keep_running = AtomicBool::new(true);
    let private_tx = tx.clone();
    let c = config.clone();
    actix_rt::spawn(async move {
        let mut private_ws: WebSockets<WebsocketEvent> = WebSockets::new(private_tx);
        private_ws.connect("private").await.unwrap();
        private_ws
            .login(
                c.api_key.unwrap(),
                c.secret_key.unwrap(),
                c.passphrase.unwrap(),
            )
            .await
            .unwrap();

        while let Err(e) = private_ws.event_loop(&private_keep_running).await {
            warn!("private_ws event_loop Error: {}, starting reconnect...", e);

            while let Err(e) = private_ws.connect("private").await {
                warn!("private_ws connect Error: {}, try again...", e);
            }
        }
    });

    let public_keep_running = AtomicBool::new(true);
    actix_rt::spawn(async move {
        let public_tx = tx.clone();
        let mut public_ws: WebSockets<WebsocketEvent> = WebSockets::new(public_tx);

        public_ws.connect("public").await.unwrap();
        public_ws.subscribe_request(&sub).await.unwrap();

        while let Err(e) = public_ws.event_loop(&public_keep_running).await {
            warn!("public_ws event_loop Error: {}, starting reconnect...", e);

            while let Err(e) = public_ws.connect("public").await {
                public_ws.subscribe_request(&sub).await.unwrap();
                warn!("public_ws connect Error: {}, try again...", e);
            }
        }
    });

    let mut strategy = AvellanedaStoikov::new(config);
    strategy.run_forever(rx).await;
}
