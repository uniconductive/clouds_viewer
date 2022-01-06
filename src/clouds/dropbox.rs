//#[path = "./clouds_shared.rs"] mod clouds_shared;
// https://curl.se/docs/manpage.html
//#[path = "base.rs"] mod base;

use std::{
    error::{Error},
    result::{Result},
    io::{Read},
    sync::{Arc},
    time::{Instant, Duration},
};
use tokio::{
    sync::{
        RwLock,
//        mpsc::UnboundedSender,
    },
};
use serde::{
    Serialize,
    Deserialize,
};
use chrono::prelude::*;
use hyper::{Body, Client};
use backoff::{ExponentialBackoff, future::retry};
//use futures_util::future::err;
//use serde::de::DeserializeOwned;
use crate::body_aggregator;
use crate::clouds;
use crate::call_messages;
use crate::common_types::*;

pub trait WithRefreshToken<T> {
    fn refresh_token(&self) -> T;
    fn refresh_token_complete(&self) -> T;
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum ListFolderCallError {
    #[error(transparent)]
    Base(clouds::BaseError),
    #[error("path not found: {0}")]
    PathNotFound(String),
//    #[error("Std error with text: {0}")]
//    Std(Box<dyn std::error::Error>),
}

impl clouds::BaseErrorAccess for ListFolderCallError {
    fn get_base_error(&self) -> Option<&clouds::BaseError> {
        match self {
            Self::Base(res) => Some(res),
            _ => None
        }
    }

    fn get_base_error_mut(&mut self) -> Option<&mut clouds::BaseError> {
        match self {
            Self::Base(res) => Some(res),
            _ => None
        }
    }

