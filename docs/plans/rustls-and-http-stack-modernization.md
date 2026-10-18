# Plan: rustls 0.23 and the hyper 1 HTTP stack

**Status:** done (shipped 2026-10-17/18, PRs #37–#40, #42, #44; two field checks open) — **Last reviewed:** 2026-10-18

> **Written retrospectively, after the work shipped**, under rule P3 in
> [`docs/GIT_FLOW_AND_WORKING_PROCESS.md`](../GIT_FLOW_AND_WORKING_PROCESS.md#plans-and-the-changelog):
> the series carries several decisions that the diff cannot show — what was *not* done and
> why, and which costs were knowingly accepted. Those are recorded in §3; §4 lists what is
> still unverified, which is a live risk rather than history. The per-advisory ledger stays in
> [`v0.2.0-dependency-hygiene.md`](v0.2.0-dependency-hygiene.md); this file is the decision
> record for the one piece of work, and
> [`walletconnect-fork-upgrade.md`](walletconnect-fork-upgrade.md) is superseded by it.

## 1. What this was

The workspace carried **three stale `rustls` majors** (0.19.1, 0.20.4, 0.22.4) and **two
hyper majors**, and `cargo deny check advisories` had never been green: RUSTSEC-2026-0349
(and, before #37, RUSTSEC-2026-0346) matched one rustls line or another whatever was fixed
first. The TLS layer under Electrum, the EVM HTTP client, WalletConnect, the P2P `wss`
transport, lightwalletd gRPC and the Tendermint HTTP client all sat on those versions.

**It was hygiene, not incident response, and that was measured rather than assumed.**
RUSTSEC-2026-0349 is about losing *post-quantum* confidentiality when a client is steered off
a hybrid key-exchange group; none of the affected rustls versions implements one (zero
`mlkem`/`kyber` references; offered groups are X25519, secp256r1, secp384r1 only), so there
was nothing to downgrade from, and the advisory's own text states no authentication bypass and
no key compromise. RUSTSEC-2026-0346 needs a custom `ClientSessionStore` that `Debug`-formats
session values; there is none. The maintainer still chose to fix the dependencies rather than
add `deny.toml` ignore lines, which is why the series exists and why no ignores were added for
either advisory.

## 2. What shipped, by increment

The work was cut into increments by `rustls 0.20.4` consumer group, each its own PR, so each
was independently reviewable and revertible. All merged 2026-10-17/18.

| # | PR | Increment | Effect |
|---|---|---|---|
| 1 | #37 | rustls 0.19.1 out; `tonic` 0.11 → 0.14 (rustls 0.22.4 out) | 2 advisory IDs → 1 |
| 2 | #38 | `sia-rust` pin bumped off `reqwest` 0.11 / rustls 0.20 | 5 consumer groups → 4 |
| 3 | #39 | **I1** `libp2p-websocket` vendored, TLS ported to rustls 0.23 | `futures-rustls` out |
| 4 | #40 | **I2** `tokio-tungstenite` group via `[patch]`; **I4** Electrum TLS → rustls 0.23 | only `hyper-rustls 0.23` left |
| 5 | #42 | **I5** native HTTP client and server → hyper 1 | **rustls 0.20.4 out; advisories green** |
| 6 | #44 | **I6** `cargo-deny` bans + CI `bans` job | regressions become visible |

End state, measured with `cargo tree --target all -e all`: one major each of `hyper` (1.12.0),
`http` (1.4.0), `h2` (0.4.20), `http-body` (1.1.0), `rustls` (0.23.46) and `ring` (0.17.14);
no `aws-lc` anywhere, native or wasm32; 15 package versions left the lockfile in #42 alone.
`cargo deny check advisories` and `cargo deny check bans` both pass.

Smaller things the series also closed: six stale `deny.toml` ignores that no longer matched
anything (`RUSTSEC-2024-0336`, `-2023-0052`, `-2025-0009`, `-2026-0258`, `-2026-0049`, the
three `rustls-webpki` rows and `RUSTSEC-2023-0065`); the ~110-entry `skip` list, dead under
`multiple-versions = "allow"`; and the listen backlog regression that hyper 1 introduced
(restored to 1024 with `SO_REUSEADDR` set explicitly, measured with `ss -ltn` and `strace`).

## 3. The decisions the code does not record

### 3.1 `libp2p-websocket` was vendored, not version-bumped (#39)

Checked, not assumed. The `KomodoPlatform/rust-libp2p` fork has **no commit newer than the
pinned revision** — no tag past `k-0.52.12`, `komodo-master` identical for websocket, and its
`master` carries `libp2p-websocket 0.43.0` on `futures-rustls` 0.24, which is rustls **0.21**
and equally unusable. Upstream reaches rustls 0.23 only at `libp2p-websocket 0.43.2`, part of
libp2p **0.53**, and moving to 0.53 means **abandoning the fork and its relay-mesh gossipsub
extension and wasm-ext work** — a separate project, not a dependency bump.

So the crate was vendored, following the `[patch]` precedent `libp2p-gossipsub` and
`libp2p-yamux` already set, and only the TLS hunks were taken from upstream 0.43.2 (~55 lines
in `tls.rs`, one in `framed.rs`, one doc-test line); upstream's accompanying core/identity/tcp
bumps were not. The cost accepted with this: the fork's TLS is now ours to maintain, and the
vendored crate's own tests are **not wired into CI**, so they can rot (they also cannot be run
from a git worktree — Cargo resolves the main checkout's workspace root — the same limitation
the yamux vendor already had).

Two choices inside the port are not visible as choices in the diff:

- **`tls12` is enabled explicitly.** Dropping default features would have removed TLS 1.2,
  which rustls 0.20 offered, and **every TLS-1.2-only peer would have failed to handshake** —
  a silent capability regression no build or lint catches.
- **No process-global crypto provider is consulted**; both configs use
  `builder_with_provider(ring::default_provider())`, so this crate cannot trip rustls's
  runtime "could not automatically determine the process-level CryptoProvider" panic.

Upstream's `PrivateKey::new` panics on a malformed key file; the vendored copy returns
`Result`, which is why `build_dns_ws_transport` is now fallible and
`AdexBehaviourError::WssConfig` / `P2PInitError::InvalidWssConfig` exist. `P2PInitError` is
`Serialize`, so that adds a serialized error variant — acceptable because it appears only
where the process previously **aborted**.

### 3.2 `tokio-tungstenite` did **not** need vendoring — an earlier analysis reversed (#40)

The `tokio-tungstenite 0.16.1` / `tungstenite 0.16.0` group arrived through `relay_client`
(WalletConnect), which pins `KomodoPlatform/tokio-tungstenite-wasm@8fc7e2f`. That fork also
has no commit past the pinned revision, so the earlier conclusion was that a third vendored
tree was unavoidable. It was wrong: the fork's crate **name** is the upstream's, so a one-line
`[patch."https://github.com/KomodoPlatform/tokio-tungstenite-wasm.git"]` onto the **crates.io
release `=0.4.0`** resolves, and that release rides `tokio-tungstenite` 0.24 on rustls 0.23.
No vendored copy and no third owned tree.

The accepted cost: upstream 0.4.0 is **not byte-identical** to the fork revision (the fork had
added a `CloseFrame` re-export, wasm `Send + Sync`, and a TLS-field cleanup).
`relay_client` and `kdf_walletconnect` compile natively and on wasm32 and the 27
`kdf_walletconnect` tests pass, but a **live relay session was not exercised** (§4).

This is what supersedes [`walletconnect-fork-upgrade.md`](walletconnect-fork-upgrade.md),
whose whole premise was that clearing RUSTSEC-2023-0065 meant forking two Gleec-maintained
repositories permanently.

### 3.3 hyper 1 was the route for the HTTP client — "option B" (#42)

`hyper-rustls` reaches rustls 0.23 only on **hyper 1**, so the last `rustls 0.20.4` group
(`hyper-rustls 0.23.0` / `tokio-rustls 0.23.2`, used by `coins` for Tendermint HTTP and by
`common::wio`) could not be cleared without a hyper major. Option B — move the native HTTP
client *and* server to hyper 1 — was chosen over keeping hyper 0.14 and carrying the advisory.
Two consequences follow from hyper 1 itself rather than from our code:

- hyper 1 has **no `Server`**, so the RPC and Prometheus accept loops are now ours
  (`hyper_util::server::conn::auto` keeps HTTP/1 and HTTP/2-prior-knowledge on one port;
  `GracefulShutdown` replaces `with_graceful_shutdown`; accept errors back off 1 s, which
  hyper 0.14 did internally and without which a descriptor-exhaustion error would spin).
- An **idle-connection pool timer must be supplied** explicitly; in hyper 0.14 idle
  connections expired by themselves and nothing warns when the timer is missing.

Deliberate, user-visible side effect: `SlurpError` messages now carry the whole source chain
(e.g. `client error (Connect): invalid peer certificate: UnknownIssuer`), because the legacy
client's own `Display` is only `client error (Connect)` and hides the cause. No caller in the
tree matches on the old text.

#37 had already put the lightwalletd gRPC transport on hyper 1 by taking `tonic` to **0.14**,
chosen over 0.12/0.13 because it shares **prost 0.14 with `zcash_client_backend`**, and on
`tls-ring` rather than aws-lc to avoid a C/cmake build and a second crypto provider.
`tonic-prost-build` runs with default features **off**, because the default
`cleanup-markdown` feature pulls `pulldown-cmark-to-cmark` (Apache-2.0) and breaks
`cargo deny check licenses`. One line there is load-bearing and easy to lose:
`ClientTlsConfig::new().with_enabled_roots()` — from tonic 0.12 the `tls-webpki-roots`
feature no longer applies the roots implicitly, and **without that call every `https`
lightwalletd fails certificate verification**.

### 3.4 `webpki-roots` 1.0, and the distrust-after roots we knowingly gave up (#40)

The Electrum TLS port moves `webpki-roots` 0.22 → **1.0**, and 1.0 is **not the same trust set**:
121 anchors against 135, with 45 removed and 32 added. It omits roots Mozilla marks
*distrust-after* (DigiCert Global Root CA, GlobalSign Root CA, Baltimore, AAA/COMODO,
Go Daddy/Starfield Class 2, Entrust) and roots Mozilla removed outright (**TrustCor**). A
server whose only chain terminates at a dropped root was accepted before and is rejected now
in **safe** mode.

That was accepted deliberately, against a measurement rather than an argument: a probe ran the
**old** stack (rustls 0.20.4 / webpki-roots 0.22.3 / the old verifier) and the new one side by
side against every `SSL` endpoint in the public Electrum lists — 369 endpoints, **225
reachable**. Safe mode gave **identical per-server outcomes, 204 accepted and 21 rejected on
both sides**; skip-validation accepted 225/225 on both; zero diffs, all 225 negotiating TLS
1.3. (The first run showed diffs; they were all TCP timeouts from the probe's own concurrency
and vanished on a lower-concurrency rerun — worth knowing before anyone repeats it.)

The alternative — keeping the old anchor set — was rejected because it would **retain
TrustCor**, a root removed for cause, and would match no other TLS client in the tree:
`libp2p-websocket`, `tonic` and `reqwest` are all on `webpki-roots` 1.0. The residual risk is
that the published lists are not the field: a user-configured Electrum server can be the one
whose chain ends at a dropped root. Skip-validation users are unaffected.

Also deliberate in that port: `tls12` enabled explicitly (as in §3.1), no process-global
provider, and **`skip_validation` keeps its old meaning** — chain and name checks skipped, but
the handshake signature still verified by the ring provider, so the certificate must parse and
the peer must prove possession of its key. A weaker port that accepted any signature would
have widened what is accepted; this one does not.

### 3.5 `multiple-versions = "allow"` is a scoping decision (#44)

The `[bans]` section bans `aws-lc-rs` / `aws-lc-sys` / `aws-lc-fips-sys` (a second crypto
provider panics at *runtime*, not at build time, and no lint catches it) and `rustls` below
**0.23.46** (the patched advisory floor, which a transitive bump could silently undercut). Each
ban was demonstrated to fire, except `aws-lc-fips-sys`, which is preventive and untested; the
version-range syntax was cross-checked by temporarily raising the floor to `<0.23.47` and
watching the real tree's 0.23.46 fail.

`multiple-versions = "allow"` is **not a clean bill of health**. The tree still carries many
duplicate versions (bech32, bitcoin_hashes, bitflags and others) and a **duplicate-version
cleanup remains unscoped**. The ~110-entry `skip` list that recorded which duplicates had been
looked at and accepted is inert under `allow` and was removed, but it is preserved in git as
`deny.toml` at `bc47dc07d` and pointed to by a comment in `[bans]`. Flipping
`multiple-versions` to `warn` or `deny` without reinstating or re-triaging that list would
surface every duplicate at once.

The `bans` job lives in `audit.yml`, which is **not** part of `test.yml`'s `tests-passed`
aggregator, so it is a normal check and becomes required only if branch protection adds it.

### 3.6 `reqwest` stopped at 0.12, not 0.13 (#38, #44)

The `sia-rust` pin moved to the revision that took that crate to `reqwest` 0.12 and `http` 1.
**0.13 was deliberately not taken: it pulls in `aws-lc-rs`**, a second crypto provider — which
is precisely the regression #44 then made CI-visible. Zero source changes were needed upstream;
`http` 1 is API-compatible for the `Method`, `HeaderMap` and `StatusCode` usage there.

The one thing that could have bitten: the new `sia-rust` raises its `web-sys` floor to
**0.3.70** (found empirically upstream — 0.3.70 and 0.3.77 compile reqwest 0.12.28's wasm
backend, 0.3.69 and older fail on `RequestInit::set_cache` / `AbortController::abort_with_reason`).
Our lockfile resolves 0.3.94, verified after the update rather than assumed.

## 4. What is **not** verified — open, not history

1. **The new lightwalletd transport has never run against a real server.** #37 and #42 moved
   it from hyper 0.14 / tower 0.4 to hyper 1.x / tower 0.5 with rustls 0.23 on ring. Service
   definitions, call sites, `Endpoint` timeouts, HTTP/2 keep-alive and the 4 MB decode limit
   are unchanged, and `cargo test -p coins z_coin::` passes 204 tests, but only compilation and
   unit tests. Deliberate consequences to expect in the field: a different default user-agent,
   transport error text that reads differently, and `GetBlockRange` streaming through the
   hyper 1 body path. A live `https` endpoint, a sync, and a reorg if one can be provoked are
   wanted before the beta — the same gate as the reorg-recovery fix
   ([`arrr-ironwood-compatibility.md`](arrr-ironwood-compatibility.md)), and the ARRR field
   tests starting 2026-10-18 exercise exactly this path.
2. **The Electrum TLS port still wants two live checks** that the probe itself named: a re-run
   of the old-vs-new comparison **from a second network vantage point** (and against the
   operator's own configured servers), looking specifically for `UnknownIssuer` in the new
   column where the old column says `OK`; and **a real TLS-1.2-only Electrum server** — none
   of the 225 offered TLS 1.2, so the loopback test is the only evidence there.
3. **No live WalletConnect relay session** has run on the patched `tokio-tungstenite`, native
   or in a browser (§3.2).
4. **No `mm2_main`/`mm2_p2p` mesh over `wss`** has run on the vendored `libp2p-websocket`;
   that crate's own loopback handshake tests cover PKCS#8 and PKCS#1 keys, untrusted
   certificates, malformed keys and key/certificate mismatch, but the mesh is covered by
   compile, clippy and the wasm/docker builds only. Those vendor tests are not in CI (§3.1).
5. **The hyper 1 server was exercised only on loopback and only on Linux.** Not run: a real
   GUI/wallet client against the RPC server and the SSE endpoint, a slow SSE consumer
   (backpressure should work as before, untested), graceful shutdown with an SSE client
   attached, Windows and macOS builds of the new accept loop, and an IPv6 bind (the
   development host has no IPv6 loopback, and that test skips itself).
6. **Secret-scanner noise, by design:** a throwaway RSA test key is checked in at
   `vendor-patches/libp2p-websocket-0.42.0/tests/fixtures/rsa_pkcs1_key.der` (documented in its
   `PATCH-NOTES.md`). It is test-only with no deployment, but the alert is real and may want an
   allowlist entry.
7. **Pre-existing, unrelated, and still true:** `mm2src/coins/utxo/pb.rs` regenerates dirty on
   every build and shows up as a modified file to anyone building the tree.

## 5. Follow-ups this leaves behind

- The two field checks in §4 items 1 and 2, before the beta.
- Wire the vendored `libp2p-websocket` tests into CI, or accept that they rot (§3.1).
- A duplicate-version cleanup is **unscoped**; `deny.toml` at `bc47dc07d` is its starting
  point (§3.5).
- `common/log/native_log.rs` filters two rustls log messages; one of the two texts no longer
  exists in 0.23.46, so that filter line is dead. Harmless, left alone.
- `RUSTSEC-2026-0349` is gone, but the rustls floor that keeps it gone is now enforced only by
  the `bans` job — which is not a required check (§3.5).
