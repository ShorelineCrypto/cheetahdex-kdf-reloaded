# Chapter 07 — Wallet Lifecycle and Key Export

**Status:** driving-spec.

This chapter binds the named-wallet identity model: encrypted mnemonic blobs on
disk, password-based key derivation, startup verification handshake, the three
management RPCs that operate the store while the node is running, and the
**key-export surface** — the RPCs that surrender key material to the operator
for recovery, audit and migration. Key export is **secure by default**: a
dangerous offline / bulk / hierarchical-deterministic (HD) superset is refused
unless the operator explicitly opts in through a new configuration switch
(§7.3A), while the everyday single-coin reveals and the password-gated
self-export of the operator's own seed remain available without it.

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

**R3.** *Key export is default-off; the dangerous superset is opt-in; the
everyday reveals stay available.* Key export over RPC is governed by the
secure-by-default posture bound in §7.3A. Without any extra switch, the
node still exposes (a) the single-coin reveal of one already-activated coin
(`show_priv_key`), (b) the reduced activated-coins form of `get_private_keys`,
and (c) the wallet-password-authenticated self-export of the operator's *own*
seed (`get_mnemonic`). The offline / no-activation bulk export, the HD
per-derivation-path ranges, and the ZHTLC shielded-viewing-key export together
form a **superset** that the node MUST refuse unless the operator has set the
`allow_insecure_key_export` configuration switch (R-K1). No export path ever
surrenders a counterparty's secret: every response carries only key material
the calling node already controls, authenticated by the wallet context. This
requirement supersedes the earlier blanket prohibition on secret export; the
spirit (secure by default, no silent exposure) is retained while permitting
opt-in parity with the upstream capability.

**R4.** Two runtime-configuration field names are bound at the configuration
boundary: `wallet_name` (optional string) and `wallet_password` (optional
string). The pre-existing `passphrase` configuration field is preserved
unchanged.

## 7.3A Bound Key-Export Surface

Reloaded adopts the upstream key-export capability in full but partitions it
into two tiers: a **default-available** tier that ships on every node, and an
**opt-in** tier (the dangerous offline/HD/ZHTLC superset) reached only by an
explicit configuration switch. The split exists because bulk export of secret
key material for coins that the node has never activated is a materially higher
risk than revealing the key of a coin the operator has already turned on.

**R-K1.** *The opt-in switch.* A new top-level `MM2.json` boolean
`allow_insecure_key_export` is bound, with **default false** (an absent key is
false). It MUST be read with the same convention as the existing
`allow_weak_password` flag \u2014 truthy only when the parsed configuration value is
the boolean `true` (`conf["allow_insecure_key_export"].as_bool() == Some(true)`).
It is a **separate** switch from `allow_weak_password`: the two address
different threat models (key disclosure versus password strength) and MUST NOT
be overloaded onto one flag. `allow_insecure_key_export` governs **only** the
superset of R-K4; it MUST NOT alter the behaviour bound in R-K2, R-K3 or R-K7.

**R-K2.** *Single-coin activated reveal \u2014 always available.* The legacy
v1-dispatcher method `show_priv_key` reveals the private key of exactly one
already-activated coin. Request carries a single `coin` ticker; the coin MUST
be activated. Response carries the coin ticker and that coin's private key in
its native display format. This method is available irrespective of
`allow_insecure_key_export` and retains its baseline-shipped wire shape.

**R-K3.** *Reduced `get_private_keys` \u2014 always available.* With the switch
off or absent, the v2-dispatcher method `get_private_keys` operates in its
reduced form over **already-activated** coins only. Request field
`coins: Vec<String>` lists activated tickers. For each ticker the response
carries `{coin, address, priv_key, pubkey}` \u2014 the private key in the coin's
native format (WIF for UTXO, hex for EVM, and so on) and a hex-encoded
compressed public key. Iguana wallets return the single passphrase-derived
key per coin. A request naming a coin that is not activated MUST be refused.
In this mode the offline/no-activation export, HD ranges and ZHTLC viewing-key
export are NOT offered and MUST be refused.

**R-K4.** *Full `get_private_keys` superset \u2014 opt-in.* When
`allow_insecure_key_export` is `true`, `get_private_keys` gains full parity
with the upstream capability:

- **Offline.** It works for any coin defined in the node configuration,
  regardless of activation status.
- **Mode selection.** The request gains an optional `mode` discriminator with
  two values, `iguana` and `hd`, plus optional `start_index`, `end_index` and
  `account_index` integer fields.
