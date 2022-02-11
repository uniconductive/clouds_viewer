use crate::call_messages;
use crate::call_states;
use crate::clouds;
use crate::clouds::AuthInfoHolder;
use crate::common_types::*;
use crate::storage_models;
use crate::storages;
use std::collections::HashMap;

enum StorageAction {
    To { path: String },
    Forward { folder_name: String },
    Backward,
    DownloadFile { path: String, to_path: String },
    CancelDownloadFile { call_id: u64 },
    StartAuth,
    CancelAuth,
}

pub struct StorageActiveFolder {
    pub path: String,
    pub items: Vec<storage_models::Item>,
}

pub struct AuthProcessState {
    pub call_id: u64,
}

pub struct StorageVisualState {
    pub folder: Option<StorageActiveFolder>,
    pub list_folder_error: Option<String>,
    pub auth_needed: bool,
    pub auth_process_state: Option<AuthProcessState>,
    pub v_path: String,
}

impl Default for StorageVisualState {
    fn default() -> Self {
        Self {
            folder: None,
            list_folder_error: None,
            auth_needed: false,
            auth_process_state: None,
            v_path: "".into(),
        }
    }
}

pub struct StorageInstanceMessages {
    pub sender: MessagesSender<call_messages::Message>,
    pub receiver: MessagesReceiver<call_messages::Message>,
}

impl Default for StorageInstanceMessages {
    fn default() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        Self { sender, receiver }
    }
}

type CallStates = HashMap<u64, call_states::State>;

pub struct StorageInstance {
    pub caption: String,
    pub id: String,
    pub storage_type: storage_models::StorageType,
    pub rth: RuntimeHolder,
    //    pub auth_server_state_holder: CloudAuthServerStateHolder,
    pub messages: StorageInstanceMessages,
    pub call_states: CallStates,
    pub last_call_id: std::rc::Rc<std::sync::RwLock<u64>>,
    pub visual_state: StorageVisualState,
    pub auth_info_holder: clouds::AuthInfoHolder,
    pub save_to_path: String,
}

impl StorageInstance {
    fn https_connector() -> hyper_rustls::HttpsConnector<hyper::client::HttpConnector> {
        let mut http_connector = hyper::client::HttpConnector::new();
        http_connector.enforce_http(false);
        hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http1()
            //            .enable_http2()
            .wrap_connector(http_connector)
    }

    fn client_http_or_https(
        rth: &RuntimeHolder,
    ) -> hyper::Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>> {
        let https = Self::https_connector();
        hyper::Client::builder()
            //            .http2_only(true)
            .executor(rth.clone())
            .build::<_, hyper::Body>(https)
    }

    fn abort_handles_and_senders(
        handles: &mut Vec<tokio::task::JoinHandle<()>>,
        cancellers: &mut Vec<call_states::Canceller>,
    ) {
        handles.drain(..).for_each(|handle| {
            handle.abort();
        });
        cancellers.drain(..).for_each(|canceller| {
            let _ = canceller.cancel_informer.send(());
            let _ = canceller.cancel_awaiter.blocking_recv();
        });
    }

    fn process_list_folder_msg(
        visual_state: &mut StorageVisualState,
        _call_id: u64,
        msg: call_messages::ListFolder,
        state: &mut call_states::State,
        remove_call_info: &mut bool,
    ) {
        if let call_states::Data::ListFolder {
            data: ref mut state_data,
            ..
        } = state.data
        {
            match msg {
                call_messages::ListFolder::Start { .. } => panic!("start"),
                call_messages::ListFolder::Started { .. } => panic!("started"),
                call_messages::ListFolder::RefreshToken => {
                    *state_data = call_states::ListFolder::RefreshToken;
                }
                call_messages::ListFolder::RefreshTokenComplete => {
                    *state_data = call_states::ListFolder::RefreshTokenComplete;
                }
                call_messages::ListFolder::Progress { value, value_str } => {
                    *state_data = call_states::ListFolder::InProgress {
                        value: Some(value),
                        value_str,
                    }
                }
                call_messages::ListFolder::Finished {
                    result: Ok(mut res),
                } => {
                    visual_state.folder = Some(StorageActiveFolder {
                        path: res.path.clone(),
                        items: res.items.take().unwrap_or(vec![]),
                    });
                    visual_state.v_path = "/".to_owned() + &res.path;
                    *state_data = call_states::ListFolder::Ok;
                    *remove_call_info = true;
                }
                call_messages::ListFolder::Finished { result: Err(e) } => {
                    visual_state.list_folder_error = Some(e.to_string());
                    *state_data = call_states::ListFolder::Failed(e);
                }
            }
        } else {
            panic!("{:?}", state.data);
        }
    }

