use crate::storage_models;

#[derive(Debug)]
pub enum ListFolder {
    Ok,
    Failed(storage_models::ListFolderError),
    InProgress {
        value: Option<u64>,
        value_str: Option<String>,
    },
    RefreshToken,
    RefreshTokenComplete,
}

#[derive(Debug)]
pub enum DownloadFile {
    Ok,
    Failed(storage_models::DownloadFileError),
    Started,
    SizeInfo { size: Option<u64> },
    InProgress { progress: u64 },
    RefreshToken,
    RefreshTokenComplete,
}

#[derive(Debug)]
pub enum AuthProgress {
    /// before bind
    StartingLocalServer,
    Binded {
        redirect_url: String,
    },
    WaitingForLocalServerUp {
        redirect_url: String,
    },
    TryOpenBrowser {
        auth_url: String,
    },
    BrowserOpened {
        auth_url: String,
    },
}

#[derive(Debug)]
pub enum Auth {
    Ok,
    Failed(storage_models::AuthError),
    InProgress(AuthProgress),
}

// call state data
#[derive(Debug)]
pub enum Data {
    ListFolder {
        data: ListFolder,
        path: String,
    },
    DownloadFile {
        data: DownloadFile,
        remote_path: String,
        local_path: String,
        size: Option<u64>,
        downloaded: u64,
    },
    Auth {
        data: Auth,
    },
}

#[derive(Debug)]
pub struct Canceller {
    pub cancel_informer: tokio::sync::oneshot::Sender<()>,
    pub cancel_awaiter: tokio::sync::oneshot::Receiver<()>,
}

#[derive(Debug)]
pub struct State {
    pub handles: Vec<tokio::task::JoinHandle<()>>,
    pub cancellers: Vec<Canceller>,
    pub data: Data,
}


