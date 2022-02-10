use crate::storage_models;

#[derive(Debug)]
pub enum ListFolder {
    Start {
        path: String,
    },
    Started {
        handle: tokio::task::JoinHandle<()>,
        path: String,
    },
    Progress {
        value: u64,
        value_str: Option<String>,
    },
    Finished {
        result: Result<storage_models::list_folder_out_data, storage_models::ListFolderError>,
    },
    RefreshToken,
    RefreshTokenComplete,
}

#[derive(Debug)]
pub enum DownloadFile {
    Started {
        handle: tokio::task::JoinHandle<()>,
        remote_path: String,
        local_path: String,
    },
    SizeInfo {
        size: Option<u64>,
    },
    Progress {
        value: u64,
    },
    Finished {
        result: Result<storage_models::download_file_out_data, storage_models::DownloadFileError>,
    },
    Cancelled,
    RefreshToken,
    RefreshTokenComplete,
}

#[derive(Debug)]
pub enum Auth {
    Start,
    Binded {
        redirect_url: String,
        addr: std::net::SocketAddr,
        state_field: String,
    },
    LocalServerShutdowner {
        cancel_informer: tokio::sync::oneshot::Sender<()>,
        cancel_awaiter: tokio::sync::oneshot::Receiver<()>,
    },
    CheckAvailability {
        redirect_url: String,
        state_field: String,
    },
    ServerReady {
        redirect_url: String,
        state_field: String,
    },
    Finished {
        result: Result<(), storage_models::AuthError>,
    },
    Cancel,
}

// Message data
#[derive(Debug)]
pub enum Data {
    ListFolder(ListFolder),
    DownloadFile(DownloadFile),
    Auth(Auth),
}

#[derive(Debug)]
pub struct Message {
    pub call_id: Option<u64>,
    pub data: Data,
}
