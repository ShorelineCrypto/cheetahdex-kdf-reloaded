# Cutting a Release

This is the operational runbook for producing an official, signed KDF Reloaded
release. It complements [`RELEASE_CHECKLIST.md`](../RELEASE_CHECKLIST.md) (the
gate that must be fully ticked) — this file describes the *mechanics*.

## Overview

Releases are **tag-driven**. Pushing an annotated tag matching `v*` to `main`
triggers [`.github/workflows/release.yml`](../.github/workflows/release.yml),
which:

1. builds the Linux x86-64 binary inside a pinned **Debian 11** container
   (glibc 2.31 floor → runs on Debian 11/12, Ubuntu 20.04+, RHEL 9, …);
2. builds macOS (x86_64 / aarch64 / universal) and Windows binaries via the
   existing per-platform workflows;
3. writes a `SHA256SUMS` manifest over all binaries;
4. **GPG-signs** `SHA256SUMS` with the maintainer key
   (`FEE1ACA52C65FF3EBF31818CB5595E1752BC2A82`) inside the protected `release`
   environment, producing `SHA256SUMS.asc`;
5. drafts a **GitHub Release** with every binary + `SHA256SUMS` + `SHA256SUMS.asc`
   attached.

Both `release.yml` and the unsigned dev-snapshot workflow
[`dev-build.yml`](../.github/workflows/dev-build.yml) watch `v*` tags. Because
GitHub tag triggers cannot be scoped to a branch, each workflow begins with a
`gate` job that inspects which branch the tagged commit lives on:

- tag commit on **`main`** → `release.yml` proceeds (signed, published draft);
  `dev-build.yml` self-skips.
- tag commit on **`dev` / `staging`** (and not `main`) → `dev-build.yml`
  proceeds, producing unsigned per-platform CI artifacts (not signed, not
  published as a GitHub Release); `release.yml` self-skips.

`dev-build.yml` can also be run manually via **workflow_dispatch**. Its Linux job
reuses the same [`build-linux.yml`](../.github/workflows/build-linux.yml) Debian
11 build as the release, so dev snapshots carry the same glibc 2.31 floor and
stay backwards-compatible.

Signing happens **only** in `release.yml`.

## Steps for the maintainer

1. Complete every box in [`RELEASE_CHECKLIST.md`](../RELEASE_CHECKLIST.md).
2. Bump the workspace version(s) and update `CHANGELOG.md`; merge to `main`.
3. Create and push an annotated, GPG-signed tag from `main`:
   ```sh
   git checkout main && git pull
   git tag -s v0.1.0-alpha.1 -m "v0.1.0-alpha.1"
   git push origin v0.1.0-alpha.1
   ```
4. Watch the **Release** workflow. When it finishes, a **draft** release exists.
5. Review the drafted notes and the attached assets, then **publish** the
   release in the GitHub UI.

## CI configuration

- **`release` environment** holds `GPG_PRIVATE_KEY` (the maintainer signing
  key, no passphrase). It is the only place the key is exposed; restrict the
  environment to protected `main` + tag refs. See the "GPG key → CI secret"
  note below.
- **Optional DockerHub image**: set repository variable `PUBLISH_DOCKERHUB=true`
  and provide `DOCKERHUB_USERNAME` / `DOCKERHUB_TOKEN` secrets and a
  `DOCKERHUB_REPO` variable; the `docker` job then builds
  [`Dockerfile.release`](../Dockerfile.release) (Ubuntu 24.04 runtime) from the
  Linux binary and pushes `:<tag>` and `:latest`.

### Rotating / installing the signing key

Do this on a trusted workstation — never paste the private key anywhere but the
GitHub Secrets store:

```sh
# Export a dedicated signing subkey (preferred over the primary key):
gpg --export-secret-subkeys --armor <FPR> | base64 -w0 > kdf-sign.b64
gh secret set GPG_PRIVATE_KEY --env release < kdf-sign.b64
shred -u kdf-sign.b64
```

The import step in `release.yml` accepts the secret either base64-encoded or as
a raw armored key block.

## Verifying a release (for users)

```sh
# Import the maintainer public key (shipped in-repo):
gpg --import docs/keys/takologi.asc

# Verify the signed checksum manifest, then the binary:
gpg --verify SHA256SUMS.asc SHA256SUMS
sha256sum --ignore-missing -c SHA256SUMS
```

A good signature from `FEE1ACA52C65FF3EBF31818CB5595E1752BC2A82` plus a matching
checksum authenticates the download.
