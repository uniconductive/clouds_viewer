use log4rs::{append::{console::{ConsoleAppender, Target}, file::FileAppender},
             config::{Appender, Config, Root},
             encode::pattern::PatternEncoder,
             filter::threshold::ThresholdFilter,
};
use log::{LevelFilter};
use crate::clouds;
use crate::storage_models;
use crate::config;
use crate::storage_instance;

#[derive(thiserror::Error, Debug, Clone)]
pub enum InitError {
    #[error("Invalid download_to path '{path:?}' for storage '{storage:?}'")]
    InvalidStorageDownloadToPath {
        path: String,
        storage: String,
    },
}

fn init_log() -> Result<log4rs::Handle, Box<dyn std::error::Error>> {
    let stderr = ConsoleAppender::builder().target(Target::Stderr).build();

    // Logging to log file.
    let log_file = FileAppender::builder()
        // Pattern: https://docs.rs/log4rs/*/log4rs/encode/pattern/index.html
        .encoder(Box::new(PatternEncoder::new("{l} - {m}\n"))).append(false)
        .build("log.log")?;

//    let log_all_file = FileAppender::builder()
//        // Pattern: https://docs.rs/log4rs/*/log4rs/encode/pattern/index.html
//        .encoder(Box::new(PatternEncoder::new("{l} - {m}\n")))
//        .build("log_all.log")
//        .unwrap();


    // Log Trace level output to file where trace is the default level
    // and the programmatically specified level to stderr.
    let config = Config::builder()
        .appender(Appender::builder().build("log", Box::new(log_file)))
//        .appender(Appender::builder().build("log_all", Box::new(log_all_file)))
        .appender(Appender::builder()
            .filter(Box::new(ThresholdFilter::new(log::LevelFilter::Info)))
            .build("stderr", Box::new(stderr))
        )
        .build(Root::builder()
                   .appender("log")
//                   .appender("log_all")
                   .appender("stderr")
                   .build(LevelFilter::max()),
        )?;

    // Use this to change log levels at runtime.
    // This means you can change the default log level to trace
    // if you are trying to debug an issue and need more logs on then turn it off
    // once you are done.
    let res = log4rs::init_config(config)?;
    log::info!("started");
    Ok(res)

//    info!(target: "yyy", "{}", test());
//    error!("Goes to stderr and file");
//    warn!("Goes to stderr and file");
//    info!("Goes to stderr and file");
//    debug!("Goes to file only");
//    trace!("Goes to file only");
}

pub fn init() -> Result<crate::AppState, Box<dyn std::error::Error>> {
    let _ = init_log()?;

    let rth = std::sync::Arc::new(std::sync::RwLock::new(Some(tokio::runtime::Runtime::new()?)));
//    let rth_clone = std::sync::Arc::clone(&rth);

    let app_config = config::load_config()?;

    let mut storages: Vec<storage_instance::StorageInstance> = vec![];
    for config in &app_config.storages {
        let do_auth = !config.api_key.client_id.is_empty() && !config.api_key.secret.is_empty() && config.tokens.token.is_empty();
        let (events_sender, events_receiver) = tokio::sync::mpsc::unbounded_channel();
        let storage = storage_instance::StorageInstance {
            caption: config.caption.clone(),
            id: config.id.clone(),
            do_auth: do_auth,
            storage_type: storage_models::StorageType::Cloud(clouds::CloudId::Dropbox),
            rth: rth.clone(),
            auth_server_state_holder: std::sync::Arc::new(std::sync::RwLock::new(storage_instance::CloudAuthServerState::default())),
            events_sender: events_sender,
            events: events_receiver,
//            calls_info: std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
            calls_info: std::collections::HashMap::new(),
//            last_call_id: std::rc::Rc::new(std::sync::atomic::AtomicU64::new(0)),
            last_call_id: std::rc::Rc::new(std::sync::RwLock::new(0)),
//            last_call_id: 0,
//            call_states_holder: std::sync::Arc::new(tokio::sync::RwLock::new(crate::storages::CallStates::default())),
            visual_state: storage_instance::StorageVisualState::default(),
            auth_info_holder: std::sync::Arc::new(tokio::sync::RwLock::new(
                clouds::AuthInfo {
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
                    Err(e) => Err(InitError::InvalidStorageDownloadToPath { path: path, storage: config.caption.clone() })
                }?;
                if !path.ends_with("/") {
                    path = path + "/";
                }
                log::info!("download path for {}: {}", &config.caption, &path);
                path
            }
        };
        storages.push(storage);
    }

    Ok(crate::AppState {
        config: app_config,
        rth: rth,
        storages: storages,
    })
}