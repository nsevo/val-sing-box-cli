#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    User {
        message: String,
        hint: Option<String>,
    },

    #[error("{message}")]
    Environment {
        message: String,
        hint: Option<String>,
    },

    #[error("{message}")]
    Network {
        message: String,
        hint: Option<String>,
    },

    #[error("{message}")]
    Data {
        message: String,
        hint: Option<String>,
    },

    #[error("{message}")]
    Runtime {
        message: String,
        hint: Option<String>,
    },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    #[error(transparent)]
    UrlParse(#[from] url::ParseError),
}

impl AppError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::User { .. } => 2,
            Self::Environment { .. } => 3,
            Self::Network { .. } | Self::Reqwest(_) => 4,
            Self::Data { .. } | Self::SerdeJson(_) | Self::UrlParse(_) => 5,
            Self::Runtime { .. } | Self::Io(_) => 1,
        }
    }

    pub fn error_code(&self) -> &str {
        match self {
            Self::User { .. } => "USER_ERROR",
            Self::Environment { .. } => "ENVIRONMENT_ERROR",
            Self::Network { .. } | Self::Reqwest(_) => "NETWORK_ERROR",
            Self::Data { .. } | Self::SerdeJson(_) | Self::UrlParse(_) => "DATA_ERROR",
            Self::Runtime { .. } => "RUNTIME_ERROR",
            Self::Io(_) => "IO_ERROR",
        }
    }

    pub fn hint(&self) -> Option<&str> {
        match self {
            Self::User { hint, .. }
            | Self::Environment { hint, .. }
            | Self::Network { hint, .. }
            | Self::Data { hint, .. }
            | Self::Runtime { hint, .. } => hint.as_deref(),
            _ => None,
        }
    }

    pub fn user_with_hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::User {
            message: message.into(),
            hint: Some(hint.into()),
        }
    }

    pub fn env(message: impl Into<String>) -> Self {
        Self::Environment {
            message: message.into(),
            hint: None,
        }
    }

    pub fn env_with_hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::Environment {
            message: message.into(),
            hint: Some(hint.into()),
        }
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::Network {
            message: message.into(),
            hint: None,
        }
    }

    pub fn network_with_hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::Network {
            message: message.into(),
            hint: Some(hint.into()),
        }
    }

    pub fn data(message: impl Into<String>) -> Self {
        Self::Data {
            message: message.into(),
            hint: None,
        }
    }

    pub fn data_with_hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::Data {
            message: message.into(),
            hint: Some(hint.into()),
        }
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self::Runtime {
            message: message.into(),
            hint: None,
        }
    }

    pub fn runtime_with_hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::Runtime {
            message: message.into(),
            hint: Some(hint.into()),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