- **HD ranges.** In `hd` mode the method derives one key per BIP-44-style
  derivation path across the requested `[start_index, end_index]` range under
  `account_index`, and returns the full `derivation_path` for each address.
  The range is **bounded** (a maximum of 100 addresses per call); an inverted
  or over-large range MUST be refused, and the index fields are valid only in
  `hd` mode.
- **Protocol-specific formatting.** Address and key formats are produced per
  protocol: UTXO (WIF), EVM (hex), Tendermint, and ZHTLC. For ZHTLC shielded
  coins the response additionally carries the shielded `viewing_key` and both a
  transparent `derivation_path` and a shielded `z_derivation_path`.
- **Response shape (interop).** The response is an untagged union: in `iguana`
  mode an array of per-coin objects `{coin, pubkey, address, priv_key,
  viewing_key?}`; in `hd` mode an array of per-coin objects
  `{coin, addresses: [{derivation_path, z_derivation_path?, pubkey, address,
  priv_key, viewing_key?}]}`. Optional fields are omitted when not applicable.

**R-K5.** *Bound error surface for `get_private_keys`.* The method exposes a
single type-tagged error enum whose `error_type` tokens are part of the wire
contract. The bound conditions are: coin configuration not found; coin
protocol could not be parsed; key derivation failed for a coin; the HD index
range is invalid (inverted); the HD index range exceeds the bound maximum; a
required protocol prefix value is missing; index parameters were supplied
outside `hd` mode; a hardware-wallet session was used (see R-K6); and a
catch-all internal error. Client-input faults map to HTTP 400; derivation and
internal faults map to HTTP 500. The enum implements the project-wide
type-tagged error-serialization trait and the HTTP-status trait bound in
Chapter 04.

**R-K6.** *Guardrails (invariants in BOTH tiers).* For every key-export path
bound in this section the following invariants hold regardless of the switch:

- A hardware-wallet (Trezor) session MUST be rejected \u2014 host-side key export is
  never performed for hardware wallets.
- Secret material MUST NOT be logged, traced, or persisted in plaintext; it is
  serialized exactly once, for the response only.
- The surface is intended for localhost / trusted-channel use; the chapter does
  not bind transport encryption and assumes a trusted local control channel.
- Every response is authenticated by the wallet context and returns only key
  material the calling node itself controls.

**R-K7.** *Own-seed self-export \u2014 wallet-password gated, always available.*
The method `get_mnemonic` returns the caller's *own* seed, authenticated by the
wallet password; it never returns a counterparty secret. The request selects a
`format`: `encrypted` returns the stored encryption envelope unchanged (no
password needed, no plaintext exposure); `plaintext` requires the wallet
`password`, decrypts the stored mnemonic and returns it. The response mirrors
the format: the encrypted form carries the encrypted envelope, the plaintext
form carries the decoded mnemonic string. A wrong password MUST yield an
invalid-password error and never a usable-but-corrupt plaintext. This method
is **not** gated by `allow_insecure_key_export` \u2014 it is lower risk than bulk
private-key export (password-authenticated, own seed only), and is the standard
\"reveal recovery phrase\" capability.

**R-K8.** *Wallet-password re-encryption \u2014 no plaintext exposure.* The method
`change_mnemonic_password` re-wraps the stored mnemonic envelope under a new
password without ever surrendering plaintext over RPC. Request carries
`current_password` and `new_password`. The handler loads the active wallet with
`current_password`, re-encrypts the same secret under `new_password` using the
\u00a77.7 envelope, and persists the result. An empty `new_password` MUST be
refused; the new password is subject to the project password policy unless
`allow_weak_password` is set (the same convention used elsewhere). This method
is not gated by `allow_insecure_key_export`.

**R-K9.** *Platform gating for the export surface.* The export paths that
depend on the on-disk wallet store \u2014 `get_mnemonic`, `change_mnemonic_password`,
and the offline/HD/ZHTLC superset of R-K4 \u2014 follow the native-only gating bound
in \u00a77.10. The activated-coin reveals (`show_priv_key` and the reduced
`get_private_keys` of R-K3) are available on whatever targets the requested
coins can be activated on.

**R-K10.** *Refactor-later (security hardening) \u2014 informative.* A future
hardening review is bound as deferred work, not as a current requirement: it
should consider rate-limiting and/or an explicit confirmation step for the
opt-in superset, an option to disable the export surface entirely, and audit
logging of export events. Any hardened behaviour introduced later MUST NOT be
weaker than the upstream protections this chapter adopts (hardware-wallet
rejection, no-plaintext-logging, single-serialization, password authentication).

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

