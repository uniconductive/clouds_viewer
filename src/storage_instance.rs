use std::{
    sync::{
        Arc,
    },
    collections::HashMap,
};
use http::Uri;
use crate::storages;
use crate::clouds;
use crate::call_messages;
use crate::call_states;
use crate::storage_models;
use crate::common_types::*;

enum StorageAction {
    To { path: String },
    Forward { folder_name: String },
    Backward,
    DownloadFile { path: String, to_path: String },
}

pub struct CloudAuthServerState {
    pub known_state_strings: Vec<String>,
    // KnownStatesHolder,
    //    last_error: Option<String>,
    http_server: Option<crate::http_server::InstanceInfo>,
    last_error: Option<String>,
}

pub type CloudAuthServerStateHolder = Arc<std::sync::RwLock<CloudAuthServerState>>;

impl Default for CloudAuthServerState {
    fn default() -> Self {
        Self {
            known_state_strings: vec![],
            http_server: None,
            last_error: None,
        }
    }
}

pub struct StorageActiveFolder {
    pub path: String,
    pub items: Vec<storage_models::Item>,
//    in_auth_process: bool,
}

pub struct StorageVisualState {
    pub folder: Option<StorageActiveFolder>,
    pub list_folder_error: Option<String>,
    in_auth_process: bool,
//    call_states: VisualCallStates,
    pub v_path: String,
}

impl Default for StorageVisualState {
    fn default() -> Self {
        Self {
            folder: None,
            list_folder_error: None,
            in_auth_process: false,
//            call_states: VisualCallStates::default(),
            v_path: "".into(),
        }
    }
}

//#[derive(Copy)]
pub struct StorageInstance {
    pub caption: String,
    pub id: String,
    pub do_auth: bool,
    pub storage_type: storage_models::StorageType,
    pub rth: RuntimeHolder,
    pub auth_server_state_holder: CloudAuthServerStateHolder,
    //    pub call_states_holder: storages::CallStatesHolder,
    pub events_sender: EventsSender<call_messages::Message>,
    pub events: EventsReceiver<call_messages::Message>,
//    pub calls_info: Arc<std::sync::RwLock<HashMap<u64, Option<call_states::State>>>>,
    pub calls_info: HashMap<u64, call_states::State>,
//    pub last_call_id: std::rc::Rc<std::sync::RwLock<u64>>,
    pub last_call_id: std::rc::Rc<std::sync::RwLock<u64>>,
//    pub last_call_id: u64,
    pub visual_state: StorageVisualState,
    pub auth_info_holder: clouds::AuthInfoHolder,
    pub save_to_path: String,
}

impl StorageInstance {

