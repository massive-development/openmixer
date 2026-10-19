use thiserror::Error;

/// Why a string is not a valid parameter id.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParamError {
    #[error("empty parameter id")]
    Empty,
    #[error("malformed parameter id {0:?}")]
    Malformed(String),
    #[error("unknown parameter id {0:?}")]
    Unknown(String),
    #[error("{what} index out of range in {id:?} (max {max})")]
    OutOfRange {
        id: String,
        what: &'static str,
        max: u16,
    },
}
