# Chapter 07 — Wallet Lifecycle and Encrypted Mnemonic Persistence

**Status:** driving-spec.

This chapter binds the named-wallet identity model: encrypted mnemonic blobs on
disk, password-based key derivation, startup verification handshake, and the
three management RPCs that operate the store while the node is running.

## 7.1 Executive Summary

The baseline daemon has no concept of a *wallet*. The operator supplies a
passphrase in the runtime configuration, the startup path derives a single
secp256k1 key pair from it and stashes the derived material in the central
context. The plaintext passphrase lives only in process memory; nothing is
persisted, nothing is encrypted at rest, and the running identity cannot be
switched without restarting with a different configuration.

This chapter binds a complete wallet-lifecycle substrate on top of the same
passphrase-derived key pair, *without* changing the wire shape of the
pre-existing read-only key-export endpoints `get_public_key` and
`get_public_key_hash`. The new substrate has four bound pieces:

- a **named-wallet store** consisting of one encrypted file per wallet under a
  dedicated subdirectory of the per-node data directory;
- a **password-based encryption envelope** that protects the BIP-39 mnemonic
  at rest using Argon2id key derivation feeding AES-256-CBC encryption and an
  HMAC-SHA-256 tag, in encrypt-then-MAC order;
- a **startup verification handshake** that lifts an optional
  `wallet_name` / `wallet_password` pair out of the runtime configuration,
  encrypts-and-persists on first start with a given name, and decrypts-and-
  compares on every subsequent start;
- three **V2 RPCs** — `create_wallet`, `get_wallet_names`, `delete_wallet` —
  plus a write-once active-wallet slot on the central context.

## 7.2 Subsystem Shape

The wallet substrate spans three concerns: a cryptographic envelope (key
derivation + symmetric encryption + authentication tag), a filesystem store
(one file per named wallet), and a startup-time handshake that ties an
externally supplied passphrase to a persisted wallet identity.

The cryptographic envelope is self-describing: every persisted record carries
the key-derivation parameters used to produce it, so a future parameter bump
remains backward-compatible — old records decrypt under their own embedded
parameters.

The filesystem store is keyed by a constrained wallet name (see §7.6) that is
also the file-stem portion of the on-disk record. The store is native-only;
WASM builds do not register the three new RPCs and do not maintain the on-disk
directory.

The handshake distinguishes four configuration cases (see §7.8) and
deliberately fails closed: an inconsistent configuration refuses to start
rather than silently degrading.

## 7.3 Bound Wire Surface

**R1.** The substrate adds exactly three new V2 RPC methods, registered on
the version-2 dispatcher only on native targets:

- `create_wallet` — request fields `wallet_name: String`,
  `password: String`, `mnemonic: String`; response field
  `wallet_name: String`.
- `get_wallet_names` — request body is an empty object; response fields
  `wallet_names: Vec<String>` and `active_wallet: Option<String>`.
- `delete_wallet` — request fields `wallet_name: String`,
  `password: String`; response field `wallet_name: String`.

**R2.** The two pre-existing read-only key-export methods `get_public_key`
and `get_public_key_hash` MUST remain unchanged in request shape, response
shape, dispatcher namespace and underlying key source (the passphrase-derived
key pair on the central context). They MUST NOT be re-routed through the
named-wallet store, irrespective of whether a wallet is currently active.

**R3.** No mnemonic-export RPC is introduced. The running node MUST NOT
surrender the plaintext mnemonic over RPC under any method name; recovery of
the mnemonic requires direct read access to the on-disk record and the
correct password.

**R4.** Two runtime-configuration field names are bound at the configuration
boundary: `wallet_name` (optional string) and `wallet_password` (optional
string). The pre-existing `passphrase` configuration field is preserved
unchanged.

## 7.4 Bound Error Surface

**R5.** A single error enum is exposed by all three handlers, with eight
named variants whose names are visible to clients through the standard
type-tagged error envelope: `InvalidRequest`, `InvalidPassword`,
`WalletAlreadyExists`, `WalletNotFound`, `CannotDeleteActiveWallet`,
`StorageError`, `EncryptionError`, `Internal`.

