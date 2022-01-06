//use tiny_http::{Response, Server, Header};
use tiny_http::{Response, Server};
use crate::clouds;
use crate::common_types::*;
use crate::storage_instance;
use crate::dropbox;
//use image::error::UnsupportedErrorKind::Format;
//use std::result::Result;
//use std::error::Error;

pub struct InstanceInfo {
    pub thread_handle: tokio::task::JoinHandle<bool>,
    pub redirect_uri: String
}

/*
fn get_one(port_range: &[u16]) -> Option<(Server, u16)> {
    for port in port_range {
        let server_res = Server::http("127.0.0.1:".to_owned() + &(*port.to_string()));
        if server_res.is_ok() {
            return Some((server_res.unwrap(), *port));
        }
    }
    None
}
*/
fn get_one(list: &Vec<String>) -> Option<(Server, String)> {
    for item in list {
//        log::info!("try server running at {:?}", item);
        let server_res = Server::http(item);
        if server_res.is_ok() {
//            log::info!("server running at {:?}", item);
            return Some((server_res.unwrap(), item.clone()));
        }
        else {
            log::error!("server at {:?} failed to run: {:?}", item, server_res.err().unwrap().to_string());
        }
    }
    None
}

fn extract_value(list: &Vec<&str>, name: &str) -> Option<String> {
//    let list = s.split("&");
    let start = name.to_owned() + "=";
    for item in list {
        if item.to_ascii_lowercase().starts_with(&start) {
            let mut tmp = item.to_string();
            tmp.drain(..start.len());
            return Some(tmp);
        }
    }
    None
}
/*
fn extract_value(s: &String, name: &String) -> Option<String> {
    let list = s.split("&");
    let start = name.to_string() + "=";
    for item in list {
        if item.to_ascii_lowercase().starts_with(&start) {
            let mut tmp = item.to_string();
            tmp.drain(..start.len());
            return Some(tmp);
        }
    }
    None
}
*/

// http://127.0.0.1:7072/dropbox#access_token=KEgHEXk7ynoAAAAAAAAAAbw6G9XxFzRU6fcG4jIq_DSfMn0_miwk_2_KOcKHBv3Y&token_type=bearer&uid=1891243584&account_id=dbid%3AAABmhBJ2AwmaHetXyU9YwZ_WAeSDsocC_yE&scope=account_info.read+files.content.read+files.content.write+files.metadata.read+files.metadata.write
pub fn try_run(rth: RuntimeHolder, auth_info_holder: clouds::AuthInfoHolder, addr_list: Vec<String>, state_holder: storage_instance::CloudAuthServerStateHolder) -> Option<InstanceInfo> {
    if let Some(res) = get_one(&addr_list) {
        let (server, host_with_port) = res;
        let redirect_uri = "http://".to_owned() + &host_with_port.clone() + "/dropbox";
        let redirect_uri_clone = redirect_uri.clone();
        log::info!("Server running at {:?}, redirect url: {:?}", host_with_port.clone(), redirect_uri.clone());
//        let host_with_port_clone = host_with_port.clone();
        let rtg = rth.read().unwrap();
        let rt = rtg.as_ref().unwrap();
        let auth_info_holder_copy = auth_info_holder.clone();
        let h = rt.spawn( async move {
            for req in server.incoming_requests() {
                log::info!("Serving {:?}", req.url());

                let response = if req.url().starts_with("/dropbox?") {
                    let mut s = req.url().to_string();
                    s.drain(.."/dropbox?".len());
//                    if let (Some(access_token), Some(refresh_token)) = (extract_value(&s, &"access_token".to_string()), extract_value(&s, &"refresh_token".to_string())) {
                    let list: Vec<_> = s.split("&").collect();
                    if let (Some(code), Some(state)) = (extract_value(&list, "code"), extract_value(&list, "state")) {
                        let state_found = { state_holder.read().unwrap().known_state_strings.contains(&state) };
                        if state_found {
                            let res_res = {
                                let auth_info = {
                                    auth_info_holder_copy.read().await.clone()
                                };
                                dropbox::auth_code_to_tokens(code.clone(), redirect_uri.clone(), auth_info.client_id.clone(), auth_info.secret.clone()).await
                            };
                            if let Ok(res) = res_res {
                                {
                                    let auth_info = &mut *auth_info_holder_copy.write().await;
                                    auth_info.refresh_token = res.refresh_token.clone();
                                    auth_info.token = res.access_token.clone();
                                }
                                Response::from_string("Ok. Successful.")

                                //                            Response::from_string(format!("access_token: {}, refresh_token: {}", &res.access_token, &res.refresh_token))
                            } else {
                                let err = format!("auth_code_to_tokens error: {}", res_res.err().unwrap().to_string());
                                log::info!("{}", err);
                                Response::from_string(err)
                            }
                            //                        Response::from_string(format!("access_token: {}, refresh_token: {}", access_token, refresh_token))
                        }
                        else {
                            let err = format!("unknown state field value in: {}", req.url());
                            log::info!("{}", err);
                            Response::from_string(err)
                        }
                    }
                    else if let (Some(error), Some(error_description), Some(state)) =
                                (extract_value(&list, &"error"), extract_value(&list, &"error_description"), extract_value(&list, &"state"))
                    {
                        let state_found = {
                            state_holder.read().unwrap().known_state_strings.contains(&state)
                        };
                        if state_found {
                            let err = format!("error: {}, error description: {}, url: {}", error, error_description, req.url());
                            log::info!("{}", err);
                            Response::from_string(err)
                        }
                        else {
                            let err = format!("unknown state field value in: {}", req.url());
                            log::info!("{}", err);
                            Response::from_string(err)
                        }
                    }
                    else {
                        Response::from_string(format!("can't extract code and state from: {}", s))
                    }
                }
/*
                else if req.url().starts_with("/dropbox/res/") {
                    let mut s = req.url().to_string();
                    s.drain(.."/dropbox/res/".len());
//                    if let (Some(access_token), Some(refresh_token)) = (extract_value(&s, &"access_token".to_string()), extract_value(&s, &"refresh_token".to_string())) {
                    if let (Some(code), Some(state)) = (extract_value(&s, &"code".to_string()), extract_value(&s, &"state".to_string())) {
                        Response::from_string(format!("code: {}, state: {}", code, state))
//                        Response::from_string(format!("access_token: {}, refresh_token: {}", access_token, refresh_token))
                    }
                    else {
                        Response::from_string("can't extract access_token from: ".to_string() + &s)
                    }
                }

 */
                else {
                    Response::from_string("unknown request: ".to_owned() + req.url())
                };
                req.respond(response).unwrap();
            }
            true
        });
        Some(InstanceInfo {
            thread_handle: h,
            redirect_uri: redirect_uri_clone
        })

    }
    else {
        None
    }
}

