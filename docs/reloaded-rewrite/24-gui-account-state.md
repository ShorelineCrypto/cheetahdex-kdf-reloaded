# Chapter 24 -- GUI-Facing Account-State Persistence

**Status:** driving-spec

> **One-sentence claim:** the project carries a dedicated
> workspace crate for persisting GUI-visible "wallet account"
> state -- one record per Iguana / HD / hardware-wallet
> account, with display metadata and a per-account activated-
> ticker set -- behind a single async trait with a native
> backend and a browser-target stub, and exposes an eleven-
> method JSON-RPC namespace as its public surface.

## 24.0 Executive Summary

A GUI consumer of the codebase needs a stable concept of "the
current account" that is distinct from the in-process wallet
identity used by the coin support modules and the swap engine.
A dedicated workspace crate provides that surface: it stores
per-user account records, per-account display metadata (name,
description, fiat balance), and the set of activated coin
tickers each account has selected, all persisted between
daemon runs.

The crate is bounded by the following architectural rules:

1. **Three account-identity variants.** Iguana (legacy single-
   account mode), HD (BIP-44-style indexed accounts on a
   single seed), and HW (hardware-wallet device-keyed
   accounts) coexist under one identity enum. The crate does
   not invent additional account kinds.
2. **One trait, two backend implementations.** A single async
   trait abstracts persistence. The native build target carries
   a fully-functional native-SQL backend; the browser build
   target carries a stub backend that returns an explicit
   "not implemented" error from every method.
3. **Eleven JSON-RPC handlers** form the public surface a GUI
   uses to manage accounts and their activated-coin sets.
4. **Validation at the trait boundary.** Length and shape
   validation runs in the trait/handler layer before any
   storage call; backends do not re-check inputs.

At the time of writing the native backend, the type surface,
the trait, and the handler module are complete; the browser
backend is the stub of (2); and the eleven handlers are not
yet registered in the public RPC dispatcher (§24.9 D1).

## 24.1 Subsystem Shape

The crate is organised into the following functional regions:

| Region                     | Responsibility                                |
|----------------------------|-----------------------------------------------|
| Public re-exports          | Account identity, metadata, error types       |
| Per-context handle         | Lazy-init handle pattern; owns the backend    |
| Account identity & metadata| The identity enum and the metadata record     |
| Storage trait              | Async CRUD + activation surface               |
| Storage -- native          | Three-table backend over the native SQL layer |
| Storage -- browser         | Stub returning "not implemented" everywhere   |
| RPC handlers               | Eleven typed handlers, one per surface method |

The per-context handle is obtained through the lazy-init
pattern of the codebase's central-context substrate; call
sites acquire the handle from the central context and never
construct the backend directly.

## 24.2 Account Identity

The wire-level account identity is a tagged enum with three
variants:

| Variant | Carries                          | Purpose                                |
|---------|----------------------------------|----------------------------------------|
| Iguana  | (no payload)                     | Legacy single-account mode             |
| HD      | 32-bit account index             | BIP-44-style indexed HD account        |
| HW      | 160-bit hardware-device pubkey   | Hardware-wallet-keyed account          |

R1. **Closed identity set.** The three-variant identity enum
    is the single registration point for an account kind. New
    account kinds (for example, a future MPC-keyed account)
    require an explicit additional variant; the storage
    layout (§24.4) shall accommodate any new variant through
    its composite primary key without schema change.

R2. **Enabled-account restriction.** Only Iguana and HD
    variants are eligible to be marked as the **enabled**
    (active) account. Hardware-wallet accounts can be added,
    enumerated, renamed, and removed, but cannot be selected
    as the active context through this surface. (The
    rationale: hardware-wallet activation has a different
    lifecycle managed elsewhere; this surface intentionally
    does not own it.)

## 24.3 Account Metadata

Each account record carries:

| Field         | Constraint                                                    |
|---------------|---------------------------------------------------------------|
| Display name  | Up to 255 characters                                          |
| Description   | Up to 600 characters                                          |
| Fiat balance  | Decimal value stored as a string, up to 255 characters,       |
|               | required at storage but omittable on the wire (defaults to "")|

R3. **Validation at the trait boundary.** Name length,
    description length, ticker length, and balance shape are
    validated by the trait or handler layer **before** any
    storage call is issued. Backends do not re-check; they
    rely on the trait-layer guarantee.

R4. **Balance is opaque.** The fiat balance is stored as a
    decimal-string scalar; it is not interpreted as currency,
    not used for any in-tree calculation, and not bound to
    any specific currency code. The field is a pass-through
    cache from whatever oracle the GUI consults.