**T6.** *Export-switch gating.* With `allow_insecure_key_export` absent or
false, a `get_private_keys` request that asks for an offline (non-activated)
coin, an `hd`-mode range, or a ZHTLC viewing key MUST be refused, while the
reduced activated-coins form (R-K3), `show_priv_key` (R-K2) and `get_mnemonic`
(R-K7) MUST still succeed. With the switch set to `true`, the same offline /
`hd` / ZHTLC requests MUST succeed. A hardware-wallet session MUST be rejected
by every key-export path in both switch states (R-K6).

## 7.12 Deferred Work

**D1.** A wallet-switch RPC that re-pins the active-wallet slot without
restarting the process is deferred. The bound write-once semantics of the
slot are not loosened in this chapter.

**D2.** *(Superseded.)* The earlier indefinite deferral of mnemonic export is
retired. Key export is now adopted under the default-off / opt-in posture of
§7.3A: own-seed self-export (R-K7) and the gated private-key superset (R-K4)
are bound capabilities, not deferred work. The security-hardening follow-ups
for the export surface are tracked in R-K10.

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
- *BIP-32 / BIP-44 — Hierarchical deterministic wallets and the
  multi-account/derivation-path hierarchy.* Define the derivation-path model
  bound for the HD export mode in R-K4.
- *ZIP-32 — Shielded hierarchical deterministic wallets.* Defines the shielded
  derivation-path model (`z_derivation_path`) and viewing-key derivation bound
  for ZHTLC export in R-K4.
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

### 7.13.1 Security and compatibility cross-references

The `allow_insecure_key_export` switch (R-K1) crosses the secure-by-default
boundary and re-enables upstream-parity key export; the same warning therefore
MUST be carried on every doc surface that an operator or integrator might read
in isolation. The following surfaces are identified for the cross-reference
(authored here; the others scheduled separately):

- This chapter (§7.3A) — the normative binding and the guardrail invariants.
- `docs/CODING_STANDARDS.md` §5.1 ("Documented compatibility exceptions") —
  currently states that the security exception never extends to fund-controlling
  secrets (wallet seeds and private keys). This switch is exactly such a
  carefully-bounded, **default-off, opt-in** exception for private-key export
  and MUST be reconciled there as the single CRD-authorised carve-out.
- `docs/GLEEC_COMPATIBILITY.md` — the central operator-facing compatibility
  table MUST gain a row: setting `allow_insecure_key_export=true` restores the
  GLEEC/upstream behaviour of unrestricted offline/HD/ZHTLC private-key export.
- `docs/COMPAT_SWITCHES.md` — the divergence convention this switch follows
  (KDF Reloaded chose the *more secure* default; the GLEEC-compatible value is
  the less-secure opt-in). Because the exposure here *is* a fund-controlling
  secret, the compatible value MUST remain opt-in (not the default), consistent
  with that document's security-versus-compatibility rule.
- `RELOADED_VS_GLEEC.md` (repository root) — the public catalogue of divergences
  MUST list the gated key-export divergence.
- Chapter 01 (clean-room rules) — the adopted request/response field names and
  the untagged response union of R-K4 are bound as Interop / wire-format reuse.

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
  (error envelope), Chapter 05 (HD-wallet mnemonic primitives), the
  reloaded-shipped activated-coins key-export code (`get_private_keys`,
  `show_priv_key`), the configuration switch convention of
  `COMPAT_SWITCHES.md` / `GLEEC_COMPATIBILITY.md`, and the external
  specifications listed in §7.13.
- *Permitted-input classes used:* baseline and reloaded source; chapter-bound
  protocol identifiers introduced here as wire contract (RPC method names,
  configuration field names, request/response field names, the untagged
  response union, on-disk file extension, error-`error_type` tokens); public
  IETF/BIP/SLIP/ZIP specifications; standard cryptographic primitive names.
- *Sibling chapters cross-referenced:* Chapter 01, Chapter 04, Chapter 05.
- *Author of this chapter:* clean-room round-2 driving-spec working set.
- *Forbidden corpus:* consulted only for dictated-interop wire surface (the
  public request/response field layout and method strings of the adopted
  key-export RPCs). No private identifiers, function bodies, control flow, or
  error/log string literals were carried across; all behaviour is restated as
  functional requirements.
