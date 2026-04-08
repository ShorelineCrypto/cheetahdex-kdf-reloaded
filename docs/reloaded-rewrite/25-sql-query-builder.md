# Chapter 25 — SQL Query-Builder Replacement

> **Chapter type:** document existing. No IMPL marker.

## 25.0 Executive summary

The reloaded tree carries a substantially extended
[`mm2src/db_common/`](../../mm2src/db_common/) crate. Its role
is to be the **single SQLite gateway** for every native-side
persistence consumer in the workspace: an async wrapper around
`rusqlite::Connection`, a typed query-builder DSL, and a small
toolbox of identifier-validation and pragma helpers.

Compared with the baseline (`c1d46c0`), the crate was three
files (`Cargo.toml`, `lib.rs`, `sqlite.rs`). Everything else
in the current tree is post-baseline:

- `async_sql_conn.rs` (about 300 LOC) -- an async `Connection`
  facade backed by a dedicated worker thread + crossbeam
  channels.
- The DSL files `sql_condition.rs`, `sql_constraint.rs`,
  `sql_create.rs`, `sql_delete.rs`, `sql_insert.rs`,
  `sql_query.rs`, `sql_update.rs`, `sql_value.rs` (about 2.2
  KLOC combined) -- the typed builder API, re-exported under a
  virtual `db_common::sql_build` namespace.
- `async_conn_tests.rs` (about 250 LOC) -- async unit tests.
- A 330-line extension to `sqlite.rs` adding validation
  helpers and a `SqliteConnShared` alias.

The whole crate is native-only: every module other than the
test module is gated on `#[cfg(not(target_arch =
"wasm32"))]`. WASM persistence is the responsibility of
`mm2_db::indexed_db`, documented in
[Chapter 26](26-cross-platform-and-wasm.md).

## 25.1 Crate layout

```
mm2src/db_common/
|-- Cargo.toml
`-- src/
    |-- lib.rs                  Module roots + sql_build re-export
    |-- sqlite.rs               Low-level helpers, validation, pragmas
    |-- async_sql_conn.rs       AsyncConnection + worker thread
    |-- async_conn_tests.rs     Async tests
    |-- sql_condition.rs        WHERE-clause trait
    |-- sql_constraint.rs       PrimaryKey / Unique / ForeignKey
    |-- sql_create.rs           SqlCreateTable, SqlColumn, SqlType
    |-- sql_delete.rs           SqlDelete
    |-- sql_insert.rs           SqlInsert
    |-- sql_query.rs            SqlQuery (SELECT), SqlSubquery
    |-- sql_update.rs           SqlUpdate
    `-- sql_value.rs            SqlValue, SqlValueOptional, FromQuoted
```

