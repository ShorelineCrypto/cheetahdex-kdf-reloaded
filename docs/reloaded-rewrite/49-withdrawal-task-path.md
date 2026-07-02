# 49. Withdrawal task path

## 49.1 Scope

This chapter covers the withdrawal JSON-RPC surface for the direct `withdraw`
method and the task family under `task::withdraw::*`.

The direct `withdraw` method returns a completed transaction payload when the
operation completes in the request/response round trip. The task family uses the
standard mmrpc 2.0 task pattern: `init` returns a `task_id`, `status` returns a
task-status object, `user_action` supplies hardware-wallet input when requested,
and `cancel` aborts an in-flight task.

## 49.2 Init request and sender selection

R49.1. `task::withdraw::init` shall accept the canonical withdrawal request
object used by the direct `withdraw` method. The common request fields are:

- `coin` (string, required) -- source coin ticker.
- `to` (string, required) -- destination address.
- `from` (object, optional) -- wallet-family sender selector; see R49.2.
- `amount` -- requested withdrawal amount when `max` is false.
- `max` (boolean, optional, default false) -- request a full spendable-balance
  withdrawal.
- `fee` (object, optional) -- coin-family fee descriptor.

Coin families may bind additional public fields already accepted by the shared
withdrawal request, such as memo, broadcast, protocol-channel, or expiry
controls. The task endpoint shall not add a second withdrawal-specific request
shape.

R49.2. When `from` is present, it shall be an HD-address selector in exactly one
of these public JSON wire forms:

- Address-id form:

```json
{
  "account_id": 0,
  "chain": "External",
  "address_id": 0
}
```

- Derivation-path form:

```json
{
  "derivation_path": "m/44'/0'/0'/0/0"
}
```

The address-id form uses `account_id` and `address_id` as unsigned integers.
The `chain` value shall be the public BIP-44 chain discriminant `External` or
`Internal`. The derivation-path form shall identify a full standard HD address
path for the activated coin. A selector with a mismatched coin type, mismatched
coin path, unknown account, unsupported chain, or address that is not activated
for withdrawal shall fail the withdrawal with a structured withdrawal error.

R49.3. For Iguana or other legacy single-key wallet activations, `from` shall be
omitted. If a single-key wallet receives an explicit `from` selector, the
withdrawal shall fail with a structured withdrawal error whose public
discriminant identifies an unexpected sender selector. The server shall not
silently ignore an explicit `from` on a single-key wallet.

R49.4. For HD UTXO-family task withdrawals, `from` is required. If an HD UTXO
task withdrawal omits `from`, the task shall fail with a structured withdrawal
error whose public discriminant identifies that no sender address was supplied.
If `from` is present, the implementation shall resolve it to the selected
activated HD address and use that address as the only withdrawal sender.

R49.5. EVM-family native-coin and fungible-token task withdrawals shall be
supported. For EVM-family software-HD withdrawals, both direct `withdraw` and
`task::withdraw::init` shall use the activated software key/address for balance
checks, nonce selection, signing, and the completed transaction's sender when
`from` is omitted. When `from` is present, both paths shall resolve it as an HD
address selector and use the selected activated HD address and derived key
instead. For EVM-family hardware-wallet task withdrawals, omitting `from` shall
use the enabled HD address already associated with the activated wallet; when
`from` is present, the task shall use the selected HD address and derivation
path for the hardware-signing flow. Unless activation has selected a different
enabled address, the default enabled HD address is the address-id tuple
`account_id: 0`, `chain: External`, `address_id: 0`.

R49.6. `task::withdraw::init` shall return only the standard task-init response:

- `task_id` (integer) -- identifier for subsequent `status`, `user_action`, and
  `cancel` calls.

No transaction payload shall be returned by `init`.

T49.1. Start a single-key withdrawal task without `from`. The init call shall
return `task_id`, and the task shall proceed to a status result other than a
sender-selector validation failure.

T49.2. Start a single-key withdrawal task with either accepted `from` wire form.
The task shall reject the request with a structured withdrawal error indicating
an unexpected sender selector.

T49.3. Start an HD UTXO-family withdrawal task with an address-id selector for
an activated external address. The task shall use that address as the sender.

T49.4. Start an HD UTXO-family withdrawal task with a derivation-path selector
that resolves to the same activated address as T49.3. The task shall use the
same sender address.

T49.5. Start an HD UTXO-family withdrawal task without `from`. The task shall
reach a terminal error status whose public withdrawal-error discriminant
identifies that the sender address was not supplied.

T49.6. Start an HD withdrawal task with a selector for an unknown account, a
mismatched coin path, or an address that has not been activated. The task shall
reach a terminal error status whose public withdrawal-error discriminant
identifies an invalid or unexpected sender selector.

T49.7. Start EVM-family software-HD withdrawals without `from` through both the
direct `withdraw` method and `task::withdraw::init`. Each withdrawal shall use
the activated software key/address as the sender. Repeat the task withdrawal
with a valid `from` selector for a different activated HD address; the task
shall use the selected address.

## 49.3 Status request and retention

R49.7. `task::withdraw::status` shall accept:

- `task_id` (integer, required) -- task identifier returned by R49.6.
- `forget_if_finished` (boolean, optional, default true) -- whether a terminal
  task shall be removed after this status read.

