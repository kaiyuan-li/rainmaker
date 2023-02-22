#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub rest_api_endpoint: String,
    pub ws_public_endpoint: String,
    pub ws_private_endpoint: String,
    pub is_testnet: bool,
}

impl Config {
    /// Configure binance with default production endpoints
    /// # Examples            .set_rest_api_endpoint("https://testnet.binance.vision")

    /// ```
    /// use binance::config::Config;
    /// let config = Config::default();
    /// ```
    pub fn new(is_testnet: bool) -> Config {
        match is_testnet {
            true => Config::testnet(),
            false => Config::default()
        }
    }

    pub fn default() -> Config {
        Config {
            rest_api_endpoint: "https://www.okex.com".into(),
            ws_public_endpoint: "wss://ws.okex.com:8443/ws/v5/public".into(),
            ws_private_endpoint: "wss://ws.okex.com:8443/ws/v5/private".into(),
            is_testnet: false,
        }
    }

    /// Configure binance with all testnet endpoints
    /// # Examples
    /// ```
    /// use binance::config::Config;
    /// let config = Config::testnet();
    /// ```
    pub fn testnet() -> Config {
        Config::default()
            .set_ws_public("wss://wspap.okex.com:8443/ws/v5/public?brokerId=9999")
            .set_ws_private("wss://wspap.okex.com:8443/ws/v5/private?brokerId=9999")
            .set_is_testnet()
    }

    pub fn set_rest_api_endpoint<T: Into<String>>(mut self, rest_api_endpoint: T) -> Self {
        self.rest_api_endpoint = rest_api_endpoint.into();
        self
    }

    pub fn set_ws_public<T: Into<String>>(mut self, ws_endpoint: T) -> Self {
        self.ws_public_endpoint = ws_endpoint.into();
        self
    }

    pub fn set_ws_private<T: Into<String>>(mut self, ws_endpoint: T) -> Self {
        self.ws_private_endpoint = ws_endpoint.into();
        self
    }
    pub fn set_is_testnet(mut self) -> Self {
        self.is_testnet = true;
        self
    }
}
