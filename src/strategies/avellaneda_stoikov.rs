use super::eie::{
    calibration::aksolver_factory::{AkSolverFactory, SolverType},
    intensity_estimator::IntensityEstimator,
    intensity_info::IntensityInfo,
};
use crate::{config::Config, util};

use anyhow::Result;
use exrs::binance_f::{
    account::{FuturesAccount, PositionSide},
    api::BinanceF,
    rest_model::TimeInForce,
    util::get_timestamp,
    ws_model::{AccountUpdateEvent, BookTickerEvent, FuturesWebsocketEvent},
};
use log::{debug, info, warn};
use std::collections::VecDeque;
use tokio::sync::mpsc;

#[derive(Debug, Copy, Clone)]
pub struct Spread {
    ask: f64,
    bid: f64,
}

#[derive(Debug, Clone)]
pub struct StrategyData {
    pub capacity: usize,
    pub timestamp: VecDeque<u64>,
    pub ask_price: VecDeque<f64>,
    pub ask_qty: VecDeque<f64>,
    pub bid_price: VecDeque<f64>,
    pub bid_qty: VecDeque<f64>,
    pub wap: VecDeque<f64>,
    pub imb: VecDeque<f64>,
    pub spread: VecDeque<f64>,
    pub tv: VecDeque<f64>,
}

impl StrategyData {
    pub fn with_capacity(capacity: usize) -> Self {
        StrategyData {
            capacity: capacity,
            timestamp: VecDeque::with_capacity(capacity),
            ask_price: VecDeque::with_capacity(capacity),
            ask_qty: VecDeque::with_capacity(capacity),
            bid_price: VecDeque::with_capacity(capacity),
            bid_qty: VecDeque::with_capacity(capacity),
            wap: VecDeque::with_capacity(capacity),
            imb: VecDeque::with_capacity(capacity),
            spread: VecDeque::with_capacity(capacity),
            tv: VecDeque::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, event: Box<BookTickerEvent>) {
        if self.timestamp.len() > self.capacity - 1 {
            self.timestamp.pop_front();
            self.ask_price.pop_front();
            self.ask_qty.pop_front();
            self.bid_price.pop_front();
            self.bid_qty.pop_front();
            self.wap.pop_front();
            self.imb.pop_front();
            self.spread.pop_front();
            self.tv.pop_front();
        }

        self.timestamp.push_back(event.transaction_time);
        self.ask_price.push_back(event.best_ask);
        self.ask_qty.push_back(event.best_ask_qty);
        self.bid_price.push_back(event.best_bid);
        self.bid_qty.push_back(event.best_bid_qty);

        let wap = ((event.best_bid * event.best_ask_qty) + (event.best_ask * event.best_bid_qty))
            / (event.best_bid_qty + event.best_ask_qty);
        let imb = event.best_bid_qty / (event.best_bid_qty + event.best_ask_qty);
        let spread = (event.best_ask - event.best_bid) / wap;

        self.wap.push_back(wap);
        self.imb.push_back(imb);
        self.spread.push_back(spread);

        let tv = (wap / self.wap.front().unwrap() - 1f64).abs() + (spread / wap);
        self.tv.push_back(tv);
    }
}

#[derive(Debug, Clone)]
struct Position {
    pub symbol: String,
    pub position_amount: f64,
    pub entry_price: f64,
}

pub struct AvellanedaStoikov {
    config: Config,
    start_time: u64,
    timer: u64,
    account_client: FuturesAccount,
    strategy_data: StrategyData,
    base_asset: String,
    quote_asset: String,
    pair: String,
    order_qty: f64,
    tick_size: f64,
    tick_round: u32,
    n_spreads: usize,
    estimate_window: u64,
    period: u64,
    gamma: f64,
    sigma_multiplier: f64,
    ie: IntensityEstimator,
    sigma: f64,
    buy_a: f64,
    buy_k: f64,
    sell_a: f64,
    sell_k: f64,
    position: Position,
    cash: f64,
    total_profit: f64,
    stoploss: f64,
    stoploss_sleep: u64,
    stopprofit: f64,
    in_stoploss: bool,
    unrealized_pnl: f64,
    trailing_stop: f64,
    active_trailing_stop: bool,
    q_max: f64,
}

impl AvellanedaStoikov {
    pub fn new(config: Config) -> Box<Self> {
        let solver_type = SolverType::LogRegression;

        let sf = AkSolverFactory::new(&solver_type);
        let ie = IntensityEstimator::new(
            config.tick_size.clone(),
            config.n_spreads.clone(),
            config.estimate_window.clone(),
            config.period.clone(),
            sf,
        );

        let account_client: FuturesAccount =
            BinanceF::new(config.api_key.clone(), config.secret_key.clone());

        let tick_round = config
            .tick_size
            .to_string()
            .split(".")
            .collect::<Vec<&str>>()[1]
            .len() as u32;
        let pair = format!(
            "{}{}",
            config.base_asset.clone(),
            config.quote_asset.clone()
        );

        Box::new(AvellanedaStoikov {
            config: config.clone(),
            start_time: get_timestamp().unwrap(),
            timer: 0,
            account_client: account_client,
            strategy_data: StrategyData::with_capacity(config.sigma_tick_period),
            base_asset: config.base_asset,
            quote_asset: config.quote_asset,
            pair: pair.clone(),
            order_qty: config.order_qty,
            tick_size: config.tick_size,
            tick_round: tick_round,
            n_spreads: config.n_spreads,
            estimate_window: config.estimate_window,
            period: config.period,
            gamma: 0.1,
            sigma_multiplier: config.sigma_multiplier,
            ie: ie,
            sigma: 1.0,
            buy_a: 0.4,
            buy_k: 0.2,
            sell_a: 0.4,
            sell_k: 0.2,
            position: Position {
                symbol: pair.clone(),
                position_amount: 0f64,
                entry_price: 0f64,
            },
            cash: 0f64,
            total_profit: 0f64,
            stoploss: config.stoploss,
            stoploss_sleep: config.stoploss_sleep,
            in_stoploss: false,
            unrealized_pnl: 0f64,
            stopprofit: config.stopprofit,
            trailing_stop: config.trailing_stop,
            active_trailing_stop: false,
            q_max: config.q_max,
        })
    }

