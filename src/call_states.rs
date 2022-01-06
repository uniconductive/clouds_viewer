use crate::storage_models;

pub enum ListFolder {
    Ok,
    Failed(storage_models::ListFolderError),
    Cancelled,
    InProgress {
        value: Option<u64>,
        value_str: Option<String>,
    },
    RefreshToken,
    RefreshTokenComplete,
}

pub enum DownloadFile {
    Ok,
    Failed(storage_models::DownloadFileError),
    Cancelled,
    Started,
    SizeInfo {
        size: Option<u64>
    },
    InProgress {
        progress: u64,
    },
    RefreshToken,
    RefreshTokenComplete,
}

// call state data
pub enum Data {
    ListFolder { data: ListFolder, path: String },
    DownloadFile { data: DownloadFile, remote_path: String, local_path: String, size: Option<u64>, downloaded: u64 },
}

pub struct State {
//    pub call_id: u64,
    pub handle: tokio::task::JoinHandle<()>,
    pub data: Data,
}
