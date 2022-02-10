#[derive(thiserror::Error, Debug, Clone)]
pub enum BindError {
    #[error("can't bind to '{addr}': {text}")]
    Error {
        addr: std::net::SocketAddr,
        text: String,
    },
    #[error("can't get host:port from url '{url}': {text}")]
    CantGetHostPortFromUrl { url: String, text: String },
    #[error("invalid url '{url}': {text}")]
    InvalidUrl { url: String, text: String },
    #[error("invalid url on parse '{url}': {text}")]
    InvalidUrlOnParse { url: String, text: String },
}
