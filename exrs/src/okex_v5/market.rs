use serde_json::from_str;

use super::client::*;
use super::errors::*;
use super::rest_model::*;
use super::util::bool_to_string;

static API_V5_CANDLES: &str = "/api/v5/market/candles";

#[derive(Clone)]
pub struct Market {
    pub client: Client,
}

impl Market {
    pub async fn get_candles(&self, 
        symbol:String, 
        bar:Option<String>, 
        after:Option<i64>, 
        before:Option<i64>, 
        limit:Option<u16>
    ) -> Result<CandleResponse>
    {
        let req = CandleRequest {
            symbol, bar, after, before, limit
        };
        
        println!("{:?}", req);
        self.client.get_d(API_V5_CANDLES, Some(req)).await
    }
}