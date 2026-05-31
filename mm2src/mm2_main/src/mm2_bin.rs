#![allow(uncommon_codepoints)]
#![allow(dead_code)]
#![allow(mismatched_lifetime_syntaxes)]
#![allow(clippy::result_large_err)]
#![allow(clippy::diverging_sub_expression)]
#![allow(clippy::explicit_auto_deref)]
#![recursion_limit = "512"]

#[macro_use] extern crate common;
#[macro_use] extern crate fomat_macros;
#[macro_use] extern crate gstuff;
#[macro_use] extern crate mm2_metrics;
#[macro_use] extern crate serde_json;
#[macro_use] extern crate serde_derive;
#[macro_use] extern crate serialization_derive;
#[macro_use] extern crate ser_error_derive;

#[path = "mm2.rs"] mod mm2;

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        mm2::mm2_main()
    }
}
