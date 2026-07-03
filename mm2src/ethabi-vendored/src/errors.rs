#![allow(unknown_lints)]
#![allow(missing_docs)]
// The vendored `error_chain!` expansion probes an old `has_error_description_deprecated`
// cfg that no longer exists; the check is internal to the macro and cannot be
// influenced from here without upgrading `error_chain`.
#![allow(unexpected_cfgs)]

use std::{num, string};
use {hex, serde_json};

error_chain! {
    foreign_links {
        SerdeJson(serde_json::Error);
        ParseInt(num::ParseIntError);
        Utf8(string::FromUtf8Error);
        Hex(hex::FromHexError);
    }

    errors {
        InvalidName(name: String) {
            description("Invalid name"),
            display("Invalid name `{}`", name),
        }

        InvalidData {
            description("Invalid data"),
            display("Invalid data"),
        }
    }
}
