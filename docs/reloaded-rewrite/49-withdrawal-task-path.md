# 49. Withdrawal task path

## 49.1 Scope

This chapter covers the withdrawal JSON-RPC surface for both the direct `withdraw` method and the task family under `task::withdraw::*`.

The task family uses the standard mmrpc 2.0 envelope. The direct `withdraw` method returns the completed transaction payload directly, while the task family returns an init response plus a status wrapper until the task reaches a terminal result.

## 49.2 Init request shape

`task::withdraw::init` accepts the canonical withdrawal request object with these fields:

- `coin` — required string identifying the source coin.
- `to` — required destination address string.
- `from` — optional sender selector.
- `amount` — requested withdrawal amount.
- `max` — boolean flag selecting a full-balance withdrawal.
- `fee` — optional coin-specific fee descriptor.

The request is shared by wallet families, but the sender rules differ by wallet mode:

- **Iguana / legacy single-key wallets**: `from` must be omitted.
- **HD wallets**: `from` may be omitted. When omitted, the wallet chooses its default spendable sender for the withdrawal.

For HD wallets, the `from` selector can be expressed in either of these public wire forms:

- address-id form: an object with `account_id`, `chain`, and `address_id`
- derivation-path form: an object with `derivation_path`

If `max` is true, the request means “withdraw the maximum spendable amount.”

## 49.3 Init response shape

`task::withdraw::init` returns the standard task init response:

- `task_id`

No transaction payload is returned at init time.

## 49.4 Status request shape

`task::withdraw::status` uses the standard task-status request:

- `task_id`
- `forget_if_finished` with a default of `true`

## 49.5 Status response semantics

`task::withdraw::status` reports one of the following outer states:

- `InProgress` — the task is still running.
- `UserActionRequired` — the task is waiting for a hardware-wallet action.
- `Ok` — the task has reached a terminal state.

The outer `Ok` state is used for both terminal success and terminal failure. Callers must inspect the nested `details` payload:

- success is reported as a nested `result`
- failure is reported as a nested `error`

So a terminal task does **not** need a non-`Ok` outer status to represent failure.

## 49.6 User-action request shape

`task::withdraw::user_action` uses the standard task user-action request:

- `task_id`
- `user_action`

For withdraw flows, user action is relevant only when a hardware wallet is waiting for user input. Pure software-signing withdraws do not require a user-action round trip.

## 49.7 User-action status semantics

When a withdraw task needs hardware-wallet input, the awaiting status tells the client what to do next. The public withdraw flow currently uses the Trezor PIN prompt as the user-action class that must be handled.

## 49.8 Completed transaction payload

The completed withdraw result is the transaction-details object used throughout the wallet layer. A successful result includes at least these public fields:

- `tx_hex`
- `tx_hash`
- `from`
- `to`
- `total_amount`
- `spent_by_me`
- `received_by_me`
- `my_balance_change`
- `block_height`
- `timestamp`
- `fee_details`
- `coin`
- `internal_id`
- `transaction_type`

`kmd_rewards` may also be present when the coin family supports it.

Consumers should treat `tx_hex` and `tx_hash` as mandatory for a completed withdraw transaction, and they should use the rest of the metadata for history, balance, and confirmation display.

## 49.9 Wallet-family behavior

The withdrawal result payload does not change between Iguana and HD wallets. The difference is only how the sender is selected before signing:

- Iguana wallets require an implicit legacy sender and reject an explicit `from`.
- HD wallets can accept a sender selector or infer the default sender when `from` is omitted.

## 49.10 Compatibility note

Legacy clients should not treat the task wrapper as a bare transaction result. The outer task status is a wrapper; the actual withdraw transaction or error lives inside the nested `details` payload.
