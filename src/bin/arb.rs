extern crate rainmaker;
use env_logger::Builder;

use exrs::binance::websockets::WebSockets as BinanceWebSockets;
use exrs::binance::ws_model::WebsocketEventUntag as BinanceWSEvent;
use exrs::huobi::websockets::WebSockets as HuobiWebSockets;
use exrs::huobi::ws_model::WebsocketEvent as HuobiWSEvent;

use log::{debug, info, warn};
use rainmaker::strategies::cross_exchange_arbitrage;
use serde::Deserialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{env, fs};
use tokio::sync::mpsc;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Event {
    HuobiWS(HuobiWSEvent),
    BinanceWS(BinanceWSEvent),
}

#[derive(Debug, Clone)]
struct StrategyData {
    capacity: usize,
    hb_ask_price: VecDeque<f64>,
    hb_bid_price: VecDeque<f64>,
    hb_mid_price: VecDeque<f64>,
    ba_ask_price: VecDeque<f64>,
    ba_bid_price: VecDeque<f64>,
    ba_mid_price: VecDeque<f64>,
    // demo版不考虑量
    hb_bid_div_ba_ask: VecDeque<f64>,
    hb_bid_div_ba_ask_ma: VecDeque<f64>,
    hb_ask_div_ba_bid: VecDeque<f64>,
    hb_ask_div_ba_bid_ma: VecDeque<f64>,
}

