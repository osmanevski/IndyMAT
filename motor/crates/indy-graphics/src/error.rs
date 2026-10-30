//! Errors raised to the interpreter. `id` is a MATLAB-style identifier the
//! interpreter can put into `MException.identifier`.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicsError {
    pub id: String,
    pub message: String,
}

impl GraphicsError {
    pub fn new(id: &str, message: impl Into<String>) -> GraphicsError {
        GraphicsError {
            id: id.to_string(),
            message: message.into(),
        }
    }

    pub fn invalid_handle() -> GraphicsError {
        GraphicsError::new("MATLAB:hg:InvalidHandle", "invalid graphics handle")
    }

    pub fn bad_property(kind: &str, name: &str) -> GraphicsError {
        GraphicsError::new(
            "MATLAB:hg:InvalidProperty",
            format!("invalid property '{name}' for {kind} object"),
        )
    }

    pub fn bad_value(name: &str) -> GraphicsError {
        GraphicsError::new(
            "MATLAB:hg:InvalidPropertyValue",
            format!("invalid value for property '{name}'"),
        )
    }

    pub fn args(func: &str, detail: impl fmt::Display) -> GraphicsError {
        GraphicsError::new("MATLAB:narginchk:invalidInput", format!("{func}: {detail}"))
    }

    pub fn unsupported(func: &str, detail: impl fmt::Display) -> GraphicsError {
        GraphicsError::new(
            "IndyMAT:graphics:unsupported",
            format!("{func}: {detail} is not supported by the Rust graphics engine"),
        )
    }
}

impl fmt::Display for GraphicsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GraphicsError {}

pub type Result<T> = std::result::Result<T, GraphicsError>;
