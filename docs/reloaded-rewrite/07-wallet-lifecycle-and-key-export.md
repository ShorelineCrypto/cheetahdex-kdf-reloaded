# Chapter 07 — Wallet Lifecycle & Encrypted Mnemonic Persistence

## Executive Summary

In the baseline tree the node has no concept of a *wallet*. Whoever starts the
binary supplies a passphrase through the `MM2.json` configuration (the field is
literally named `passphrase`), the startup path derives a single secp256k1 key
pair from it and stashes the derived material in the central context. The
plaintext passphrase lives only in memory; nothing is persisted, nothing is
encrypted at rest, and there is no way to switch identities at runtime.

The post-baseline tree introduces a complete wallet lifecycle on top of that
passphrase plumbing without changing the on-the-wire RPC for already-existing
read-only key-export endpoints (`get_public_key`, `get_public_key_hash` —
baseline). The additions are:

- a **named-wallet model** stored on the file system as encrypted blobs;
- a **password-based encryption format** that protects the BIP-39 mnemonic at
  rest using Argon2id key derivation plus AES-256-CBC + HMAC-SHA-256 (encrypt-
  then-MAC);
- a **startup integration hook** that lifts an optional `wallet_name` /
  `wallet_password` pair out of the runtime configuration, encrypts the
  passphrase on first use and verifies it on subsequent starts;
- three new V2 RPCs — `create_wallet`, `get_wallet_names`, `delete_wallet` —
  that let a front-end manage the wallet store while the node is running;
- a write-once `wallet_name` slot on the central context that records the
  active wallet, so the same RPC namespace can simultaneously list inactive
  wallets and refuse to delete the one currently in use.

The chapter documents the new types, files, error taxonomy and startup
sequence so an implementer can rebuild the wallet layer from the baseline
without reading the post-baseline implementation.

## Reproduction Detail

### 7.1 Baseline shape (one passphrase, no persistence)

At commit `c1d46c0…` the dispatcher exposes only two key-related V2 methods,
both pre-existing and unchanged through this rewrite:

| Method | Handler module |
| --- | --- |
| `get_public_key` | `mm2_main/src/mm2/rpc/lp_commands` |
| `get_public_key_hash` | same |

There is no wallet RPC namespace, no `create_wallet` / `get_wallet_names` /
`delete_wallet`, no `lp_wallet` module, and no encrypted-mnemonic type. The
central context exposes a passphrase-derived key pair plus a RIPEMD-160 hash of
the corresponding pubkey (`rmd160`), and that is the whole identity surface.

Persistence: none. If the operator restarts the node they re-supply the
passphrase in the JSON config.

### 7.2 Post-baseline data model

A new top-level module `mm2_main/src/lp_wallet.rs` owns the wallet lifecycle.
On-disk layout (native targets only — WASM targets do not persist wallets):

```
<dbdir>/
└── wallets/
    ├── alice.wallet
    ├── bob.wallet
    └── …
```

Each `<name>.wallet` file is a pretty-printed JSON serialisation of an
`EncryptedMnemonicData` value (defined in `crypto/src/mnemonic.rs`):

```rust
pub struct EncryptedMnemonicData {
    pub encrypted_data:  EncryptedData,         // ciphertext + IV + HMAC tag
    pub key_derivation:  KeyDerivationDetails,  // Argon2 params + salt
                                                // OR SLIP-0021 path marker
}
```

`EncryptedData` (`crypto/src/encrypt.rs`) carries three byte-vectors:
`encrypted` (AES-256-CBC ciphertext, PKCS-7 padded), `iv` (16 random bytes per
encryption) and `hmac` (HMAC-SHA-256 over `IV || ciphertext`). The encryption
key and HMAC key are derived independently — see §7.4.

No password hash is stored. Password verification = attempting decryption and
checking that the HMAC tag verifies; a wrong password produces a clean
`InvalidPassword` failure rather than a usable but corrupt plaintext.

### 7.3 New RPCs (V2)

Three handlers are registered on the V2 dispatcher inside a native-only
`cfg_native!` block:

| Method | Request fields | Response fields | Status codes |
| --- | --- | --- | --- |
| `create_wallet` | `wallet_name: String`, `password: String`, `mnemonic: String` | `wallet_name: String` | 200; `409 Conflict` if the name already exists; `400` for invalid input |
| `get_wallet_names` | (empty object) | `wallet_names: Vec<String>`, `active_wallet: Option<String>` | 200 |
| `delete_wallet` | `wallet_name: String`, `password: String` | `wallet_name: String` | 200; `404` if missing; `400` if password wrong or wallet is active |

