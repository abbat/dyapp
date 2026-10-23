# 0020. dyappd release channels: GitHub Releases and GHCR with build provenance

- **Status:** Accepted
- **Date:** 2026-10-23

## Context

The first release ships installable `dyappd` artifacts. `make deb` already builds a Debian 12
package and a tarball with one release binary and test-installs both on a clean Debian 12;
`make image` builds and tests a container image ([operator manual](../manuals/bootstrap.md#getting-dyappd)).
Nothing is published. Operators need to download these files and check that they come from this
repository's CI ([supply chain](../security/threat-model.md#supply-chain)).

## Decision

A tag `vX.Y.Z` on a commit whose workspace version is `X.Y.Z` runs
[`release.yml`](../../.github/workflows/release.yml). It builds with `make deb image`, from the
Dockerfiles alone with no layer cache, and publishes:

1. to GitHub Releases: `dyappd_X.Y.Z_amd64.deb`, `dyappd-X.Y.Z-x86_64-linux-gnu.tar.gz`, an SPDX
   SBOM `dyappd-X.Y.Z.spdx.json` and `SHA256SUMS` over them;
2. the image to `ghcr.io/abbat/dyappd:X.Y.Z`.

Every file and the image digest get a Sigstore-signed SLSA build provenance attestation
(`actions/attest-build-provenance`), checked with `gh attestation verify`. One tag covers the
whole repository: the client artifacts of the same release join it later. x86_64 only for now.

Rejected for the first release:

- Docker Hub: an extra account and a token secret; GHCR serves `docker pull` with `GITHUB_TOKEN`.
- openSUSE Build Service: builds outside our CI, so its packages carry no provenance from this
  repository; it can repackage the published tarball later.
- A static musl binary: the glibc build already matches the Debian 12 floor and the package.
- arm64: later, on the `ubuntu-24.04-arm` runner, as a second binary, package and a multi-arch
  image.

## Consequences

- Only `GITHUB_TOKEN` is needed: no outside account and no signing-key secret.
- Anyone can rebuild the binary from the tag and compare (`tests/scripts/check-reproducible.py`).
- A tag publishes immediately; a wrong release is fixed with a new version, not by replacing files.
- A plain `vX.Y.Z` tag ties `dyappd` and the apps to one version line.
