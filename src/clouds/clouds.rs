use thiserror;
use std::sync::Arc;

#[derive(Copy, Clone)]
pub enum CloudId {
    Dropbox = 1
}

#[derive(Clone)]
pub struct AuthInfo {
    pub token: String,
    pub refresh_token: String,
    pub client_id: String,
    pub secret: String,
    pub redirect_addresses: Vec<String>,
    pub write_mutex: Arc<tokio::sync::RwLock<u64>>,
}

pub type AuthInfoHolder = Arc<tokio::sync::RwLock<AuthInfo>>;

#[derive(thiserror::Error, Debug, Clone)]
pub enum BaseError {
/*
    #[error("Bad result: (action: {action:?}, hint: {hint:?}, code: {result_code:?}, data: {text:?}, additional: {text:?})")]
    BadResultCode {
        action: String,
        hint: String,
        result_code: u16,
        text: String,
        additional: String,
    },

 */
    #[error("Expired access token{:?}", extract_rte(refresh_error))]
    ExpiredAccessToken {
        refresh_error: Option<Box<RefreshTokenCallError>>
    },
    #[error("Aggregate response body failed: (action: {action:?}, hint: {hint:?}, error: {error:?})")]
    ResponseBodyAggregate {
        action: String,
        hint: String,
        error: String,
    },
    #[error("Aggregate error body failed: (action: {action:?}, hint: {hint:?}, error: {error:?})")]
    ErrorBodyAggregate {
        action: String,
        hint: String,
        error: String,
    },
    #[error("response wait failed: (action: {action:?}, hint: {hint:?}, error: {error:?})")]
    ResponseWait {
        action: String,
        hint: String,
        error: String,
    },
    #[error("response body deserialization failed: (action: {action:?}, hint: {hint:?}, error: {error:?})")]
    ResponseBodyDeserialization {
        action: String,
        hint: String,
        error: String,
    },
    #[error("response body deserialization failed: (action: {action:?}, error: {error:?}, data: {error_data:?})")]
    ErrorBodyDeserialization {
        action: String,
        error: String,
        error_data: String,
    },
    #[error("unknown api error data structure: (action: {action:?}, hint: {hint:?}, error: {error:?})")]
    UnknownApiErrorStructure {
        action: String,
        hint: String,
        error: String,
    },
    #[error("access token malformed: (action: {action:?})")]
    AccessTokenMalformed {
        action: String,
    },
    #[error("unknown api error result: (action: {action:?}, status: {status:?}, error: {error:?})")]
    UnknownApiErrorResult {
        action: String,
        status: u16,
        error: String,
    },

//    #[error("unknown data store error")]
//    Unknown,
}

fn extract_rte(e: &Option<Box<RefreshTokenCallError>>) -> String {
    match e {
        Some(e) => {
            format!(", with refresh token error: {}", (*e).to_string())
        },
        None => "".to_owned()
    }
}


impl BaseError {
    pub fn is_permanent(&self) -> bool {
        match &self {
            Self::ExpiredAccessToken {..}
            | Self::ResponseBodyDeserialization {..}
            | Self::UnknownApiErrorStructure {..}
            | Self::AccessTokenMalformed {..}
            => true,

            Self::ResponseBodyAggregate {..}
//            | Self::BadResultCode {..}
            | Self::ResponseWait {..}
            | Self::ErrorBodyAggregate {..}
            | Self::ErrorBodyDeserialization {..}
            | Self::UnknownApiErrorResult {..}
            => false
        }
    }

    pub fn is_expired_token(&self) -> bool {
        match self {
            Self::ExpiredAccessToken {..} => true,
            _ => false
        }
    }

    pub fn is_token_error(&self) -> bool {
        match self {
            Self::ExpiredAccessToken {..} | Self::AccessTokenMalformed {..} => true,
            _ => false
        }
    }

    pub fn e_response_wait(action: &str, e: &hyper::Error) -> Self {
        log::error!("{}(request await error): {}", action, e.to_string());
        Self::ResponseWait {
            action: action.to_owned(),
            hint: "".to_owned(),
            error: e.to_string(),
        }
    }