    fn process_list_folder_msg_new_state(&mut self, call_id: u64, msg: call_messages::ListFolder) {
        match msg {
            call_messages::ListFolder::Started { handle, path } => {
                self.call_states.retain(|_, v| match &v.data {
                    call_states::Data::ListFolder { .. } => {
                        Self::abort_handles_and_senders(&mut v.handles, &mut v.cancellers);
                        false
                    }
                    _ => true,
                });
                self.call_states.insert(
                    call_id,
                    call_states::State {
                        handles: vec![handle],
                        cancellers: vec![],
                        data: call_states::Data::ListFolder {
                            data: call_states::ListFolder::InProgress {
                                value: None,
                                value_str: None,
                            },
                            path,
                        },
                    },
                );
            }
            _ => log::debug!("unknown call_id: {call_id}"),
        }
    }

    fn process_download_file_msg(
        _call_id: u64,
        msg: call_messages::DownloadFile,
        state: &mut call_states::State,
        remove_call_info: &mut bool,
    ) {
        if let call_states::Data::DownloadFile {
            data: ref mut state_data,
            remote_path: _,
            ref local_path,
            size: ref mut total_size,
            ref mut downloaded,
        } = state.data
        {
            match msg {
                call_messages::DownloadFile::Started { .. } => panic!("started"),
                call_messages::DownloadFile::RefreshToken => {
                    *state_data = call_states::DownloadFile::RefreshToken;
                }
                call_messages::DownloadFile::RefreshTokenComplete => {
                    *state_data = call_states::DownloadFile::RefreshTokenComplete;
                }
                call_messages::DownloadFile::SizeInfo { size } => {
                    if size.is_some() {
                        log::debug!("file size: {} kbytes", (size.unwrap() / 1024).to_string());
                    } else {
                        log::debug!("file size: {}", "unknown");
                    }
                    *state_data = call_states::DownloadFile::SizeInfo { size };
                    *total_size = size;
                }
                call_messages::DownloadFile::Progress { value } => {
                    if total_size.is_some() {
                        //log::info!("downloaded {} of {} kbytes",
                        // (value/1024).to_string(), total_size.unwrap()/1024)
                    } else {
                        //log::info!("downloaded {} kbytes of unknown total",
                        // (value/1024).to_string())
                    }
                    *downloaded = value;
                    *state_data = call_states::DownloadFile::InProgress { progress: value };
                }
                call_messages::DownloadFile::Finished { result: Ok(_res) } => {
                    *state_data = call_states::DownloadFile::Ok;
                    log::debug!("open file: {}", &local_path);
                    let _ = open::that_in_background(local_path);
                    *remove_call_info = true;
                }
                call_messages::DownloadFile::Finished { result: Err(e) } => {
                    *state_data = call_states::DownloadFile::Failed(e);
                }
                call_messages::DownloadFile::Cancelled => {
                    //                    state.handle.abort();
                    *remove_call_info = true;
                }
            }
        } else {
            panic!("{:?}", state.data);
        }
    }

    fn process_download_file_msg_new_state(
        &mut self,
        call_id: u64,
        msg: call_messages::DownloadFile,
    ) {
        if let call_messages::DownloadFile::Started {
            handle,
            remote_path,
            local_path,
        } = msg
        {
            self.call_states.insert(
                call_id,
                call_states::State {
                    handles: vec![handle],
                    cancellers: vec![],
                    data: call_states::Data::DownloadFile {
                        data: call_states::DownloadFile::Started,
                        remote_path,
                        local_path,
                        size: None,
                        downloaded: 0,
                    },
                },
            );
        } else {
            log::debug!("unknown call_id: {call_id}");
        }
    }