    fn from_base(base: clouds::BaseError) -> Self {
        Self::Base(base)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum DownloadFileCallError {
    #[error(transparent)]
    Base(clouds::BaseError),
    #[error("file create error (file: {file:?}, cloud file: {cloud_file:?}): {error:?}")]
    FileCreate {
        cloud_file: String,
        file: String,
        error: String,
    },
    #[error("file write error (file: {file:?}, cloud file: {cloud_file:?}): {error:?}")]
    FileWrite {
        cloud_file: String,
        file: String,
        error: String,
    },
    #[error("file sync error (file: {file:?}, cloud file: {cloud_file:?}): {error:?}")]
    FileSyncAll {
        cloud_file: String,
        file: String,
        error: String,
    },
    #[error("Download file result does not contain needed header ({header:?}), cloud file: '{cloud_file:?}', headers: [{headers:?}], status: {status:?})")]
    HeaderNotFound {
        cloud_file: String,
        status: u16,
        header: String,
        headers: String,
    },
    #[error("next data chunk error (cloud_file: {cloud_file:?}, position: {position:?}): {error:?}")]
    NextChunk {
        cloud_file: String,
        position: u64,
        error: String,
    },
    #[error("cloud file not found: {0}")]
    NotFound(String),
}

impl DownloadFileCallError {
/*
    pub fn is_permanent(&self) -> bool {
        match &self {
            Self::Base(base) => base.is_permanent(),
            Self::NextChunk {..} => false,

            Self::FileCreate {..}
            | Self::FileWrite {..}
            | Self::FileSyncAll {..}
            | Self::HeaderNotFound {..}
            => true,
        }
    }
*/
    fn e_file_create(action: &str, e: std::io::Error, cloud_file: &String, file: &String) -> Self {
        log::error!("{}(error on file save): {}, file: '{}'", action, e.to_string(), file);
        Self::FileCreate {
            cloud_file: cloud_file.clone(),
            file: file.clone(),
            error: e.to_string(),
        }
    }

    fn e_file_write(action: &str, e: std::io::Error, cloud_file: &String, file: &String) -> Self {
        log::error!("{}(error on file save): {}, file: '{}'", action, e.to_string(), file);
        Self::FileWrite {
            cloud_file: cloud_file.clone(),
            file: file.clone(),
            error: e.to_string(),
        }
    }

    fn e_file_sync_all(action: &str, e: std::io::Error, cloud_file: &String, file: &String) -> Self {
        log::error!("{}(error on sync_all): {}, file: '{}'", action, e.to_string(), file);
        Self::FileSyncAll {
            cloud_file: cloud_file.clone(),
            file: file.clone(),
            error: e.to_string(),
        }
    }

    fn e_header_not_found(action: &str, header: &str, headers: &String, cloud_file: &String, status: u16) -> Self {
        log::error!("{}(result does not contain needed header ({})), headers: {}, cloud file: '{}'", action, header, headers, cloud_file);
        Self::HeaderNotFound {
            cloud_file: cloud_file.clone(),
            status,
            header: header.to_string(),
            headers: headers.clone(),
        }
    }

    fn e_next_chunk(action: &str, e: hyper::Error, cloud_file: &String, position: u64) -> Self {
        log::error!("{}(error on getting next chunk of data): {}, cloud file: '{}'", action, e.to_string(), cloud_file);
        Self::NextChunk {
            cloud_file: cloud_file.clone(),
            position,
            error: e.to_string(),
        }
    }
}

impl clouds::BaseErrorAccess for DownloadFileCallError {
    fn get_base_error(&self) -> Option<&clouds::BaseError> {
        match self {
            Self::Base(res) => Some(res),
            _ => None
        }
    }
/*
    fn get_base_error_mut(&mut self) -> Option<&mut (&mut BaseCloudError)> {
        match self {
            Self::Base(res) => Some(&mut res),
            _ => None
        }
    }
*/
    fn get_base_error_mut(&mut self) -> Option<&mut clouds::BaseError> {
        match self {
            Self::Base(res) => Some(res),
            _ => None
        }
    }

    fn from_base(base: clouds::BaseError) -> Self {
        Self::Base(base)
    }
}

#[derive(Default, Serialize, Debug, Clone)]
pub struct ListFolderParams {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recursive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_deleted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_has_explicit_shared_members: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_mounted_folders: Option<bool>, //  = true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>, //  = min=1, max=2000)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_non_downloadable_files: Option<bool>,
}

#[derive(Default, Serialize, Debug, Clone)]
pub struct ListFolderContinueParams {
    pub cursor: String,
}

#[derive(Deserialize, Debug)]
pub struct FileMeta {
    pub name: String,
    pub path_lower: String,
    pub path_display: String,
    pub id: String,
    pub client_modified: DateTime<Utc>,
    pub server_modified: DateTime<Utc>,
    #[serde(default)]
    pub rev: String,
    #[serde(default)]
    pub size: u64,
    //    parent_shared_folder_id: string;
    //    media_info:
    //    symlink_info:
    //    sharing_info:
    #[serde(default)]
    pub is_downloadable: bool,
    //    export_info:
    //    property_groups:
    //    #[serde(default)]
    //    pub has_explicit_shared_members: bool,
    #[serde(default)]
    pub content_hash: String,
}

#[derive(Deserialize, Debug)]
pub struct FolderMeta {
    pub name: String,
    pub path_lower: String,
    pub path_display: String,
    pub id: String,
//    pub client_modified: DateTime<Utc>,
//    pub server_modified: DateTime<Utc>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = ".tag")]
pub enum Meta {
    #[serde(rename = "file")]
    File(FileMeta),
    #[serde(rename = "folder")]
    Folder(FolderMeta),
}

#[derive(Default, Deserialize, Debug)]
pub struct ListFolderCallResult {
    pub entries: Vec<Meta>,
    pub cursor: String,
    pub has_more: bool,
}

#[derive(Default, Deserialize, Debug)]
pub struct CodeToTokensResult {
    pub access_token: String,
    pub expires_in: i32,
    pub token_type: String,
    pub scope: String,
    pub refresh_token: String,
    pub account_id: String,
    pub uid: String,
}

#[derive(Default, Deserialize, Debug)]
pub struct RefreshTokenResult {
    pub access_token: String,
    pub expires_in: i32,
    pub token_type: String,
}

#[derive(Default, Serialize, Debug, Clone)]
pub struct DownloadFileParams {
    pub path: String,
    #[serde(skip_serializing)]
    pub save_to: String,
}

#[derive(Deserialize, Debug)]
pub struct DownloadFileCallResult {
    pub name: String,
    pub id: String,
    pub client_modified: DateTime<Utc>,
    pub server_modified: DateTime<Utc>,
    pub rev: String,
    pub size: u64,
    pub path_lower: String,
    pub path_display: String,
//        #[serde(default)]
//        pub file_data: Vec<u8>,
}

/*
// https://www.dropbox.com/developers/documentation/http/documentation#files-list_folder
LookupError
malformed_path: String?
not_found
not_file
not_folder
restricted_content
unsupported_content_type
locked
*/

#[derive(Deserialize, Debug)]
#[serde(tag = ".tag")]
pub enum BaseErrorTag {
    Unknown,
    #[serde(rename = "expired_access_token")]
    ExpiredAccessToken,
}

#[derive(Deserialize, Debug)]
#[serde(tag = ".tag")]
pub enum ListFolderPathErrorTag {
    #[serde(rename = "not_found")]
    NotFound,
}

#[derive(Deserialize, Debug)]
#[serde(tag = ".tag")]
pub enum ListFolderErrorTag {
    #[serde(rename = "path")]
    Path { path: ListFolderPathErrorTag },
}

#[derive(Deserialize, Debug)]
#[serde(tag = ".tag")]
pub enum DownloadFilePathErrorTag {
    #[serde(rename = "not_found")]
    NotFound,
}

#[derive(Deserialize, Debug)]
#[serde(tag = ".tag")]
pub enum DownloadFileErrorTag {
    #[serde(rename = "path")]
    Path { path: DownloadFilePathErrorTag },
}

#[derive(Deserialize, Debug)]
#[serde(tag = ".tag")]
pub enum RefreshTokenErrorTag {
//    #[serde(rename = "_")]
    NotImplemented,
}

#[derive(Deserialize, Debug)]
#[serde(tag = ".tag")]
pub enum AuthCodeToTokenErrorTag {
//    #[serde(rename = "_")]
    NotImplemented,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum ErrorTag<T> {
    Base(BaseErrorTag),
    Routine(T) // ListFolderErrorTag, etc
}

impl<T> Default for ErrorTag<T> {
    fn default() -> Self {
        Self::Base(BaseErrorTag::Unknown)
    }
}

#[derive(Default, Deserialize, Debug)]
pub struct ApiCallError<T> {
    pub error_summary: String,
//    #[serde(flatten)]
    pub error: ErrorTag<T>
}

fn https_connector() -> hyper_rustls::HttpsConnector<hyper::client::HttpConnector> {
    let mut http_connector = hyper::client::HttpConnector::new();
    http_connector.enforce_http(false);
//        http_connector.set_keepalive(Some(std::time::Duration::from_secs(60)));

    hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
//        .https_or_http()
        .https_only()
//        .enable_http1()
        .enable_http2()
        .wrap_connector(http_connector)
}

fn client_http2() -> Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>> {
    let https = https_connector();
    Client::builder().http2_only(true).build::<_, Body>(https)
}

/*
fn request_builder() -> http::request::Builder {
    hyper::Request::builder()
}
*/

fn request_builder_http2() -> http::request::Builder {
    hyper::Request::builder().version(http::version::Version::HTTP_2)
}

async fn extract_error_body(response: http::Response<hyper::body::Body>) -> Result<String, Box<dyn Error>> {
    use bytes::Buf;// for body.reader()

    let body_res = body_aggregator::async_aggregate(response).await;
    if body_res.is_ok() {
        let body = body_res.unwrap();
        let mut v = Vec::new();
        let res = body.reader().read_to_end(&mut v);
        if res.is_ok() {
            let error = String::from_utf8_lossy(&v).to_string();
            Ok(error)
        }
        else {
            Err(res.err().unwrap().into())
        }
    }
    else {
        Err(body_res.err().unwrap().into())
    }
}

async fn create_error_from_body<T, TRoutine, CF>(response: http::Response<hyper::body::Body>, action: &str, status: u16, convert_f: CF) -> T
where
    T: clouds::BaseErrorAccess,
    TRoutine: serde::de::DeserializeOwned,
//    ApiCallErrorType: ApiCallError<TRoutine>,
    CF: FnOnce(TRoutine) -> T
{
    let res: Result<(), T> = extract_error_body(response).await.map_err(|e| T::from_base(clouds::BaseError::e_error_body_aggregate(action, e))
    ).and_then(|error_data| {
        if status == 400 {
            if error_data.contains("The given OAuth 2 access token is malformed") {
                Err(T::from_base(clouds::BaseError::e_access_token_malformed(action)))
            }
            else {
                Err(T::from_base(clouds::BaseError::e_unknown_api_error_result(action, status, &error_data)))
            }
        } else {
            (serde_json::from_str(&error_data) as Result<ApiCallError<TRoutine>, serde_json::Error>).map_err(|e| T::from_base(clouds::BaseError::e_error_body_deserialization(action, e, &error_data))
            ).and_then(|err_data| {
                let res =  match err_data.error {
                    ErrorTag::Base(BaseErrorTag::ExpiredAccessToken) => {
                        log::error!("{}(ErrorTag::ExpiredAccessToken)", action);
                        T::from_base(clouds::BaseError::ExpiredAccessToken { refresh_error: None })
                    },
                    ErrorTag::Base(BaseErrorTag::Unknown) => {
                        log::error!("{}(ErrorTag::Unknown), error_summary: {}", action, err_data.error_summary.clone());
                        T::from_base(clouds::BaseError::UnknownApiErrorStructure {
                            action: action.to_owned(),
                            hint: "".to_owned(),
                            error: err_data.error_summary.clone(),
                        })
                    },
                    ErrorTag::Routine(e) => {
                        convert_f(e)
                    }
                };
                Err(res)
                //convert_error(action, &err_data, convert_f)
            })
        }
    });
    res.err().unwrap()
}

async fn refresh_token_impl(token: &String, auth_info_holder: &clouds::AuthInfoHolder) -> Result<String, clouds::RefreshTokenCallError> {
    log::info!("refresh_token_impl");
    let lock: Arc<RwLock<u64>> = {
        (auth_info_holder.read().await).write_mutex.clone()
    };
    let _guard = lock.write().await;
    {
        let after = {
            (auth_info_holder.read().await).clone()
        };
        if token.eq(&after.token) {
            let res = refresh_token(after.refresh_token, after.client_id, after.secret).await;
            match res {
                Ok(r) => {
                    let info_ref = &mut *auth_info_holder.write().await;
                    info_ref.token = r.access_token.clone();
                    Ok(r.access_token)
                },
                Err(e) => Err(e)
            }
        } else {
            Ok(after.token)
        }
    }
}

// https://serde.rs/stream-array.html
async fn int_list_folder(
    auth_info_holder: clouds::AuthInfoHolder,
    params: ListFolderParams,
    call_id: u64,
    events: EventsSender<call_messages::Message>,
) -> Result<ListFolderCallResult, ListFolderCallError> {

    let token = {
        auth_info_holder.read().await.token.clone()
    };

    let action = "dropbox.list_folder";
    let serialized = serde_json::to_string(&params).unwrap();
    let path = params.path.clone();

    let client = client_http2();
    let req = request_builder_http2()
        .method("POST")
        .header("Authorization", "Bearer ".to_owned() + &token)
        .header("Content-Type", "application/json")
        .uri("https://api.dropboxapi.com/2/files/list_folder")
        .body(Body::from(serialized))
        .expect("request builder");

    process_simple_request(client, req, action, |tr| {
        match tr {
            ListFolderErrorTag::Path { path: ListFolderPathErrorTag::NotFound } => ListFolderCallError::PathNotFound(path)
        }
    }).await
}

struct ListFolderTokenEventsCreator {
    call_id: u64,
}

impl WithRefreshToken<call_messages::Message> for ListFolderTokenEventsCreator {
    fn refresh_token(&self) -> call_messages::Message {
        call_messages::Message { call_id: self.call_id, data: call_messages::Data::ListFolder(call_messages::ListFolder::RefreshToken) }
    }

    fn refresh_token_complete(&self) -> call_messages::Message {
        call_messages::Message { call_id: self.call_id, data: call_messages::Data::ListFolder(call_messages::ListFolder::RefreshTokenComplete) }
    }
}

struct DownloadFileTokenEventsCreator {
    call_id: u64,
}

impl WithRefreshToken<call_messages::Message> for DownloadFileTokenEventsCreator {
    fn refresh_token(&self) -> call_messages::Message {
        call_messages::Message { call_id: self.call_id, data: call_messages::Data::DownloadFile(call_messages::DownloadFile::RefreshToken) }
    }

    fn refresh_token_complete(&self) -> call_messages::Message {
        call_messages::Message { call_id: self.call_id, data: call_messages::Data::DownloadFile(call_messages::DownloadFile::RefreshTokenComplete) }
    }
}

pub async fn list_folder(
    auth_info_holder: clouds::AuthInfoHolder,
    params: ListFolderParams,
    call_id: u64,
    events: EventsSender<call_messages::Message>,
) -> Result<ListFolderCallResult, ListFolderCallError> {
    call_with_token_auto_refresh(auth_info_holder.clone(), events.clone(), ListFolderTokenEventsCreator {call_id}, || async {
        int_list_folder(auth_info_holder.clone(), params.clone(), call_id, events.clone()).await
    }).await
}

pub async fn int_download_file(
    auth_info_holder: clouds::AuthInfoHolder,
    params: DownloadFileParams,
    call_id: u64,
    events: EventsSender<call_messages::Message>,
) -> Result<DownloadFileCallResult, DownloadFileCallError> {
//    type ResType = DownloadFileCallResult;
//    type ErrType = DownloadFileCallError;
    use http_body::Body; // for body.data()
    use call_messages::Message;
    use call_messages::Data::DownloadFile as MsgData;
    use call_messages::DownloadFile as msg;

    let token = {
        auth_info_holder.read().await.token.clone()
    };

    let action = "dropbox.download_file";
    let serialized = serde_json::to_string(&params).unwrap();
    let file_path = params.path.clone();

    let client = client_http2();
    let req = request_builder_http2()
        .method("POST")
        .header("Authorization", "Bearer ".to_owned() + &token)
        .header("Dropbox-API-Arg", serialized)
        .uri("https://content.dropboxapi.com/2/files/download")
        .body(hyper::Body::empty())
        .expect("request builder");

    let start = Instant::now();

//    state.send(CallState::WaitingForResponse);
    let mut bytes_recieved: u64 = 0;
    let res = match client.request(req).await {
        Ok(response) => {
            let status = response.status().as_u16();
            if response.status().is_success() {
                let mut headers_str = String::new();
                for h in response.headers() {
                    headers_str.push_str(&(h.0.to_string() + ":" + &(h.1.to_str().unwrap().to_string()) + ", "));
                }
                let header_name = "dropbox-api-result";
                let header = response.headers().get(header_name);
                if header.is_some() {
                    let header_value = header.unwrap();
                    match serde_json::from_slice(header_value.as_bytes()) as Result<DownloadFileCallResult, serde_json::Error> {
                        Ok(final_res) => {
                            use std::io::prelude::*;
                            let file_size = final_res.size;
                            let _ = events.send(Message { call_id, data: MsgData(msg::SizeInfo { size: Some(file_size) }) });
                            match std::fs::File::create(&params.save_to) {
                                Ok(mut f) => {
                                    let mut tmp_res: Result<DownloadFileCallResult, DownloadFileCallError> = Ok(final_res);
                                    let (_, mut body) = response.into_parts();
                                    while let Some(next) = body.data().await {
                                        match next {
                                            Ok(chunk) => {
//                                                    log::info!("chunk size: {}", chunk.len().to_string());
                                                bytes_recieved = bytes_recieved + chunk.len() as u64;
                                                let _ = events.send(Message { call_id, data: MsgData(msg::Progress { value: bytes_recieved }) });
                                                if let Err(e) = f.write_all(&chunk) {
                                                    tmp_res = Err(DownloadFileCallError::e_file_write(&action, e, &params.path, &params.save_to));
                                                    break;
                                                }
                                            },
                                            Err(e) => {
                                                tmp_res = Err(DownloadFileCallError::e_next_chunk(&action, e, &params.path, bytes_recieved));
                                                break;
                                            }
                                        }
                                    }
                                    if tmp_res.is_ok() {
                                        log::info!("chunks done");
                                        if let Err(e) = f.sync_all() {
                                            tmp_res = Err(DownloadFileCallError::e_file_sync_all(&action, e, &params.path, &params.save_to));
                                        }
                                    }
                                    tmp_res
                                }
                                Err(e) => Err(DownloadFileCallError::e_file_create(&action, e, &params.path, &params.save_to))
                            }
                        },
                        Err(e) => Err(DownloadFileCallError::Base(clouds::BaseError::e_response_body_deserialization(&action, e)))
                    }
                }
                else {
                    Err(DownloadFileCallError::e_header_not_found(&action, header_name, &headers_str, &params.path, status))
                }
            } else {
                let err = create_error_from_body(response, &action, status, |e| {
                   match e {
                       DownloadFileErrorTag::Path { path: DownloadFilePathErrorTag::NotFound } => DownloadFileCallError::NotFound(file_path),
                   }
                }).await;
                Err(err)
//                Err(DownloadFileCallError::Base(err))
            }
        },
        Err(e) => Err(DownloadFileCallError::Base(clouds::BaseError::e_response_wait(action, &e)))
    };

    let total = start.elapsed();
    if total.as_secs() !=0 {
        log::info!("downloaded {} kbytes at {:#?}, speed: {} kbytes/sec", (bytes_recieved / 1024).to_string(), total, (bytes_recieved / 1024 /total.as_secs()).to_string())
    }
    else {
        log::info!("downloaded {} kbytes at {:#?}", (bytes_recieved / 1024).to_string(), total)
    }
    res
}

pub async fn download_file(
    auth_info_holder: clouds::AuthInfoHolder,
    params: DownloadFileParams,
    call_id: u64,
    events: EventsSender<call_messages::Message>,
) -> Result<DownloadFileCallResult, DownloadFileCallError> {
    call_with_token_auto_refresh(auth_info_holder.clone(), events.clone(), DownloadFileTokenEventsCreator {call_id}, || async {
        int_download_file(auth_info_holder.clone(), params.clone(), call_id, events.clone()).await
    }).await
}

async fn call_with_token_auto_refresh<ResType, ErrType, Fn, Fut, T>(
    auth_info_holder: clouds::AuthInfoHolder,
    events: EventsSender<T>,
    //call_id: u64,
    events_creator: impl WithRefreshToken<T>,
    func: Fn
) -> Fut::Output
    where
        ErrType: clouds::BaseErrorAccess,
        Fn: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<ResType, ErrType>>,
{
    let token = {
        auth_info_holder.read().await.token.clone()
    };
    let mut f = func;
    let final_res = match f().await {
        Ok(res) => Ok(res),
        Err(mut e) => {
            if let Some(base) = e.get_base_error() {
                if base.is_expired_token() {
                    let _ = events.send(events_creator.refresh_token());
                    match refresh_token_impl(&token.clone(), &auth_info_holder.clone()).await {
                        Ok(_token) => {
                            let _ = events.send(events_creator.refresh_token_complete());
                            f().await
                        },
                        Err(refresh_e) => {
                            let base_error = e.get_base_error_mut();
                            if base_error.is_some() {
                                let base = base_error.unwrap();
                                if let clouds::BaseError::ExpiredAccessToken{ ref mut refresh_error } = base {
                                    *refresh_error = Some(Box::new(refresh_e));
                                    Err(e)
                                } else {
                                    panic!()
                                }
                            } else {
                                panic!()
                            }
                        }
                    }

                }
                else {
                    Err(e)
                }
            }
            else {
                Err(e)
            }
        }
    };
//    state.send(CallState::Done);
    final_res
}

async fn process_simple_request<ResType, ErrorType, TRoutine, CF>(
    client: Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
    req: http::Request<hyper::Body>,
    action: &str,
    convert_f: CF
) -> Result<ResType, ErrorType>
    where
        ResType: serde::de::DeserializeOwned,
        ErrorType: clouds::BaseErrorAccess,
        CF: FnOnce(TRoutine) -> ErrorType,
        TRoutine: serde::de::DeserializeOwned
{
    use bytes::Buf;// for buf.reader()
    match client.request(req).await {
        Ok(response) => {
            let status = response.status().as_u16();
//                    if status == http::StatusCode::OK {
            if response.status().is_success() {
                match body_aggregator::async_aggregate(response).await {
                    Ok(buf) => {
                        match serde_json::from_reader(buf.reader()) as Result<ResType, serde_json::Error> {
                            Ok(res) => Ok(res),
                            Err(e) => {
                                Err(ErrorType::from_base(clouds::BaseError::e_response_body_deserialization(action, e)))
                            }
                        }
                    },
                    Err(e) => Err(ErrorType::from_base(clouds::BaseError::e_response_body_aggregate(action, e)))
                }
            }
            else {
                Err(create_error_from_body(response, &action, status, convert_f).await)
            }
        },
        Err(e) => Err(ErrorType::from_base(clouds::BaseError::e_response_wait(action, &e)))
    }
}

// https://www.dropbox.com/developers/documentation/http/documentation
// https://curl.se/docs/manpage.html
pub async fn auth_code_to_tokens(code: String, redirect_uri: String, client_id: String, secret: String) -> Result<CodeToTokensResult, clouds::AuthCodeToTokensCallError>
{
    type ResType = CodeToTokensResult;
    type ErrorType = clouds::AuthCodeToTokensCallError;
    type FuncRes = Result<ResType, ErrorType>;
    let action = "dropbox.auth_code_to_tokens";

    let exec = || async {
        let client = client_http2();
        let body = form_urlencoded::Serializer::new(String::new())
            .append_pair("code", &code)
            .append_pair("grant_type", "authorization_code")
            .append_pair("redirect_uri", &redirect_uri)
            .finish();
        let req = request_builder_http2()
            .method("POST")
            .header("Authorization", "Basic ".to_string() + &base64::encode(format!("{}:{}", client_id, secret).as_bytes()))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .uri("https://api.dropbox.com/oauth2/token")
            .body(Body::from(body))
            .expect("request builder");

        let res: FuncRes = process_simple_request(client, req, action, |tr| {
            match tr {
                AuthCodeToTokenErrorTag::NotImplemented => clouds::AuthCodeToTokensCallError::Other("some AuthCodeToTokensCallError error".to_string())
            }
        }).await;

        //process_oauth2type_request(client, req, action).await;
        let res = res.map_err(|e| {
            match e.is_permanent() {
                true => backoff::Error::Permanent(e),
                false => backoff::Error::Transient{err: e, retry_after: Some(std::time::Duration::from_millis(100))}
            }
        });
        res
    };

    let mut backoff = ExponentialBackoff::default();
    backoff.max_interval = Duration::from_secs(1);
    backoff.max_elapsed_time = Some(Duration::from_secs(1));
    retry(backoff, exec).await
}

#[derive(Serialize, Deserialize, Debug)]
struct Oauth2Code400ErrorData {
    error_description: String,
    error: String
}

pub async fn refresh_token(refresh_token: String, client_id: String, secret: String) -> Result<RefreshTokenResult, clouds::RefreshTokenCallError> {
    type ResType = RefreshTokenResult;
    type ErrorType = clouds::RefreshTokenCallError;
    type FuncRes = Result<ResType, ErrorType>;
    let action = "dropbox.refresh_token";

    let exec = || async {
        let client = client_http2();
        let body = form_urlencoded::Serializer::new(String::new())
            .append_pair("refresh_token", &refresh_token)
            .append_pair("grant_type", "refresh_token")
            .finish();
        let req = request_builder_http2()
            .method("POST")
            .header("Authorization", "Basic ".to_string() + &base64::encode(format!("{}:{}", client_id, secret).as_bytes()))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .uri("https://api.dropbox.com/oauth2/token")
            .body(Body::from(body))
            .expect("request builder");


        let res: FuncRes = process_simple_request(client, req, action, |tr| {
            let res =
                match tr {
                    RefreshTokenErrorTag::NotImplemented => clouds::RefreshTokenCallError::InvalidRefreshToken_
                };
            res
        }).await;

        //process_oauth2type_request(client, req, action).await;
        let res = res.map_err(|e| {
            match e.is_permanent() {
                true => backoff::Error::Permanent(e),
                false => backoff::Error::Transient{err: e, retry_after: Some(std::time::Duration::from_millis(100))}
            }
        });
        res
    };

    let mut backoff = ExponentialBackoff::default();
    backoff.max_interval = Duration::from_secs(1);
    backoff.max_elapsed_time = Some(Duration::from_secs(1));
    retry(backoff, exec).await
}