    pub fn e_response_body_aggregate(action: &str, e: impl std::error::Error) -> Self {
        log::error!("{}(body aggregate error): {}", action, e.to_string());
        Self::ResponseBodyAggregate {
            action: action.to_owned(),
            hint: "".to_owned(),
            error: e.to_string(),
        }
    }

    pub fn e_response_body_deserialization(action: &str, e: serde_json::Error) -> Self {
        log::error!("{}(response body deserialization error): {}", action, e.to_string());
        Self::ResponseBodyDeserialization {
            action: action.to_owned(),
            hint: "".to_owned(),
            error: e.to_string(),
        }
    }

    pub fn e_error_body_aggregate(action: &str, e: Box<dyn std::error::Error>) -> Self {
        log::error!("{}(error body aggregate error): {}", action, e.to_string());
        Self::ErrorBodyAggregate {
            action: action.to_owned(),
            hint: "".to_owned(),
            error: e.to_string(),
        }
    }

    pub fn e_error_body_deserialization(action: &str, e: serde_json::Error, error_data: &String) -> Self {
        log::error!("{}(error body deserialization error): {}, data: '{}'", action, e.to_string(), error_data);
        Self::ErrorBodyDeserialization {
            action: action.to_owned(),
            error: e.to_string(),
            error_data: error_data.clone(),
        }
    }

    pub fn e_access_token_malformed(action: &str) -> Self {
        log::error!("{}: access token malformed", action);
        Self::AccessTokenMalformed {
            action: action.to_owned(),
        }
    }

    pub fn e_unknown_api_error_result(action: &str, status: u16, error: &String) -> Self {
        log::error!("{}(unknown api error result for status {}): {}", action, status, error);
        Self::UnknownApiErrorResult {
            action: action.to_owned(),
            error: error.clone(),
            status: status,
        }
    }
}

pub trait BaseErrorAccess {
    fn get_base_error(&self) -> Option<&BaseError>;
//    fn get_base_error_mut(&mut self) -> Option<&mut (&mut BaseCloudError)>;
    fn get_base_error_mut(&mut self) -> Option<&mut BaseError>;
    fn from_base(base: BaseError) -> Self;
//    fn from_base_selfed(&self, base: BaseCloudError) -> Self;
}


#[derive(thiserror::Error, Debug)]
pub enum AuthCodeToTokensCallError {
    #[error(transparent)]
    Base(BaseError),
    #[error("Some error with text: {0}")]
    Other(String),
}

impl AuthCodeToTokensCallError {
    pub fn is_permanent(&self) -> bool {
        match &self {
            Self::Base(base) => base.is_permanent(),
            Self::Other(_) => true // ?
        }
    }
}

impl BaseErrorAccess for AuthCodeToTokensCallError {
    fn get_base_error(&self) -> Option<&BaseError> {
        match self {
            Self::Base(res) => Some(res),
            _ => None
        }
    }

    fn get_base_error_mut(&mut self) -> Option<&mut BaseError> {
        match self {
            Self::Base(res) => Some(res),
            _ => None
        }
    }

    fn from_base(base: BaseError) -> Self {
        Self::Base(base)
    }
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum RefreshTokenCallError {
    #[error(transparent)]
    Base(BaseError),
    #[error("Invalid refresh token")]
    InvalidRefreshToken_,
//    #[error("Invalid refresh token")]
//    InvalidRefreshToken,
}

impl RefreshTokenCallError {
    pub fn is_permanent(&self) -> bool {
        match &self {
            Self::Base(base) => base.is_permanent(),
            Self::InvalidRefreshToken_ => true
        }
    }
}

impl BaseErrorAccess for RefreshTokenCallError {

    fn get_base_error(&self) -> Option<&BaseError> {
        match self {
            Self::Base(res) => Some(res),
            _ => None
        }
    }

    fn get_base_error_mut(&mut self) -> Option<&mut BaseError> {
        match self {
            Self::Base(res) => Some(res),
            _ => None
        }
    }

    fn from_base(base: BaseError) -> Self {
        Self::Base(base)
    }
}