**R6.** HTTP status mapping is bound:

| Variant                       | Status |
| ----------------------------- | -----: |
| `InvalidRequest`              |    400 |
| `InvalidPassword`             |    400 |
| `WalletAlreadyExists`         |    409 |
| `WalletNotFound`              |    404 |
| `CannotDeleteActiveWallet`    |    400 |
| `StorageError`                |    500 |
| `EncryptionError`             |    500 |
| `Internal`                    |    500 |

The enum implements the project-wide type-tagged error-serialization trait
and the HTTP-status trait bound in Chapter 04.

## 7.5 Bound On-Disk Layout

**R7.** Persisted wallets MUST live in a single dedicated subdirectory of
the per-node data directory, sibling to (and at a level *above*) any
per-identity subdirectories. One regular file per wallet.

**R8.** The on-disk file name MUST be `<wallet_name>.<extension>` where the
extension is bound as `wallet`. The file content MUST be the
JSON serialization of the persisted encryption envelope value defined in
§7.7. JSON encoding (rather than a compact binary form) is bound so that the
records remain operator-inspectable.

**R9.** The on-disk store MUST NOT contain a separate password hash or
verifier. Password verification is performed exclusively by attempting
decryption and observing whether the authentication tag verifies; a wrong
password produces a clean authentication failure mapped to `InvalidPassword`
and never produces a usable-but-corrupt plaintext.

## 7.6 Bound Wallet-Name Grammar

**R10.** Wallet names accepted by `create_wallet` and `delete_wallet` MUST
match the regular language `[A-Za-z0-9 _-]{1,64}` — between 1 and 64
characters, drawn from the ASCII alphanumeric set plus space, underscore and
hyphen. The grammar is bound for two reasons: it guarantees that a wallet
name can be used directly as a single path component under the store
directory without escaping, and it forbids names containing path separators,
parent-directory tokens, or shell-significant characters.

**R11.** `create_wallet` MUST additionally reject empty passwords with
`InvalidRequest`, and MUST reject creation when a record under the requested
name already exists, with `WalletAlreadyExists` (status 409). Replacing an
existing wallet requires explicit deletion first.

## 7.7 Bound Encryption Envelope

**R12.** The persisted record carries two bound fields:

- `encrypted_data` — a structure with three byte vectors: the ciphertext, the
  initialization vector, and the authentication tag.
- `key_derivation` — a self-describing key-derivation descriptor (see R15).

**R13.** Encryption is AES-256-CBC with PKCS-7 padding under a 32-byte
symmetric key. The initialization vector MUST be 16 fresh random bytes drawn
per encryption from the project's secure random source.

**R14.** Authentication is HMAC-SHA-256 under a separate 32-byte key,
computed over the byte concatenation `iv || ciphertext`. The authentication
order is bound as **encrypt-then-MAC**: decryption MUST verify the
authentication tag (in constant time) *before* attempting any cipher
operation on the ciphertext.

**R15.** Key derivation is bound as a tagged enum with two variants:

- *Password-derived (Argon2id).* Carries an `Argon2Params` record with three
  fields — memory cost in kibibytes, iteration count, and parallelism — plus
  a 32-byte salt drawn from the project's secure random source. The bound
  parameter triple at substrate-introduction time is **(memory_cost: 65536
  KiB, iterations: 3, parallelism: 1)**. The parameter triple travels with
  every persisted record so future raises remain backward-compatible.
- *Seed-derived (SLIP-0021).* Used by the higher-level mnemonic-from-seed
  bootstrap flow described in Chapter 05; not exercised by the user-facing
  RPCs bound in this chapter.

**R16.** The 64 bytes of key material returned by the bound derivation MUST
be split with the lower 32 bytes used as the AES encryption key and the
upper 32 bytes used as the HMAC key. The substrate MUST NEVER use the same
32-byte half for both encryption and authentication.

**R17.** Inputs to `create_wallet` MUST be validated as English BIP-39
mnemonics *before* the encryption envelope is constructed. Non-mnemonic
input MUST be rejected with `InvalidRequest`; an invalid mnemonic MUST NOT
produce a written `.wallet` file.