    pub fn name() -> String {
        "Avellaneda_Stoikov".into()
    }

    pub async fn run_forever(&mut self, mut rx: mpsc::Receiver<FuturesWebsocketEvent>) {
        let account_balance = self.account_client.account_balance().await.unwrap();

        info!("account_balance: {:?}", account_balance);

        let pair = "DOGEUSDT";

        let positions = self
            .account_client
            .position_information(pair)
            .await
            .unwrap();

        info!("position_information: {:?}", positions);

        // let qty = positions
        //     .iter()
        //     .find(|&x| x.symbol.eq(pair) && x.position_side.eq("BOTH"))
        //     .and_then(|x| Some(x.position_amount)).unwrap();

        // match self
        //     .account_client
        //     .market_sell(pair, 1000.)
        //     .await
        // {
        //     Ok(answer) => info!("market sell {:?}", answer),
        //     Err(err) => warn!("market sell Error: {}", err),
        // }

        loop {
            if let Some(event) = rx.recv().await {
                match event {
                    FuturesWebsocketEvent::BookTicker(book_event) => {
                        // debug!("book_event: {:?}", book_event);
                        self.on_tick(book_event).await.unwrap();
                    }
                    FuturesWebsocketEvent::AccountUpdate(account_event) => {
                        // debug!("account_event: {:?}", account_event);
                        self.on_account(account_event).await.unwrap();
                    }
                    FuturesWebsocketEvent::OrderTradeUpdate(order_event) => {
                        debug!("ORDER_TRADE_UPDATE: {:?}", order_event);
                    }
                    FuturesWebsocketEvent::AccountConfigUpdate(config_event) => {
                        debug!("ACCOUNT_CONFIG_UPDATE: {:?}", config_event);
                    }
                    _ => {
                        warn!("Websockets parse error! {:?}", event);
                    }
                }
            }
            actix_rt::task::yield_now().await;
        }
    }

