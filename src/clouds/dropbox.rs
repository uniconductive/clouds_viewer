use crate::call_messages;
use crate::clouds;
use crate::common_types::*;
use backoff::{future::retry, ExponentialBackoff};
use chrono::prelude::*;
use hyper::{Body, Client};
use serde::{Deserialize, Serialize};
use std::{
    result::Result,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

// https://www.dropbox.com/developers/documentation/http/documentation
/*
    good paths: "", "/folder"
    bad paths: "/";  "folder"
*/

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
}

impl clouds::BaseErrorAccess for ListFolderCallError {
    fn get_base_error(&self) -> Option<&clouds::BaseError> {
        match self {
            Self::Base(res) => Some(res),
            _ => None,
        }
    }

    fn get_base_error_mut(&mut self) -> Option<&mut clouds::BaseError> {
        match self {
            Self::Base(res) => Some(res),
            _ => None,
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
    #[error(
        "Download file result does not contain needed header ({header:?}), \
        cloud file: '{cloud_file:?}', headers: [{headers:?}], status: {status:?})"
    )]
    HeaderNotFound {
        cloud_file: String,
        status: u16,
        header: String,
        headers: String,
    },
    #[error(
        "next data chunk error (cloud_file: {cloud_file:?}, position: {position:?}): {error:?}"
    )]
    NextChunk {
        cloud_file: String,
        position: u64,
        error: String,
    },
    #[error("cloud file not found: {0}")]
    NotFound(String),
}

impl DownloadFileCallError {
    fn e_file_create(action: &str, e: std::io::Error, cloud_file: &str, file: &str) -> Self {
        log::error!(
            "{}(error on file save): {}, file: '{}'",
            action,
            e.to_string(),
            file
        );
        Self::FileCreate {
            cloud_file: cloud_file.to_owned(),
            file: file.to_owned(),
            error: e.to_string(),
        }
    }

    fn e_file_write(action: &str, e: std::io::Error, cloud_file: &str, file: &str) -> Self {
        log::error!(
            "{}(error on file save): {}, file: '{}'",
            action,
            e.to_string(),
            file
        );
        Self::FileWrite {
            cloud_file: cloud_file.to_owned(),
            file: file.to_owned(),
            error: e.to_string(),
        }
    }

    fn e_file_sync_all(action: &str, e: std::io::Error, cloud_file: &str, file: &str) -> Self {
        log::error!(
            "{}(error on sync_all): {}, file: '{}'",
            action,
            e.to_string(),
            file
        );
        Self::FileSyncAll {
            cloud_file: cloud_file.to_owned(),
            file: file.to_owned(),
            error: e.to_string(),
        }
    }

    fn e_header_not_found(
        action: &str,
        header: &str,
        headers: &str,
        cloud_file: &str,
        status: u16,
    ) -> Self {
        log::error!(
            "{}(result does not contain needed header ({})), headers: {}, cloud file: '{}'",
            action,
            header,
            headers,
            cloud_file
        );
        Self::HeaderNotFound {
            cloud_file: cloud_file.to_owned(),
            status,
            header: header.to_owned(),
            headers: headers.to_owned(),
        }
    }

    fn e_next_chunk(action: &str, e: hyper::Error, cloud_file: &str, position: u64) -> Self {
        log::error!(
            "{}(error on getting next chunk of data): {}, cloud file: '{}'",
            action,
            e.to_string(),
            cloud_file
        );
        Self::NextChunk {
            cloud_file: cloud_file.to_owned(),
            position,
            error: e.to_string(),
        }
    }
}

impl clouds::BaseErrorAccess for DownloadFileCallError {
    fn get_base_error(&self) -> Option<&clouds::BaseError> {
        match self {
            Self::Base(res) => Some(res),
            _ => None,
        }
    }

