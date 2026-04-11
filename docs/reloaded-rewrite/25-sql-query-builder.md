# Chapter 25 — Single SQLite Gateway Substrate

**Status:** driving-spec.

The chapter binds the substrate by which the chapter-bound
storage crate `db_common` becomes the single SQLite gateway for
every native-side persistence consumer in the workspace: a
chapter-bound asynchronous connection facade, a chapter-bound
typed query-builder substrate exposed under a chapter-bound
single re-export module, a chapter-bound identifier-validation
and pragma-helper toolbox, and a chapter-bound consumer-routing
discipline.

## 25.1 Executive Summary

The chapter-bound storage crate `db_common` at the chapter-02-
anchored baseline carried three chapter-bound source files only:
the chapter-bound manifest, the chapter-bound crate-root module,
and a chapter-bound low-level synchronous SQLite-helper module
(R2). The chapter-bound substrate extends the crate to be the
single SQLite gateway for every native-side persistence consumer
in the workspace.

The substrate consists of three chapter-bound stacking layers:

| Bound layer                                  | Bound contract                                                                                                      |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Validation-and-pragma layer (R3, R5)         | Identifier-validation accessors, parameter-newtype aliases, pragma-application helpers.                              |
| Typed query-builder layer (R6–R10)           | Eight chapter-bound query-builder modules collected behind a single chapter-bound re-export module.                  |
| Asynchronous-connection layer (R11–R14)      | An awaitable connection handle backed by a chapter-bound dedicated worker thread plus a chapter-bound message-channel substrate. |

The crate is native-only: every chapter-bound module other than
the test module is gated on the chapter-bound non-WebAssembly
target predicate (R15). Chapter 26 binds the parallel WebAssembly
persistence substrate; this chapter binds the native side.

Bound rules R1–R5 cover the crate-layout and validation layer;
R6–R10 cover the typed query-builder substrate; R11–R14 cover
the asynchronous-connection facade; R15–R17 cover the platform
gate, the consumer-routing discipline, and the chapter-bound
deprecated-application-programming-interface allowance.

## 25.2 Subsystem Shape

The substrate occupies a structural seam between the chapter-bound
SQLite engine, the chapter-bound third-party synchronous SQLite
binding crate, the chapter-bound third-party query-string-assembly
crate the typed query-builder layer composes on top of, the
chapter-bound asynchronous runtime, and every native-side chapter-
bound persistence consumer in the workspace (R16). The substrate
does *not* modify the chapter-bound SQLite engine, the chapter-
bound binding crate, or the chapter-bound query-string-assembly
crate; it composes them into the chapter-bound single-gateway
shape.

## 25.3 Bound Crate Layout

**R1.** The chapter-bound crate layout at the substrate landing
point MUST extend the chapter-02-anchored three-file crate to
exactly the following module set:

| Bound module                | Bound role                                                                                                                                                               |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Crate-root module           | Module roots plus the chapter-bound re-export module of R10.                                                                                                              |
| Low-level helper module     | Low-level synchronous SQLite helpers, the chapter-bound validation accessors of R3, the chapter-bound parameter newtypes of R4, the chapter-bound pragma-application helper of R5, and the chapter-bound row-mapper helpers of R3. |
| Asynchronous-connection module | The chapter-bound asynchronous connection facade of R11–R14.                                                                                                          |
| Asynchronous-test module    | Asynchronous unit tests for the chapter-bound asynchronous facade (T1).                                                                                                  |
| Query-builder condition module     | The chapter-bound WHERE-clause trait of R8.                                                                                                                       |
| Query-builder constraint module    | The chapter-bound primary-key / unique / foreign-key constraint substrate (R7).                                                                                  |
| Query-builder create-table module  | The chapter-bound CREATE-TABLE builder, the chapter-bound column descriptor, and the chapter-bound type enumeration (R7).                                          |
| Query-builder delete module        | The chapter-bound DELETE builder (R6).                                                                                                                            |
| Query-builder insert module        | The chapter-bound INSERT builder (R6).                                                                                                                            |
| Query-builder select module        | The chapter-bound SELECT builder and the chapter-bound sub-query substrate (R6, R9).                                                                              |
| Query-builder update module        | The chapter-bound UPDATE builder (R6).                                                                                                                            |
| Query-builder value module         | The chapter-bound bound-value type and its optional-value variant plus the chapter-bound from-quoted accessor (R10).                                              |

