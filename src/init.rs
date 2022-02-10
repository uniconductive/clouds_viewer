use crate::config;
use crate::storage_instance;
use crate::storage_models;
use crate::{clouds, RuntimeHolder};

#[derive(thiserror::Error, Debug, Clone)]
pub enum InitError {
    #[error("Invalid download_to path '{path:?}' for storage '{storage:?}'")]
    InvalidDownloadToPathForStorage { path: String, storage: String },
}

#[cfg(feature = "log_4rs")]
fn init_log(module_name: String) -> Result<log4rs::Handle, Box<dyn std::error::Error>> {
    use log::{LevelFilter, Record};
    use log4rs::{
        append::{
            console::{ConsoleAppender, Target},
            file::FileAppender,
        },
        config::{Appender, Config, Root},
        encode::pattern::PatternEncoder,
        filter::threshold::ThresholdFilter,
        filter::{Filter, Response},
    };

    //    #[derive(Clone, Eq, PartialEq, Hash, Debug)]
    #[derive(Debug)]
    pub struct ModuleFilter {
        name: String,
    }

    impl ModuleFilter {
        pub fn new(name: String) -> Self {
            Self { name }
        }
    }

    impl Filter for ModuleFilter {
        fn filter(&self, record: &Record) -> Response {
            match record.module_path() {
                None => Response::Neutral,
                Some(path) => match path.starts_with(&self.name) {
                    true => Response::Neutral,
                    false => Response::Reject,
                },
            }
        }
    }

    let stderr = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d}: {l} - {m}\n")))
        .target(Target::Stderr)
        .build();

    let log_file = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d}: {l} - {m}\n")))
        .append(false)
        .build("log.log")?;

    let config = Config::builder()
        .appender(
            Appender::builder()
                .filter(Box::new(ThresholdFilter::new(log::LevelFilter::Debug)))
                .filter(Box::new(ModuleFilter::new(module_name.clone())))
                .build("log", Box::new(log_file)),
        )
        .appender(
            Appender::builder()
                .filter(Box::new(ThresholdFilter::new(log::LevelFilter::Debug)))
                .filter(Box::new(ModuleFilter::new(module_name.clone())))
                .build("stderr", Box::new(stderr)),
        )
        .build(
            Root::builder()
                .appender("log")
                .appender("stderr")
                .build(LevelFilter::max()),
        )?;

    let res = log4rs::init_config(config)?;
    Ok(res)
}

pub fn init() -> Result<crate::AppState, Box<dyn std::error::Error>> {
    if std::env::var("RUST_LOG").is_err() {
        if std::env::var("CARGO_MANIFEST_DIR").is_ok() {
            std::env::set_var("RUST_LOG", "clouds_viewer=debug");
        }
    }

    #[cfg(feature = "log_4rs")]
    let _ = init_log("clouds_viewer".to_string())?;
    #[cfg(feature = "log_env")]
    env_logger::Builder::from_default_env()
        .format_timestamp_nanos()
        .init();
    #[cfg(feature = "log_pretty_env")]
    pretty_env_logger::init();

    log::debug!("started");

    //    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error,warn,info,debug,trace")).init();

    let rth = RuntimeHolder::new();
    let app_config = config::load_config()?;
    let mut storages: Vec<storage_instance::StorageInstance> = vec![];
    for config in &app_config.storages {
        let do_auth = !config.api_key.client_id.is_empty()
            && !config.api_key.secret.is_empty()
            && (config.tokens.token.is_empty() && config.tokens.refresh_token.is_empty());
        let storage = storage_instance::StorageInstance {
            caption: config.caption.clone(),
            id: config.id.clone(),
            storage_type: storage_models::StorageType::Cloud(clouds::CloudId::Dropbox),
            rth: rth.clone(),
            messages: std::default::Default::default(),
            call_states: std::collections::HashMap::new(),
            last_call_id: std::rc::Rc::new(std::sync::RwLock::new(0)),
            visual_state: storage_instance::StorageVisualState {
                auth_needed: do_auth,
                ..storage_instance::StorageVisualState::default()
            },
            auth_info_holder: std::sync::Arc::new(tokio::sync::RwLock::new(clouds::AuthInfo {
                client_id: config.api_key.client_id.clone(),
                secret: config.api_key.secret.clone(),
                refresh_token: config.tokens.refresh_token.clone(),
                token: config.tokens.token.clone(),
                write_mutex: std::sync::Arc::new(tokio::sync::RwLock::new(0)),
                redirect_addresses: config.redirect_addresses.clone(),
            })),
            save_to_path: {
                let mut path = config.download_to.clone();
                if path.is_empty() {
                    path = "./".to_string();
                }
                let mut path: String = match std::fs::canonicalize(&path) {
                    Ok(path) => Ok(path.to_str().unwrap().to_string()),
                    Err(_e) => Err(InitError::InvalidDownloadToPathForStorage {
                        path,
                        storage: config.caption.clone(),
                    }),
                }?;
                if !path.ends_with('/') {
                    path += "/";
                }
                log::debug!("download path for {}: {}", &config.caption, &path);
                path
            },
        };
        storages.push(storage);
    }

    Ok(crate::AppState {
        config: app_config,
        rth,
        storages,
    })
}
