use crate::call_messages;
use crate::clouds;
use crate::common_types::*;
use crate::dropbox;
use crate::storage_models;

pub async fn storage_call(
    rth: RuntimeHolder,
    messages: MessagesSender<call_messages::Message>,
    auth_info_holder: clouds::AuthInfoHolder,
    storage_type: storage_models::StorageType,
    call_in_data: storage_models::CallInData,
    call_id: u64,
) -> Result<(), AsyncRuntimeError> {
    match (storage_type, call_in_data) {
        (
            storage_models::StorageType::Cloud(clouds::CloudId::Dropbox),
            storage_models::CallInData::list_folder(in_data),
        ) => {
            let path = in_data.path.clone();
            let path_clone = path.clone();
            let messages_clone = messages.clone();
            let rth_clone = rth.clone();
            let (s, r) = tokio::sync::oneshot::channel::<()>();
            let handle = rth.spawn(async move {
                let _ = r.await;
                list_folder_dropbox(
                    rth_clone,
                    auth_info_holder,
                    path_clone,
                    call_id,
                    messages_clone,
                )
                .await
            })?;
            let _ = messages.send(call_messages::Message {
                call_id: Some(call_id),
                data: call_messages::Data::ListFolder(call_messages::ListFolder::Started {
                    handle,
                    path,
                }),
            });
            let _ = s.send(());
            log::debug!("storage_call.list_folder: {}", in_data.path);
            Ok(())
        }
        (
            storage_models::StorageType::Cloud(clouds::CloudId::Dropbox),
            storage_models::CallInData::download_file(in_data),
        ) => {
            let remote_path = in_data.remote_path.clone();
            let remote_path_clone = remote_path.clone();
            let local_path = in_data.local_path.clone();
            let local_path_clone = local_path.clone();
            let messages_clone = messages.clone();
            let rth_clone = rth.clone();
            let (s, r) = tokio::sync::oneshot::channel::<()>();
            let handle = rth.spawn(async move {
                let _ = r.await;
                download_file_dropbox(
                    rth_clone,
                    auth_info_holder,
                    remote_path_clone,
                    local_path_clone,
                    call_id,
                    messages_clone,
                )
                .await
            })?;
            let _ = messages.send(call_messages::Message {
                call_id: Some(call_id),
                data: call_messages::Data::DownloadFile(call_messages::DownloadFile::Started {
                    handle,
                    local_path,
                    remote_path,
                }),
            });
            let _ = s.send(());
            log::debug!(
                "storage_call.download_file: {} -> {}",
                in_data.remote_path,
                in_data.local_path
            );
            Ok(())
        }
    }
}

async fn list_folder_dropbox_impl(
    rth: RuntimeHolder,
    auth_info_holder: clouds::AuthInfoHolder,
    path: String,
    call_id: u64,
    messages: MessagesSender<call_messages::Message>,
) -> Result<storage_models::list_folder_out_data, storage_models::ListFolderError> {
    fn add_to_result(
        res: dropbox::ListFolderCallResult,
        result: &mut Vec<storage_models::Item>,
    ) -> u64 {
        let mut cnt: u64 = 0;
        for entry in res.entries {
            let item: storage_models::Item;
            match entry {
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
            cnt += 1;
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
        limit: Some(1000),             //  = 1000
        include_non_downloadable_files: None,
    };

    params.path = path.clone();
    if params.path.eq("/") {
        panic!("path = '/'");
    }
    if !params.path.is_empty() {
        params.path = "/".to_owned() + &params.path;
    }

    let res = dropbox::list_folder(
        rth.clone(),
        auth_info_holder.clone(),
        params,
        call_id,
        messages.clone(),
    )
    .await;
    match res {
        Ok(res) => {
            let mut has_more = res.has_more;
            let mut cursor = res.cursor.clone();
            let mut count: u64 = res.entries.len() as u64;
            add_to_result(res, &mut result);
            while has_more {
                let _ = messages.send(call_messages::Message {
                    call_id: Some(call_id),
                    data: call_messages::Data::ListFolder(call_messages::ListFolder::Progress {
                        value: count,
                        value_str: None,
                    }),
                });

                let continue_params = dropbox::ListFolderContinueParams {
                    cursor: cursor.clone(),
                };
                let res = dropbox::list_folder_continue(
                    rth.clone(),
                    auth_info_holder.clone(),
                    continue_params,
                    call_id,
                    messages.clone(),
                )
                .await;
                match res {
                    Ok(res) => {
                        has_more = res.has_more;
                        cursor = res.cursor.clone();
                        count += res.entries.len() as u64;
                        let _ = messages.send(call_messages::Message {
                            call_id: Some(call_id),
                            data: call_messages::Data::ListFolder(
                                call_messages::ListFolder::Progress {
                                    value: count,
                                    value_str: None,
                                },
                            ),
                        });
                        add_to_result(res, &mut result);
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
            Ok(storage_models::list_folder_out_data {
                path,
                items: Some(result),
            })
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
            Err(err)
        }
    }
}

async fn list_folder_dropbox(
    rth: RuntimeHolder,
    auth_info_holder: clouds::AuthInfoHolder,
    path: String,
    call_id: u64,
    messages: MessagesSender<call_messages::Message>,
) {
    let res =
        list_folder_dropbox_impl(rth, auth_info_holder, path, call_id, messages.clone()).await;
    let _ = messages.send(call_messages::Message {
        call_id: Some(call_id),
        data: call_messages::Data::ListFolder(call_messages::ListFolder::Finished { result: res }),
    });
}

async fn download_file_dropbox_impl(
    rth: RuntimeHolder,
    auth_info_holder: clouds::AuthInfoHolder,
    remote_path: String,
    local_path: String,
    call_id: u64,
    messages: MessagesSender<call_messages::Message>,
) -> Result<storage_models::download_file_out_data, storage_models::DownloadFileError> {
    let mut params = dropbox::DownloadFileParams {
        path: remote_path.to_owned(),
        save_to: local_path.to_owned(),
    };

    params.path = remote_path.clone();
    if params.path.eq(r"/") {
        panic!("path = '/'");
    }
    if !params.path.is_empty() {
        params.path = "/".to_owned() + &params.path;
    }

    match dropbox::download_file(rth, auth_info_holder.clone(), params, call_id, messages).await {
        Ok(res) => Ok(storage_models::download_file_out_data { name: res.name }),
        Err(e) => {
            let err = match e {
                dropbox::DownloadFileCallError::Base(base) => {
                    if base.is_token_error() {
                        storage_models::DownloadFileError::Token(base.to_string())
                    } else {
                        storage_models::DownloadFileError::Other(base.to_string())
                    }
                }
                dropbox::DownloadFileCallError::NotFound(path) => {
                    storage_models::DownloadFileError::RemotePathNotFound(path)
                }
                _ => storage_models::DownloadFileError::Other(e.to_string()),
            };
            Err(err)
        }
    }
}

async fn download_file_dropbox(
    rth: RuntimeHolder,
    auth_info_holder: clouds::AuthInfoHolder,
    remote_path: String,
    local_path: String,
    call_id: u64,
    messages: MessagesSender<call_messages::Message>,
) {
    let res = download_file_dropbox_impl(
        rth,
        auth_info_holder,
        remote_path,
        local_path,
        call_id,
        messages.clone(),
    )
    .await;
    let _ = messages.send(call_messages::Message {
        call_id: Some(call_id),
        data: call_messages::Data::DownloadFile(call_messages::DownloadFile::Finished {
            result: res,
        }),
    });
}