**R2.** The chapter-02-anchored three-file baseline (the chapter-
bound manifest, the chapter-bound crate-root module, and the
chapter-bound low-level synchronous SQLite-helper module) MUST be
preserved as the starting point: every substrate addition is a
chapter-bound new module, not a chapter-bound rewrite of the
chapter-02-anchored low-level helper module.

## 25.4 Bound Validation-and-Pragma Layer

**R3.** The chapter-bound low-level helper module MUST expose the
following chapter-bound validation, query-helper, and row-mapper
accessor set:

| Bound accessor                                   | Bound contract                                                                                                                                                  |
| ------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `validate_ident` and `validate_table_name`       | Reject anything outside the chapter-bound alphanumeric-and-underscore character class (with the period character additionally permitted by the identifier variant to admit chapter-bound dotted column references); reject empty strings; reject identifiers starting with a digit; reject a chapter-bound fixed list of SQL keywords. Every typed query-builder MUST run validation before SQL-string assembly. |
| `ToValidSqlTable` and `ToValidSqlIdent` traits   | Implemented for `&str`, `String`, and chapter-bound newtypes; drive validation at the type level.                                                                |
| `query_single_row<T>(conn, sql, params, mapper)` | A chapter-bound thin wrapper around the chapter-bound binding-crate single-row accessor that maps the chapter-bound no-rows arm to `Ok(None)` instead of an error. |
| `offset_by_id`                                   | Chapter-bound helper for chapter-bound row-number-style cursor pagination.                                                                                       |
| `h256_slice_from_row`, `h256_option_slice_from_row` | Chapter-bound hexadecimal-string-to-32-byte-array row mappers consumed by chapter-bound chain-data tables.                                                  |
| `is_constraint_error(err)`                       | Inspect a chapter-bound binding-crate error to decide whether to map it to a domain *no-such-row* error.                                                         |

**R4.** The chapter-bound parameter-newtype aliases MUST be
exactly two:

| Bound alias                  | Bound shape                                                                                                                       |
| ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `SqliteConnShared`           | A chapter-bound reference-counted, mutex-guarded synchronous-connection handle used by chapter-bound legacy synchronous paths.    |
| `OwnedSqlParams`             | An owned vector of chapter-bound binding-crate value cells so closures can move bound parameters across thread boundaries.        |
| `OwnedSqlNamedParams`        | An owned vector of chapter-bound named-parameter pairs (a chapter-bound static-lifetime key paired with a chapter-bound value cell) for the same cross-thread move discipline. |

**R5.** The chapter-bound pragma-application helper
`run_optimization_pragmas(conn)` MUST apply exactly the following
chapter-bound four-pragma set: write-ahead-log journal mode;
normal-level synchronous-mode; in-memory temporary-store mode;
foreign-key enforcement on.

**R6.** The substrate MUST expose two chapter-bound supporting
macros:

| Bound macro              | Bound role                                                                       |
| ------------------------ | -------------------------------------------------------------------------------- |
| `owned_named_params!`    | Ergonomic construction of the chapter-bound owned named-parameter vector of R4. |
| `foreign_columns!`       | Declaration of chapter-bound foreign-key column pairs, re-exported through the chapter-bound re-export module of R10. |

## 25.5 Bound Typed Query-Builder Substrate

**R7.** The chapter-bound typed query-builder substrate MUST
expose exactly the following builder set, each as a chapter-bound
stateful struct that records columns, parameters, constraints,
conditions, and an output target, and then emits an
`(sql_string, owned_params)` pair to be run on a chapter-bound
connection handle:

| Bound builder        | Bound operation               |
| -------------------- | ----------------------------- |
| `SqlCreateTable`     | CREATE TABLE                  |
| `SqlInsert`          | INSERT (with chapter-bound `or_replace` variant) |
| `SqlUpdate`          | UPDATE                        |
| `SqlDelete`          | DELETE                        |
| `SqlQuery`           | SELECT (with chapter-bound order, limit, and field accessors) |
| `SqlSubquery`        | A chapter-bound select substrate embeddable inside another query where the chapter-bound dialect allows it (R9). |

The chapter-bound CREATE-TABLE substrate MUST carry a chapter-
bound column-type enumeration `SqlType` that enumerates the
chapter-bound column types the substrate renders (the chapter-
bound integer / real / text / varchar-of-bound-width / blob arm
set), a chapter-bound table-key descriptor `TableKey`, and the
chapter-bound constraint substrate of R8.