    fn process_auth_msg(
        visual_state: &mut StorageVisualState,
        call_id: u64,
        msg: call_messages::Auth,
        state: &mut call_states::State,
        messages: MessagesSender<call_messages::Message>,
        remove_call_info: &mut bool,
        rth: RuntimeHolder,
        auth_info_holder: AuthInfoHolder,
    ) {
        if let call_states::Data::Auth {
            data: ref mut state_data,
        } = state.data
        {
            match msg {
                call_messages::Auth::Start => panic!("start"),
                call_messages::Auth::Binded {
                    redirect_url,
                    addr: _,
                    state_field,
                } => {
                    let _ = messages.send(call_messages::Message {
                        call_id: Some(call_id),
                        data: call_messages::Data::Auth(call_messages::Auth::CheckAvailability {
                            redirect_url: redirect_url.clone(),
                            state_field,
                        }),
                    });
                    *state_data =
                        call_states::Auth::InProgress(call_states::AuthProgress::Binded {
                            redirect_url,
                        });
                }
                call_messages::Auth::LocalServerShutdowner {
                    cancel_informer,
                    cancel_awaiter,
                } => {
                    state.cancellers.push(call_states::Canceller {
                        cancel_informer,
                        cancel_awaiter,
                    });
                }
                call_messages::Auth::CheckAvailability {
                    redirect_url,
                    state_field,
                } => {
                    let redirect_url_clone = redirect_url.clone();
                    let (s, r) = tokio::sync::oneshot::channel::<()>();
                    if let Ok(handle) = rth.clone().spawn(async move {
                        let _ = r.await;
                        let uri: http::Uri = redirect_url_clone.parse().unwrap();
                        let mut parts = uri.into_parts();
                        parts.path_and_query = Some(http::uri::PathAndQuery::from_static("/ping"));
                        let ping_uri = http::Uri::from_parts(parts).expect("ping_uri");
                        let client = Self::client_http_or_https(&rth);
                        loop {
                            let start = std::time::Instant::now();
                            match client.get(ping_uri.clone()).await {
                                Ok(_response) => {
                                    let _ = messages.send(call_messages::Message {
                                        call_id: Some(call_id),
                                        data: call_messages::Data::Auth(
                                            call_messages::Auth::ServerReady {
                                                redirect_url: redirect_url_clone,
                                                state_field,
                                            },
                                        ),
                                    });
                                    break;
                                }
                                Err(e) => {
                                    log::debug!("ping failed for now: {}", e);
                                    if start.elapsed() > std::time::Duration::from_secs(10) {
                                        let _ = messages.send(call_messages::Message {
                                            call_id: Some(call_id),
                                            data: call_messages::Data::Auth(
                                                call_messages::Auth::Finished {
                                                    result: Err(
                                                        storage_models::AuthError::LocalServerWaitForAvailabileFailed
                                                    )
                                                },
                                            ),
                                        });
                                        break;
                                    }
                                }
                            }
                        }
                    }) {
                        state.handles.push(handle);
                        *state_data = call_states::Auth::InProgress(
                            call_states::AuthProgress::WaitingForLocalServerUp {
                                redirect_url,
                            },
                        );
                        let _ = s.send(());
                    }
                }
                call_messages::Auth::ServerReady {
                    redirect_url,
                    state_field,
                } => {
                    if let Ok(client_id) = rth.block_on(async move {
                        let res = auth_info_holder.read().await;
                        res.client_id.clone()
                    }) {
                        let params = format!(
                            "client_id={}&response_type={}&state={}&token_access_type={}&redirect_uri={}",
                            client_id,
                            "code",
                            state_field,
                            "offline",
                            redirect_url
                        );
                        let auth_url: String =
                            "https://www.dropbox.com/oauth2/authorize?".to_string() + &params;
                        log::debug!("auth url: {}", &auth_url);
                        *state_data = call_states::Auth::InProgress(
                            call_states::AuthProgress::TryOpenBrowser {
                                auth_url: auth_url.clone(),
                            },
                        );
                        match webbrowser::open(&auth_url) {
                            Ok(_) => {
                                *state_data = call_states::Auth::InProgress(
                                    call_states::AuthProgress::BrowserOpened { auth_url },
                                );
                            }
                            Err(e) => {
                                let _ = messages.send(call_messages::Message {
                                    call_id: Some(call_id),
                                    data: call_messages::Data::Auth(
                                        call_messages::Auth::Finished {
                                            result: Err(
                                                storage_models::AuthError::CantOpenAuthPageInDefaultBrowser {
                                                    auth_url,
                                                    error: e.to_string()
                                                }
                                            )
                                        }
                                    )
                                });
                            }
                        };
                    }
                }
                call_messages::Auth::Cancel => {
                    visual_state.auth_process_state = None;
                    *remove_call_info = true;
                }
                call_messages::Auth::Finished { result: Ok(_) } => {
                    *state_data = call_states::Auth::Ok;
                    log::debug!("auth Ok");
                    visual_state.auth_process_state = None;
                    let _ = messages.send(call_messages::Message {
                        call_id: None,
                        data: call_messages::Data::ListFolder(call_messages::ListFolder::Start {
                            path: "".to_string(),
                        }),
                    });

                    *remove_call_info = true;
                }
                call_messages::Auth::Finished { result: Err(e) } => {
                    log::error!("auth finished with error: {}", &e);
                    Self::abort_handles_and_senders(&mut state.handles, &mut state.cancellers);
                    visual_state.auth_process_state = None;
                    *state_data = call_states::Auth::Failed(e);
                }
            }
        } else {
            panic!("{:?}", state.data);
        }
    }