A single error enum `WalletError` (eight variants: `InvalidRequest`,
`InvalidPassword`, `WalletAlreadyExists`, `WalletNotFound`,
`CannotDeleteActiveWallet`, `StorageError`, `EncryptionError`, `Internal`)
implements both `SerializeErrorType` and `HttpStatusCode`, following the
project pattern documented in chapter 04.

`validate_wallet_name` constrains names to the regular language
`[A-Za-z0-9 _-]{1,64}` so that wallet identifiers can safely become path
components inside `<dbdir>/wallets/`.

`create_wallet` also rejects empty passwords and refuses to overwrite a
pre-existing wallet — callers must `delete_wallet` first if they want to
replace one. `delete_wallet` decrypts the stored mnemonic before unlinking the
file: the password check and the deletion live in the same critical section so
a leaked file name cannot be used to erase another user's wallet.

### 7.4 Encryption format

`encrypt_mnemonic(mnemonic_str: &str, password: &str) ->
Result<EncryptedMnemonicData, MnemonicError>` (in `crypto/src/mnemonic.rs`) does
the following:

1. Parse `mnemonic_str` as English BIP-39 to confirm it is a real mnemonic
   before encrypting anything.
2. Draw 32 random bytes as the Argon2 salt from `common::os_rng`.
3. Pin the Argon2 parameters: Argon2id, 64 MiB memory cost, 3 iterations,
   1 lane — wrapped in an `Argon2Params { memory_cost_kib, iterations,
   parallelism }` struct so the on-disk record is self-describing.
4. Call `derive_keys_for_mnemonic(password, KeyDerivationDetails::Argon2 {…})`,
   which returns 64 bytes split into a 32-byte `encryption_key` and a 32-byte
   `hmac_key`. Independent halves of the Argon2 output — never the same key
   used for both purposes.
5. Draw 16 random bytes as the AES-CBC IV.
6. Encrypt the mnemonic bytes with AES-256-CBC under `encryption_key`/`iv`.
7. Compute HMAC-SHA-256 over `iv || ciphertext` under `hmac_key`.
8. Return `EncryptedMnemonicData { encrypted_data, key_derivation }` for
   serialisation.

`decrypt_mnemonic` reverses the process: rederive both keys from the password
and the stored `KeyDerivationDetails`, verify the HMAC (constant-time), then
decrypt and return the UTF-8 plaintext.

`KeyDerivationDetails` is an enum with two variants — `Argon2 { params, salt }`
for password-based wallets and `Slip21` for seed-bootstrapped key derivation
(the latter is used by the higher-level mnemonic-from-seed flow but not by the
user-facing RPCs). Storing parameters alongside the ciphertext means a future
parameter bump (e.g. raising the memory cost) is backward-compatible: old files
are still decryptable because their own parameters travel with them.

### 7.5 Active-wallet slot on the central context

`MmCtx` (in `mm2_core/src/mm_ctx.rs`) gains a public field

```rust
pub wallet_name: Constructible<Option<String>>,
```

`Constructible<T>` is the baseline-era write-once container (`pin` /
`as_option`). The two non-trivial states are *anonymous* (`pin(None)` — node
ran with no `wallet_name` configured) and *active* (`pin(Some("alice"))` — the
named wallet is in use). Switching wallets at runtime requires a node restart
because `Constructible` rejects a second `pin`.

`get_wallet_names_rpc` reads this slot to populate the `active_wallet`
response field; `delete_wallet_rpc` reads it to refuse deletion of the wallet
currently in use.

### 7.6 Startup integration

`initialize_wallet_passphrase(ctx, passphrase, wallet_name, wallet_password) ->
Result<Option<String>, MmError<WalletError>>` is the boundary between the new
wallet layer and the existing passphrase initialisation. It is called from
`lp_init` once the passphrase has been read from the configuration. Behaviour:

- **No `wallet_name`** → `pin(None)` and return `Ok(None)`. This is *anonymous
  mode*: the node behaves like the baseline (a passphrase floats only in
  memory) and no file is ever written.
- **`wallet_name` set, `wallet_password` missing** → fail with
  `InvalidRequest`. The configuration is invalid; refusing to start prevents
  silently degrading to anonymous mode.