The flat `sql_*.rs` modules are declared as `mod` in `lib.rs`
and then collected behind a single re-export module:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod sql_build {
    pub use crate::sql_condition::SqlCondition;
    pub use crate::sql_constraint::{foreign_key, ForeignKey,
                                    PrimaryKey, SqlConstraint, Unique};
    pub use crate::sql_create::{SqlColumn, SqlCreateTable,
                                SqlType, TableKey};
    pub use crate::sql_delete::SqlDelete;
    pub use crate::sql_insert::SqlInsert;
    pub use crate::sql_query::{SqlQuery, SqlSubquery};
    pub use crate::sql_update::SqlUpdate;
    pub use crate::sql_value::{FromQuoted, SqlValue, SqlValueOptional};
}
```

Consumers therefore import via `db_common::sql_build::{...}`
even though there is no `sql_build/` directory on disk.

## 25.2 Three layers

The crate stacks cleanly into three layers:

1. **Validation / helpers** ([`sqlite.rs`](../../mm2src/db_common/src/sqlite.rs))
   -- identifier checks, parameter newtypes, pragma helpers.
2. **DSL builders** (the eight `sql_*.rs` modules) -- typed,
   fluent constructors for SELECT / INSERT / UPDATE / DELETE
   / CREATE TABLE, all sitting on top of the third-party
   `sql-builder` crate.
3. **Async facade** ([`async_sql_conn.rs`](../../mm2src/db_common/src/async_sql_conn.rs))
   -- an awaitable handle that dispatches closures onto a
   dedicated worker thread holding the synchronous
   `rusqlite::Connection`.

A consumer typically composes a builder in async context,
hands the closure to `AsyncConnection::call`, and lets the
worker thread run `to_sql()` + `execute` / `query`.

## 25.3 Validation layer (`sqlite.rs`)

[`sqlite.rs`](../../mm2src/db_common/src/sqlite.rs) exposes
about twenty public items. The ones most relevant to
consumers:

- `type SqliteConnShared = Arc<Mutex<Connection>>` -- the
  reference-counted, mutex-guarded connection handle used in
  legacy synchronous paths.
- `type OwnedSqlParams = Vec<rusqlite::types::Value>` and
  `type OwnedSqlNamedParams = Vec<(&'static str, Value)>` --
  owned param vectors so closures can move them across
  threads.
- `validate_ident(ident)` and `validate_table_name(table)` --
  reject anything outside `[A-Za-z0-9_]` (and `.` for the
  identifier variant, which has to accept dotted column
  references), reject empty strings and identifiers starting
  with a digit, and reject a fixed list of SQL keywords. Used
  by every DSL builder before SQL string assembly.
- `ToValidSqlTable` / `ToValidSqlIdent` -- traits implemented
  for `&str` / `String` / newtypes that drive validation at
  the type level.
- `query_single_row<T>(conn, sql, params, mapper)` --
  small wrapper around `conn.query_row()` that maps the
  "no rows" case to `Ok(None)` instead of an error.
- `offset_by_id` -- helper for `ROW_NUMBER`-style cursor
  pagination.
- `h256_slice_from_row`, `h256_option_slice_from_row` -- hex
  string -> `[u8; 32]` row mappers used by chain-data tables.
- `run_optimization_pragmas(conn)` -- sets `journal_mode =
  WAL`, `synchronous = normal`, `temp_store = memory`, and
  `foreign_keys = ON`.
- `is_constraint_error(err)` -- inspect a `rusqlite::Error`
  to decide whether to map it to a domain "no such row"
  error.

The crate also provides two macros: `owned_named_params!` for
ergonomic construction of `OwnedSqlNamedParams`, and
`foreign_columns!` (re-exported by `sql_build`) for
declaring foreign-key column pairs.

## 25.4 DSL layer

Every builder is a stateful struct that records columns,
parameters, constraints, conditions, and an output target,
then emits an `(sql_string, owned_params)` pair to be run on a
`Connection`.

### CREATE TABLE

```rust
use db_common::sql_build::{SqlColumn, SqlCreateTable, SqlType,
                           PrimaryKey, foreign_key};

let mut create = SqlCreateTable::new(conn, "gui_account");
create
    .if_not_exist()
    .column(SqlColumn::new("account_type", SqlType::Integer).not_null())
    .column(SqlColumn::new("account_idx",  SqlType::Integer).not_null())
    .column(SqlColumn::new("device_pubkey", SqlType::Varchar(20)).not_null())
    .column(SqlColumn::new("name",         SqlType::Varchar(255)))
    .constraint(PrimaryKey::new(
        "pk_gui_account",
        ["account_type", "account_idx", "device_pubkey"],
    )?)?;
create.create()?;
```

`SqlType` enumerates the column types that the DSL knows how
to render (`Integer`, `Real`, `Text`, `Varchar(n)`, `Blob`,
...); `TableKey` and the constraint types live in
`sql_constraint.rs`.

### INSERT / UPDATE / DELETE

```rust
use db_common::sql_build::{SqlInsert, SqlCondition};

let mut insert = SqlInsert::new(conn, "gui_account");
insert
    .or_replace()
    .column_param("account_type", account.kind_id())?
    .column_param("account_idx",  account.idx())?
    .column_param("device_pubkey", account.device_pubkey_hex())?
    .column_param("name",          account.name())?;
let _rows = insert.insert()?;
```

`SqlUpdate` and `SqlDelete` follow the same shape; both
implement `SqlCondition`, the WHERE-clause trait, which gives
them `.and_where_eq(col, val)`, `.and_where_eq_param(...)`,
`.and_where_in_params(col, params)`, plus `or_*` variants and
`IS NULL` handling via `Option`.

### SELECT

```rust
use db_common::sql_build::{SqlQuery, SqlCondition};

let mut query = SqlQuery::select_from(conn, "gui_account")?;
query
    .field("account_idx")?
    .and_where_eq_param("account_type", account_type)?
    .order_desc("account_idx")?
    .limit(10)?;
let rows: Vec<u32> = query.query(|row| row.get(0))?;
```

`SqlSubquery` lets a `SqlQuery` be embedded inside another
query in places where the dialect allows it.

### Value handling

`SqlValue` and `SqlValueOptional` give the DSL a single typed
shape for "a bound value" so that constraint and condition
helpers can take either an owned literal or a `None`. The
DSL deliberately keeps user-supplied data parameterised --
the only path where values enter the rendered SQL string is
the `*_quoted` family on `SqlInsert`, which is reserved for
literals known statically.

## 25.5 Async facade

[`async_sql_conn.rs`](../../mm2src/db_common/src/async_sql_conn.rs)
exposes `AsyncConnection`, an awaitable handle around a
single dedicated worker thread:

```rust
pub struct AsyncConnection { sender: crossbeam_channel::Sender<Message> }

type CallFn = Box<dyn FnOnce(&mut Connection) + Send + 'static>;

enum Message {
    Execute(CallFn),
    Close(oneshot::Sender<rusqlite::Result<()>>),
}
```

The constructor (`AsyncConnection::open(path)` or
`AsyncConnection::open_in_memory()`) `thread::spawn`s the
worker, hands it the synchronous `rusqlite::Connection`, and
awaits a "ready" signal over a oneshot channel before
returning. From then on every call is a one-shot
request/response:

```rust
pub async fn call<F, R>(&self, f: F) -> Result<R>
where F: FnOnce(&mut Connection) -> Result<R> + Send + 'static,
      R: Send + 'static;
```

`call` packages the closure into a `Message::Execute`,
allocates a `futures::channel::oneshot` for the reply, sends
the message over the crossbeam channel, and awaits the reply.
There is also a `call_unwrap` variant for paths where the
caller is willing to panic on internal failures.

`close` sends a `Message::Close`, the worker performs
`conn.close()` and replies, then exits its loop. Errors are
modeled by `AsyncConnError` (transport, internal, or
wrapped `rusqlite::Error`).

This design intentionally avoids `tokio::task::spawn_blocking`
for two reasons: SQLite is a single-writer engine, so
serialising on a dedicated thread matches the engine model;
and using the runtime's blocking pool would mix database I/O
with other unrelated blocking work and complicate shutdown.

## 25.6 Cross-cutting use in the workspace

The crate is consumed by every native-side persistence
component:

- `mm2_core/src/mm_ctx.rs` -- `MmCtx` owns both
  `Constructible<SqliteConnShared>` and an
  `AsyncMutex<AsyncConnection>`, see
  [Chapter 8](08-mm-ctx-and-state-layering.md).
- `mm2_gui_storage` ([Chapter 24](24-gui-account-state.md))
  -- the three GUI-account tables go through the DSL.
- `coins/sql_tx_history_storage.rs`, `coins/tx_history_db.rs`
  and the various per-protocol history stores -- typed
  builders for tx history.
- `coins/lightning_persister/*` -- channel state and
  payments persistence.
- `kdf_walletconnect/src/storage/sqlite.rs`
  ([Chapter 22](22-walletconnect-v2.md)) -- the WC session
  store.
- `coins/nft/store/sqlite/`
  ([Chapter 19](19-nft-module-layout.md)) -- NFT tables.

The intent is that everything in the workspace that touches
SQLite touches it through `db_common`; bare `rusqlite::open`
in feature code is the exception rather than the rule.

## 25.7 Tests

[`async_conn_tests.rs`](../../mm2src/db_common/src/async_conn_tests.rs)
covers the async facade end-to-end against in-memory
databases:

- `open_in_memory_test` -- basic construction and ready
  signal.
- `call_success_test`, `call_unwrap_success_test` -- closure
  execution and result propagation.
- `call_failure_test` -- error propagation across the
  channel.
- `close_success_test`, `double_close_test`, `close_call_test`
  -- close semantics including double-close and post-close
  calls.

Each DSL module also carries inline `#[test]` cases that
exercise the rendered SQL string and the round-trip against
an in-memory connection (NULL handling, `OR REPLACE`,
single-column inserts, `DELETE ALL`, etc.). There is no
integration test in `mm2_main` that exercises the crate as a
whole; instead, each consumer crate covers its own usage.

## 25.8 Limitations and known gaps

1. **rusqlite is pinned at `0.24.2`.** Upgrading would touch
   every `lib.rs` and the deprecated-API allowance in
   `async_sql_conn.rs`; an explicit TODO in the source notes
   the wish to remove `#![allow(deprecated)]` once the
   workspace MSRV permits a newer rusqlite.
2. **No streaming `fetch`.** `SqlQuery::query` materialises
   the entire result set; queries that need to iterate
   row-by-row over a large table must drop down to raw
   `rusqlite` inside the closure.
3. **JOIN support is intentionally minimal.** Non-trivial
   multi-table queries today are written as raw SQL inside a
   `call` closure; the DSL covers single-table CRUD and the
   common SELECT shapes only.
4. **All operations serialise on a single worker thread.**
   This is a feature for write-heavy paths (matches SQLite's
   single-writer model), but it does mean read concurrency on
   a single `AsyncConnection` is zero. Multi-handle pools are
   not provided.
5. **`StringError` lives in this crate.** A TODO comment in
   `sqlite.rs` flags moving it to
   [`mm2_err_handle::common_errors`](../../mm2src/mm2_err_handle/)
   so it can be shared more broadly.
6. **Constraint-error mapping is by-string-prefix.**
   `is_constraint_error` works against the `rusqlite::Error`
   variants, but the higher-level "translate FK violation to
   NoSuchRow" pattern is reimplemented in every consumer; a
   shared helper would be a small follow-on.

## 25.9 Provenance

In the baseline (`c1d46c0`), `mm2src/db_common/` contained
only `Cargo.toml`, `lib.rs`, and the original `sqlite.rs`
(verified via `git ls-tree c1d46c0 -- mm2src/db_common/src`).
The async facade, every `sql_*.rs` DSL module, the async
tests, and the validation extensions to `sqlite.rs` are all
post-baseline. The DSL builders sit on top of the
third-party `sql-builder` crate (registered under the legal
classification register in
[`local/legal/AUDIT_FILE_CLASSIFICATION.md`](../../local/legal/AUDIT_FILE_CLASSIFICATION.md)).