    fn process_auth_msg_new_state(&mut self, call_id: u64, msg: call_messages::Auth) {
        if let call_messages::Auth::Start = msg {
            self.call_states.retain(|_, v| match &v.data {
                call_states::Data::Auth { .. } => {
                    Self::abort_handles_and_senders(&mut v.handles, &mut v.cancellers);
                    false
                }
                _ => true,
            });
            self.visual_state.auth_process_state = Some(AuthProcessState { call_id });
            self.int_start_auth(call_id);
            self.call_states.insert(
                call_id,
                call_states::State {
                    handles: vec![],
                    cancellers: vec![],
                    data: call_states::Data::Auth {
                        data: call_states::Auth::InProgress(
                            call_states::AuthProgress::StartingLocalServer,
                        ),
                    },
                },
            );
        } else {
            log::debug!("unknown call_id: {call_id}");
        }
    }

    fn process_messages(&mut self) {
        while let Ok(msg) = self.messages.receiver.try_recv() {
            let mut remove_call_info: bool = false;
            if let Some(call_id) = msg.call_id {
                if let Some(state) = self.call_states.get_mut(&call_id) {
                    match msg.data {
                        call_messages::Data::ListFolder(data) => Self::process_list_folder_msg(
                            &mut self.visual_state,
                            call_id,
                            data,
                            state,
                            &mut remove_call_info,
                        ),
                        call_messages::Data::DownloadFile(data) => Self::process_download_file_msg(
                            call_id,
                            data,
                            state,
                            &mut remove_call_info,
                        ),
                        call_messages::Data::Auth(data) => Self::process_auth_msg(
                            &mut self.visual_state,
                            call_id,
                            data,
                            state,
                            self.messages.sender.clone(),
                            &mut remove_call_info,
                            self.rth.clone(),
                            self.auth_info_holder.clone(),
                        ),
                    }
                } else {
                    match msg.data {
                        call_messages::Data::ListFolder(data) => {
                            self.process_list_folder_msg_new_state(call_id, data)
                        }
                        call_messages::Data::DownloadFile(data) => {
                            self.process_download_file_msg_new_state(call_id, data)
                        }
                        call_messages::Data::Auth(data) => {
                            self.process_auth_msg_new_state(call_id, data)
                        }
                    }
                }
                if remove_call_info {
                    if let Some(state) = self.call_states.get_mut(&call_id) {
                        Self::abort_handles_and_senders(&mut state.handles, &mut state.cancellers);
                    }
                    self.call_states.remove(&call_id);
                }
            } else {
                match msg.data {
                    call_messages::Data::ListFolder(data) => match data {
                        call_messages::ListFolder::Start { path } => self.nav_to(path),
                        _ => panic!(),
                    },
                    _ => panic!(),
                }
            }
        }
    }

    pub fn prepare_visual_state(&mut self) {
        self.process_messages();
    }

