# Chapter 05 — Hierarchical-Deterministic Wallet Support

## Executive Summary

At the baseline the project's `crypto` crate supported a single
key-source model: a "passphrase" — historically called an *Iguana
passphrase* in the project's documentation — was hashed into one
secp256k1 secret per running daemon, and that single secret was
used to derive per-coin Bitcoin-style keys. The crate already had
the shape of hierarchical-deterministic derivation (a `Bip32Child`
type, a `Bip44DerivationPath` type, a Trezor-friendly
`HwWalletCtx`), but no end-to-end BIP-39 mnemonic flow and no
support for any derivation scheme other than the strict BIP-44
path `m/44'/coin'/account'/chain/index`.

The post-baseline modernisation adds a complete hierarchical-
deterministic wallet stack derived from public Bitcoin and Trezor
standards (BIP-32, BIP-39, BIP-43, BIP-44, BIP-49, BIP-84,
SLIP-0010, SLIP-0021, SLIP-0044), without removing the legacy
passphrase path. Concretely, the post-baseline crate gains: a
BIP-39 mnemonic generator and an encrypted-mnemonic format; a
`GlobalHDAccountCtx` that holds a BIP-39 seed and two parallel
master keys (BIP-32 secp256k1 for Bitcoin-family coins and
SLIP-0010 ed25519 for Cosmos- and Solana-family coins); a
`StandardHDPath` generic over BIP-43 purpose values so the same
path machinery serves BIP-44, BIP-49, and BIP-84; a `KeyPairPolicy`
discriminator on the `CryptoCtx` so the rest of the daemon can ask
"am I in legacy-Iguana or HD mode?" without caring about the
mechanics; an Argon2id-or-SLIP-0021-keyed mnemonic encryption
format; and a `Bip39Seed` wrapper that zeroises its 64-byte
contents on drop. The hardware-wallet path (`HwCtx`, Trezor
protocol) is retained from the baseline.

Coexistence of the legacy and HD paths is mediated by `CryptoCtx`,
which the daemon initialises with either an Iguana passphrase or a
BIP-39 mnemonic at startup. Coins ask the context for their key
material through neutral interfaces (`derive_secp256k1_secret`,
`derive_ed25519_signing_key`); each interface dispatches to the
HD machinery when an `HDAccountCtx` is present and to the
legacy-Iguana machinery otherwise. RPC handlers that need to
inspect the policy at runtime do so through the public
`KeyPairPolicy` enum.

The wire side — RPC method names, RPC field names, the
JSON-serialised representation of a derivation path — is either
unchanged from the baseline or new and named after the public
specifications it expresses. No identifier is borrowed from a
private source.

A reader leaving this chapter should be able to (a) reconstruct
the file layout of the post-baseline `crypto` crate, (b) state
which public specification each new type is derived from, and
(c) explain how legacy-Iguana and HD policies coexist behind a
single `CryptoCtx`.

## Reproduction Detail

### 5.1 The baseline crypto crate

At commit `c1d46c0c1592faa0860f704008b2b2381bc3840f` the
`mm2src/crypto/` crate contains nine source files:

```
mm2src/crypto/Cargo.toml
mm2src/crypto/src/bip32_child.rs
mm2src/crypto/src/bip44.rs
mm2src/crypto/src/crypto_ctx.rs
mm2src/crypto/src/hw_client.rs
mm2src/crypto/src/hw_ctx.rs
mm2src/crypto/src/hw_rpc_task.rs
mm2src/crypto/src/key_pair_ctx.rs
mm2src/crypto/src/lib.rs
mm2src/crypto/src/privkey.rs
```

The crate's job at the baseline is to:

- Accept an "Iguana passphrase" (a freeform string) on daemon
  startup, deterministically derive a single secp256k1 secret from
  it (`privkey.rs`), and hand the resulting key pair to the rest
  of the daemon via `IguanaCtx` (`key_pair_ctx.rs`).
- Provide a type-safe `Bip32Child` linked-list encoding of
  BIP-32 child indices, parameterised by whether each level must
  be hardened, non-hardened, or one of the BIP-44 chain values
  (`bip32_child.rs`).
- Compose `Bip32Child` into the strict BIP-44 path
  `m/44'/coin_type'/account'/chain/address_index`
  (`bip44.rs`), with helper accessors for each component.
- Provide a hardware-wallet client and context that wrap the
  `trezor` crate's protocol bindings (`hw_client.rs`, `hw_ctx.rs`,
  `hw_rpc_task.rs`).
- Wire all the above into a shared `CryptoCtx` accessible from
  the rest of the daemon (`crypto_ctx.rs`).