**R8.** The chapter-bound constraint substrate MUST expose
exactly three chapter-bound constraint kinds (the chapter-bound
primary-key constraint `PrimaryKey`, the chapter-bound unique
constraint `Unique`, and the chapter-bound foreign-key constraint
`ForeignKey`), plus a chapter-bound `SqlConstraint` umbrella
trait. The chapter-bound WHERE-clause trait `SqlCondition` MUST
be implemented by every chapter-bound DELETE / UPDATE / SELECT
builder of R7 and MUST expose:

- the chapter-bound equality accessor pair `and_where_eq` and
  `and_where_eq_param`;
- the chapter-bound `IN`-clause accessor `and_where_in_params`;
- a chapter-bound `or_*` variant set for every accessor above;
- a chapter-bound `IS NULL` discriminator routed through the
  chapter-bound optional bound-value type of R10.

**R9.** The chapter-bound select substrate MUST expose a chapter-
bound `SqlSubquery` accessor that lets a chapter-bound select
builder be embedded inside another query in places where the
chapter-bound dialect allows it. Cross-table joins are out of
scope under D3.

**R10.** The chapter-bound bound-value type set MUST be exactly:

| Bound type            | Bound role                                                                                                  |
| --------------------- | ----------------------------------------------------------------------------------------------------------- |
| `SqlValue`            | The chapter-bound single typed shape for *a bound value*.                                                  |
| `SqlValueOptional`    | The chapter-bound optional-bound-value shape consumed by the chapter-bound `IS NULL` discriminator of R8. |
| `FromQuoted`          | The chapter-bound trait by which statically-known literal values enter the chapter-bound `*_quoted` insert-builder accessor family. |

User-supplied data MUST be carried as bound parameters; the only
chapter-bound path by which a chapter-bound value enters the
rendered SQL string is the chapter-bound `*_quoted` accessor
family on the chapter-bound INSERT builder, which is reserved for
literals known statically.

The chapter-bound flat query-builder module set MUST be declared
as crate-root modules and collected behind a chapter-bound single
re-export module bound at the chapter-bound qualified path
`db_common::sql_build`. Consumers MUST import the typed query-
builder substrate via the chapter-bound re-export module
notwithstanding the absence of a chapter-bound on-disk directory
of that name.

## 25.6 Bound Asynchronous-Connection Facade

**R11.** The chapter-bound asynchronous-connection module MUST
expose a chapter-bound `AsyncConnection` handle: a chapter-bound
awaitable wrapper around a chapter-bound dedicated worker thread
that holds the chapter-bound synchronous binding-crate connection
handle. The chapter-bound transport between the chapter-bound
awaitable handle and the chapter-bound worker thread MUST be a
chapter-bound message channel.

**R12.** The chapter-bound message-channel substrate MUST carry
exactly two chapter-bound message kinds:

| Bound kind          | Bound payload                                                                                                                  |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Execute             | A chapter-bound closure routed to a chapter-bound boxed function-once trait object accepting the chapter-bound synchronous connection handle by mutable reference. |
| Close               | A chapter-bound one-shot reply channel carrying the chapter-bound binding-crate close-result.                                 |

**R13.** The chapter-bound constructor pair (the chapter-bound
file-path constructor `AsyncConnection::open(path)` and the
chapter-bound in-memory constructor
`AsyncConnection::open_in_memory()`) MUST spawn the chapter-bound
worker thread, hand it the chapter-bound synchronous binding-
crate connection handle, and await a chapter-bound *ready* signal
over a chapter-bound one-shot reply channel before returning.

The chapter-bound primary accessor `call` MUST package the caller-
supplied closure into the chapter-bound Execute message kind of
R12, allocate a chapter-bound one-shot reply channel, send the
message over the chapter-bound message channel, and await the
reply. The chapter-bound `call_unwrap` variant accessor is
permitted for paths where the caller is willing to panic on
chapter-bound internal failures.

**R14.** The chapter-bound `close` accessor MUST send the
chapter-bound Close message kind of R12; the chapter-bound worker
thread MUST perform the chapter-bound binding-crate close
operation, reply over the chapter-bound one-shot reply channel,
and exit its loop. Errors MUST be modelled by a chapter-bound
`AsyncConnError` enumeration with chapter-bound transport,
internal, and chapter-bound wrapped-binding-crate-error arms.

