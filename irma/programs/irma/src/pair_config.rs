use crate::MarketMakingMode;
use crate::error::Error;
use anchor_lang::prelude::Pubkey;

#[derive(Debug, Clone, Default)]
pub struct PairConfig {
    pub pair_address: String,
    pub x_amount: u64,
    pub y_amount: u64,
    pub mode: MarketMakingMode,
}

pub fn should_market_making(config: &Vec<PairConfig>) -> bool {
    for pair in config.iter() {
        if pair.mode != MarketMakingMode::ModeView {
            return true;
        }
    }
    return false;
}

pub fn get_pair_config(config: &Vec<PairConfig>, pair_addr: Pubkey) -> PairConfig {
    for pair_config in config.iter() {
        if pair_config.pair_address == pair_addr.to_string() {
            return pair_config.clone();
        }
    }
    return PairConfig::default();
}

pub fn get_config() -> Result<Vec<PairConfig>, Error> {
    let config: Vec<PairConfig> = vec![
        PairConfig {
            pair_address: "DLmm".to_string(),
            x_amount: 1000,
            y_amount: 2000,
            mode: MarketMakingMode::ModeBoth,
        }
    ];
    Ok(config)
}

#[cfg(test)]
mod config_test {
    use super::*;
    use std::env;
    #[test]
    fn test_get_get_config_from_file() {
        let mut owned_string: String = env::current_dir()
            .unwrap()
            .into_os_string()
            .into_string()
            .unwrap();
        let borrowed_string: &str = "/src/pair_config.json";
        owned_string.push_str(borrowed_string);

        let config = get_config().unwrap();
        println!("{:?}", config);
    }
}