There is no BIP-39 mnemonic handling, no ed25519 derivation, no
SLIP-0010 derivation, no SLIP-0021 derivation, no notion of an
encrypted-mnemonic-at-rest, no per-coin policy discriminator, no
generalisation across BIP-43 purposes, and no MetaMask
integration.

### 5.2 The post-baseline crypto crate

The post-baseline crate (current tree) replaces the nine-file
layout with twenty source files. The baseline files all survive
(in some cases with rewrites, in some cases with backward-compat
re-exports); the new files are:

| File | Role | Derived from |
|---|---|---|
| `mnemonic.rs` | BIP-39 mnemonic generation, encryption, decryption | BIP-39 |
| `encrypt.rs`, `decrypt.rs` | Symmetric authenticated encryption (AES-CBC + HMAC) of the BIP-39 mnemonic at rest | NIST FIPS 197 / RFC 2104 |
| `key_derivation.rs` | Password-derived or seed-derived key material for `encrypt.rs` / `decrypt.rs` | Argon2 (RFC 9106), SLIP-0021 |
| `slip21.rs` | SLIP-0021 symmetric-key-tree node derivation | SLIP-0021 |
| `global_hd_ctx.rs` | `GlobalHDAccountCtx`: holds the BIP-39 seed and the two master keys derived from it | BIP-32, SLIP-0010 |
| `standard_hd_path.rs` | `StandardHDPath` generic over `Bip43Purpose` (BIP-32 / BIP-44 / BIP-49 / BIP-84) | BIP-43, BIP-44, BIP-49, BIP-84 |
| `xpub.rs` | `XPubConverter`: cross-network base58 extended-public-key version-byte conversion | BIP-32 version-byte tables |
| `secret_hash_algo.rs` | Atomic-swap secret-hash algorithm discriminator | atomic-swap protocol |
| `metamask_login.rs` (WASM only) | MetaMask wallet login challenge/response | EIP-191, EIP-712 |
| `metamask_ctx.rs` (WASM only) | `MetamaskCtx` analogue of `HardwareWalletCtx` for MetaMask | EIP-1193 |

The baseline `bip44.rs` is retained verbatim in its core shape so
existing callers that name `Bip44DerivationPath`, `Bip44PathToCoin`,
or `Bip44PathToAccount` continue to compile. The new
`StandardHDPath` types are introduced alongside it and re-exported
from the crate root.

### 5.3 The BIP-39 mnemonic flow

`mnemonic.rs` exposes three public functions and one struct:

- `generate_mnemonic(word_count: usize) -> Result<Mnemonic, MnemonicError>`
  generates a fresh BIP-39 mnemonic with the requested word count
  (12, 15, 18, 21, or 24 — the BIP-39-permitted lengths corresponding
  to 128, 160, 192, 224, or 256 bits of entropy). Entropy is drawn
  from the operating-system random-number generator
  (`common::os_rng`).
- `encrypt_mnemonic(mnemonic_str, derivation, ...) -> EncryptedMnemonicData`
  encrypts a mnemonic at rest. The key-derivation step is either
  Argon2id over a user-supplied password, or SLIP-0021 over an
  existing root seed (so a daemon that already holds an unlocked
  BIP-39 seed can re-key a mnemonic without prompting the user
  again).
- `decrypt_mnemonic(encrypted: EncryptedMnemonicData, ...) -> Mnemonic`
  reverses the operation.
- `EncryptedMnemonicData` is a transparent struct holding the
  ciphertext block (`EncryptedData`, defined in `encrypt.rs`) and
  the key-derivation parameters (`KeyDerivationDetails`, defined
  in `key_derivation.rs`) needed to reproduce the key at decrypt
  time.

The wire format of `EncryptedMnemonicData` is fully specified by
its `serde::Serialize` derivation; the field names — `encrypted_data`,
`key_derivation` — are the type's own struct field names and are
the only thing a downstream wallet GUI needs to round-trip the
ciphertext.

### 5.4 The `GlobalHDAccountCtx`

`global_hd_ctx.rs` introduces a single struct,
`GlobalHDAccountCtx`, that owns the unlocked BIP-39 seed and the
two master extended private keys derived from it. The construction
sequence is:

```
mnemonic string
    ──BIP-39──▶ 64-byte seed (Bip39Seed)
                    │
                    ├──BIP-32──▶ secp256k1 ExtendedPrivateKey
                    │
                    └──SLIP-0010──▶ ed25519 ExtendedSigningKey
```

