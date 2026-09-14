# ARRR ZIP-212 receipt recovery

Cheetah recognizes both Sapling note plaintext versions used by Pirate. This
fix covers confirmed wallet scanning, pending receipts, full-transaction note
decryption, and outgoing-viewing-key recovery for fee validation. It preserves
the configured upgrade heights, transaction branch IDs, and sending behavior.

On the first activation after this update, an existing affected wallet rescans
from its original wallet birthday through the current target. Activation can
take longer while it reports the existing cache and wallet scanning progress.
Keep the existing sync settings to recover the previously covered range;
explicitly changing the start date or height still requests the existing
re-anchor behavior. Recovery also applies when `skip_sync_params` requests reuse
of the prior scan state.

The recovery reuses validated compact blocks and updates the current wallet
database in place. It retains known notes, spend links, transaction history IDs,
and the original birthday. Before recording recovered notes it reconstructs the
previous scanned tip and checks its Sapling root, hash, height, and tree size
against stored state. A failed or interrupted pass remains retryable. A
successful subsequent activation resumes normally without repeating recovery.

Do not delete wallet databases or change Canopy activation to recover a missing
deposit. The update leaves legacy `<TICKER>_WALLET.db` and
`<TICKER>_COMPACT_BLOCKS.db` files untouched. Deposits before the original sync
start still require the existing explicit earlier-date/height scan workflow.

## Configuration scope

The receive override requires every field in the deployed ARRR consensus tuple:

| Field | Value |
| --- | --- |
| Overwinter and Sapling activation heights | Both `152855` |
| `coin_type` | `133` |
| Sapling spending-key HRP | `secret-extended-key-main` |
| Sapling viewing-key HRP | `zxviews` |
| Sapling address HRP | `zs` |
| Transparent public-key prefix | `[28, 184]` |
| Transparent script prefix | `[28, 189]` |

Later upgrade heights do not identify the coin. The separate shielded derivation
path `m/32'/141'` stays unchanged. Other parameter sets keep their current
height-based ZIP-212 policy. See [CRD §39.9](reloaded-rewrite/39-zcash---z_coin-shielded-coin.md#399-pirate-zip-212-receive-compatibility-and-historical-recovery)
for the pinned public protocol and registry references.

## Durable completion record

The current Pirate wallet database uses SQLite `application_id = 0x41525232`
(ASCII `ARR2`) after a successful scan through the target. Zero means recovery
has not completed. Other nonzero values cause activation to fail while
preserving the database. The marker creates no schema objects and changes
neither `user_version` nor the selected wallet-schema fingerprint. It must not
be edited manually: setting it early could skip recovery of missed receipts.

Use a separate backup copy if you need to run an older Cheetah binary. Older
binaries do not understand this marker and could scan additional blocks under
the restrictive policy while leaving it set. Automatic recovery after that
downgrade/re-upgrade sequence is not guaranteed. Keep the corrected database
for use with this version or later.

Offline regression fixtures cover both plaintext versions, strict non-Pirate
boundaries, cryptographic rejection, confirmed balance/history, interrupted
historical recovery, and reopen persistence. Synthetic full transactions in
these tests contain placeholder proof/signature bytes and are never broadcast;
the tests establish note-decryption behavior, not live transaction acceptance.
