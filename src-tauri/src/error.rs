use serde::{Serialize, Serializer};

/// An error sent to the frontend as its message.
#[derive(Debug)]
pub struct CommandError(String);

impl CommandError {
    pub fn internal(error: &dyn std::error::Error) -> Self {
        Self(format!("internal error: {error}"))
    }
}

impl From<winter_core::Error> for CommandError {
    fn from(error: winter_core::Error) -> Self {
        Self(error.to_string())
    }
}

impl Serialize for CommandError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}