The BIP-32 master key is produced by passing the 64-byte seed to
`bip32::ExtendedPrivateKey::new`. The SLIP-0010 master key is
produced by passing the same seed to
`ed25519_dalek_bip32::ExtendedSigningKey::from_seed`. Both
operations are spelled exactly as the public crates intend them
to be spelled; there is no project-specific cryptographic
operation in this construction.

The 64-byte seed is wrapped in a `Bip39Seed(pub [u8; 64])`
new-type. `Bip39Seed` has an explicit `Drop` impl that calls
`self.0.zeroize()` (from the `zeroize` crate), guaranteeing that
the seed bytes are overwritten when the context is dropped. This
satisfies the project's security rule that "secrets are zeroised
on drop" recorded in [`AGENTS.md`](../../AGENTS.md).

`GlobalHDAccountCtx` exposes two derivation helpers, both spelled
in terms of public types:

- `derive_secp256k1_secret(&self, &DerivationPath) -> MmResult<Secp256k1Secret, Bip32Error>`
- `derive_ed25519_signing_key(&self, &Ed25519DerivationPath) -> MmResult<ed25519::SigningKey, ...>`

Coins call whichever helper matches their key family. A Bitcoin-
family coin calls the secp256k1 helper with a path derived through
the `StandardHDPath` machinery; a Cosmos- or Solana-family coin
calls the ed25519 helper with a path constructed similarly.

In addition to the two master keys, the context derives one
specially-named secp256k1 key pair at construction time, used by
the daemon itself for libp2p peer identity and for internal
signing:

```rust
fn mm2_internal_der_path() -> DerivationPath {
    DerivationPath::from_str("m/44'/141'/2147483647/0/0").expect("valid")
}
```

The path encodes (in BIP-44 notation): purpose 44, coin type 141
(KMD, per the SLIP-0044 registry), account 2147483647
(= 2³¹ − 1, deliberately chosen to be the largest BIP-32
non-hardened index so it cannot collide with any real KMD account),
chain 0, address 0. This key is not user-facing; it identifies the
running daemon.

### 5.5 `StandardHDPath` and BIP-43 purpose values

`standard_hd_path.rs` introduces `StandardHDPath`, parameterised
over a `Bip43Purpose` enum at the path's `purpose'` level. The
enum's variants correspond directly to the public specifications:
`Bip32` (purpose value 32), `Bip44` (44), `Bip49` (49), `Bip84`
(84). Two convenience aliases truncate the path at the levels the
daemon actually needs:

- `HDPathToCoin` — first two levels (`purpose'/coin_type'`).
- `HDPathToAccount` — first three levels (`purpose'/coin_type'/account'`).

`StandardHDPath` itself is the full five-level
`purpose'/coin_type'/account'/chain/address_index` form, identical
in shape to the baseline `Bip44DerivationPath` but generic over
the purpose value.

The baseline `Bip44DerivationPath` and friends are retained as
backward-compat re-exports from `lib.rs`; downstream callers that
named them at the baseline continue to compile without change.

### 5.6 `KeyPairPolicy` in `CryptoCtx`

`crypto_ctx.rs` is extended with a `KeyPairPolicy` enum, exported
from the crate root, that distinguishes the daemon's two startup
modes:

- `Iguana` — the legacy passphrase path. The daemon holds an
  `IguanaCtx` containing the single secp256k1 key pair derived
  from the passphrase, exactly as at the baseline.
- `GlobalHDAccount` — the HD path. The daemon holds a
  `GlobalHDAccountArc` (the `Arc<GlobalHDAccountCtx>` from §5.4),
  from which any per-coin key can be derived on demand.

The discriminator lets callers ask the context which mode it is
in without making them inspect the inner state. Most callers do
not ask: they call the policy-neutral derivation helpers on
`CryptoCtx` and the context dispatches internally.

### 5.7 Mnemonic encryption at rest

`encrypt.rs` and `decrypt.rs` together provide an authenticated
symmetric encryption layer:

- The plaintext is a UTF-8 BIP-39 mnemonic string.
- The cipher is AES in CBC mode, with the key derived as in
  §5.7.1 below.
- A 16-byte initialisation vector (IV) is generated per
  encryption.
- An HMAC-SHA256 tag is computed over `iv || ciphertext` and
  appended.

The serialised `EncryptedData` struct carries the IV, the
ciphertext, and the HMAC tag in named fields; the format is fully
specified by its `serde::Serialize` derivation.

`key_derivation.rs` produces the 64-byte AES + HMAC key. It
supports two derivation modes, both publicly specified:

- **Password** — Argon2id (RFC 9106). Salt, iterations, memory,
  and parallelism are recorded in `Argon2Params` and serialised
  alongside the ciphertext so decryption is self-describing.
