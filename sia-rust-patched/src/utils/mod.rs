mod tx_builder;
pub use tx_builder::*;

use serde::{Deserialize, Deserializer};

/// Run a block of unit tests on both wasm32 and non-wasm32 targets
/// Can only be used once per scope due to wasm_bindgen initialization
#[cfg(test)]
macro_rules! cross_target_tests {
    ($($test_fn:item)*) => {
        #[cfg(all(test, target_arch = "wasm32"))]
        use wasm_bindgen_test::*;

        #[cfg(all(test, target_arch = "wasm32"))]
        wasm_bindgen_test_configure!(run_in_browser);

        $(
            #[cfg(all(test, target_arch = "wasm32"))]
            #[wasm_bindgen_test]
            $test_fn

            #[cfg(not(target_arch = "wasm32"))]
            #[test]
            $test_fn
        )*
    };
}

/// Deserialize a null value as an empty vector.
/// Allows using Vec<> instead of Option<Vec<>> for convenience.
pub(crate) fn deserialize_null_as_empty_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer).map(|opt| opt.unwrap_or_default())
}
