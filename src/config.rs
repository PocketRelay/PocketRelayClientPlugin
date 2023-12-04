use log::debug;
use native_windows_gui::error_message;
use serde::{Deserialize, Serialize};
use std::env::current_exe;

/// Name of the file that stores saved pocket relay configuration info
pub const CONFIG_FILE_NAME: &str = "pocket-relay-client.json";

#[derive(Debug, Deserialize, Serialize)]
pub struct ClientConfig {
    pub connection_url: String,
}

pub fn read_config_file() -> Option<ClientConfig> {
    let current_path = current_exe().unwrap();
    let parent = current_path
        .parent()
        .expect("Missing parent directory to current exe path");

    let file_path = parent.join(CONFIG_FILE_NAME);
    if !file_path.exists() {
        return None;
    }

    debug!("Reading config from: {}", file_path.display());

    let bytes = match std::fs::read(file_path) {
        Ok(value) => value,
        Err(err) => {
            error_message("Failed to read client config", &err.to_string());
            return None;
        }
    };

    let config: ClientConfig = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(err) => {
            error_message("Failed to parse client config", &err.to_string());
            return None;
        }
    };

    Some(config)
}

pub fn write_config_file(config: ClientConfig) {
    let current_path = current_exe().unwrap();
    let parent = current_path
        .parent()
        .expect("Missing parent directory to current exe path");

    let file_path = parent.join(CONFIG_FILE_NAME);

    let bytes = match serde_json::to_vec(&config) {
        Ok(value) => value,
        Err(err) => {
            error_message("Failed to save client config", &err.to_string());
            return;
        }
    };

    if let Err(err) = std::fs::write(file_path, bytes) {
        error_message("Failed to save client config", &err.to_string());
    }
}