- **Seed** — SLIP-0021 symmetric-key-tree derivation
  (`slip21.rs`). When the daemon already holds an unlocked BIP-39
  seed, this lets it re-key a mnemonic without prompting the user
  for a password.

The `KeyDerivationDetails` enum records which mode was used and
carries the corresponding parameters.

### 5.8 The `XPubConverter`

`xpub.rs` introduces a single helper, `XPubConverter`, that
re-serialises an extended public key (xpub) under a different
version-byte prefix. This is needed because the Bitcoin community
adopted distinct base58 prefixes per BIP-32 / BIP-49 / BIP-84
flavour (the well-known `xpub`/`ypub`/`zpub` family and the
test-network equivalents). The conversion is mechanical: parse
the input under one version-byte table, re-emit under another.
Both tables are public (BIP-32, BIP-49, BIP-84).

### 5.9 MetaMask (WASM only)

`metamask_ctx.rs` and `metamask_login.rs` add a context analogous
to `HardwareWalletCtx`, but backed by a browser-side MetaMask
provider exposed through the WASM environment. The login flow
follows EIP-191 ("Personal Sign") and EIP-712 (typed-data
signing) — public Ethereum standards. The post-baseline `crypto`
crate gates these modules behind `#[cfg(target_arch = "wasm32")]`
and re-exports the relevant types only under WASM, because
MetaMask is meaningful only inside a browser.

The deeper MetaMask integration (the WASM-side bridge, the JSON
shapes of the requests sent to the browser provider) lives in
the separate `mm2_metamask` crate, which the WASM build of
`crypto` re-exports as `crypto::metamask`. The `mm2_metamask`
crate has its own treatment in a later chapter.

### 5.10 Hardware-wallet path

The baseline hardware-wallet path (`hw_client.rs`, `hw_ctx.rs`,
`hw_rpc_task.rs`, and the `HardwareWalletArc`, `HardwareWalletCtx`,
`TrezorConnectProcessor`, `HwClient`, `HwError`,
`HwProcessingError`, `HwResult`, `HwWalletType` types they
export) is retained. Trezor is the only hardware-wallet
implementation at the baseline, and remains so in the area covered
by this chapter; the `trezor` crate has its own subsequent
modernisation that belongs in a later chapter.

When a daemon starts with a hardware-wallet policy active, the
`CryptoCtx` holds a `HardwareWalletArc` and dispatches derivation
requests to the device through the Trezor protocol rather than
through an in-memory master key. The HD-path types from §5.5 are
the same; the only difference is who computes the signature.

### 5.11 Coexistence of legacy and HD paths

The crucial design property of the post-baseline `crypto` crate
is that the legacy-Iguana path is *not* removed. A daemon
configured with the baseline-style `passphrase` field in
`MM2.json` still starts, still derives the single secp256k1 key
pair, still routes coin operations through it, and still
inter-operates on the live peer-to-peer network. A daemon
configured with the new mnemonic-based startup carries an
`HDAccountCtx` instead, and coins derive their per-coin keys at
request time.

The choice is opaque to most callers because both the
single-secret-from-passphrase derivation and the multi-secret-from-
seed derivation produce values of the same `Secp256k1Secret`
type. RPC handlers that need to know which mode the daemon is in
inspect the public `KeyPairPolicy` enum exported from `crypto`.

### 5.12 Reproducing the modernisation from the baseline

A reader can reproduce the modernisation step by step:

1. Add a `bip39` crate dependency for mnemonic word-list handling.
2. Add an `ed25519_dalek_bip32` crate dependency for SLIP-0010
   derivation on the ed25519 curve.
3. Add a `zeroize` crate dependency for the seed-on-drop guard.
4. Add an `argon2` crate dependency for password-based
   `EncryptedMnemonic` key derivation.
5. Create `mm2src/crypto/src/encrypt.rs` and
   `mm2src/crypto/src/decrypt.rs` with the AES-CBC + HMAC-SHA256
   authenticated-encryption pair.
6. Create `mm2src/crypto/src/key_derivation.rs` with two key-
   derivation paths: Argon2id over a password, and SLIP-0021 over
   a seed. Both paths must serialise their parameters
   self-describingly.
7. Create `mm2src/crypto/src/slip21.rs` implementing the
   SLIP-0021 node-derivation algorithm verbatim from the
   specification.
8. Create `mm2src/crypto/src/mnemonic.rs` exposing
   `generate_mnemonic`, `encrypt_mnemonic`, `decrypt_mnemonic`,
   and the `EncryptedMnemonicData` struct.
