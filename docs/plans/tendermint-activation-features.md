# Plan: Tendermint activation features

**Status:** parked (deferred by maintainer decision 2026-10-11; nothing started) — **Last reviewed:** 2026-10-18

> **Context:** created 2026-10-11 on
> `fix/activation-api-parity`. That branch brings Tendermint activation to wire
> parity (error names, `error_data` shapes, HTTP statuses, error placement and
> failure mapping, CRD ch. 36). The features below are what the reference
> implementation (GLEEC KDF `v3.0.0-beta`) supports and Reloaded does not; they
> were deferred by maintainer decision on 2026-10-11 and belong to a separate
> branch.

## Interim behavior on `fix/activation-api-parity`

Reloaded used to accept any `activation_params` and ignore it, so a request for
a public-key (Ledger / watch-only) or WalletConnect activation silently
activated with the local context key. A user who asked for a device-backed
activation got a hot-key one without knowing. Until the features below land,
Reloaded rejects a request that carries `activation_params` with
`PlatformCoinCreationError` (`{ticker, error}`, HTTP 500) -- the discriminant the
reference uses for an unsupported signing context -- instead of activating with
the local key. This is a deliberate, temporary divergence recorded in ch. 36
§36.7; the reference accepts these requests.

## Features to implement

References are to CRD ch. 36 (Dirty-Gate-passed).

1. **`activation_params` signing policies** (R36.1.3):
   - *Public-key form* `{"type": "WithPubkey", "params": {"pubkey": {"type":
     "secp256k1" | "ed25519", "value": [<bytes>]}, "is_ledger_connection":
     <bool>}}`: watch-only / Ledger-flagged activation from a caller-supplied
     key.
   - *WalletConnect form* `{"type": "WalletConnect", "params":
     {"session_topic": <string>}}` (ch. 22 for the WalletConnect side).
   - *Public-docs WalletConnect form* `{"priv_key_policy": {"type":
     "WalletConnect", "data": <topic>}}`, treated exactly as the WalletConnect
     form. The public docs show this form; the reference rejects it. The
     maintainer decided to accept both forms.
   - Malformed input is the dispatcher's `InvalidRequest`, never a crash (the
     reference panics on some malformed keys; Reloaded must not).
   - When this lands, drop the interim rejection above.
2. **HD-wallet mode**: honor `path_to_address` under the coin's derivation path
   (R36.1.2, R36.1.4 step 3). The ch. 36 code-quality finding applies: derive
   the public key from the activated private key, not the wallet's root public
   key, and verify with an HD signing test.
3. **Custom tokens**: the `protocol` member of `tokens_params[]` and of
   `enable_tendermint_token`, with `CustomTokenError`
   (`{"DuplicateTickerInConfig": {"ticker_in_config": ...}}`).
   A custom token must be wallet-only, as for EVM (ch. 35 R35.2.7); the EVM
   implementation gap is tracked in kdf-reloaded/kdf#16.
4. **`tx_history: true`** starts background transaction-history fetching
   (Reloaded accepts the flag and does nothing).
5. **Hardware `priv_key_policy`.** The reference ignores `priv_key_policy` for
   Tendermint, so `"Trezor"` silently activates with the context key (ch. 36
   code-quality finding). Ch. 36 currently binds the reference behavior. Decide
   whether Reloaded refuses it with `UnexpectedDeviceActivationPolicy` as a
   deliberate deviation; the same safety argument as the interim rejection of
   `activation_params` applies.
6. **Task-managed single token family** `task::enable_tendermint_token::*`
   (D36.2), if wanted.

## Order

1 (it removes the interim rejection), then 2, 3, 4. Items 5 and 6 need a
maintainer decision first.