    async fn on_tick(&mut self, data: Box<BookTickerEvent>) -> Result<()> {
        debug!("on_ticker: {:?}", data);
        self.strategy_data.push(data.clone());

        if let Some(intensity_info) =
            self.calculate_intensity_info(data.best_ask, data.best_bid, data.transaction_time)
        {
            let (buy_a, buy_k, sell_a, sell_k) = intensity_info.get_ak();

            self.buy_a = buy_a + std::f64::EPSILON;
            self.buy_k = buy_k + std::f64::EPSILON;
            self.sell_a = sell_a + std::f64::EPSILON;
            self.sell_k = sell_k + std::f64::EPSILON;

            let spread = self.calculate_spread();
            info!("speard: {:?}", spread);

            if !self.in_stoploss {
                if self.position.position_amount > 0f64 {
                    self.unrealized_pnl = (self.strategy_data.bid_price.back().unwrap()
                        * self.position.position_amount)
                        / (self.position.entry_price * self.position.position_amount)
                        - 1f64;
                } else if self.position.position_amount < 0f64 {
                    self.unrealized_pnl = -((self.strategy_data.ask_price.back().unwrap()
                        * self.position.position_amount)
                        / (self.position.entry_price * self.position.position_amount)
                        - 1f64);
                }

                info!(
                    "unrealized_pnl: {}, -stoploss: {}, stoploss?: {}, stopprofit: {}",
                    self.unrealized_pnl,
                    -self.stoploss,
                    self.unrealized_pnl < -self.stoploss,
                    self.stopprofit
                );

                if self.unrealized_pnl > self.trailing_stop
                    && (self.timer <= data.transaction_time / 1e3 as u64 - (10000 / 1000))
                {
                    self.active_trailing_stop = true;
                }

                if self.active_trailing_stop && (self.unrealized_pnl < self.trailing_stop) {
                    if self.position.position_amount > 0f64 {
                        match self
                            .account_client
                            .market_sell(&self.pair, self.position.position_amount)
                            .await
                        {
                            Ok(answer) => info!("Trailing stop market sell {:?}", answer),
                            Err(err) => warn!("Trailing stop market sell Error: {}", err),
                        }
                    } else if self.position.position_amount < 0f64 {
                        match self
                            .account_client
                            .market_buy(&self.pair, self.position.position_amount.abs())
                            .await
                        {
                            Ok(answer) => info!("Trailing stop market buy {:?}", answer),
                            Err(err) => warn!("Trailing stop market buy Error: {}", err),
                        }
                    } else {
                        info!("Already Trailing Stoped, pass.")
                    }

                    self.unrealized_pnl = 0f64;

                    self.active_trailing_stop = false;

                    self.timer = data.transaction_time / 1e3 as u64;
                }

                if self.unrealized_pnl < -self.stoploss {
                    warn!("unrealized_pnl: {:?}, small than stoploss: {:?} stoploss then sleep: {:?}ms", self.unrealized_pnl, self.stoploss, self.stoploss_sleep);

                    match self.account_client.cancel_all_open_orders(&self.pair).await {
                        Ok(answer) => info!("Cancel all open orders: {:?}", answer),
                        Err(err) => warn!("Cancel all open orders Error: {:?}", err),
                    }

                    if self.position.position_amount > 0f64 {
                        match self
                            .account_client
                            .market_sell(&self.pair, self.position.position_amount)
                            .await
                        {
                            Ok(answer) => info!("Stop loss market sell {:?}", answer),
                            Err(err) => warn!("Stop loss market sell Error: {}", err),
                        }
                    } else {
                        match self
                            .account_client
                            .market_buy(&self.pair, self.position.position_amount.abs())
                            .await
                        {
                            Ok(answer) => info!("Stop loss market buy {:?}", answer),
                            Err(err) => warn!("Stop loss market buy Error: {}", err),
                        }
                    }

                    self.unrealized_pnl = 0f64;

                    self.in_stoploss = true;

                    self.active_trailing_stop = false;

                    self.timer = data.transaction_time / 1e3 as u64;
                } else if (self.unrealized_pnl > self.stopprofit)
                    && (self.timer <= data.transaction_time / 1e3 as u64 - (self.period / 1000))
                {
                    warn!(
                        "unrealized_pnl: {:?}, bigger than stopprofit: {:?}",
                        self.unrealized_pnl, self.stopprofit
                    );

                    match self.account_client.cancel_all_open_orders(&self.pair).await {
                        Ok(answer) => info!("Cancel all open orders: {:?}", answer),
                        Err(err) => warn!("Cancel all open orders Error: {:?}", err),
                    }

                    if self.position.position_amount > 0f64 {
                        match self
                            .account_client
                            .market_sell(&self.pair, self.position.position_amount)
                            .await
                        {
                            Ok(answer) => info!("Stop stopprofit market sell {:?}", answer),
                            Err(err) => warn!("Stop stopprofit market sell Error: {}", err),
                        }
                    } else {
                        match self
                            .account_client
                            .market_buy(&self.pair, self.position.position_amount.abs())
                            .await
                        {
                            Ok(answer) => info!("Stop stopprofit market buy {:?}", answer),
                            Err(err) => warn!("Stop stopprofit market buy Error: {}", err),
                        }
                    }

                    self.unrealized_pnl = 0f64;

                    self.timer = data.transaction_time / 1e3 as u64;
                } else if self.timer <= data.transaction_time / 1e3 as u64 - (self.period / 1000) {
                    debug!(
                        "timer: {}, now - {} = {}",
                        self.timer,
                        (self.period / 1000),
                        data.transaction_time / 1e3 as u64 - 2
                    );

                    let account_client = self.account_client.clone();
                    let last_wap = self.strategy_data.wap.back().unwrap().clone();
                    let pair = self.pair.clone();
                    let order_qty = self.order_qty.clone();
                    let tick_round = self.tick_round.clone();

                    actix_rt::spawn(async move {
                        debug!("on_ticker thread");

                        match account_client.cancel_all_open_orders(&pair).await {
                            Ok(answer) => info!("Cancel all open orders: {:?}", answer),
                            Err(err) => warn!("Cancel all open orders Error: {:?}", err),
                        }

                        let sell_price = util::round_to(last_wap + spread.ask, tick_round);

                        let buy_price = util::round_to(last_wap - spread.bid, tick_round);

                        debug!(
                            "wap: {}, ask_spread: {}, bid_spread: {}, sell_price {}, buy_price {}",
                            last_wap, spread.ask, spread.bid, sell_price, buy_price
                        );

                        match account_client
                            .limit_buy(
                                &pair,
                                order_qty,
                                buy_price,
                                PositionSide::Both,
                                TimeInForce::GTC,
                            )
                            .await
                        {
                            Ok(answer) => info!("Limit buy {:?}", answer),
                            Err(err) => warn!("Limit buy Error: {}", err),
                        }

                        match account_client
                            .limit_sell(
                                &pair,
                                order_qty,
                                sell_price,
                                PositionSide::Both,
                                TimeInForce::GTC,
                            )
                            .await
                        {
                            Ok(answer) => info!("Limit sell {:?}", answer),
                            Err(err) => warn!("Limit sell Error: {}", err),
                        }
                    });

                    self.timer = data.transaction_time / 1e3 as u64;
                    debug!("new timer {}", self.timer);
                }
            } else if self.timer
                <= data.transaction_time / 1e3 as u64 - (self.stoploss_sleep / 1000)
            {
                self.in_stoploss = false;
                info!("stoploss sleep finished!");
            } else {
                info!("in stoploss sleep, please wait...");
            }
        } else {
            info!("waiting for get more data...");
        }
        Ok(())
    }

