use crate::call_messages;
use crate::clouds;
use crate::clouds::AuthInfoHolder;
use crate::common_types::*;
use crate::dropbox;
use crate::error::BindError;
use crate::storage_models;
use hyper::{Body, Request, Response};
//use itertools::Itertools;

struct ServerData {
    rth: RuntimeHolder,
    auth_info_holder: clouds::AuthInfoHolder,
    state_field: String,
    redirect_url: String,
    redirect_path: String,
    call_id: u64,
    messages: MessagesSender<call_messages::Message>,
}

#[derive(thiserror::Error, Debug, Clone)]
#[error("{0}")]
pub struct RequestProcessError(String);

async fn handle_request(
    req: Request<Body>,
    auth_info_holder: AuthInfoHolder,
    rth: RuntimeHolder,
    state_field: String,
    redirect_url: String,
    call_id: u64,
    messages: MessagesSender<call_messages::Message>,
) -> Result<Response<Body>, RequestProcessError> {
    let uri = req.uri();
    let url = uri.to_string();

    let res: Result<String, RequestProcessError> = {
        match uri.query() {
            Some(query) => {
                let list: std::collections::HashMap<String, String> = query
                    .split('&')
                    .map(|v| {
                        let vv: Vec<&str> = v.splitn(2, '=').collect();
                        (vv[0].to_string(), vv[1].to_string())
                    })
                    .collect();
                match (
                    list.get("state"),
                    list.get("code"),
                    list.get("error"),
                    list.get("error_description"),
                ) {
                    (None, _, _, _) => Err(RequestProcessError("state field is empty".to_string())),
                    (Some(state), _, _, _) if !state.eq(&state_field) => {
                        Err(RequestProcessError(format!(
                            "unknown state field value: '{}', waiting for state: '{}'",
                            &state, &state_field
                        )))
                    }
                    (Some(state), _, Some(error), Some(error_description))
                        if state.eq(&state_field) =>
                    {
                        Err(RequestProcessError(format!(
                            "error: {}, error description: {}",
                            error, error_description
                        )))
                    }
                    (Some(state), Some(code), _, _) if state.eq(&state_field) => {
                        let auth_info = { auth_info_holder.read().await.clone() };
                        match dropbox::auth_code_to_tokens(
                            &rth,
                            code.clone(),
                            redirect_url.clone(),
                            auth_info.client_id.clone(),
                            auth_info.secret.clone(),
                        )
                        .await
                        {
                            Ok(res) => {
                                {
                                    let auth_info = &mut *auth_info_holder.write().await;
                                    auth_info.refresh_token = res.refresh_token;
                                    auth_info.token = res.access_token;
                                }
                                let _ = messages.send(call_messages::Message {
                                    call_id: Some(call_id),
                                    data: call_messages::Data::Auth(
                                        call_messages::Auth::Finished { result: Ok(()) },
                                    ),
                                });
                                Ok("Ok. Successful.".to_string())
                            }
                            Err(e) => Err(RequestProcessError(format!(
                                "auth_code_to_tokens failed: {:?}",
                                e
                            ))),
                        }
                    }
                    _ => Err(RequestProcessError(
                        "can't extract code and state from url".to_string(),
                    )),
                }
            }
            None => Err(RequestProcessError("query is empty".to_string())),
        }
    };
    match res {
        Ok(res) => Ok(Response::builder()
            .status(hyper::StatusCode::OK)
            .header(hyper::header::CONTENT_TYPE, "text/plain")
            .body(Body::from(res))
            .unwrap()),
        Err(e) => {
            log::error!("error: {:?}, url: {}", &e, &url);
            let _ = messages.send(call_messages::Message {
                call_id: Some(call_id),
                data: call_messages::Data::Auth(call_messages::Auth::Finished {
                    result: Err(storage_models::AuthError::IncomingRequestError {
                        url,
                        error: e.0.clone(),
                    }),
                }),
            });
            Ok(Response::builder()
                .status(hyper::StatusCode::BAD_REQUEST)
                .header(hyper::header::CONTENT_TYPE, "text/plain")
                .body(Body::from(e.0))
                .unwrap())
        }
    }
}

