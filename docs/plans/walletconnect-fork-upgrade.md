# Plan: WalletConnect relay transport — tungstenite bump

> **Status:** not started — scoping only, 2026-08-06.

## Goal

Clear `RUSTSEC-2023-0065` (tungstenite DoS via unbounded frame buffering on
large frames) from the WalletConnect relay client's native transport, without
changing the WalletConnect pairing/relay wire protocol or breaking the WASM
transport path.

## Confirmed root cause and ownership (2026-08-06)

`deny.toml` currently labels this "upstream/fork-blocked", but tracing the
actual dependency chain shows **reloaded/Komodo already owns every fork in
the path** — this is not blocked on a third party, it's just unstarted work:

```
kdf_walletconnect
 └─ relay_client (git: komodoplatform/walletconnectrust, tag k-0.1.3)
     └─ tokio-tungstenite-wasm 0.1.1-alpha.0
        (git: KomodoPlatform/tokio-tungstenite-wasm, rev 8fc7e2f
         — itself a Komodo fork of TannerRogalsky/tokio-tungstenite-wasm)
         └─ [target.not(wasm32)] tokio-tungstenite = "0.16"
             └─ tungstenite 0.16.0   ← the flagged crate
         └─ [target.wasm32] raw `web-sys::WebSocket` (no tungstenite at all)
```

Verified directly against both forks' checked-out manifests: `relay_client`
depends unconditionally on `tokio-tungstenite-wasm`, which internally
branches on `target_arch = "wasm32"` — the WASM build path uses a raw
`web-sys::WebSocket` shim with **no tungstenite dependency at all**, so this
advisory only affects the **native** relay transport. That also means the
WASM path is not at risk here and needs no change.

The fix is a two-hop, fully-owned fork bump:

1. In `KomodoPlatform/tokio-tungstenite-wasm`, bump `tokio-tungstenite =
   "0.16"` (native target block) to a current release (pulls a current,
   non-vulnerable `tungstenite`). Port any native-path API breakage — check
   `tokio-tungstenite`'s changelog between 0.16 and the target version for
   `WebSocketStream`/message-type changes; the crate's native branch is a
   thin wrapper so expect a small, mechanical diff, but verify.
2. In `komodoplatform/walletconnectrust`'s `relay_client/Cargo.toml`, bump
   the pinned `rev` to the new `tokio-tungstenite-wasm` commit from step 1,
   and cut a new tag (or bump `k-0.1.3` → `k-0.1.4`) for reloaded to pin to.
3. In this repo's workspace `Cargo.toml`, bump the `walletconnectrust`
   `tag` for `pairing_api`/`relay_client`/`relay_rpc`/`wc_common` to the new
   tag from step 2.

## Scope

- `KomodoPlatform/tokio-tungstenite-wasm` (external repo, owned — needs a
  PR/commit there first)
- `komodoplatform/walletconnectrust` (external repo, owned — needs a
  PR/commit there second, referencing the updated dep)
- `Cargo.toml` (workspace) — `pairing_api`/`relay_client`/`relay_rpc`/
  `wc_common` git `tag` bump (this repo's side, last)
- `mm2src/kdf_walletconnect/` — no expected direct code change (it consumes
  `relay_client`'s public API, not `tungstenite` directly), but run its full
  test suite since it's the only consumer of the bumped transport

## Planned order

1. Confirm current upstream `tungstenite`/`tokio-tungstenite` stable release
   and scan its changelog from 0.16 for breaking API changes relevant to
   `tokio-tungstenite-wasm`'s (small) native-path usage.
2. Land the bump in `KomodoPlatform/tokio-tungstenite-wasm`; build + test
   that crate standalone (native target) before touching anything else.
3. Land the `rev`/tag bump in `komodoplatform/walletconnectrust`'s
   `relay_client`; build + test `relay_client` standalone.
4. Bump the tag in this repo's workspace `Cargo.toml`; full
   `cargo build`/`cargo test -p kdf_walletconnect` + WASM check (should be a
   no-op for WASM per the root-cause section above, but verify — a no-op
   confirmation is still a required check, not an assumption).
5. Remove `RUSTSEC-2023-0065` from `deny.toml`'s `ignore` list;
   `cargo deny check advisories` must stay green.

## Risk areas

- Native relay transport reconnect/backoff behavior if `tokio-tungstenite`'s
  API shape changed between 0.16 and the target version
- WalletConnect pairing/session-persistence flows that depend on the relay
  transport staying connected during a session — regression-test an actual
  pairing + message round-trip, not just a compile check
- Keeping the WASM path untouched (it doesn't use tungstenite) — a
  regression here would be a sign the bump touched more than intended

## Required verification

- `cargo tree -i tungstenite@0.16.0` returns nothing after the bump
- Native `cargo test -p kdf_walletconnect` and `-p relay_client` (via
  `--manifest-path` against the updated fork checkout during development)
- `cargo check --target wasm32-unknown-unknown -p kdf_walletconnect` stays
  clean (regression check that the WASM path, which doesn't touch
  tungstenite, wasn't broken)
- Manual smoke test: pair with a real WalletConnect-compatible wallet and
  round-trip at least one signing request over the native relay transport
- `cargo deny check advisories` green with the ignore line removed

## Release posture

Independent of `secp256k1-migration.md` and `solana-sdk-upgrade.md` — can
run in parallel with either. Needs its own branch
(`dep/walletconnect-tungstenite`) off `dev` since it spans two external
fork repos plus this one; land the two upstream-fork commits and get them
merged/tagged before starting the reloaded-side branch, so the reloaded PR
is a clean single-tag bump rather than three repos moving at once.
