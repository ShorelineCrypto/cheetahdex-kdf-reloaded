# KDF Reloaded — Licensing Policy

This document states the licensing posture of the KDF Reloaded repository. It
is a policy statement; the operative license texts are [`LICENSE`](LICENSE),
[`COPYING`](COPYING), and [`THIRDPARTY-LICENSES`](THIRDPARTY-LICENSES).
Repository-level provenance is tracked in
[`../docs/reloaded-rewrite/34-provenance-ledger.md`](../docs/reloaded-rewrite/34-provenance-ledger.md).

## 1. The combined work is distributed under GPL-2.0-only

The repository as a whole is distributed under the **GNU General Public License,
version 2 (GPL-2.0-only)**.

KDF Reloaded is a *continuation* anchored to the last upstream Komodo DeFi
Framework commit that was unambiguously distributed under GPL-2.0-only
(`c1d46c0c1592faa0860f704008b2b2381bc3840f`, 2022-06-03). That pre-anchor base
is copyrighted by its original authors (The SuperNET Developers, Atomic Private
Limited and its contributors, and others). **We are not a copyright holder of
the pre-anchor base and have neither the right nor the consent to relicense
it.** The base is and remains GPL-2.0-only, so the combined/aggregate work we
distribute is GPL-2.0-only. No "or later" option can be exercised over the work
as a whole, because the base pins version 2.

## 2. Our original post-anchor code is offered under GPL-2.0-or-later

Original files authored by the KDF Reloaded Authors **after** the anchor commit
are offered by their authors under **GPL-2.0-or-later**.

This is a grant we are entitled to make, because copyright in those original
works belongs to us. It is fully compatible with §1: combining a
GPL-2.0-or-later file with the GPL-2.0-only base yields a GPL-2.0-only
combination (the "or later" simply isn't reachable while the v2-only base is
present). The grant matters when our original files are considered on their own
or extracted: downstream then has the option to use them under GPL-2.0 *or any
later GPL version*. It also keeps the door open for a future maintainer to move
the project to a later GPL version **if and only if** every remaining
GPL-2.0-only-pinned component has by then been removed or independently
rewritten — without needing to re-contact us.

### How a file qualifies as "post-anchor original"

A file is covered by the GPL-2.0-or-later grant in §2 if **both** hold:

1. It was first added to the tree after the anchor commit
   `c1d46c0c1592faa0860f704008b2b2381bc3840f` (Git history is the record), and
2. It is **not** listed as a vendored or adapted third-party component in
   [`THIRDPARTY-LICENSES`](THIRDPARTY-LICENSES) or the provenance ledger.

Per-file `SPDX-License-Identifier: GPL-2.0-or-later` headers may be added to
such files over time; the absence of a header does **not** withdraw the grant
for a qualifying file. Files carrying their own SPDX/header notice are governed
by that notice.

## 3. Third-party components keep their own licenses

Vendored or adapted third-party code retains its upstream license, as recorded
in [`THIRDPARTY-LICENSES`](THIRDPARTY-LICENSES) and the provenance ledger. These
are unaffected by §2. Current examples:

- `mm2src/db_common/src/async_sql_conn.rs` / `async_conn_tests.rs` — adapted
  from `tokio-rusqlite` (MIT).
- `mm2src/coins/eth/legacy_tx.rs` — derivative of Parity's `ethcore-transaction`
  (GPL-3.0).
- WalletConnect client dependencies — Apache-2.0.

## 4. Known open license items (tracked, deferred)

Some license tensions are **inherited from the upstream codebase**, predate this
project's clean-room work, and are equally present in other downstream forks
(including GLEEC). They are not introduced by KDF Reloaded. They are documented
here for transparency and tracked as deferred items in the provenance ledger:

- **`legacy_tx.rs` (GPL-3.0) inside a GPL-2.0-only work.** GPL-3.0 and
  GPL-2.0-only are not mutually compatible at the whole-work level. The file is
  vendored from upstream and honestly attributed in its header. Resolution
  options (future): independent clean-room reimplementation from the public
  EIP-155 / RLP specification, or removal. Tracked, not a v1 blocker beyond
  disclosure.
- **Apache-2.0 WalletConnect dependencies.** Apache-2.0 is one-way compatible
  with GPLv3 but not with GPLv2. This affects the optional WalletConnect
  feature surface and is inherited from upstream WalletConnect support.

These items are the shared responsibility of the upstream lineage; they are
called out rather than hidden.

## 5. Inbound = outbound for new contributions

New **original** contributions are accepted under **GPL-2.0-or-later** (so the
§2 posture is preserved as the project grows). Contributions that import or
adapt third-party code must keep that code under its own license and record it
in the provenance ledger and `THIRDPARTY-LICENSES`. See
[`../CONTRIBUTING.md`](../CONTRIBUTING.md) and
[`DEVELOPER-AGREEMENT`](DEVELOPER-AGREEMENT).