impl tower::Service<Request<Body>> for ServerData {
    type Response = Response<Body>;
    type Error = RequestProcessError;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>,
    >;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let uri = req.uri();
        log::debug!("Serving {}", &uri);
        match uri.path() {
            "/ping" => {
                let rsp = Response::builder()
                    .status(200)
                    .body(Body::from("ok"))
                    .unwrap();
                Box::pin(futures::future::ok(rsp))
            }
            path if self.redirect_path.eq(path) => {
                // /dropbox?code=xxxxxxxxxxxxxxxxxxxxxxxxx&state=1234567890
                let auth_info_holder = self.auth_info_holder.clone();
                let rth = self.rth.clone();
                let state_field = self.state_field.clone();
                let redirect_url: String = self.redirect_url.clone();
                let call_id = self.call_id;
                let messages = self.messages.clone();

                let fut = async move {
                    handle_request(
                        req,
                        auth_info_holder,
                        rth,
                        state_field,
                        redirect_url,
                        call_id,
                        messages,
                    )
                    .await
                };
                Box::pin(fut)
            }
            _ => {
                let rsp = Response::builder()
                    .status(404)
                    .body(Body::from(""))
                    .unwrap();
                Box::pin(futures::future::ok(rsp))
            }
        }
    }
}

#[derive(Debug)]
struct FoldOkRes {
    pub builder: hyper::server::Builder<hyper::server::conn::AddrIncoming>,
    pub redirect_url: String,
    pub addr: std::net::SocketAddr,
}

#[derive(Debug)]
struct FoldRes {
    pub ok_res: Option<FoldOkRes>,
    pub errors: Vec<BindError>,
}

