use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokensConfig {
    pub token: String,
    pub refresh_token: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApiKeyConfig {
    pub client_id: String,
    pub secret: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StorageConfig {
    pub caption: String,
    pub id: String,
    #[serde(default)]
    pub current_path: String,
    pub tokens: TokensConfig,
    pub api_key: ApiKeyConfig,
    pub redirect_addresses: Vec<String>,
    #[serde(default)]
    pub download_to: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub storages: Vec<StorageConfig>,
}

impl AppConfig {
    pub fn storage_by_id_mut(&mut self, id: String) -> Option<&mut StorageConfig> {
        for storage in &mut self.storages {
            if storage.id.eq(&id) {
                return Some(storage);
            }
        }
        None
    }

    pub fn storage_by_id(&self, id: String) -> Option<&StorageConfig> {
        for storage in &self.storages {
            if storage.id.eq(&id) {
                return Some(storage);
            }
        }
        None
    }
}

pub fn get_config_file_path(force_near_binary: bool) -> Result<(PathBuf, bool), Box<dyn Error>> {
    let mut path = std::env::current_exe()?;
    if !path.set_extension("config") {
        panic!("!file_name.set_extension('config')")
    }
    if force_near_binary {
        Ok((path, false))
    } else if path.exists() && path.is_file() {
        Ok((path, true))
    } else {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        if manifest_dir.is_empty() {
            Ok((path, false))
        } else {
            let file_name = path.file_name().unwrap().to_str().unwrap().to_string();
            let mut res = PathBuf::from(manifest_dir);
            res.push("configs");
            res.push(&file_name);
            Ok((res, false))
        }
    }
}

pub fn load_config() -> Result<AppConfig, Box<dyn Error>> {
    let (file_path, checked) = get_config_file_path(false)?;
    let file_path_str = file_path.to_str().unwrap().to_string();
    if checked || (file_path.exists() && file_path.is_file()) {
        log::debug!("config file: '{}'", file_path_str);
        let data = std::fs::read(file_path)
            .expect(&("Unable to read config file: ".to_owned() + &file_path_str));
        let res: AppConfig = serde_json::from_slice(&data)
            .expect(&("Unable to parse config file: ".to_owned() + &file_path_str));
        Ok(res)
    } else {
        panic!("config file '{}' does not exists", &file_path_str)
    }
}

pub fn save_config(app_config: &AppConfig) -> Result<(), Box<dyn Error>> {
    let (file_path, _) = get_config_file_path(false)?;
    let res = serde_json::to_vec_pretty(app_config)?;
    std::fs::write(file_path, &res)?;
    Ok(())
}

/*
pub fn create_default_config() -> Result<(), Box<dyn Error>> {
    save_config(&AppConfig {
        listen_on: vec!["127.0.0.1:8080".into(), "192.168.88.251:8080".into()],
        shares: vec![
            ShareConfig {name: "dist".into(), path: "/lindata/Distr/".into()},
        ],
    })
}
*/