    fn get_base_error_mut(&mut self) -> Option<&mut clouds::BaseError> {
        match self {
            Self::Base(res) => Some(res),
            _ => None,
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
    #[serde(default)]
    pub is_downloadable: bool,
    #[serde(default)]
    pub content_hash: String,
}

#[derive(Deserialize, Debug)]
pub struct FolderMeta {
    pub name: String,
    pub path_lower: String,
    pub path_display: String,
    pub id: String,
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
}

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
    NotImplemented,
}

#[derive(Deserialize, Debug)]
#[serde(tag = ".tag")]
pub enum AuthCodeToTokenErrorTag {
    NotImplemented,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum ErrorTag<T> {
    Base(BaseErrorTag),
    Routine(T), // ListFolderErrorTag, etc
}

impl<T> Default for ErrorTag<T> {
    fn default() -> Self {
        Self::Base(BaseErrorTag::Unknown)
    }
}

#[derive(Default, Deserialize, Debug)]
pub struct ApiCallError<T> {
    pub error_summary: String,
    pub error: ErrorTag<T>,
}

fn https_connector() -> hyper_rustls::HttpsConnector<hyper::client::HttpConnector> {
    let mut http_connector = hyper::client::HttpConnector::new();
    http_connector.enforce_http(false);

    hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_only()
        .enable_http2()
        .wrap_connector(http_connector)
}

fn client_http2(
    rth: &RuntimeHolder,
) -> Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>> {
    let https = https_connector();
    Client::builder()
        .http2_only(true)
        .executor(rth.clone())
        .build::<_, Body>(https)
}

fn request_builder_http2() -> http::request::Builder {
    hyper::Request::builder().version(http::version::Version::HTTP_2)
}

#[derive(Serialize, Deserialize, Debug)]
struct Code400ErrorData {
    error: String,
    error_description: String,
}

fn create_error_from_body_data<T, TRoutine, CF>(
    body_data: &[u8],
    action: &str,
    status: u16,
    convert_f: CF,
) -> T
where
    T: clouds::BaseErrorAccess,
    TRoutine: serde::de::DeserializeOwned,
    CF: FnOnce(TRoutine) -> T,
{
    let error_data: String = String::from_utf8_lossy(body_data).to_string();
    if status == 400 {
        match serde_json::from_str::<Code400ErrorData>(&error_data) {
            Ok(data) => {
                if data.error == "invalid_grant"
                    && ["refresh token is malformed", "refresh token is invalid or revoked"]
                    .contains(&data.error_description.as_str())
                {
                    T::from_base(clouds::BaseError::e_refresh_token_malformed(action))
                } else {
                    T::from_base(clouds::BaseError::e_unknown_api_error_result(
                        action,
                        status,
                        &error_data,
                    ))
                }
            }
            Err(_) => {
                if error_data.contains("The given OAuth 2 access token is malformed") {
                    T::from_base(clouds::BaseError::e_access_token_malformed(action))
                } else if error_data.contains("invalid_client: Invalid client_id or client_secret")
                {
                    T::from_base(clouds::BaseError::e_invalid_client_id_or_client_secret(
                        action,
                    ))
                } else if error_data
                    .contains(r#"Invalid authorization value in HTTP header "Authorization": "#)
                {
                    T::from_base(clouds::BaseError::e_invalid_authorization_value(action))
                } else {
                    T::from_base(clouds::BaseError::e_unknown_api_error_result(
                        action,
                        status,
                        &error_data,
                    ))
                }
            }
        }
    } else {
        serde_json::from_str::<ApiCallError<TRoutine>>(&error_data)
            .map_err(|e| {
                T::from_base(clouds::BaseError::e_error_body_deserialization(
                    action,
                    e,
                    &error_data,
                ))
            })
            .and_then::<ApiCallError<TRoutine>, _>(|err_data| {
                Err(match err_data.error {
                    ErrorTag::Base(BaseErrorTag::ExpiredAccessToken) => {
                        log::error!("{}(ErrorTag::ExpiredAccessToken)", action);
                        T::from_base(clouds::BaseError::ExpiredAccessToken {
                            refresh_error: None,
                        })
                    }
                    ErrorTag::Base(BaseErrorTag::Unknown) => {
                        log::error!(
                            "{}(ErrorTag::Unknown), error_summary: {}",
                            action,
                            err_data.error_summary
                        );
                        T::from_base(clouds::BaseError::UnknownApiErrorStructure {
                            action: action.to_owned(),
                            hint: "".to_owned(),
                            error: err_data.error_summary,
                        })
                    }
                    ErrorTag::Routine(e) => convert_f(e),
                })
            })
            .err()
            .unwrap()
    }
}

async fn refresh_token_impl(
    rth: RuntimeHolder,
    token: &str,
    auth_info_holder: &clouds::AuthInfoHolder,
) -> Result<String, clouds::RefreshTokenCallError> {
    log::debug!("refresh_token_impl");
    let lock: Arc<RwLock<u64>> = { (auth_info_holder.read().await).write_mutex.clone() };
    let _guard = lock.write().await;
    {
        let after = { (auth_info_holder.read().await).clone() };
        if token.eq(&after.token) {
            let res = refresh_token(rth, after.refresh_token, after.client_id, after.secret).await;
            match res {
                Ok(r) => {
                    let info_ref = &mut *auth_info_holder.write().await;
                    info_ref.token = r.access_token.clone();
                    Ok(r.access_token)
                }
                Err(e) => Err(e),
            }
        } else {
            Ok(after.token)
        }
    }
}

async fn int_list_folder(
    rth: &RuntimeHolder,
    auth_info_holder: clouds::AuthInfoHolder,
    params: ListFolderParams,
    _call_id: u64,
    _messages: MessagesSender<call_messages::Message>,
) -> Result<ListFolderCallResult, ListFolderCallError> {
    let token = { auth_info_holder.read().await.token.clone() };

    let action = "dropbox.list_folder";
    let serialized = serde_json::to_string(&params).unwrap();
    let path = params.path.clone();

    let client = client_http2(rth);
    let req = request_builder_http2()
        .method("POST")
        .header("Authorization", "Bearer ".to_owned() + &token)
        .header("Content-Type", "application/json")
        .uri("https://api.dropboxapi.com/2/files/list_folder")
        .body(Body::from(serialized))
        .expect("request builder");

    process_simple_request(client, req, action, |tr| match tr {
        ListFolderErrorTag::Path {
            path: ListFolderPathErrorTag::NotFound,
        } => ListFolderCallError::PathNotFound(path),
    })
    .await
}

async fn int_list_folder_continue(
    rth: &RuntimeHolder,
    auth_info_holder: clouds::AuthInfoHolder,
    params: ListFolderContinueParams,
    _call_id: u64,
    _messages: MessagesSender<call_messages::Message>,
) -> Result<ListFolderCallResult, ListFolderCallError> {
    let token = { auth_info_holder.read().await.token.clone() };

    let action = "dropbox.list_folder_continue";
    let serialized = serde_json::to_string(&params).unwrap();

    let client = client_http2(rth);
    let req = request_builder_http2()
        .method("POST")
        .header("Authorization", "Bearer ".to_owned() + &token)
        .header("Content-Type", "application/json")
        .uri("https://api.dropboxapi.com/2/files/list_folder/continue")
        .body(Body::from(serialized))
        .expect("request builder");

    process_simple_request(client, req, action, |tr| match tr {
        ListFolderErrorTag::Path {
            path: ListFolderPathErrorTag::NotFound,
        } => ListFolderCallError::PathNotFound("".into()),
    })
    .await
}

struct ListFolderTokenMessageCreator {
    call_id: u64,
}

impl WithRefreshToken<call_messages::Message> for ListFolderTokenMessageCreator {
    fn refresh_token(&self) -> call_messages::Message {
        call_messages::Message {
            call_id: Some(self.call_id),
            data: call_messages::Data::ListFolder(call_messages::ListFolder::RefreshToken),
        }
    }

    fn refresh_token_complete(&self) -> call_messages::Message {
        call_messages::Message {
            call_id: Some(self.call_id),
            data: call_messages::Data::ListFolder(call_messages::ListFolder::RefreshTokenComplete),
        }
    }
}

struct DownloadFileTokenMessageCreator {
    call_id: u64,
}

impl WithRefreshToken<call_messages::Message> for DownloadFileTokenMessageCreator {
    fn refresh_token(&self) -> call_messages::Message {
        call_messages::Message {
            call_id: Some(self.call_id),
            data: call_messages::Data::DownloadFile(call_messages::DownloadFile::RefreshToken),
        }
    }

    fn refresh_token_complete(&self) -> call_messages::Message {
        call_messages::Message {
            call_id: Some(self.call_id),
            data: call_messages::Data::DownloadFile(
                call_messages::DownloadFile::RefreshTokenComplete,
            ),
        }
    }
}

pub async fn list_folder(
    rth: RuntimeHolder,
    auth_info_holder: clouds::AuthInfoHolder,
    params: ListFolderParams,
    call_id: u64,
    messages: MessagesSender<call_messages::Message>,
) -> Result<ListFolderCallResult, ListFolderCallError> {
    let rth_clone = rth.clone();
    call_with_token_auto_refresh(
        rth,
        auth_info_holder.clone(),
        messages.clone(),
        ListFolderTokenMessageCreator { call_id },
        || async {
            int_list_folder(
                &rth_clone,
                auth_info_holder.clone(),
                params.clone(),
                call_id,
                messages.clone(),
            )
            .await
        },
    )
    .await
}

pub async fn list_folder_continue(
    rth: RuntimeHolder,
    auth_info_holder: clouds::AuthInfoHolder,
    params: ListFolderContinueParams,
    call_id: u64,
    messages: MessagesSender<call_messages::Message>,
) -> Result<ListFolderCallResult, ListFolderCallError> {
    let rth_clone = rth.clone();
    call_with_token_auto_refresh(
        rth,
        auth_info_holder.clone(),
        messages.clone(),
        ListFolderTokenMessageCreator { call_id },
        || async {
            int_list_folder_continue(
                &rth_clone,
                auth_info_holder.clone(),
                params.clone(),
                call_id,
                messages.clone(),
            )
            .await
        },
    )
    .await
}

pub async fn int_download_file(
    rth: &RuntimeHolder,
    auth_info_holder: clouds::AuthInfoHolder,
    params: DownloadFileParams,
    call_id: u64,
    messages: MessagesSender<call_messages::Message>,
) -> Result<DownloadFileCallResult, DownloadFileCallError> {
    use call_messages::Data::DownloadFile as MsgData;
    use call_messages::DownloadFile as msg;
    use call_messages::Message;
    use http_body::Body; // for body.data()

    type ResultType = DownloadFileCallResult;
    type ErrorType = DownloadFileCallError;

    let token = { auth_info_holder.read().await.token.clone() };

    let action = "dropbox.download_file";
    let serialized = serde_json::to_string(&params).unwrap();
    let file_path = params.path.clone();

    let client = client_http2(rth);
    let req = request_builder_http2()
        .method("POST")
        .header("Authorization", "Bearer ".to_owned() + &token)
        .header("Dropbox-API-Arg", serialized)
        .uri("https://content.dropboxapi.com/2/files/download")
        .body(hyper::Body::empty())
        .expect("request builder");

    let start = Instant::now();

    //    state.send(CallState::WaitingForResponse);
    let mut bytes_received: u64 = 0;
    let res = match client.request(req).await {
        Ok(response) => {
            let status = response.status().as_u16();
            if response.status().is_success() {
                let mut headers_str = String::new();
                for h in response.headers() {
                    headers_str.push_str(
                        &(h.0.to_string() + ":" + &(h.1.to_str().unwrap().to_string()) + ", "),
                    );
                }
                let header_name = "dropbox-api-result";
                let header = response.headers().get(header_name);
                if header.is_some() {
                    let header_value = header.unwrap();
                    match serde_json::from_slice(header_value.as_bytes())
                        as Result<ResultType, serde_json::Error>
                    {
                        Ok(final_res) => {
                            use std::io::prelude::*;
                            let file_size = final_res.size;
                            let _ = messages.send(Message {
                                call_id: Some(call_id),
                                data: MsgData(msg::SizeInfo {
                                    size: Some(file_size),
                                }),
                            });
                            match std::fs::File::create(&params.save_to) {
                                Ok(mut f) => {
                                    let mut tmp_res: Result<ResultType, ErrorType> = Ok(final_res);
                                    let (_, mut body) = response.into_parts();
                                    while let Some(next) = body.data().await {
                                        match next {
                                            Ok(chunk) => {
                                                // log::info!("chunk size: {}", chunk.len().to_string());
                                                bytes_received += chunk.len() as u64;
                                                let _ = messages.send(Message {
                                                    call_id: Some(call_id),
                                                    data: MsgData(msg::Progress {
                                                        value: bytes_received,
                                                    }),
                                                });
                                                if let Err(e) = f.write_all(&chunk) {
                                                    tmp_res = Err(ErrorType::e_file_write(
                                                        action,
                                                        e,
                                                        &params.path,
                                                        &params.save_to,
                                                    ));
                                                    break;
                                                }
                                            }
                                            Err(e) => {
                                                tmp_res = Err(ErrorType::e_next_chunk(
                                                    action,
                                                    e,
                                                    &params.path,
                                                    bytes_received,
                                                ));
                                                break;
                                            }
                                        }
                                    }
                                    if tmp_res.is_ok() {
                                        log::debug!("chunks done");
                                        if let Err(e) = f.sync_all() {
                                            tmp_res = Err(ErrorType::e_file_sync_all(
                                                action,
                                                e,
                                                &params.path,
                                                &params.save_to,
                                            ));
                                        }
                                    }
                                    tmp_res
                                }
                                Err(e) => Err(ErrorType::e_file_create(
                                    action,
                                    e,
                                    &params.path,
                                    &params.save_to,
                                )),
                            }
                        }
                        Err(e) => Err(ErrorType::Base(
                            clouds::BaseError::e_response_body_deserialization(action, e),
                        )),
                    }
                } else {
                    Err(ErrorType::e_header_not_found(
                        action,
                        header_name,
                        &headers_str,
                        &params.path,
                        status,
                    ))
                }
            } else {
                match hyper::body::to_bytes(response.into_body()).await {
                    Ok(data) => {
                        let err = create_error_from_body_data(&data, action, status, |e| match e {
                            DownloadFileErrorTag::Path {
                                path: DownloadFilePathErrorTag::NotFound,
                            } => ErrorType::NotFound(file_path),
                        });
                        Err(err)
                    }
                    Err(e) => Err(ErrorType::Base(clouds::BaseError::e_error_body_aggregate(
                        action,
                        e.into(),
                    ))),
                }
            }
        }
        Err(e) => Err(ErrorType::Base(clouds::BaseError::e_response_wait(
            action, &e,
        ))),
    };

    let total = start.elapsed();
    if total.as_secs() != 0 {
        log::debug!(
            "downloaded {} kbytes at {:#?}, speed: {} kbytes/sec",
            (bytes_received / 1024).to_string(),
            total,
            (bytes_received / 1024 / total.as_secs()).to_string()
        )
    } else {
        log::debug!(
            "downloaded {} kbytes at {:#?}",
            (bytes_received / 1024).to_string(),
            total
        )
    }
    res
}

pub async fn download_file(
    rth: RuntimeHolder,
    auth_info_holder: clouds::AuthInfoHolder,
    params: DownloadFileParams,
    call_id: u64,
    messages: MessagesSender<call_messages::Message>,
) -> Result<DownloadFileCallResult, DownloadFileCallError> {
    let rth_clone = rth.clone();
    call_with_token_auto_refresh(
        rth,
        auth_info_holder.clone(),
        messages.clone(),
        DownloadFileTokenMessageCreator { call_id },
        || async {
            int_download_file(
                &rth_clone,
                auth_info_holder.clone(),
                params.clone(),
                call_id,
                messages.clone(),
            )
            .await
        },
    )
    .await
}

async fn call_with_token_auto_refresh<ResType, ErrType, Fn, Fut, T>(
    rth: RuntimeHolder,
    auth_info_holder: clouds::AuthInfoHolder,
    messages: MessagesSender<T>,
    //call_id: u64,
    message_creator: impl WithRefreshToken<T>,
    func: Fn,
) -> Fut::Output
where
    ErrType: clouds::BaseErrorAccess,
    Fn: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<ResType, ErrType>>,
{
    let token = { auth_info_holder.read().await.token.clone() };
    let mut f = func;
    let final_res = match f().await {
        Ok(res) => Ok(res),
        Err(mut e) => match e.get_base_error() {
            Some(base) => {
                if base.is_token_error() {
                    let _ = messages.send(message_creator.refresh_token());
                    match refresh_token_impl(rth, &token.clone(), &auth_info_holder.clone()).await {
                        Ok(_token) => {
                            let _ = messages.send(message_creator.refresh_token_complete());
                            f().await
                        }
                        Err(refresh_e) => match e.get_base_error_mut() {
                            Some(clouds::BaseError::ExpiredAccessToken {
                                ref mut refresh_error,
                            }) => {
                                *refresh_error = Some(Box::new(refresh_e));
                                Err(e)
                            }
                            Some(clouds::BaseError::AccessTokenMalformed {
                                ref mut refresh_error,
                                ..
                            }) => {
                                *refresh_error = Some(Box::new(refresh_e));
                                Err(e)
                            }
                            Some(clouds::BaseError::InvalidAuthorizationValue {
                                ref mut refresh_error,
                                ..
                            }) => {
                                *refresh_error = Some(Box::new(refresh_e));
                                Err(e)
                            }
                            Some(_) => panic!(),
                            None => panic!(),
                        },
                    }
                } else {
                    Err(e)
                }
            }
            None => Err(e),
        },
    };
    //    state.send(CallState::Done);
    final_res
}

async fn process_simple_request<ResType, ErrorType, TRoutine, CF>(
    client: Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
    req: http::Request<hyper::Body>,
    action: &str,
    convert_f: CF,
) -> Result<ResType, ErrorType>
where
    ResType: serde::de::DeserializeOwned,
    ErrorType: clouds::BaseErrorAccess,
    CF: FnOnce(TRoutine) -> ErrorType,
    TRoutine: serde::de::DeserializeOwned,
{
    match client.request(req).await {
        Ok(response) => {
            let status = response.status().as_u16();
            let is_success = response.status().is_success();
            hyper::body::to_bytes(response.into_body())
                .await
                .map_err(|e| {
                    ErrorType::from_base(clouds::BaseError::e_response_body_aggregate(action, e))
                })
                .and_then(|body_data| {
                    if is_success {
                        serde_json::from_slice(&body_data).map_err(|e| {
                            ErrorType::from_base(
                                clouds::BaseError::e_response_body_deserialization(action, e),
                            )
                        })
                    } else {
                        Err(create_error_from_body_data(
                            &body_data, action, status, convert_f,
                        ))
                    }
                })
        }
        Err(e) => Err(ErrorType::from_base(clouds::BaseError::e_response_wait(
            action, &e,
        ))),
    }
}

pub async fn auth_code_to_tokens(
    rth: &RuntimeHolder,
    code: String,
    redirect_uri: String,
    client_id: String,
    secret: String,
) -> Result<CodeToTokensResult, clouds::AuthCodeToTokensCallError> {
    type ResType = CodeToTokensResult;
    type ErrorType = clouds::AuthCodeToTokensCallError;
    type FuncRes = Result<ResType, ErrorType>;
    let action = "dropbox.auth_code_to_tokens";

    let exec = || async {
        let client = client_http2(rth);
        let body = form_urlencoded::Serializer::new(String::new())
            .append_pair("code", &code)
            .append_pair("grant_type", "authorization_code")
            .append_pair("redirect_uri", &redirect_uri)
            .finish();
        let req = request_builder_http2()
            .method("POST")
            .header(
                "Authorization",
                "Basic ".to_string()
                    + &base64::encode(format!("{}:{}", client_id, secret).as_bytes()),
            )
            .header("Content-Type", "application/x-www-form-urlencoded")
            .uri("https://api.dropbox.com/oauth2/token")
            .body(Body::from(body))
            .expect("request builder");

        let res: FuncRes = process_simple_request(client, req, action, |tr| match tr {
            AuthCodeToTokenErrorTag::NotImplemented => clouds::AuthCodeToTokensCallError::Other(
                "some AuthCodeToTokensCallError error".to_string(),
            ),
        })
        .await;

        res.map_err(|e| match e.is_permanent() {
            true => backoff::Error::Permanent(e),
            false => backoff::Error::Transient(e),
        })
    };

    let backoff = new_backoff();
    retry(backoff, exec).await
}

fn new_backoff() -> ExponentialBackoff {
    let backoff = ExponentialBackoff {
        initial_interval: Duration::from_millis(100),
        multiplier: 2.0,
        randomization_factor: 0.5,
        max_interval: Duration::from_secs(2),
        max_elapsed_time: Some(Duration::from_secs(15)),
        ..Default::default()
    };
    backoff
}

pub async fn refresh_token(
    rth: RuntimeHolder,
    refresh_token: String,
    client_id: String,
    secret: String,
) -> Result<RefreshTokenResult, clouds::RefreshTokenCallError> {
    type ResType = RefreshTokenResult;
    type ErrorType = clouds::RefreshTokenCallError;
    type FuncRes = Result<ResType, ErrorType>;
    let action = "dropbox.refresh_token";
    let rth_clone = &rth;

    let exec = || async {
        let client = client_http2(rth_clone);
        let body = form_urlencoded::Serializer::new(String::new())
            .append_pair("refresh_token", &refresh_token)
            .append_pair("grant_type", "refresh_token")
            .finish();
        let req = request_builder_http2()
            .method("POST")
            .header(
                "Authorization",
                "Basic ".to_string()
                    + &base64::encode(format!("{}:{}", client_id, secret).as_bytes()),
            )
            .header("Content-Type", "application/x-www-form-urlencoded")
            .uri("https://api.dropbox.com/oauth2/token")
            .body(Body::from(body))
            .expect("request builder");

        let res: FuncRes = process_simple_request(client, req, action, |tr| match tr {
            RefreshTokenErrorTag::NotImplemented => {
                clouds::RefreshTokenCallError::InvalidRefreshToken_
            }
        })
        .await;

        res.map_err(|e| match e.is_permanent() {
            true => backoff::Error::Permanent(e),
            false => backoff::Error::Transient(e),
        })
    };

    let backoff = new_backoff();

    retry(backoff, exec).await
}
