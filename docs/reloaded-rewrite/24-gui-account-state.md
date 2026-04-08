# Chapter 24 — GUI-Facing Account-State Persistence

> **Chapter type:** document existing. No IMPL marker.

## 24.0 Executive summary

The reloaded tree includes a dedicated crate for GUI-side
account persistence: [`mm2src/mm2_gui_storage/`](../../mm2src/mm2_gui_storage/).
It stores per-user "wallet account" records (Iguana / HD / HW
variants), per-account metadata (name, description, fiat
balance) and the set of activated coin tickers. The crate is
post-baseline (`git ls-tree c1d46c0 -- mm2src/mm2_gui_storage`
returns empty).

The intent of this crate is to give a GUI a stable RPC surface
to manage *what the user sees as an "account"*, separate from
the in-process wallet identity that `coins/` and `mm2_main/`
work with. A GUI can create / rename / enumerate accounts,
mark one as "enabled" (the active context), and track which
coins each account has activated -- all persisted between
runs.

Current status:

- **Types and trait** are complete and stable.
- **SQLite backend** is complete (three normalised tables with
  cascading deletes and validation at the trait boundary).
- **WASM backend** is a stub that returns an explicit
  "not implemented" error from every method.
- **RPC handlers** for eleven methods are written but **not
  registered in the JSON-RPC dispatcher**. Grepping
  `mm2_main/src/rpc/dispatcher/dispatcher.rs` for
  `gui_storage` returns no matches; the handlers are dead code
  from a routing standpoint today.

## 24.1 Crate layout

```
mm2src/mm2_gui_storage/
|-- Cargo.toml
`-- src/
    |-- lib.rs                          (module re-exports)
    |-- context.rs                      (AccountContext on MmArc)
    |-- rpc_commands.rs                 (ten RPC handlers)
    `-- account/
        |-- mod.rs                      (AccountId, AccountInfo)
        `-- storage/
            |-- mod.rs                  (AccountStorage trait + dispatch)
            |-- sqlite_storage.rs       (native backend)
            |-- wasm_storage.rs         (WASM stub)
            `-- account_storage_tests.rs
```

Roughly 2.7 KLOC of Rust, almost entirely on the native
backend and the handler module.

## 24.2 Account identity

The wire-level identity is a tagged enum in
[`account/mod.rs`](../../mm2src/mm2_gui_storage/src/account/mod.rs):

```rust
pub enum AccountId {
    Iguana,                         // legacy single-account mode
    HD  { account_idx: u32 },       // BIP-44 HD account index
    HW  { device_pubkey: H160 },    // hardware-wallet device id
}
```

Each variant flattens to a `(account_type, account_idx,
device_pubkey)` composite key in storage. The crate enforces a
top-level invariant: **only `Iguana` and `HD` accounts may be
marked enabled**; `HW` accounts can be enumerated but not
selected as the active context.

`AccountInfo` carries the user-facing metadata for an account:
display name (max 255 chars), description (max 600 chars),
and a fiat balance figure (stored as a `VARCHAR(255)` `NOT
NULL` decimal string in the schema; the RPC request layer
applies a `#[serde(default)]` so the field can be omitted on
the wire). The activated-coins set hangs off the account by
primary key (see §24.4 schema).

## 24.3 Storage trait and platform dispatch

`AccountStorage` is the async trait every backend implements,
defined in
[`account/storage/mod.rs`](../../mm2src/mm2_gui_storage/src/account/storage/mod.rs).
Backend selection follows the standard `cfg` pattern:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub type StorageImpl = sqlite_storage::SqliteAccountStorage;

