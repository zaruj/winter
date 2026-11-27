use std::error::Error as StdError;

pub type Result<T> = std::result::Result<T, Error>;

/// Errors carry rendered messages so they can be cloned, compared and sent to the GUI.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("cannot read kubeconfig: {0}")]
    Kubeconfig(String),

    #[error("context \"{0}\" is not in kubeconfig")]
    UnknownContext(String),

    #[error("cannot connect to \"{context}\": {message}")]
    Connect { context: String, message: String },

    #[error("timed out connecting to \"{0}\"")]
    ConnectTimeout(String),

    #[error("{0}")]
    Kube(String),

    #[error("{0}")]
    Aws(String),

    #[error("terminal: {0}")]
    Terminal(String),

    #[error("terminal session {0} is not open")]
    UnknownSession(u32),
}

impl From<kube::Error> for Error {
    fn from(error: kube::Error) -> Self {
        Self::Kube(describe(&error))
    }
}

/// Joins an error with its sources, since kube's top-level messages (e.g. "ServiceError: client
/// error (Connect)") hide the useful part, such as "connection refused".
pub(crate) fn describe(error: &dyn StdError) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = cause.source();
    }
    message
}