The substrate MUST NOT route the chapter-bound asynchronous-
connection work through the chapter-bound runtime's chapter-bound
blocking-pool accessor. The chapter-bound rationale is twofold:
the chapter-bound SQLite engine is a single-writer engine, so
serialising on a chapter-bound dedicated thread matches the
chapter-bound engine model; and routing through the chapter-bound
runtime's chapter-bound blocking pool would mix the chapter-bound
database input/output with unrelated chapter-bound blocking work
and complicate the chapter-bound shutdown discipline.

## 25.7 Bound Platform Gate

**R15.** Every chapter-bound module other than the chapter-bound
asynchronous-test module MUST carry the chapter-bound non-
WebAssembly target predicate as a chapter-bound conditional-
compilation gate. The chapter-bound re-export module of R10 MUST
carry the same gate. The chapter-26-bound parallel WebAssembly
persistence substrate is bound by chapter 26.

## 25.8 Bound Consumer-Routing Discipline

**R16.** Every chapter-bound native-side persistence consumer in
the workspace MUST consume the chapter-bound SQLite engine
through the chapter-bound crate `db_common`; bare chapter-bound
binding-crate connection construction in chapter-bound feature
code is the chapter-bound exception rather than the rule. The
chapter-bound consumer set known at the substrate landing point
includes:

- the chapter-bound central-context substrate of the chapter-
  bound application-context crate `mm2_core`, which owns both a
  chapter-bound constructible `SqliteConnShared` handle and a
  chapter-bound asynchronous-mutex-guarded `AsyncConnection`
  handle;
- the chapter-24-bound graphical-user-interface account-state
  substrate (chapter 24);
- the chapter-bound transaction-history coin-module substrate and
  the chapter-bound per-protocol history-store consumers;
- the chapter-bound Lightning-persister sub-crate (chapter-bound
  channel state and chapter-bound payments persistence);
- the chapter-22-bound WalletConnect session-store substrate
  (chapter 22);
- the chapter-19-bound non-fungible-token table substrate
  (chapter 19).

## 25.9 Bound Deprecated-Application-Programming-Interface Allowance

**R17.** The chapter-bound asynchronous-connection module MAY
carry a chapter-bound crate-wide deprecated-API allowance
attribute pinned to the chapter-bound binding-crate version
consumed by the substrate; removal of the chapter-bound allowance
attribute is deferred under D1 to the chapter-bound binding-
crate-version upgrade.

## 25.10 Tests

**T1.** *Asynchronous-facade end-to-end test set.* The chapter-
bound asynchronous-test module MUST cover the chapter-bound
asynchronous facade end-to-end against in-memory databases via
exactly the following test set:

| Bound test                                                              | Bound covered behaviour                                          |
| ----------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `open_in_memory_test`                                                   | Basic construction and the chapter-bound *ready* signal of R13. |
| `call_success_test`, `call_unwrap_success_test`                         | Closure execution and result propagation across the chapter-bound message channel. |
| `call_failure_test`                                                     | Error propagation across the chapter-bound message channel.     |
| `close_success_test`, `double_close_test`, `close_call_test`            | Close semantics including double-close and post-close calls.    |

**T2.** *Per-builder inline test discipline.* Every chapter-bound
query-builder module of R7 MUST carry chapter-bound inline test
cases that exercise the rendered SQL string and the round-trip
against an in-memory connection (the chapter-bound null-handling
case, the chapter-bound `or-replace` case, the chapter-bound
single-column-insert case, the chapter-bound delete-all case, et
cetera).

**T3.** *Per-consumer self-coverage discipline.* No chapter-bound
integration test in the chapter-bound application-entry crate
exercises the substrate as a whole; instead, every chapter-bound
consumer of R16 MUST cover its own usage of the substrate.

## 25.11 Deferred Work

**D1.** Upgrade of the chapter-bound third-party SQLite binding
crate to a chapter-bound newer version once the chapter-bound
minimum-supported-Rust-version permits it, with removal of the
chapter-bound deprecated-API allowance of R17.

**D2.** A chapter-bound streaming-fetch accessor on the chapter-
bound select builder of R7. The chapter-bound substrate at
landing materialises the entire result set; chapter-bound large-
result-set consumers that need to iterate row-by-row currently
drop down to raw binding-crate calls inside the chapter-bound
closure routed through R11.