9. Create `mm2src/crypto/src/global_hd_ctx.rs` with the
   `Bip39Seed(pub [u8; 64])` newtype (with `Drop` calling
   `zeroize`), the `GlobalHDAccountCtx` struct, and a `new`
   constructor that derives both the BIP-32 secp256k1 master and
   the SLIP-0010 ed25519 master from the same seed.
10. Create `mm2src/crypto/src/standard_hd_path.rs` with a
    `Bip43Purpose` enum (`Bip32`, `Bip44`, `Bip49`, `Bip84`) and
    the three generic path aliases shown in §5.5.
11. Create `mm2src/crypto/src/xpub.rs` with the `XPubConverter`
    helper.
12. Create `mm2src/crypto/src/secret_hash_algo.rs` with the
    atomic-swap secret-hash discriminator (used by the V2 swap
    code in later chapters).
13. Extend `mm2src/crypto/src/crypto_ctx.rs` with a
    `KeyPairPolicy` enum carrying `Iguana(IguanaArc)` and
    `GlobalHDAccount(GlobalHDAccountArc)` variants; route the
    coin-facing helpers through the variant.
14. Update `mm2src/crypto/src/lib.rs` to declare and re-export
    the new modules while preserving every existing re-export
    (so downstream callers that named `Bip44DerivationPath`,
    `IguanaCtx`, etc. keep compiling).
15. For the WASM target only, add `metamask_login.rs` and
    `metamask_ctx.rs`, gated behind
    `#[cfg(target_arch = "wasm32")]`, and pull in the separate
    `mm2_metamask` crate.

After steps 1–15, the `crypto` crate compiles for every target
the baseline supported and exposes both the legacy Iguana path
and the new HD path to the rest of the daemon.

## External References

- *BIP-32 — Hierarchical Deterministic Wallets.* Pieter Wuille.
  https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki
- *BIP-39 — Mnemonic code for generating deterministic keys.*
  https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki
- *BIP-43 — Purpose Field for Deterministic Wallets.*
  https://github.com/bitcoin/bips/blob/master/bip-0043.mediawiki
- *BIP-44 — Multi-Account Hierarchy for Deterministic Wallets.*
  https://github.com/bitcoin/bips/blob/master/bip-0044.mediawiki
- *BIP-49 — Derivation scheme for P2WPKH-nested-in-P2SH based accounts.*
  https://github.com/bitcoin/bips/blob/master/bip-0049.mediawiki
- *BIP-84 — Derivation scheme for P2WPKH based accounts.*
  https://github.com/bitcoin/bips/blob/master/bip-0084.mediawiki
- *SLIP-0010 — Universal private key derivation from master private key.*
  https://github.com/satoshilabs/slips/blob/master/slip-0010.md
- *SLIP-0021 — Hierarchical derivation of symmetric keys.*
  https://github.com/satoshilabs/slips/blob/master/slip-0021.md
- *SLIP-0044 — Registered coin types for BIP-44.*
  https://github.com/satoshilabs/slips/blob/master/slip-0044.md
- *RFC 9106 — Argon2 Memory-Hard Function for Password Hashing
  and Proof-of-Work Applications.*
  https://www.rfc-editor.org/rfc/rfc9106
- *RFC 2104 — HMAC: Keyed-Hashing for Message Authentication.*
  https://www.rfc-editor.org/rfc/rfc2104
- *NIST FIPS 197 — Advanced Encryption Standard (AES).*
  https://csrc.nist.gov/pubs/fips/197/final
- *EIP-191 — Signed Data Standard.*
  https://eips.ethereum.org/EIPS/eip-191
- *EIP-712 — Typed structured data hashing and signing.*
  https://eips.ethereum.org/EIPS/eip-712
- *EIP-1193 — Ethereum Provider JavaScript API.*
  https://eips.ethereum.org/EIPS/eip-1193
- *crates.io — `bip32`.* https://crates.io/crates/bip32
- *crates.io — `bip39`.* https://crates.io/crates/bip39
- *crates.io — `ed25519-dalek-bip32`.* https://crates.io/crates/ed25519-dalek-bip32
- *crates.io — `zeroize`.* https://crates.io/crates/zeroize
- *crates.io — `argon2`.* https://crates.io/crates/argon2

## Provenance Footer

*This chapter v1; verified directly against the baseline tree at
commit `c1d46c0c1592faa0860f704008b2b2381bc3840f` and the current
tree on 2026-05-31. Reviewer #1 and reviewer #2 reports stored at
`local/clean-room-doc/reviews/05-hd-wallet-support-r{1,2}.md`.*