pub async fn run(
    rth: RuntimeHolder,
    auth_info_holder: clouds::AuthInfoHolder,
    addr_list: Vec<String>,
    state_field: String,
    call_id: u64,
    messages: MessagesSender<call_messages::Message>,
) {
    use std::net::{SocketAddr, ToSocketAddrs};
    use std::ops::ControlFlow;

    let rth_clone = rth.clone();
    let addr_list: Vec<(String, Result<SocketAddr, BindError>)> = addr_list
        .iter()
        .flat_map(|v| {
            let redirect_url = v.clone();
            let host_port_res: Result<String, BindError> = {
                match redirect_url.parse::<http::Uri>() {
                    Ok(uri) => match uri.host() {
                        Some(host) => match uri.port_u16() {
                            Some(port) => Ok(format!("{host}:{port}")),
                            None => match uri.scheme_str() {
                                None => Err(BindError::InvalidUrl {
                                    url: redirect_url.clone(),
                                    text: "unknown url scheme".to_string(),
                                }),
                                Some("http") => Ok(format!("{host}:80")),
                                Some("https") => Err(BindError::InvalidUrl {
                                    url: redirect_url.clone(),
                                    text: "https scheme not supported".to_string(),
                                }),
                                Some(scheme) => Err(BindError::InvalidUrl {
                                    url: redirect_url.clone(),
                                    text: format!("unknown url scheme: {scheme}"),
                                }),
                            },
                        },
                        None => Err(BindError::InvalidUrl {
                            url: redirect_url.clone(),
                            text: format!("invalid host"),
                        }),
                    },
                    Err(e) => Err(BindError::InvalidUrlOnParse {
                        url: redirect_url.clone(),
                        text: e.to_string(),
                    }),
                }
            };
            match host_port_res {
                Ok(host_port) => {
                    let res: Vec<(String, Result<SocketAddr, BindError>)> = match host_port.to_socket_addrs()
                    {
                        Ok(addrs) => addrs.map(|addr| (redirect_url.clone(), Ok(addr))).collect(),
                        Err(e) => std::iter::once((
                            redirect_url.clone(),
                            Err(BindError::CantGetHostPortFromUrl {
                                url: redirect_url.clone(),
                                text: e.to_string(),
                            }),
                        ))
                        .collect(),
                    };
                    res
                }
                Err(e) => vec![(redirect_url, Err(e))],
            }
        })
        .collect();

    let res = addr_list.iter().try_fold(
        FoldRes {
            ok_res: None,
            errors: vec![],
        },
        |mut prev, v| {
            let redirect_url = v.0.clone();
            log::debug!("try {}", v.0);
            match &v.1 {
                Ok(addr) => match hyper::Server::try_bind(addr) {
                    Ok(builder) => {
                        log::debug!("binded to '{}'", addr);
                        ControlFlow::Break(FoldRes {
                            ok_res: Some(FoldOkRes {
                                builder,
                                redirect_url,
                                addr: addr.clone(),
                            }),
                            errors: prev.errors,
                        })
                    }
                    Err(e) => {
                        prev.errors.push(BindError::Error {
                            addr: addr.clone(),
                            text: e.to_string(),
                        });
                        ControlFlow::Continue(prev)
                    }
                },
                Err(e) => {
                    prev.errors.push(e.clone());
                    ControlFlow::Continue(prev)
                }
            }
        },
    );

    match res {
        ControlFlow::Break(FoldRes {
            ok_res:
                Some(FoldOkRes {
                    builder,
                    redirect_url,
                    addr,
                }),
            errors,
        }) => {
            if !errors.is_empty() {
                log::debug!("bind errors: {:?}", errors);
            }
            let _ = messages.send(call_messages::Message {
                call_id: Some(call_id),
                data: call_messages::Data::Auth(call_messages::Auth::Binded {
                    redirect_url: redirect_url.clone(),
                    addr: addr.clone(),
                    state_field: state_field.clone(),
                }),
            });

            let make_service = hyper::service::make_service_fn(|_conn| {
                let svc = ServerData {
                    rth: rth.clone(),
                    auth_info_holder: auth_info_holder.clone(),
                    state_field: state_field.clone(),
                    redirect_url: redirect_url.clone(),
                    redirect_path: {
                        let uri: http::Uri = redirect_url.parse().unwrap();
                        uri.path().to_string()
                    },
                    call_id,
                    messages: messages.clone(),
                };
                async move { Ok::<_, std::convert::Infallible>(svc) }
            });

            let (sender, receiver) = tokio::sync::oneshot::channel::<()>();
            let (shutdown_awaiter_s, shutdown_awaiter_r) = tokio::sync::oneshot::channel::<()>();
            let server = builder
                .executor(rth_clone.clone())
                .serve(make_service)
                .with_graceful_shutdown(async {
                    receiver.await.ok();
                });
            let _ = messages.send(call_messages::Message {
                call_id: Some(call_id),
                data: call_messages::Data::Auth(call_messages::Auth::LocalServerShutdowner {
                    cancel_informer: sender,
                    cancel_awaiter: shutdown_awaiter_r,
                }),
            });

            log::debug!(
                "Server running at {:?}, redirect url: {:?}",
                &addr,
                &redirect_url
            );
            let res = server.await;
            let _ = shutdown_awaiter_s.send(());
            match res {
                Ok(_) => {
                    log::debug!("server.await.OK");
                }
                Err(e) => {
                    log::error!("Server error: {}", e.to_string());
                    let _ = messages.send(call_messages::Message {
                        call_id: Some(call_id),
                        data: call_messages::Data::Auth(call_messages::Auth::Finished {
                            result: Err(storage_models::AuthError::LocalServerError(e.to_string())),
                        }),
                    });
                }
            }
        }
        ControlFlow::Break(FoldRes { ok_res: None, .. }) => panic!(),
        ControlFlow::Continue(v) => {
            let _ = messages.send(call_messages::Message {
                call_id: Some(call_id),
                data: call_messages::Data::Auth(call_messages::Auth::Finished {
                    result: Err(storage_models::AuthError::BindErrors { errors: v.errors }),
                }),
            });
        }
    };
}
