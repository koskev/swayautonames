use std::{collections::HashMap, fs::File, path::PathBuf};

use log::error;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Default, Debug, Clone)]
pub struct SwayNameManagerConfig {
    pub app_symbols: HashMap<String, String>,
}

impl SwayNameManagerConfig {
    pub fn from_file(config_path: &PathBuf) -> Self {
        let file_result = File::open(config_path);
        match file_result {
            Ok(config_file) => {
                let serde_result = serde_yaml::from_reader(config_file);
                match serde_result {
                    Ok(result) => {
                        return result;
                    }
                    Err(e) => {
                        error!("Error while reading config: {e}. Using default config")
                    }
                }
            }
            Err(e) => {
                error!("Failed to open config file: {e}. Using default config");
            }
        }
        Self {
            ..Default::default()
        }
    }
}