    fn random_string(n: usize) -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        std::iter::repeat(())
            .map(|_| rng.sample(rand::distributions::Alphanumeric))
            .map(char::from)
            .take(n)
            .collect()
    }

    fn int_start_auth(&self, call_id: u64) {
        let rth = self.rth.clone();
        let auth_info_holder = self.auth_info_holder.clone();
        let messages = self.messages.sender.clone();
        let state_field = Self::random_string(10);
        log::debug!("state_field = {}", &state_field);
        self.rth
            .spawn(async move {
                let auth_info = { auth_info_holder.read().await.clone() };
                crate::http_server::run(
                    rth,
                    auth_info_holder,
                    auth_info.redirect_addresses,
                    state_field,
                    call_id,
                    messages,
                )
                .await;
            })
            .unwrap();
    }

    fn parent_path(path: String) -> String {
        let mut v: Vec<&str> = path.split('/').collect();
        if (v.len() == 1) & v[0].is_empty() {
            panic!("already root");
        }
        let s = v.pop().unwrap();
        if s.is_empty() {
            v.pop().unwrap();
        };
        v.join("/")
    }

    fn append_path(path: String, folder_name: String) -> String {
        let mut v: Vec<&str> = path.split('/').collect();
        let s = v.pop().unwrap();
        if !s.is_empty() {
            v.push(s);
        }
        v.push(&folder_name);
        v.join("/")
    }

    async fn int_nav_to(
        rth: RuntimeHolder,
        messages: MessagesSender<call_messages::Message>,
        auth_info_holder: clouds::AuthInfoHolder,
        storage_type: storage_models::StorageType,
        path: String,
        call_id: u64,
    ) -> Result<(), AsyncRuntimeError> {
        let call_in_data =
            storage_models::CallInData::list_folder(storage_models::list_folder_in_data { path });

        storages::storage_call(
            rth,
            messages,
            auth_info_holder,
            storage_type,
            call_in_data,
            call_id,
        )
        .await?;
        Ok(())
    }

    async fn int_download_file(
        rth: RuntimeHolder,
        messages: MessagesSender<call_messages::Message>,
        auth_info_holder: clouds::AuthInfoHolder,
        storage_type: storage_models::StorageType,
        path: String,
        save_to: String,
        call_id: u64,
    ) -> Result<(), AsyncRuntimeError> {
        let call_in_data =
            storage_models::CallInData::download_file(storage_models::download_file_in_data {
                remote_path: path,
                local_path: save_to,
            });
        storages::storage_call(
            rth,
            messages,
            auth_info_holder,
            storage_type,
            call_in_data,
            call_id,
        )
        .await?;
        Ok(())
    }

    fn gen_call_id(&self) -> u64 {
        let mut last_call_id_g = self.last_call_id.write().unwrap();
        let call_id = (*last_call_id_g) + 1;
        *last_call_id_g = call_id;
        call_id
    }

    fn action(&self, action: StorageAction, call_id: Option<u64>) {
        let rth = self.rth.clone();
        let messages_sender = self.messages.sender.clone();
        let auth_info_holder = self.auth_info_holder.clone();
        let storage_type = self.storage_type;
        let current_path = match &self.visual_state.folder {
            Some(folder) => folder.path.clone(),
            None => "".to_string(),
        };

        let call_id = call_id.unwrap_or(self.gen_call_id());

        let _ = self.rth.spawn(async move {
            match action {
                StorageAction::To { path } => {
                    let _ = Self::int_nav_to(
                        rth,
                        messages_sender,
                        auth_info_holder,
                        storage_type,
                        path,
                        call_id,
                    )
                    .await;
                }
                StorageAction::Backward => {
                    let _ = Self::int_nav_to(
                        rth,
                        messages_sender,
                        auth_info_holder,
                        storage_type,
                        Self::parent_path(current_path),
                        call_id,
                    )
                    .await;
                }
                StorageAction::Forward { folder_name } => {
                    let _ = Self::int_nav_to(
                        rth,
                        messages_sender,
                        auth_info_holder,
                        storage_type,
                        Self::append_path(current_path, folder_name),
                        call_id,
                    )
                    .await;
                }
                StorageAction::DownloadFile { path, to_path } => {
                    let _ = Self::int_download_file(
                        rth,
                        messages_sender,
                        auth_info_holder,
                        storage_type,
                        path,
                        to_path,
                        call_id,
                    )
                    .await;
                }
                StorageAction::CancelDownloadFile { call_id } => {
                    let _ = messages_sender.send(call_messages::Message {
                        call_id: Some(call_id),
                        data: call_messages::Data::DownloadFile(
                            call_messages::DownloadFile::Cancelled,
                        ),
                    });
                }
                StorageAction::StartAuth => {
                    let _ = messages_sender.send(call_messages::Message {
                        call_id: Some(call_id),
                        data: call_messages::Data::Auth(call_messages::Auth::Start),
                    });
                }
                StorageAction::CancelAuth => {
                    let _ = messages_sender.send(call_messages::Message {
                        call_id: Some(call_id),
                        data: call_messages::Data::Auth(call_messages::Auth::Cancel),
                    });
                }
            };
        });
    }

    pub fn nav_forward(&self, folder_name: String) {
        self.action(StorageAction::Forward { folder_name }, None);
    }
    pub fn nav_back(&self) {
        self.action(StorageAction::Backward, None);
    }
    pub fn nav_to(&self, path: String) {
        self.action(StorageAction::To { path }, None);
    }

    pub fn download_file(&self, path: String, to_path: String) {
        self.action(StorageAction::DownloadFile { path, to_path }, None);
    }
    pub fn cancel_download_file(&self, call_id: u64) {
        self.action(StorageAction::CancelDownloadFile { call_id }, None);
    }

    pub fn start_auth(&self) {
        self.action(StorageAction::StartAuth, None);
    }
    pub fn cancel_auth(&self, call_id: u64) {
        self.action(StorageAction::CancelAuth, Some(call_id));
    }
}