impl StrategyData {
    pub fn with_capacity(capacity: usize) -> Self {
        StrategyData {
            capacity: capacity,
            hb_ask_price: VecDeque::with_capacity(capacity),
            hb_bid_price: VecDeque::with_capacity(capacity),
            hb_mid_price: VecDeque::with_capacity(capacity),
            ba_ask_price: VecDeque::with_capacity(capacity),
            ba_bid_price: VecDeque::with_capacity(capacity),
            ba_mid_price: VecDeque::with_capacity(capacity),

            hb_bid_div_ba_ask: VecDeque::with_capacity(capacity),
            hb_bid_div_ba_ask_ma: VecDeque::with_capacity(capacity),
            hb_ask_div_ba_bid: VecDeque::with_capacity(capacity),
            hb_ask_div_ba_bid_ma: VecDeque::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, event: Event) {
        if self.hb_ask_price.len() > self.capacity - 1 {
            self.hb_ask_price.pop_front();
            self.hb_bid_price.pop_front();
            self.hb_mid_price.pop_front();
            self.ba_ask_price.pop_front();
            self.ba_bid_price.pop_front();
            self.ba_mid_price.pop_front();
            self.hb_bid_div_ba_ask.pop_front();
            self.hb_bid_div_ba_ask_ma.pop_front();
            self.hb_ask_div_ba_bid.pop_front();
            self.hb_ask_div_ba_bid_ma.pop_front();
        }

        match event {
            Event::HuobiWS(hb) => {
                match hb {
                    HuobiWSEvent::BBO(hb) => {
                        self.hb_ask_price.push_back(hb.tick.ask);
                        self.hb_bid_price.push_back(hb.tick.bid);
                        self.hb_mid_price
                            .push_back((hb.tick.ask - hb.tick.bid) / 2.);
                        // front fill
                        self.ba_ask_price
                            .push_back(*self.ba_ask_price.back().unwrap_or_else(|| &0.));
                        self.ba_bid_price
                            .push_back(*self.ba_bid_price.back().unwrap_or_else(|| &0.));
                        self.ba_mid_price
                            .push_back(*self.ba_mid_price.back().unwrap_or_else(|| &0.));

                        self.hb_bid_div_ba_ask
                            .push_back(hb.tick.bid / self.ba_ask_price.back().unwrap());
                        self.hb_bid_div_ba_ask_ma.push_back(
                            self.hb_bid_div_ba_ask.iter().sum::<f64>()
                                / self.hb_bid_div_ba_ask.len() as f64,
                        );

                        self.hb_ask_div_ba_bid
                            .push_back(hb.tick.ask / self.ba_bid_price.back().unwrap());
                        self.hb_ask_div_ba_bid_ma.push_back(
                            self.hb_ask_div_ba_bid.iter().sum::<f64>()
                                / self.hb_ask_div_ba_bid.len() as f64,
                        );
                    }
                    _ => warn!("huobi data error."),
                }
            }
            Event::BinanceWS(ba) => {
                match ba {
                    BinanceWSEvent::BookTicker(ba) => {
                        self.ba_ask_price.push_back(ba.best_ask);
                        self.ba_bid_price.push_back(ba.best_bid);
                        self.ba_mid_price
                            .push_back((ba.best_ask - ba.best_bid) / 2.);
                        // front fill
                        self.hb_ask_price
                            .push_back(*self.hb_ask_price.back().unwrap_or_else(|| &0.));
                        self.hb_bid_price
                            .push_back(*self.hb_bid_price.back().unwrap_or_else(|| &0.));
                        self.hb_mid_price
                            .push_back(*self.hb_mid_price.back().unwrap_or_else(|| &0.));

                        self.hb_bid_div_ba_ask
                            .push_back(self.hb_ask_price.back().unwrap() / ba.best_ask);
                        self.hb_bid_div_ba_ask_ma.push_back(
                            self.hb_bid_div_ba_ask.iter().sum::<f64>()
                                / self.hb_bid_div_ba_ask.len() as f64,
                        );

                        self.hb_ask_div_ba_bid
                            .push_back(self.hb_bid_price.back().unwrap() / ba.best_bid);
                        self.hb_ask_div_ba_bid_ma.push_back(
                            self.hb_ask_div_ba_bid.iter().sum::<f64>()
                                / self.hb_ask_div_ba_bid.len() as f64,
                        );
                    }
                    _ => warn!("binance data error."),
                }
            }
        }
    }
}

#[actix_rt::main]
async fn main() {
    println!("main started: {:?}", chrono::prelude::Local::now());
    Builder::new().parse_default_env().init();
    let (tx, mut rx): (mpsc::Sender<Event>, mpsc::Receiver<Event>) = mpsc::channel(1024);

    let hb_tx = tx.clone();
    let keep_running = AtomicBool::new(true);
    let bbo_req = r#"{"sub": "market.btcusdt.bbo","id": "id1"}"#;
    let mut hb_web_socket: HuobiWebSockets<Event> = HuobiWebSockets::new(hb_tx);
    actix_rt::spawn(async move {
        hb_web_socket.connect("ws").await.unwrap();
        hb_web_socket.subscribe_request(bbo_req).await.unwrap();
        if let Err(e) = hb_web_socket.event_loop(&keep_running).await {
            println!("hb Error: {}", e);
        }
    });

    let ba_tx = tx.clone();
    let keep_running = AtomicBool::new(true);
    let book_ticker: String = "btcusdt@bookTicker".to_string();
    let mut ba_web_socket: BinanceWebSockets<Event> = BinanceWebSockets::new(ba_tx);
    actix_rt::spawn(async move {
        ba_web_socket.connect(&book_ticker).await.unwrap(); // check error
        if let Err(e) = ba_web_socket.event_loop(&keep_running).await {
            println!("ba Error: {}", e);
        }
    });

    let mut strategy_data = StrategyData::with_capacity(100 * 60 * 60);
    // 手续费 万3 方便触发
    let fee = 0.0001;
    let mut buy_on_ask = 0.;
    let mut sell_on_bid = 0.;
    let mut close_on_bid = 0.;
    let mut close_on_ask = 0.;
    loop {
        let event = rx.recv().await.unwrap();
        strategy_data.push(event);

        if strategy_data.hb_ask_price.len() < 200 {
            println!("len: {}", strategy_data.hb_ask_price.len())
        }

        if strategy_data.hb_ask_price.len() > 200 {
            // println!("hb_bid_div_ba_ask {:?}, hb_bid_div_ba_ask_ma {:?}, fee: {:?}", *strategy_data.hb_bid_div_ba_ask.back().unwrap(), strategy_data.hb_bid_div_ba_ask_ma.back().unwrap(), ((fee*4.) * 2.));
            if *strategy_data.hb_bid_div_ba_ask.back().unwrap()
                > (strategy_data.hb_bid_div_ba_ask_ma.back().unwrap() + ((fee * 4.) * 2.))
            {
                buy_on_ask = *strategy_data.ba_ask_price.back().unwrap();
                sell_on_bid = *strategy_data.hb_bid_price.back().unwrap();
                println!(
                    "hb_bid_div_ba_ask {:?}, hb_bid_div_ba_ask_ma {:?}, fee: {:?}",
                    *strategy_data.hb_bid_div_ba_ask.back().unwrap(),
                    strategy_data.hb_bid_div_ba_ask_ma.back().unwrap(),
                    ((fee * 4.) * 2.)
                );
                println!("卖 hb_bid: {:?}, 买 ba_ask: {:?}", sell_on_bid, buy_on_ask);
            }

            // 持有仓位
            if buy_on_ask > 1.
                && (strategy_data.hb_ask_div_ba_bid.back().unwrap()
                    < strategy_data.hb_bid_div_ba_ask_ma.back().unwrap())
            {
                close_on_ask = *strategy_data.hb_ask_price.back().unwrap();
                close_on_bid = *strategy_data.ba_bid_price.back().unwrap();
                println!(
                    "hb_ask_div_ba_bid: {:?}, hb_ask_div_ba_bid_ma: {:?}",
                    strategy_data.hb_ask_div_ba_bid.back().unwrap(),
                    strategy_data.hb_ask_div_ba_bid_ma.back().unwrap()
                );
                println!(
                    "买平 hb_ask: {:?},  卖平 ba_bid: {:?}",
                    close_on_ask, close_on_bid
                );
                println!(
                    "利润: {:?}",
                    (sell_on_bid / close_on_ask - 1.) + (close_on_bid / buy_on_ask - 1.)
                );

                buy_on_ask = 0.;
                sell_on_bid = 0.;
                close_on_bid = 0.;
                close_on_ask = 0.;
            }
        }

        actix_rt::task::yield_now().await;
    }
}