- **`wallet_name` set, `wallet_password` set, wallet file does not exist** →
  encrypt the passphrase, write `<name>.wallet`, `pin(Some(name))`.
- **`wallet_name` set, `wallet_password` set, wallet file exists** → decrypt
  the stored mnemonic with the supplied password (`InvalidPassword` on
  failure) and confirm that the decrypted text equals the passphrase the
  operator just supplied. A mismatch returns `InvalidRequest` with the message
  *"Passphrase doesn't match the stored wallet. Create a new wallet to use a
  different passphrase"*. Then `pin(Some(name))`.

This handshake means an operator who runs

```
mm2 '{"wallet_name":"alice","wallet_password":"…","passphrase":"…",…}'
```

twice produces the same wallet name with the same identity both times,
provided the passphrase + password pair is consistent. There is no separate
"register" RPC: the very first start writes the wallet, every subsequent start
verifies it.

### 7.7 Constraints worth being explicit about

- **Native-only.** The whole `storage` submodule sits behind
  `#[cfg(not(target_arch = "wasm32"))]`. The three wallet RPCs are likewise
  gated. WASM builds do not register them and therefore do not enable the
  named-wallet model.
- **Pre-existing key-export RPCs are untouched.** `get_public_key` and
  `get_public_key_hash` keep their baseline request/response shape; they still
  use the passphrase-derived key pair on the central context regardless of
  which wallet (if any) is active.
- **No mnemonic export RPC is introduced.** There is no `get_mnemonic` /
  `export_mnemonic` handler. The encrypted blob can only be obtained by
  reading the file directly off disk and supplying the password; the running
  node will not surrender the plaintext mnemonic over RPC.
- **Active-wallet protection is per-process.** Two nodes with separate
  `dbdir`s have separate wallet stores; running the same operator's `dbdir`
  with two nodes simultaneously is undefined.

### 7.8 Reproduction recipe

For an implementer holding only the baseline tree and this chapter:

1. Add a new `mm2src/lp_wallet` source module (or, equivalently for the
   layout shown, a new `mm2_main/src/lp_wallet.rs` file) and re-export it
   from the binary crate's root.
2. Extend the central context with a `wallet_name:
   Constructible<Option<String>>` field; default-initialise it to
   `Constructible::default()` in the builder.
3. Add a `crypto/src/encrypt.rs` module exposing an `EncryptedData { encrypted,
   iv, hmac }` value type and an `encrypt_data(data, key, iv, hmac_key) ->
   EncryptedData` function implementing AES-256-CBC (PKCS-7 padding) followed
   by an HMAC-SHA-256 tag over `iv || ciphertext`.
4. Add a sibling `decrypt.rs` that performs the inverse, verifying the HMAC
   with constant-time comparison **before** any decryption.
5. Add `crypto/src/slip21.rs` implementing SLIP-0021 with two static
   constant paths `["SLIP-0021", "Encryption key"]` and `["SLIP-0021",
   "Authentication key"]`.
6. Add `crypto/src/key_derivation.rs` defining
   - `Argon2Params { memory_cost_kib: u32, iterations: u32, parallelism: u32 }`
   - `KeyDerivationDetails::Argon2 { params, salt: Vec<u8> }` /
     `::Slip21 { … }`
   - `derive_keys_for_mnemonic(password_or_seed: &[u8], details:
     &KeyDerivationDetails) -> Result<Mm2InternalKeys, KeyDerivationError>` that
     returns `{ encryption_key: [u8; 32], hmac_key: [u8; 32] }` either via
     Argon2id (`Algorithm::Argon2id`, `Version::V0x13`, 64 bytes of output, split
     32/32) or via SLIP-0021 (one path for encryption, one for HMAC).
7. Add `crypto/src/mnemonic.rs` exposing `generate_mnemonic(word_count:
   usize)`, `encrypt_mnemonic(mnemonic_str, password)`,
   `decrypt_mnemonic(encrypted, password)`, the `EncryptedMnemonicData` struct,
   and a `MnemonicError` enum. Validate BIP-39 inputs via the public
   `bip39::Mnemonic::parse_in_normalized(Language::English, …)` before
   encrypting.
8. In `lp_wallet.rs`, write a private `storage` submodule (native only) that
   exposes `wallets_dir`, `wallet_path`, `save_encrypted_passphrase`,
   `read_encrypted_passphrase`, `read_all_wallet_names` and `delete_wallet`,
   all operating under `<dbdir>/wallets/`.