    async fn on_account(&mut self, data: Box<AccountUpdateEvent>) -> Result<()> {
        info!("on_account: {:?}", data);

        for balance in &data.account_update.balances {
            if balance.asset.eq(&self.quote_asset) {
                self.cash = balance.cross_wallet_balance;
            }
        }

        let tmp_q = data
            .account_update
            .positions
            .iter()
            .find(|&x| x.symbol.eq(&self.pair) && x.position_side.eq("BOTH"))
            .and_then(|x| Some(x.position_amount));

        let entry_price = data
            .account_update
            .positions
            .iter()
            .find(|&x| x.symbol.eq(&self.pair) && x.position_side.eq("BOTH"))
            .and_then(|x| Some(x.entry_price));

        self.position.entry_price = entry_price.unwrap_or_else(|| self.position.entry_price);
        self.position.position_amount = tmp_q.unwrap_or_else(|| self.position.position_amount);

        info!(
            "cash {:?}, q {:?}",
            self.cash, self.position.position_amount
        );
        Ok(())
    }

    fn calculate_intensity_info(&mut self, ask: f64, bid: f64, ts: u64) -> Option<IntensityInfo> {
        let can_get = self.ie.on_tick(bid, ask, ts);

        // wait to get more data
        if can_get && ts > self.start_time + self.estimate_window + 1 {
            let ii = self.ie.estimate(ts);
            debug!("intensity_info {:#?}", ii);
            return Some(ii);
        } else {
            None
        }
    }

