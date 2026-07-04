use serde_json::Value as Json;

#[cfg(target_arch = "wasm32")]
pub fn rick_electrums() -> Vec<Json> {
    vec![
        json!({ "url": "electrum1.cipig.net:30017", "protocol": "WSS" }),
        json!({ "url": "electrum2.cipig.net:30017", "protocol": "WSS" }),
        json!({ "url": "electrum3.cipig.net:30017", "protocol": "WSS" }),
    ]
}

#[cfg(not(target_arch = "wasm32"))]
pub fn rick_electrums() -> Vec<Json> {
    vec![
        json!({ "url": "electrum1.cipig.net:10017" }),
        json!({ "url": "electrum2.cipig.net:10017" }),
        json!({ "url": "electrum3.cipig.net:10017" }),
    ]
}

#[allow(dead_code)]
#[cfg(target_arch = "wasm32")]
pub fn morty_electrums() -> Vec<Json> {
    vec![
        json!({ "url": "electrum1.cipig.net:30018", "protocol": "WSS" }),
        json!({ "url": "electrum2.cipig.net:30018", "protocol": "WSS" }),
        json!({ "url": "electrum3.cipig.net:30018", "protocol": "WSS" }),
    ]
}

#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
pub fn morty_electrums() -> Vec<Json> {
    vec![
        json!({ "url": "electrum1.cipig.net:10018" }),
        json!({ "url": "electrum2.cipig.net:10018" }),
        json!({ "url": "electrum3.cipig.net:10018" }),
    ]
}

// DOC/MARTY are the live successors of the retired RICK/MORTY dev assetchains.
// The legacy `electrum*.cipig.net:10017` (RICK) and `:10018` (MORTY) services no
// longer listen; cipig moved them to per-coin port allocations on the same
// hosts: DOC on :10020 (TCP) / :30020 (WSS), MARTY on :10021 / :30021. The
// RICK/MORTY helpers above are kept only so that other tests still using them
// stay compilable until they are migrated as well.
#[allow(dead_code)]
#[cfg(target_arch = "wasm32")]
pub fn doc_electrums() -> Vec<Json> {
    vec![
        json!({ "url": "electrum1.cipig.net:30020", "protocol": "WSS" }),
        json!({ "url": "electrum2.cipig.net:30020", "protocol": "WSS" }),
        json!({ "url": "electrum3.cipig.net:30020", "protocol": "WSS" }),
    ]
}

#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
pub fn doc_electrums() -> Vec<Json> {
    vec![
        json!({ "url": "electrum1.cipig.net:10020" }),
        json!({ "url": "electrum2.cipig.net:10020" }),
        json!({ "url": "electrum3.cipig.net:10020" }),
    ]
}

#[allow(dead_code)]
#[cfg(target_arch = "wasm32")]
pub fn marty_electrums() -> Vec<Json> {
    vec![
        json!({ "url": "electrum1.cipig.net:30021", "protocol": "WSS" }),
        json!({ "url": "electrum2.cipig.net:30021", "protocol": "WSS" }),
        json!({ "url": "electrum3.cipig.net:30021", "protocol": "WSS" }),
    ]
}

#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
pub fn marty_electrums() -> Vec<Json> {
    vec![
        json!({ "url": "electrum1.cipig.net:10021" }),
        json!({ "url": "electrum2.cipig.net:10021" }),
        json!({ "url": "electrum3.cipig.net:10021" }),
    ]
}

#[allow(dead_code)]
#[cfg(target_arch = "wasm32")]
pub fn tbtc_electrums() -> Vec<Json> {
    vec![
        json!({ "url": "electrum1.cipig.net:30068", "protocol": "WSS" }),
        json!({ "url": "electrum2.cipig.net:30068", "protocol": "WSS" }),
        json!({ "url": "electrum3.cipig.net:30068", "protocol": "WSS" }),
    ]
}

#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
pub fn tbtc_electrums() -> Vec<Json> {
    vec![
        json!({ "url": "blockstream.info:143" }),
        json!({ "url": "blackie.c3-soft.com:57005" }),
        json!({ "url": "testnet.qtornado.com:51001" }),
    ]
}

#[cfg(target_arch = "wasm32")]
pub fn qtum_electrums() -> Vec<Json> {
    vec![
        json!({ "url": "electrum1.cipig.net:30071", "protocol": "WSS" }),
        json!({ "url": "electrum2.cipig.net:30071", "protocol": "WSS" }),
        json!({ "url": "electrum3.cipig.net:30071", "protocol": "WSS" }),
    ]
}

#[cfg(not(target_arch = "wasm32"))]
pub fn qtum_electrums() -> Vec<Json> {
    vec![
        json!({ "url": "s1.qtum.info:50001" }),
        json!({ "url": "s4.qtum.info:50001" }),
        json!({ "url": "s1.qtum.info:50001" }),
    ]
}