The activated-coins set hangs off each account as a separate
set of (account-key, ticker) pairs (§24.4).

## 24.4 Storage Trait

The single async storage trait exposes the following
behavioural operations:

| Category            | Operations                                                   |
|---------------------|--------------------------------------------------------------|
| Account CRUD        | Load one, upload one, delete one, load all                   |
| Enabled-account     | Get the active account, set the active account               |
| Metadata setters    | Set name, set description, set balance                       |
| Activated coins     | Activate tickers, deactivate tickers, load tickers           |

All trait methods are async and return the codebase's standard
`MmError`-wrapped result type, parameterised by the crate's
own storage-error enum.

R5. **Single trait surface.** All persistence operations on
    GUI account state flow through this trait. The handlers
    in §24.6 are the only public surface above the trait; no
    callers reach into the backend directly.

## 24.5 Native Backend Schema

The native backend owns three tables, created idempotently on
first connect:

| Table                    | Role                                                |
|--------------------------|-----------------------------------------------------|
| Accounts                 | One row per account (composite primary key + metadata) |
| Account-coin assignments | (account-key, ticker) pairs, cascade-deleted        |
| Enabled-account marker   | Single row identifying the active account           |

The accounts table's composite primary key is `(account_type,
account_idx, device_pubkey)`, so the three identity variants
of §24.2 coexist without collision: Iguana uses a sentinel
value in `account_idx` and `device_pubkey`; HD uses a real
`account_idx` and a sentinel `device_pubkey`; HW uses a
sentinel `account_idx` and a real `device_pubkey`.

The account-coin table and the enabled-account table each
carry foreign keys back to the composite primary key with
`ON DELETE CASCADE`, so deleting an account also removes its
activated tickers and the enabled marker if it was active.

R6. **Composite-key invariant.** The three-column composite
    primary key shall accommodate any future identity variant
    by appropriate use of sentinels for the columns that
    variant does not populate. Schema-level changes shall be
    additive; column removals are not permitted.

R7. **Cascade-on-delete.** Deletion of an account row shall
    cascade to its activated-coin rows and to the enabled-
    account marker if applicable. The cascade is enforced at
    the schema level, not in the handler.

R8. **No schema migrations at the time of writing.** The
    schema is whatever the initialiser creates on first
    connect. Any future shape change requires adding a
    migrations layer; this is a deferred item (§24.9 D3).

## 24.6 JSON-RPC Surface

The crate's public surface is **eleven** typed JSON-RPC
handlers. Each carries its own typed request and response
struct and its own validation error variants.

| Handler                     | Effect                                       |
|-----------------------------|----------------------------------------------|
| Add account                 | Insert a new account record                  |
| Delete account              | Remove an account (cascades coins + enabled) |
| Get accounts                | Enumerate all accounts                       |
| Get account coins           | List activated tickers for one account       |
| Get enabled account         | Return the active account, if any            |
| Enable account              | Mark account active (Iguana or HD only)      |
| Set account name            | Update display name                          |
| Set account description     | Update description                           |
| Set account balance         | Update fiat balance figure                   |
| Activate coins              | Append tickers to an account                 |
| Deactivate coins            | Remove tickers from an account               |

R9. **Public namespace.** When wired into the public RPC
    dispatcher (D1) these eleven handlers shall live under
    the `gui_storage::` namespace, with method names matching
    the table above (`gui_storage::add_account`,
    `gui_storage::delete_account`, ..., `gui_storage::
    deactivate_coins`).

R10. **Standard error mapping.** Each handler returns a typed
     error that implements the codebase's standard
     `MmError` and `HttpStatusCode` conventions. The error
     surface is owned by the handler module; the storage trait
     contributes its storage-error enum as one of the
     constituent variants.

R11. **Enabled-account validation.** The `enable_account`
     handler shall enforce R2 (no hardware-wallet variant can
     be enabled) at the handler boundary and return a typed
     validation error if the caller passes a hardware-wallet
     account identity.

## 24.7 Browser Backend Stub

R12. **Explicit not-implemented stub on the browser target.**
     On the browser build target every trait method shall
     return an explicit "not implemented" error variant rather
     than silently succeed, panic, or no-op. The stub exists
     so the crate compiles cleanly for the browser target and
     so any caller hitting it gets a clear, typed error.

The browser backend is the natural location for a future
IndexedDB port (D2); the port shall mirror the three-region
shape of the native backend.

## 24.8 Tests

Unit tests are colocated with the storage module. The unit-
test set at the time of writing covers the native backend
end-to-end against an in-memory database built through a
test-helper context constructor:

- Account lifecycle: upload, enable, load, delete.
- Metadata updates: name, description, balance.
- Coin activation and deactivation, including idempotence.
- Cascade-delete behaviour: deleting an account removes its
  activated-coin rows and clears the enabled-account marker
  if applicable.

The browser-target stub has no functional tests at the time
of writing (every call is expected to error). The RPC
handlers do not have integration tests at the time of writing
because they are not reachable through the dispatcher (D1).

## 24.9 Binding Requirements and Deferred Work

R1-R12 above are binding.

The following are **deferred work** named explicitly in scope
of this chapter:

D1. **Public dispatcher registration.** The eleven handlers
    of §24.6 shall be registered in the public RPC
    dispatcher under the `gui_storage::` namespace per R9.
    At the time of writing the handler module exists but is
    not registered; closing this gap is the single biggest
    blocker to consumer adoption.

D2. **Browser-target persistence.** The browser-target stub
    of §24.7 shall be replaced by an IndexedDB-backed
    implementation that mirrors the three-region shape of
    the native backend (one object store per region;
    cascade-on-delete enforced in code at the trait
    boundary if not at the store level).

D3. **Schema migrations.** A migrations layer shall be added
    so that schema evolutions can land without breaking
    existing deployed databases. At the time of writing the
    initialiser is the only schema-creation path.

D4. **Account-record versioning.** A schema-version field on
    the accounts table shall be added so future record-
    shape changes can be staged behind a per-row version
    discriminator without requiring a full data migration.

D5. **Hardware-wallet active-account model.** The R2
    enabled-restriction is a deliberate scope limit. A
    future revision shall either relax the enabled-account
    set to include hardware-wallet accounts (with the
    appropriate confirmation flow) or introduce a parallel
    "active hardware device" concept; this chapter does not
    bind which.

D6. **Bulk import / export.** A GUI-facing backup flow
    requires the ability to export all accounts and their
    activated-coin sets in one operation and import them in
    one operation. Not present at the time of writing.

## 24.10 External References

- BIP-44 (the public derivation-path standard that backs the
  HD account index used by the HD identity variant of §24.2).
- The codebase's standard error and HTTP-status mapping
  conventions referenced by R10.
- The codebase's per-context handle pattern (lazy-init
  under the central-context substrate) referenced by the
  lazy-init rule of §24.1.
- The codebase's cross-platform persistence approach
  ([Chapter 26](26-cross-platform-and-wasm.md)) referenced by
  the browser-target stub of §24.7.
- The native SQL abstractions
  ([Chapter 25](25-sql-query-builder.md)) over which the
  native backend is built.

## 24.11 Baseline Verifications

The following are verifiable from the baseline state defined
in [Chapter 02](02-baseline-state.md), commit
`c1d46c0c1592faa0860f704008b2b2381bc3840f`:

V1. The baseline tree contains **no** GUI account-state
    persistence crate. A directory listing of the baseline
    tree (`git ls-tree c1d46c0c1592faa0860f704008b2b2381bc3840f`)
    contains no `mm2_gui_storage` entry; a tree-wide
    `git grep -l 'gui_storage\|AccountStorage\|AccountContext'`
    against the baseline returns no matches.

V2. The baseline public RPC dispatcher carries no
    `gui_storage::` namespace. The library-only posture of
    the crate at the time of writing (D1 deferred) is
    consistent with the baseline's complete absence of this
    surface.

V3. The three-variant identity enum of §24.2 corresponds to
    the three wallet identity styles the codebase already
    supports elsewhere: legacy single-key (Iguana), BIP-44
    HD account indexing, and hardware-wallet device-keyed
    identity. These are not new identity styles invented by
    this chapter.

## 24.12 Provenance Footer

- *Status:* driving-spec.
- *Version:* v2.
- *Verified against:* baseline commit
  `c1d46c0c1592faa0860f704008b2b2381bc3840f`; absence of the
  GUI account-state crate at baseline verified via
  `git ls-tree c1d46c0c1592faa0860f704008b2b2381bc3840f`
  and tree-wide `git grep` for the trait, context, and
  namespace identifiers against the baseline; BIP-44 (the
  public derivation-path standard that backs the HD identity
  variant of §24.2); the codebase's standard error and HTTP-
  status mapping conventions; the codebase's per-context
  handle pattern and cross-platform persistence approach
  (Chapters 8 and 26); the native SQL abstractions of
  Chapter 25.
- *Forbidden corpus:* not consulted.