    fn process_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            let mut remove_call_info: bool = false;
//            let mut calls_info_g = self.calls_info.write().unwrap();
            let mut list_folder_started_call_id = 0;
            let call_id = event.call_id;
//            if let Some(mut call_info) = (*calls_info_g).get_mut(&call_id) {
            if let Some(mut call_info) = self.calls_info.get_mut(&call_id) {
                match event.data {
                    call_messages::Data::ListFolder(data) => {
                        if let call_states::Data::ListFolder { data: ref mut state_data, .. } = call_info.data {
                            match data {
                                call_messages::ListFolder::Started { .. } => panic!("started"),
                                call_messages::ListFolder::RefreshToken => {
                                    *state_data = call_states::ListFolder::RefreshToken;
                                },
                                call_messages::ListFolder::RefreshTokenComplete => {
                                    *state_data = call_states::ListFolder::RefreshTokenComplete;
                                },
                                call_messages::ListFolder::Progress { value, value_str } => {
                                    *state_data = call_states::ListFolder::InProgress { value: Some(value), value_str }
                                },
                                call_messages::ListFolder::Finished { result: Ok(mut res) } => {
                                    self.visual_state.folder = Some(StorageActiveFolder { path: res.path.clone(), items: res.items.take().unwrap_or(vec![]) });
                                    self.visual_state.v_path = "/".to_owned() + &res.path;
                                    *state_data = call_states::ListFolder::Ok;
                                    remove_call_info = true;
                                },
                                call_messages::ListFolder::Finished { result: Err(e) } => {
                                    self.visual_state.list_folder_error = Some(e.to_string());
                                    *state_data = call_states::ListFolder::Failed(e);
                                }
                            }
                        }
                    },
                    call_messages::Data::DownloadFile(data) => {
                        if let call_states::Data::DownloadFile { data: ref mut state_data, remote_path: _, ref local_path, size: ref mut total_size, ref mut downloaded} = call_info.data {
                            match data {
                                call_messages::DownloadFile::Started { .. } => panic!("started"),
                                call_messages::DownloadFile::RefreshToken => {
                                    *state_data = call_states::DownloadFile::RefreshToken;
                                },
                                call_messages::DownloadFile::RefreshTokenComplete => {
                                    *state_data = call_states::DownloadFile::RefreshTokenComplete;
                                },
                                call_messages::DownloadFile::SizeInfo { size } => {
                                    if size.is_some() {
                                        log::info!("file size: {} kbytes", (size.unwrap()/1024).to_string());
                                    }
                                    else {
                                        log::info!("file size: {}", "unknown");
                                    }
                                    *state_data = call_states::DownloadFile::SizeInfo { size };
                                    *total_size = size;
                                },
                                call_messages::DownloadFile::Progress { value } => {
                                    if total_size.is_some() {
    //                                                log::info!("downloaded {} of {} kbytes", (value/1024).to_string(), total_size.unwrap()/1024)
                                    }
                                    else {
    //                                                log::info!("downloaded {} kbytes of unknown total", (value/1024).to_string())
                                    }
                                    *downloaded = value;
                                    *state_data = call_states::DownloadFile::InProgress { progress: value };
                                }
                                call_messages::DownloadFile::Finished { result: Ok(_res) } => {
                                    *state_data = call_states::DownloadFile::Ok;
                                    log::info!("open file: {}", &local_path);
                                    let _ = open::that_in_background(local_path);
                                    remove_call_info = true;
                                },
                                call_messages::DownloadFile::Finished { result: Err(e) } => {
                                    *state_data = call_states::DownloadFile::Failed(e);
                                },
                                call_messages::DownloadFile::Cancelled => {
                                    call_info.handle.abort();
                                    remove_call_info = true;
                                },
                            }
                        }
                    }
                }
            }
            else {
                match event.data {
                    call_messages::Data::ListFolder(data) => {
                        if let call_messages::ListFolder::Started { handle, path } = data {
                            self.calls_info.retain(|k, v| {
                                match &v.data {
                                    call_states::Data::ListFolder { .. } => {
                                        v.handle.abort();
                                        false
                                    },
                                    _ => panic!()
                                }
                            });
                            self.calls_info.insert(call_id, call_states::State {
                                handle: handle,
                                data: call_states::Data::ListFolder { data: call_states::ListFolder::InProgress { value: None, value_str: None }, path }
                            });
                        } else {
                            log::info!("unknown call_id: {}", &call_id);
                        }
                    },
                    call_messages::Data::DownloadFile(data) => {
                        if let call_messages::DownloadFile::Started { handle, remote_path, local_path } = data {
                            self.calls_info.insert(call_id, call_states::State {
                                handle: handle,
                                data: call_states::Data::DownloadFile { data: call_states::DownloadFile::Started, remote_path, local_path, size: None, downloaded: 0 }
                            });
                        }
                        else {
                            log::info!("unknown call_id: {}", &call_id);
                        }
                    }
                }

//                log::info!("unknown call_id: {}", &call_id);
            }
            if remove_call_info {
//                (*calls_info_g).remove(&event.call_id);
                self.calls_info.remove(&event.call_id);
            }
        }
    }

    pub fn prepare_visual_state(&mut self) {
//        log::info!("prepare_visual_state");
//        let rtg = self.rth.read().unwrap();
//        let rt = rtg.as_ref().unwrap();
//        let res = rt.block_on(Self::int_prepare_visual_state(self.call_states_holder.clone()));
        self.process_events();
        /*
                match res {
                    Some(new_state) => {
                        log::info!("prepare_visual_state: {}", "Some");
                        if new_state.folder.is_some() {
                            self.visual_state.folder = new_state.folder;
                        }
                        self.visual_state.in_auth_process = new_state.in_auth_process;
                        self.visual_state.call_states = new_state.call_states;
                    },
                    _ => {
        //                log::info!("prepare_visual_state: {}", "None");
                    }
                }
         */
    }

    pub fn start_auth(&self) {
        let rtg = self.rth.read().unwrap();
        let rt = rtg.as_ref().unwrap();
        let server_state_holder = self.auth_server_state_holder.clone();
        let auth_info_holder = self.auth_info_holder.clone();
        let rth = self.rth.clone();
        rt.spawn(async move {
            let auth_info = { auth_info_holder.read().await.clone() };
            // to list of like: 127.0.0.1:7072
            // TODO handle errors
            let addr_list: Vec<String> = auth_info.redirect_addresses.iter().map(|addr| {
                let url: Uri = addr.parse().unwrap();
                url.host().unwrap().to_string() + ":" + &url.port_u16().unwrap().to_string()
            }).collect();
            let server_state = &mut server_state_holder.write().unwrap();
            if server_state.http_server.is_none() {
                server_state.http_server = crate::http_server::try_run(
                    rth,
                    auth_info_holder,
//                    [r"127.0.0.1:7072".to_string()].to_vec(),
                    addr_list,
                    server_state_holder.clone()
                );
            }
            if server_state.http_server.is_none() {
                server_state.last_error = Some("can't run local http server to handle auth requests.".to_string());
            } else {
                let redirect_uri = server_state.http_server.as_ref().unwrap().redirect_uri.clone();
                let url = format!(r"https://www.dropbox.com/oauth2/authorize?client_id={}&response_type=code&state=some_state&token_access_type=offline&redirect_uri={}",
                                  auth_info.client_id,
                                  redirect_uri);
                if !webbrowser::open(&url).is_ok() {
                    server_state.last_error = Some("can't open auth page in default browser".to_string());
                }
            }
        });
    }

    fn parent_path(path: String) -> String {
        let mut v: Vec<&str> = path.split("/").into_iter().collect();
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
        let mut v: Vec<&str> = path.split("/").into_iter().collect();
        let s = v.pop().unwrap();
        if !s.is_empty() {
            v.push(s);
        }
        v.push(&folder_name);
        v.join("/")
    }

    async fn int_nav_to(
        rth: RuntimeHolder,
        events: EventsSender<call_messages::Message>,
        auth_info_holder: clouds::AuthInfoHolder,
        storage_type: storage_models::StorageType,
        path: String,
        call_id: u64)
    {
        let call_in_data = storage_models::CallInData::list_folder(
            storage_models::list_folder_in_data {
                path,
            }
        );

        storages::storage_call(rth, events, auth_info_holder, storage_type, call_in_data, call_id).await;
    }

    async fn int_download_file(
        rth: RuntimeHolder,
        events: EventsSender<call_messages::Message>,
        auth_info_holder: clouds::AuthInfoHolder,
        storage_type: storage_models::StorageType,
        path: String,
        save_to: String,
        call_id: u64)
    {
        let call_in_data = storage_models::CallInData::download_file(
            storage_models::download_file_in_data {
                remote_path: path,
                local_path: save_to
            }
        );

        storages::storage_call(rth, events, auth_info_holder, storage_type, call_in_data, call_id).await;
    }

    fn action(&self, action: StorageAction) {
        let rth = self.rth.clone();
        let rth_clone = self.rth.clone();
        let rtg = rth.read().unwrap();
        let rt = rtg.as_ref().unwrap();
//        let call_states_holder = self.call_states_holder.clone();
        let events_sender = self.events_sender.clone();
        let auth_info_holder = self.auth_info_holder.clone();
        let storage_type = self.storage_type;
        let current_path = match &self.visual_state.folder {
            Some(folder) => folder.path.clone(),
            None => "".to_string(),
        };

        let call_id = {
            let mut last_call_id_g = self.last_call_id.write().unwrap();
            let call_id = (*last_call_id_g) + 1;
            *last_call_id_g = call_id;
//            let _ = (*self.calls_info.write().unwrap()).insert(call_id, None);
            call_id
        };

        rt.spawn(async move {
            match action {
                StorageAction::To { path } => Self::int_nav_to(rth_clone, events_sender, auth_info_holder, storage_type, path, call_id).await,
                StorageAction::Backward => Self::int_nav_to(rth_clone, events_sender, auth_info_holder, storage_type, Self::parent_path(current_path), call_id).await,
                StorageAction::Forward { folder_name } => Self::int_nav_to(rth_clone, events_sender, auth_info_holder, storage_type, Self::append_path(current_path, folder_name), call_id).await,
                StorageAction::DownloadFile { path, to_path } => Self::int_download_file(rth_clone, events_sender, auth_info_holder, storage_type, path, to_path, call_id).await,
            };
        });
    }
    pub fn nav_forward(&self, folder_name: String) {
        self.action(StorageAction::Forward { folder_name });
    }

    pub fn nav_back(&self) {
        self.action(StorageAction::Backward);
    }

    pub fn nav_to(&self, path: String) {
        self.action(StorageAction::To { path });
    }

    pub fn download_file(&self, path: String, to_path: String) {
        self.action(StorageAction::DownloadFile { path, to_path });
    }

}