**D3.** Non-trivial chapter-bound multi-table join support on the
chapter-bound select builder of R7. The chapter-bound substrate
at landing covers single-table operations and the chapter-bound
common select shapes only; chapter-bound multi-table queries
today are written as raw SQL inside the chapter-bound closure
routed through R11.

**D4.** A chapter-bound multi-handle pool substrate. All chapter-
bound operations serialise on a chapter-bound single worker thread
per R11; this matches the chapter-bound SQLite single-writer
engine model on write-heavy paths, but it means read concurrency
on a chapter-bound single asynchronous-connection handle is zero.
Multi-handle pools are deferred.

**D5.** Move of the chapter-bound `StringError` substrate out of
this crate and into the chapter-bound error-handling-framework
crate `mm2_err_handle` (chapter 04's common-errors module) so it
can be shared more broadly.

**D6.** A chapter-bound shared *translate foreign-key violation
to no-such-row* helper. The chapter-bound substrate at landing
exposes the chapter-bound constraint-error inspector
`is_constraint_error` of R3 against the chapter-bound binding-
crate error variants, but the higher-level *no-such-row* mapping
is reimplemented by every chapter-bound consumer of R16.

## 25.12 Baseline Verifications

**V1.** The chapter-02-anchored baseline `db_common` crate MUST
be confirmed to contain exactly the three chapter-bound source
files (the manifest, the crate-root module, the chapter-bound
low-level synchronous SQLite-helper module) of R2; every
chapter-bound module added by R1 beyond those three MUST be
confirmed absent at the chapter-02-anchored baseline.

**V2.** No chapter-bound asynchronous-connection facade is
present at the chapter-02-anchored baseline.

**V3.** No chapter-bound typed query-builder substrate is present
at the chapter-02-anchored baseline; the chapter-bound re-export
module of R10 MUST be confirmed absent.

## 25.13 External References

- The chapter-bound SQLite engine documentation, in particular
  the chapter-bound write-ahead-log journal-mode page and the
  chapter-bound pragmas page covering the chapter-bound four-
  pragma set of R5.
- The chapter-bound documentation pages of the chapter-bound
  third-party synchronous SQLite binding crate the substrate
  composes on top of, the chapter-bound third-party query-string-
  assembly crate the chapter-bound typed query-builder layer
  composes on top of, and the chapter-bound third-party
  asynchronous-message-channel crate the chapter-bound
  message-channel substrate of R12 consumes.

## 25.14 Provenance Footer

- *Inputs:* the baseline workspace at the pinned baseline-revision
  commit of chapter 02; chapter 02 (the chapter-02 R4 workspace-
  member registry containing the chapter-bound `db_common` crate
  at the chapter-02-anchored three-file shape of R2); chapter 19
  (the non-fungible-token consumer of R16); chapter 22 (the
  WalletConnect consumer of R16); chapter 24 (the graphical-user-
  interface account-state consumer of R16); chapter 26 (the
  parallel WebAssembly persistence substrate this chapter's
  platform-gate R15 hands off to); chapter 31 (the central
  application-context substrate the synchronous SQLite connection
  handle is pinned on as the `sqlite_connection` once-set field of
  chapter 31 R6, and the asynchronous SQLite connection handle is
  pinned on as the `async_sqlite_connection` `OnceLock`-wrapped
  field of chapter 31 R9); chapter 04 (the error-handling
  framework D5 routes the chapter-bound `StringError` substrate
  into); the chapter-bound public SQLite engine documentation,
  the chapter-bound third-party binding-crate documentation, the
  chapter-bound third-party query-string-assembly crate
  documentation, the chapter-bound third-party asynchronous-
  message-channel crate documentation.
- *Permitted-input classes used:* the baseline itself (chapter 01
  R1); external public specifications (chapter 01 R3, for the
  SQLite engine documentation citation); sibling open-source
  repositories under compatible licenses (chapter 01 R5, for the
  third-party binding-crate, query-string-assembly crate, and
  asynchronous-message-channel crate citations).
- *Sibling-allowlist consultations:* the chapter-bound third-party
  synchronous SQLite binding crate; the chapter-bound third-party
  query-string-assembly crate; the chapter-bound third-party
  asynchronous-message-channel crate.
- *Forbidden corpus:* not consulted.
