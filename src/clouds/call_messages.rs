use crate::storage_models;

pub enum ListFolder {
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

pub enum DownloadFile {
    Started {
        handle: tokio::task::JoinHandle<()>,
        remote_path: String,
        local_path: String,
    },
    SizeInfo {
        size: Option<u64>
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

// Message data
pub enum Data {
    ListFolder(ListFolder),
    DownloadFile(DownloadFile),
}

pub struct Message {
    pub call_id: u64,
    pub data: Data,
}