    fn calculate_tv_mean(&mut self) -> Option<f64> {
        let sum: f64 = self.strategy_data.tv.iter().sum();
        let count = self.strategy_data.tv.len();

        match count {
            positive if positive > 0 => Some(sum / count as f64),
            _ => None,
        }
    }

    fn calculate_classical_volatility(&mut self) -> Option<f64> {
        let t = 10.;
        let mut classical_hv = 0.;
        for i in 0..self.strategy_data.wap.iter().len() - 1 {
            let res =
                self.strategy_data.wap.get(i + 1).unwrap() - self.strategy_data.wap.get(i).unwrap();
            classical_hv += res.powi(2)
        }

        let res = (classical_hv / t).sqrt();

        Some(res)
    }

    fn calculate_spread_volatility(&mut self) -> Option<f64> {
        let sum: f64 = self.strategy_data.spread.iter().map(|x| x.powi(2)).sum();
        let count = self.strategy_data.spread.len();
        let t = 10.;

        let res = (sum / t).sqrt();

        match count {
            positive if positive > 0 => Some(res),
            _ => None,
        }
    }

    /// https://github.com/TommasoBelluzzo/HistoricalVolatility
    // fn calculate_p_volatility(&mut self) -> Option<f64> {
    //     let wap_vec = self.strategy_data.wap.iter().cloned().collect::<Vec<f64>>();
    //     let t = 10.;
    //     let mut parkinson_hv = 0.;
    //     for chunk in wap_vec.chunks(30) {
    //         let hl = (chunk.iter().cloned().fold(0. / 0., f64::max)
    //             / chunk.iter().cloned().fold(0. / 0., f64::min))
    //         .ln();
    //         let res = hl.powi(2);
    //         parkinson_hv += res;
    //     }
    //     let res = (parkinson_hv / (4. * t * (2f64.ln()))).sqrt();

    //     Some(res)
    // }

    // fn calculate_gk_volatility(&mut self) -> Option<f64> {
    //     let wap_vec = self.strategy_data.wap.iter().cloned().collect::<Vec<f64>>();
    //     let t = 10.;

    //     let mut garman_klass_hv = 0.;
    //     for chunk in wap_vec.chunks(30) {
    //         let co = (chunk.last().unwrap() / chunk.first().unwrap()).ln();
    //         let hl = (chunk.iter().cloned().fold(0. / 0., f64::max)
    //             / chunk.iter().cloned().fold(0. / 0., f64::min))
    //         .ln();
    //         let res = 0.5 * hl.powi(2) - ((2. * 2f64.ln()) - 1.) * co.powi(2);
    //         garman_klass_hv += res;
    //     }
    //     let res = (garman_klass_hv / t).sqrt();

    //     Some(res)
    // }

    fn calculate_spread(&mut self) -> Spread {
        // self.sigma = self.calculate_tv_mean().unwrap();
        // self.sigma = self.calculate_p_volatility().unwrap();
        // self.sigma = self.calculate_gk_volatility().unwrap();
        self.sigma = self.calculate_spread_volatility().unwrap();
        let sigma_fix = self.sigma * self.sigma_multiplier.clone();
        let q_fix = self.position.position_amount / self.order_qty;

        info!(
            "sigma: {}, sigma_multiplier {}, sigma_fix {}, q {}, q_fix {}",
            self.sigma, self.sigma_multiplier, sigma_fix, self.position.position_amount, q_fix,
        );
        info!(
            "buy_k: {}, buy_a: {}, sell_k {}, sell_a {}",
            self.buy_k, self.buy_a, self.sell_k, self.sell_a
        );

        let bid = (1. + self.gamma / self.sell_k).ln() / self.gamma
            + ((q_fix + 0.5)
                * ((sigma_fix * sigma_fix * self.gamma) / (2. * self.sell_k * self.sell_a)
                    * (1. + self.gamma / self.sell_k).powf(1. + self.sell_k / self.gamma))
                .sqrt());

        let ask = (1. + self.gamma / self.buy_k).ln() / self.gamma
            - ((q_fix - (0.5))
                * ((sigma_fix * sigma_fix * self.gamma) / (2. * self.buy_k * self.buy_a)
                    * (1. + self.gamma / self.buy_k).powf(1. + self.buy_k / self.gamma))
                .sqrt());

        Spread { ask: ask, bid: bid }
    }
}
