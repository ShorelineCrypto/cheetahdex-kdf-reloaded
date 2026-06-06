# Security Policy

## Supported versions

KDF Reloaded is in public alpha. Only the latest tagged alpha release receives security fixes.

| Version          | Supported |
|------------------|-----------|
| `0.1.0-alpha.x`  | ✅        |
| Anything older   | ❌        |

## Reporting a vulnerability

If you believe you have found a security vulnerability in KDF Reloaded — particularly anything affecting swap atomicity, key handling, networking, or RPC authorisation — **please do not open a public issue**.

> A dedicated vulnerability mailbox and PGP key may be added in a future release cycle. Until then, contact maintainers via the channels listed in [`CONTRIBUTING.md`](CONTRIBUTING.md) and explicitly mark the message as a security report.

When reporting, please include:

- A clear description of the issue and its impact.
- Steps to reproduce, ideally with a minimal test case.
- Affected commit hash or release tag.
- Any proposed mitigation.

We aim to acknowledge reports within 5 business days and to provide a remediation timeline within 14 days.

## Disclosure policy

We follow coordinated disclosure. Once a fix is available we will:

1. Publish a patched release.
2. Issue a security advisory in this repository.
3. Credit the reporter (unless they request otherwise).

## Release artifact verification

> GPG and/or minisign signatures on release binaries are planned for the alpha cycle. Signing key fingerprints and a verification procedure will be published alongside signed artifacts.
>
> Until then, the only authoritative source of KDF Reloaded code is this repository. Do not trust binaries received through any other channel.

DEX fee receiver addresses are inherited from the upstream Komodo DeFi Framework configuration by design and are not under the control of the KDF Reloaded maintainers.

## Pre-release gating

Release readiness is controlled by the repository checklist and release-governance process.
See [`RELEASE_CHECKLIST.md`](RELEASE_CHECKLIST.md) for the full pre-release gating list.

## Out of scope

- Issues affecting the upstream Komodo DeFi Framework that are not present in KDF Reloaded — please report those upstream.
- Vulnerabilities in third-party coin protocols themselves (rather than this software's handling of them).
- Denial-of-service from a malicious peer that can already drop your traffic at the network layer.

## Hardening notes for operators

- Run `mm2` as a non-root user. Restrict access to the RPC port (`7783` by default).
- Use a strong, unique `rpc_password`.
- Treat `MM2.json` as secret material — it contains your mnemonic.
- Take regular backups of your seed phrase via a secure offline channel.