## 7.8 Bound Startup Handshake

**R18.** The startup integration point reads the optional pair
(`wallet_name`, `wallet_password`) from the runtime configuration and
distinguishes four cases. The behaviour for each case is bound:

| Configuration                                            | Required behaviour                                                                                                       |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `wallet_name` absent                                     | *Anonymous mode.* Pin the active-wallet slot to `None`. Write nothing. The node behaves identically to a baseline start. |
| `wallet_name` present, `wallet_password` absent          | Fail with `InvalidRequest`. Refuse to start.                                                                              |
| `wallet_name` present, password present, file absent     | Encrypt the configured passphrase, write `<wallet_name>.wallet`, pin the active-wallet slot to `Some(wallet_name)`.       |
| `wallet_name` present, password present, file exists     | Decrypt the stored mnemonic with the supplied password. Reject password mismatch as `InvalidPassword`. Reject decrypted-text vs configured-passphrase mismatch as `InvalidRequest`. On success, pin the active-wallet slot to `Some(wallet_name)`. |

**R19.** The active-wallet slot on the central context MUST be a write-once
container: pinning a second value MUST fail. Runtime switching of the
active wallet therefore requires a node restart with a different
configuration.

**R20.** The startup handshake MUST run *after* the passphrase has been
ingested from configuration and *before* RPC dispatch is enabled, so the
node never serves traffic with an inconsistent (`wallet_name` set but slot
unpinned) state.

## 7.9 Bound RPC Semantics

**R21.** `get_wallet_names` MUST read both the on-disk directory listing
(filtered by the `.wallet` extension and the bound name grammar) and the
active-wallet slot from the central context, returning both. `wallet_names`
MUST include the active wallet if one is set.

**R22.** `delete_wallet` MUST first reject when the requested
`wallet_name` equals the currently pinned active wallet, with
`CannotDeleteActiveWallet` (status 400). It MUST then load the encrypted
record, attempt decryption with the supplied password, and only on
successful decryption unlink the file. The password check and the file
removal MUST live in the same critical region: a leaked file name MUST NOT
be usable to erase another operator's wallet without knowledge of the
password.

**R23.** `create_wallet` MUST be idempotent only on identical inputs to the
extent that re-creating an existing wallet name is forbidden (see R11);
there is no separate "upsert" semantics.

## 7.10 Bound Platform Gating

**R24.** The on-disk storage submodule, the three management RPCs, the
active-wallet slot semantics that depend on disk presence, and the startup
handshake MUST be gated to native targets. WASM targets MUST NOT register
the three RPCs and MUST NOT advertise them on the V2 dispatcher.

**R25.** The active-wallet slot field itself on the central context MAY
exist on both targets so that downstream code can compile-link uniformly;
on WASM it remains permanently in its anonymous (`None`) state.

## 7.11 Tests (test invariants)