R49.8. When `forget_if_finished` is omitted, it shall behave as `true`. If the
requested task is terminal and `forget_if_finished` is true, the status call
shall return the terminal status once and remove the task from the task
registry. A later status call for the same `task_id` shall fail as an unknown
task.

R49.9. If `forget_if_finished` is false, a terminal status read shall not remove
the task. Repeated status calls for the same `task_id` shall continue to return
the same terminal task-status object until the task is forgotten by another
terminal read with `forget_if_finished` true or by implementation-defined
registry cleanup outside the compatibility contract.

R49.10. A status call for a task that is cancelling or no longer registered
shall fail as an unknown task. Cancellation is not represented as a public
withdraw task-status variant.

T49.8. Poll an in-progress withdrawal without `forget_if_finished`. The request
shall be accepted, and omission shall be equivalent to `forget_if_finished:
true` for any later terminal read.

T49.9. Poll a successful terminal withdrawal without `forget_if_finished`. The
first call shall return the terminal success status, and the next call for the
same `task_id` shall fail as an unknown task.

T49.10. Poll a terminal withdrawal with `forget_if_finished: false` twice. Both
calls shall return the same terminal status and shall not forget the task.

T49.11. Poll a terminal withdrawal with `forget_if_finished: false`, then poll
with `forget_if_finished: true`, then poll again. The second poll shall return
the terminal status and forget the task; the third poll shall fail as an
unknown task.

## 49.4 Status response wire shape

R49.11. `task::withdraw::status` shall return the standard mmrpc 2.0 response
envelope. For any registered task status, including terminal task failure, the
RPC call itself is successful and the mmrpc response contains a top-level
`result` member. Transport, decode, authorization, or unknown-task failures are
RPC errors and are not task-status objects.

R49.12. The `result` member of a successful status-RPC call shall be a
task-status object serialized with:

- `status` -- one of `InProgress`, `UserActionRequired`, `Ok`, or `Error`.
- `details` -- the payload for that status.

R49.13. `status: "InProgress"` shall carry a withdrawal progress value in
`details`. Clients shall treat the value as a progress indicator only and shall
continue polling.

R49.14. `status: "UserActionRequired"` shall carry a hardware-wallet action
request in `details`. Clients shall present or collect the requested action and
submit it through `task::withdraw::user_action` (R49.18).

R49.15. `status: "Ok"` shall carry the completed transaction-details object
directly in `details`. Clients shall not expect an additional `result` field
inside `details`.

R49.16. `status: "Error"` shall carry the serialized structured withdrawal
error directly in `details`. The error payload shall include the public
`error_type` discriminator and any bound `error_data`; compatibility metadata
such as human-readable error text or trace fields may also be present. Clients
shall classify terminal task failure from `result.status == "Error"` and
`result.details.error_type`, not from the mmrpc top-level error envelope.

T49.12. Poll an in-progress withdrawal task. The mmrpc response shall contain a
top-level `result` object whose `status` is `InProgress` and whose `details`
contains a progress value.

T49.13. Complete a withdrawal successfully and poll its status. The mmrpc
response shall contain a top-level `result` object whose `status` is `Ok` and
whose `details` is the transaction-details object, including `tx_hex`,
`tx_hash`, `from`, `to`, `total_amount`, `fee_details`, `coin`, `internal_id`,
and `transaction_type`.

T49.14. Force a withdrawal task to fail after init, for example by using an
invalid sender selector on an HD wallet. The mmrpc response for the status call
shall still contain top-level `result`; inside that result, `status` shall be
`Error` and `details.error_type` shall identify the withdrawal error.

T49.15. Poll a missing or already-forgotten task. The response shall be an
mmrpc error response, not a successful task-status object.

## 49.5 User action and cancellation

R49.17. `task::withdraw::user_action` shall accept:

- `task_id` (integer, required) -- task identifier returned by R49.6.
- `user_action` (object, required) -- hardware-wallet action payload for the
  action currently requested by R49.14.

R49.18. `user_action` shall be meaningful only while the task is in the
`UserActionRequired` state. Submitting a user action for a task that is not
waiting for that action shall fail with a structured task-action error. Pure
software-signing withdrawals shall not require a `user_action` round trip.

R49.19. `task::withdraw::cancel` shall accept `task_id` and attempt to abort an
in-flight withdrawal task. Cancelling a task that has already reached a
terminal state shall fail with the standard cancel-task terminal-state error.

T49.16. Drive a hardware-wallet withdrawal to `UserActionRequired`, submit the
matching `user_action`, and continue polling. The task shall leave the
awaiting-user-action state and either progress or terminate.

T49.17. Submit `user_action` for a software-signing withdrawal task that is not
awaiting user input. The call shall fail with a structured task-action error.

T49.18. Cancel an in-progress withdrawal task, then poll its status. The task
shall not later return a public cancellation status; a status poll shall fail
as an unknown or unavailable task according to R49.10.

## 49.6 Completed transaction payload

R49.20. A successful direct `withdraw` response and a successful
`task::withdraw::status` terminal payload shall use the same transaction-details
object. A completed withdrawal shall include at least:

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

`kmd_rewards` may also be present when the coin family supports it. Consumers
shall treat `tx_hex` and `tx_hash` as mandatory for a completed withdrawal
transaction and shall use the remaining metadata for history, balance, and
confirmation display.

T49.19. For the same supported coin family and signing policy, compare a
successful direct `withdraw` result with the `details` payload of a successful
task withdrawal. Both shall expose the same transaction-details field contract.
