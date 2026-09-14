# KDF Reloaded patch: `zcash_client_backend` 0.23.0

This directory is the published crates.io source for
`zcash_client_backend` 0.23.0 (MIT OR Apache-2.0), with the following
narrow compatibility patches. The original license files are retained.

KDF Reloaded removes the unconditional exact dependency on
`time-core = 0.1.2`. That dependency was an obsolete resolver workaround for
the optional Tor stack and prevents Cargo from selecting the security-fixed
`time` release.

For CRD chapter 39 §39.9, independently authored receive-policy hooks allow
callers to supply an optional `Zip212Enforcement` to cached-block scanning
and full-transaction decryption. The scan policy reaches both the batched
decryption pass and the per-block scanner. Existing entrypoints supply no
override and keep their height-based Zcash behavior. Cryptographic validation,
chain continuity, Orchard processing, and transaction construction are unchanged.

The Cheetah coin layer selects this hook only for the documented deployed Pirate
consensus tuple. It passes `GracePeriod` to recognize both Sapling plaintext
versions, as required by the pinned public Pirate protocol and ZIP-212. Coin
identification and historical-wallet recovery remain outside this library.
The implementation was derived from the gated §39.9 requirements and this
published crate's public interfaces, without consulting the KDF/GLEEC corpus.

The manifest patch can be removed when a compatible stable release drops the
exact `time-core` pin. The receive hooks can be removed when a compatible
backend exposes equivalent explicit receive-policy control.