**T1.** *Wallet-name validation.* For each of: empty string; a 65-character
string; a string containing `/`, `\`, `..`, `:`; a string containing
non-ASCII characters — `create_wallet` MUST reject with `InvalidRequest`
without writing any file. For each of: a single character; a 64-character
string drawn from the bound alphabet; a string containing each of space,
underscore and hyphen — `create_wallet` MUST accept.

**T2.** *Round-trip and isolation.* A `create_wallet` of (name *N*,
password *P*, mnemonic *M*) followed by `get_wallet_names` MUST return *N*
in `wallet_names`. A subsequent `delete_wallet` with password *P* MUST
succeed and remove *N* from a second `get_wallet_names`. A
`delete_wallet` with a password *P'* ≠ *P* MUST fail with
`InvalidPassword` and MUST leave the on-disk record in place. A
`delete_wallet` for a name that does not exist MUST fail with
`WalletNotFound`.

**T3.** *Active-wallet protection.* After a startup handshake that pins the
active-wallet slot to *Some("alice")*, a `delete_wallet` request with
`wallet_name="alice"` MUST fail with `CannotDeleteActiveWallet` regardless
of whether the supplied password is correct, and MUST NOT unlink the file.

**T4.** *Ciphertext tamper rejection.* Flipping a single byte of the
on-disk record's ciphertext, IV, or authentication tag MUST cause the next
`delete_wallet` decryption attempt (and the startup-handshake decryption)
to fail with `InvalidPassword` (HMAC mismatch), not with a UTF-8 decode
error and not with a panic.

**T5.** *Startup handshake matrix.* Each of the four configuration cases
listed in R18 MUST be exercised by an integration test or equivalent; the
two failure cases (no password, mismatched passphrase) MUST be confirmed to
keep the daemon from entering RPC-serving state.

## 7.12 Deferred Work

**D1.** A wallet-switch RPC that re-pins the active-wallet slot without
restarting the process is deferred. The bound write-once semantics of the
slot are not loosened in this chapter.

**D2.** A mnemonic-export RPC is intentionally deferred indefinitely. R3
forbids it under the current substrate.

**D3.** An Argon2 parameter-bump migration helper that re-encrypts older
records under stronger parameters is deferred. The self-describing
parameter envelope (R15) makes such a helper additive when introduced.

**D4.** A multi-process file-locking discipline (so two daemons sharing a
data directory cannot race on the same `.wallet` file) is deferred; the
current substrate documents the constraint that the data directory is
single-tenant.

## 7.13 External References

- *BIP-39 — Mnemonic code for generating deterministic keys.* English
  word-list is the bound dictionary.
- *SLIP-0021 — Symmetric key derivation.* Referenced as the
  non-password-derived variant of the bound key-derivation enum.
- *RFC 9106 — Argon2 Memory-Hard Function for Password Hashing and Proof-
  of-Work Applications.* Defines Argon2id and the parameter triple
  (memory, iterations, parallelism) bound in §7.7.
- *NIST SP 800-38A — Recommendation for Block Cipher Modes of Operation.*
  Specifies CBC mode bound in R13.
- *RFC 2104 — HMAC: Keyed-Hashing for Message Authentication* and
  *RFC 6234 — US Secure Hash Algorithms.* Specify the HMAC-SHA-256
  construction bound in R14.
- Krawczyk, *The Order of Encryption and Authentication for Protecting
  Communications.* Justification for encrypt-then-MAC ordering bound in
  R14.
- Chapter 04 (error-aggregation type adaptation) — bound error envelope
  shape, type-tagged serialization trait, and HTTP-status trait used by
  R5–R6.
- Chapter 05 (HD wallet support) — bound mnemonic-and-encryption
  primitives consumed by this chapter, including the SLIP-0021 variant
  of the key-derivation enum.

## 7.14 Baseline Verifications

**V1.** The baseline tree MUST be confirmed to lack any wallet-RPC
namespace: a search across the V2 dispatcher registrations for the three
bound method names MUST return zero hits.

```
git -C <baseline> grep -nE '"(create_wallet|get_wallet_names|delete_wallet)"'
```

**V2.** The baseline tree MUST be confirmed to lack a dedicated wallet
module and the on-disk store directory name: searches for a `lp_wallet`
source file and for the literal `wallets/` directory token in source MUST
return zero hits.

**V3.** The pre-existing read-only key-export methods MUST be confirmed
present in the baseline dispatcher:

```
git -C <baseline> grep -nE '"(get_public_key|get_public_key_hash)"'
```

This anchors R2: the baseline shape that the substrate is forbidden to
disturb.

## 7.15 Provenance Footer

- *Inputs consulted for this chapter:* the baseline tree at the project
  baseline commit `c1d46c0c1592faa0860f704008b2b2381bc3840f`, Chapter 04
  (error envelope), Chapter 05 (HD-wallet mnemonic primitives), and the
  external specifications listed in §7.13.
- *Permitted-input classes used:* baseline source; chapter-bound
  protocol identifiers introduced here as wire contract (RPC method names,
  configuration field names, on-disk file extension, error-variant names);
  public IETF/BIP/SLIP specifications; standard cryptographic primitive
  names.
- *Sibling chapters cross-referenced:* Chapter 04, Chapter 05.
- *Author of this chapter:* clean-room round-2 driving-spec working set.
- *Forbidden corpus:* not consulted.
