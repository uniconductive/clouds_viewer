//#[path = "clouds/clouds.rs"] mod clouds;

use crate::clouds;
use crate::call_messages;
use crate::storage_models;
use crate::common_types::*;
use crate::dropbox;

pub async fn storage_call(
    rth: RuntimeHolder,
    events: EventsSender<call_messages::Message>,
    auth_info_holder: clouds::AuthInfoHolder,
    storage_type: storage_models::StorageType,
    call_in_data: storage_models::CallInData,
    call_id: u64
) {
    log::info!("storage_call: {}", "Entered");
    let rtg = rth.read().unwrap();
    let rt = rtg.as_ref().unwrap();

//    let call_states = &mut *call_states_holder.write().await;
    match (storage_type, call_in_data) {
        (storage_models::StorageType::FileSystem, _) => panic!(),
        (storage_models::StorageType::Cloud(clouds::CloudId::Dropbox), storage_models::CallInData::list_folder(in_data)) => {
            let path = in_data.path.clone();
            let events_clone = events.clone();
            let func = rt.spawn(list_folder_dropbox(auth_info_holder, path.clone(), call_id, events_clone));
            let _ = events.send(call_messages::Message { call_id, data: call_messages::Data::ListFolder(call_messages::ListFolder::Started { handle: func, path: path }) });
            log::info!("storage_call.list_folder: {}", in_data.path);
        },
        (storage_models::StorageType::Cloud(clouds::CloudId::Dropbox), storage_models::CallInData::download_file(in_data)) => {
            let remote_path = in_data.remote_path.clone();
            let local_path = in_data.local_path.clone();
            let events_clone = events.clone();
            let func = rt.spawn(download_file_dropbox(auth_info_holder, remote_path.clone(), local_path.clone(), call_id, events_clone));
            let _ = events.send(call_messages::Message { call_id, data: call_messages::Data::DownloadFile(call_messages::DownloadFile::Started { handle: func, local_path, remote_path }) });
            log::info!("storage_call.download_file: {} -> {}", in_data.remote_path, in_data.local_path);
        }
    }
}

async fn list_folder_dropbox_impl(
//    rth: RuntimeHolder,
    auth_info_holder: clouds::AuthInfoHolder,
    path: String,
    call_id: u64,
    events: EventsSender<call_messages::Message>,
) -> Result<storage_models::list_folder_out_data, storage_models::ListFolderError> {
    log::info!("list_folder_dropbox: {}", "Entered");

    fn add_to_result(res: dropbox::ListFolderCallResult, result: &mut Vec<storage_models::Item>) -> u64 {
        let mut cnt: u64 = 0;
        for enrty in res.entries {
            let item: storage_models::Item;
            match enrty {
                dropbox::Meta::File(file) => {
                    item = storage_models::Item {
                        name: file.name.clone(),
                        id: file.id,
                        is_folder: false,
                        modified: Some(file.server_modified),
                        size: Some(file.size),
                        items: Some(vec![]),
                    }
                }
                dropbox::Meta::Folder(folder) => {
                    item = storage_models::Item {
                        name: folder.name,
                        id: folder.id,
                        is_folder: true,
                        modified: None,
                        size: None,
                        items: None,
                    }
                }
            };

            result.push(item);
            cnt = cnt + 1;
        }
        cnt
    }

    let mut result: Vec<storage_models::Item> = vec![];
    let mut params = dropbox::ListFolderParams {
        path: path.to_owned(),
        recursive: Some(false),
        include_deleted: Some(false),
        include_has_explicit_shared_members: None,
        include_mounted_folders: None, //  = true
        limit: Some(1000), //  = 1000
        include_non_downloadable_files: None,
    };

    params.path = path.clone();
    // good paths: "", "/folder"
    // bad paths: "/";  "folder"?
    if params.path.eq(r"/") {
        panic!("path = '/'");
    }
    if !params.path.is_empty() {
        params.path = r"/".to_owned() + &params.path;
    }

//    let underlying_holder = {
//        let state = &*(state_holder.write().await);
//        match &state.underlying {
//            ListFolderCallStateUnderlying::Dropbox(underlying_holder) => underlying_holder.clone(),
//            ListFolderCallStateUnderlying::Storages(_) => panic!(),
//        }
//    };

//    let (s, r): (UnboundedSender<crate::dropbox::CallState>, UnboundedReceiver<crate::dropbox::CallState>) = unbounded_channel();

//    let mut cnt: u64 = 0;

    let res = dropbox::list_folder(auth_info_holder.clone(), params, call_id, events.clone()).await;
    match res {
        Ok(res) => {
//            cnt = cnt +
                add_to_result(res, &mut result);
//            if res.has_more {
//                loop {
//                    let res = crate::dropbox::list_folder3(auth_info_holder.clone(), params, s.clone()).await;
//                }
            return Ok(storage_models::list_folder_out_data {
                path: path,
                items: Some(result),
            });
        }
        Err(e) => {
            let err = match e {
                dropbox::ListFolderCallError::Base(base) => {
                    if base.is_token_error() {
                        storage_models::ListFolderError::Token(base.to_string())
                    } else {
                        storage_models::ListFolderError::Other(base.to_string())
                    }
                }
                dropbox::ListFolderCallError::PathNotFound(p) => {
                    storage_models::ListFolderError::PathNotFound(p)
                }
            };
            return Err(err);
        }
    }
}

async fn list_folder_dropbox(
    auth_info_holder: clouds::AuthInfoHolder,
    path: String,
    call_id: u64,
    events: EventsSender<call_messages::Message>,
) {
    let res = list_folder_dropbox_impl(auth_info_holder, path, call_id, events.clone()).await;
    let _ = events.send(call_messages::Message { call_id, data: call_messages::Data::ListFolder(call_messages::ListFolder::Finished { result: res }) });
}

async fn download_file_dropbox_impl(
    auth_info_holder: clouds::AuthInfoHolder,
    remote_path: String,
    local_path: String,
    call_id: u64,
    events: EventsSender<call_messages::Message>
) -> Result<storage_models::download_file_out_data, storage_models::DownloadFileError> {
    log::info!("download_file_dropbox: {}", "Entered");

    let mut params = dropbox::DownloadFileParams {
        path: remote_path.to_owned(),
        save_to: local_path.to_owned(),
    };

    params.path = remote_path.clone();
    // good paths: "", "/folder"
    // bad paths: "/";  "folder"?
    if params.path.eq(r"/") {
        panic!("path = '/'");
    }
    if !params.path.is_empty() {
        params.path = r"/".to_owned() + &params.path;
    }

    let res = dropbox::download_file(auth_info_holder.clone(), params, call_id, events).await;
    match res {
        Ok(res) => return Ok(storage_models::download_file_out_data { name: res.name }),
        Err(e) => {
            let err = match e {
                dropbox::DownloadFileCallError::Base(base) => {
                    if base.is_token_error() {
                        storage_models::DownloadFileError::Token(base.to_string())
                    } else {
                        storage_models::DownloadFileError::Other(base.to_string())
                    }
                },
                _ => storage_models::DownloadFileError::Other(e.to_string())
            };
            return Err(err);
        }
    }
}

async fn download_file_dropbox(
    auth_info_holder: clouds::AuthInfoHolder,
    remote_path: String,
    local_path: String,
    call_id: u64,
    events: EventsSender<call_messages::Message>,
) {
    let res = download_file_dropbox_impl(auth_info_holder, remote_path, local_path, call_id, events.clone()).await;
    let _ = events.send(call_messages::Message { call_id, data: call_messages::Data::DownloadFile(call_messages::DownloadFile::Finished { result: res }) });
}