9. In the same file, define a `WalletError` enum with the eight variants
   listed in §7.3 and implement `HttpStatusCode` mapping each variant to the
   status code shown in the table.
10. Define request/response structs `CreateWalletRequest` /
    `CreateWalletResponse`, `GetWalletNamesRequest` /
    `GetWalletNamesResponse`, `DeleteWalletRequest` /
    `DeleteWalletResponse`. Implement `validate_wallet_name` as
    `1..=64` chars from `[A-Za-z0-9 _-]`.
11. Implement the three RPC handlers as documented (`create_wallet_rpc`,
    `get_wallet_names_rpc`, `delete_wallet_rpc`). For `delete_wallet_rpc`:
    refuse if `ctx.wallet_name` currently holds `Some(name)`, then load the
    encrypted blob, then `decrypt_mnemonic(…, &req.password)` and only on
    success unlink the file.
12. Implement `initialize_wallet_passphrase` per §7.6 with the four
    `(wallet_name, wallet_password)` cases.
13. Register the three new methods on the V2 dispatcher inside a native-only
    `cfg_native!` block. Do not touch `get_public_key` or
    `get_public_key_hash`.
14. From `lp_init`, call `initialize_wallet_passphrase(&ctx, &passphrase,
    ctx.conf["wallet_name"].as_str(), ctx.conf["wallet_password"].as_str()).await?`
    immediately after reading the passphrase.
15. Add unit tests covering: name validation; create→list→delete happy path;
    duplicate-create returns `Conflict`; delete with wrong password returns
    `BadRequest`/`InvalidPassword`; delete of non-existent wallet returns
    `NotFound`; deletion of the currently active wallet is blocked.

## External References

- *BIP-39: Mnemonic code for generating deterministic keys*, Marek Palatinus
  et al., <https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki>.
- *SLIP-0021: Symmetric key derivation for HMAC-SHA512*, SatoshiLabs,
  <https://github.com/satoshilabs/slips/blob/master/slip-0021.md>.
- *RFC 9106 — Argon2 Memory-Hard Function for Password Hashing and Proof-of-
  Work Applications*, <https://www.rfc-editor.org/rfc/rfc9106.html>. Defines
  Argon2id and the parameter triple (memory, iterations, parallelism).
- *NIST SP 800-38A — Recommendation for Block Cipher Modes of Operation*,
  CBC mode, <https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication800-38a.pdf>.
- *RFC 2104 — HMAC: Keyed-Hashing for Message Authentication*,
  <https://www.rfc-editor.org/rfc/rfc2104.html>; *RFC 6234 — US Secure Hash
  Algorithms*, <https://www.rfc-editor.org/rfc/rfc6234.html>.
- Hugo Krawczyk, *The Order of Encryption and Authentication for Protecting
  Communications (Or: How Secure Is SSL?)*, CRYPTO 2001 — justification for
  encrypt-then-MAC.
- `bip39` crate, <https://crates.io/crates/bip39>;
  `argon2` crate, <https://crates.io/crates/argon2>;
  `aes` and `cbc` crates from RustCrypto, <https://crates.io/crates/aes>,
  <https://crates.io/crates/cbc>;
  `hmac` and `sha2` crates from RustCrypto.

## Provenance Footer

- **Inputs:** `01-clean-room-rules.md`; the baseline dispatcher and central
  context at commit `c1d46c0…`; the post-baseline files
  `mm2_main/src/lp_wallet.rs`, `mm2_main/src/rpc/dispatcher/dispatcher.rs`,
  `mm2_core/src/mm_ctx.rs`, `crypto/src/mnemonic.rs`,
  `crypto/src/key_derivation.rs`, `crypto/src/encrypt.rs`,
  `crypto/src/slip21.rs`; the external specifications listed above.
- **Permitted-input classes used:** baseline source; first-party post-baseline
  identifiers introduced with in-chapter justification; public Rust-language,
  IETF and SLIP/BIP specifications; crates.io packages.
- **Not used:** any private repository, any internal-only document, any
  upstream post-baseline source tree (no kdf-analysis-2022 access).
- **Sibling-allowlist consultations:** none.
- **Author of this chapter:** clean-room reimplementation working set,
  reviewed under the two-reviewer protocol defined in
  `local/clean-room-doc/IMPLEMENTER_RULES.md`.