#[cfg(target_arch = "wasm32")]
pub type StorageImpl = wasm_storage::WasmAccountStorage;
```

The trait exposes account-level CRUD (`load_account`,
`upload_account`, `delete_account`, `load_accounts`), enabled-
account get/set, metadata setters (name / description /
balance), and coin-activation primitives (`activate_coins`,
`deactivate_coins`, `load_account_coins`). All methods are
async and return `MmResult<_, AccountStorageError>`.

`AccountContext`
([`context.rs`](../../mm2src/mm2_gui_storage/src/context.rs))
is the per-`MmArc` handle that lazy-builds and owns the
backend, following the [Chapter 8](08-mm-ctx-and-state-layering.md)
context pattern; call sites use `AccountContext::from_ctx(&mm)`
and never construct the backend directly.

## 24.4 SQLite backend

[`sqlite_storage.rs`](../../mm2src/mm2_gui_storage/src/account/storage/sqlite_storage.rs)
is the only fully-functional backend today. It owns three
tables, created idempotently with `CREATE TABLE IF NOT
EXISTS`:

| Table                  | Role                                        |
|------------------------|---------------------------------------------|
| `gui_account`          | one row per account; metadata               |
| `gui_account_coins`    | (account FK, ticker) -- activated coins     |
| `gui_account_enabled`  | single row marking the active account       |

Composite primary key on `gui_account` is `(account_type,
account_idx, device_pubkey)` so that the three `AccountId`
variants coexist without collision. `gui_account_coins` and
`gui_account_enabled` carry foreign keys back to that triple,
with `ON DELETE CASCADE` so that deleting an account also
removes its coins and the enabled marker if it was active.

There are no migrations: the schema is whatever `init()`
creates on first connect. Any future shape change will need a
migration layer added.

Validation -- name length, description length, ticker length,
balance shape -- is enforced in the trait/handler layer before
the SQL is issued, so the backend itself does not need to
re-check.

## 24.5 WASM backend

[`wasm_storage.rs`](../../mm2src/mm2_gui_storage/src/account/storage/wasm_storage.rs)
is a stub: every `AccountStorage` method returns a fixed
"WASM stub" error variant rather than touching IndexedDB. The
IndexedDB port is deferred work (the same pattern other crates
in [Chapter 26](26-cross-platform-and-wasm.md) describe). The
stub exists so that the crate compiles cleanly for the
`wasm32-unknown-unknown` target and the trait surface is
available; any caller hitting it on WASM gets a clear error
rather than a silent no-op.

## 24.6 RPC handlers

[`rpc_commands.rs`](../../mm2src/mm2_gui_storage/src/rpc_commands.rs)
defines eleven handlers, each with its own typed request and
response struct:

| Handler                       | Effect                              |
|-------------------------------|-------------------------------------|
| `add_account`                 | insert a new account record         |
| `delete_account`              | remove account (cascades coins)     |
| `get_accounts`                | enumerate all accounts              |
| `get_account_coins`           | list activated tickers for account  |
| `get_enabled_account`         | return active account, if any       |
| `enable_account`              | mark account active (Iguana/HD)     |
| `set_account_name`            | update display name                 |
| `set_account_description`     | update description                  |
| `set_account_balance`         | update fiat balance figure          |
| `activate_coins`              | append tickers to an account        |
| `deactivate_coins`            | remove tickers from an account      |

Each handler returns a typed error
(`AccountStorageError` plus its own validation errors) and
follows the project's `MmError` + `HttpStatusCode` convention.

**These handlers are not yet registered in the dispatcher.**
Grepping
[`mm2_main/src/rpc/dispatcher/dispatcher.rs`](../../mm2src/mm2_main/src/rpc/dispatcher/dispatcher.rs)
for `gui_storage` produces no hits, and `mm2_main` does not
declare `mm2_gui_storage` as a dependency. Wiring them in
would require adding a `gui_storage::` namespace branch to
`dispatcher_v2` plus the dependency edge. The intended
external surface (per AGENTS.md "RPC Overview") is
`gui_storage::<method>`, e.g. `gui_storage::add_account`.

## 24.7 Tests

[`account_storage_tests.rs`](../../mm2src/mm2_gui_storage/src/account/storage/account_storage_tests.rs)
covers the SQLite backend end-to-end against an in-memory
database built via the test helper `mm_ctx_with_custom_db`:

- account lifecycle (upload / enable / load / delete);
- metadata updates (name, description, balance);
- coin activation and deactivation;
- cascading delete behaviour.

The WASM stub has no functional tests (every call is expected
to error). RPC handlers do not have direct integration tests
because they are not reachable through the dispatcher yet.

## 24.8 Known limitations and deferred work

1. **No dispatcher registration.** Eleven handlers, zero
   routes. Adding the dependency and a `gui_storage::` branch
   in `dispatcher_v2` would make the surface usable; this is
   the single biggest blocker to consumer adoption.
2. **WASM backend is a stub.** Browser users get errors on
   every call; an IndexedDB implementation mirroring the
   three-table SQLite layout is needed.
3. **No schema migrations.** `init()` creates the tables on
   first run; any subsequent schema change requires a
   migrations layer.
4. **No account-versioning field.** Related: a future schema
   evolution will need either migrations or a versioned record
   format.
5. **`HW` accounts are inert.** They can be added and
   enumerated but cannot be set enabled; the eventual hardware-
   wallet UX needs the enabled-set rules relaxed (or a
   separate "active HW device" concept).
6. **No bulk import / export.** Useful for GUI backup flows
   and not yet present.

## 24.9 Provenance

`mm2src/mm2_gui_storage/` is post-baseline; the crate is new
in reloaded. SQLite access goes through the shared
[`db_common::sql_build`](../../mm2src/db_common/) helpers
documented in [Chapter 25](25-sql-query-builder.md); no
third-party persistence library is vendored into this crate.
