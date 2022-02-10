use crate::clouds;
use crate::error;
use chrono::{DateTime, Utc};

#[derive(Copy, Clone)]
pub enum StorageType {
    Cloud(clouds::CloudId),
    //    FileSystem,
}

#[derive(Debug)]
pub struct Item {
    pub name: String,
    pub id: String,
    pub is_folder: bool,
    pub modified: Option<DateTime<Utc>>,
    pub size: Option<u64>,
    pub items: Option<Vec<Item>>,
}

#[allow(non_camel_case_types)]
pub struct list_folder_in_data {
    pub path: String,
    //    pub recursive: bool,
}

#[allow(non_camel_case_types)]
#[derive(Debug)]
pub struct list_folder_out_data {
    //    pub path_id: String,
    pub path: String,
    pub items: Option<Vec<Item>>,
}

#[allow(non_camel_case_types)]
pub struct download_file_in_data {
    pub remote_path: String,
    pub local_path: String,
}

#[allow(non_camel_case_types)]
#[derive(Debug)]
pub struct download_file_out_data {
    pub name: String,
}

pub enum CallInData {
    #[allow(non_camel_case_types)]
    list_folder(list_folder_in_data),
    #[allow(non_camel_case_types)]
    download_file(download_file_in_data),
}

#[derive(thiserror::Error, Debug)]
pub enum ListFolderError {
    #[error("path not found: '{0}'")]
    PathNotFound(String),
    #[error("token error: '{0}'")]
    Token(String),
    #[error("other error: '{0}'")]
    Other(String),
}

#[derive(thiserror::Error, Debug)]
pub enum DownloadFileError {
    #[error("remote path not found: '{0}'")]
    RemotePathNotFound(String),
    #[error("token error: '{0}'")]
    Token(String),
    #[error("other error: '{0}'")]
    Other(String),
}

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("bind errors: '{}'", get_bind_errors_text(errors.clone()))]
    BindErrors { errors: Vec<error::BindError> },
    #[error("can't open auth page in default browser: {error}")]
    CantOpenAuthPageInDefaultBrowser { auth_url: String, error: String },
    #[error("local server error: {0}")]
    LocalServerError(String),
    #[error("incoming request error: '{error}', url: '{url}'")]
    IncomingRequestError { url: String, error: String },
    #[error("local server wait for available failed")]
    LocalServerWaitForAvailabileFailed,
}

fn get_bind_errors_text(errors: Vec<error::BindError>) -> String {
    itertools::Itertools::intersperse(errors.iter().map(|v| v.to_string()), ", ".to_string())
        .collect()
}
