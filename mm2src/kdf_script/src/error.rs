// Script error type. Slimmed to the variants actually surfaced by the
// classifier and parser (the full interpreter error set is omitted —
// kdf_script does not include a script interpreter).

use crate::Opcode;
use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Encountered byte cannot be decoded as a known opcode, or a
    /// PUSHDATA prefix advertised more bytes than the script contains.
    BadOpcode,
    /// Reserved for callers that need to propagate a disabled-opcode
    /// signal from a higher layer (kept for API compatibility).
    DisabledOpcode(Opcode),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadOpcode => f.write_str("bad opcode"),
            Error::DisabledOpcode(op) => write!(f, "disabled opcode: {:?}", op),
        }
    }
}

impl std::error::Error for Error {}
