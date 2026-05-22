# M7 Apple Developer ID Release Pipeline

Status: implemented in source, awaiting the first credentials-backed release
workflow smoke.

M7 adds Apple Developer ID signing and Apple notarization to the existing
tag-driven GitHub Release workflow. It does not add new product capabilities.

## Goal

- Produce macOS release archives whose `tentgent` binary is signed with a
  Developer ID Application certificate.
- Submit the signed macOS package contents to Apple notarization before upload.
- Keep existing release asset names, installer scripts, and checksum flow
  stable.
- Leave Linux and Windows package behavior unchanged.

## Implementation

- `.github/workflows/release.yml` imports the Developer ID Application
  certificate only on macOS package jobs.
- The macOS package job reads Apple credentials from the `apple-developer`
  GitHub Actions environment with `deployment: false`, so the job can access
  environment secrets without creating a GitHub deployment object.
- `scripts/package-local.sh` signs macOS binaries with Developer ID when
  `TENTGENT_MACOS_CODESIGN_IDENTITY` is set. Local development builds still
  fall back to ad-hoc signing.
- Developer ID signing uses hardened runtime, timestamping, and the bundle
  identifier `com.tentserv.tentgent`.
- `scripts/macos-notarize-package.sh` extracts the release archive, verifies
  the signed binary Team ID, creates a zip payload for Apple notarization, waits
  for acceptance, and re-runs strict `codesign` verification. Bare CLI
  executables are not app bundles, so `spctl -t exec` is not used as the release
  gate.
- `scripts/install.sh` no longer overwrites an existing valid macOS signature
  with an ad-hoc signature during install.

## Required GitHub Actions Secrets

- `APPLE_TEAM_ID`
- `APPLE_DEVELOPER_ID_APPLICATION_CERTIFICATE_BASE64`
- `APPLE_DEVELOPER_ID_APPLICATION_CERTIFICATE_PASSWORD`
- `APPLE_NOTARY_KEY_ID`
- `APPLE_NOTARY_ISSUER_ID`
- `APPLE_NOTARY_KEY_BASE64`
- `APPLE_KEYCHAIN_PASSWORD`

Optional:

- `APPLE_CODESIGN_IDENTITY`

If `APPLE_CODESIGN_IDENTITY` is not set, the workflow detects the first
`Developer ID Application:` identity from the imported temporary keychain.

These secrets should be stored on the `apple-developer` environment, not as
repository-wide release secrets.

Optional future `.pkg` installer work may also need a Developer ID Installer
certificate, but M7 keeps the current archive-based distribution.

## Verification

Source-only checks that do not require Apple credentials:

```bash
bash -n scripts/package-local.sh
bash -n scripts/macos-import-codesign-certificate.sh
bash -n scripts/macos-notarize-package.sh
```

Credentials-backed verification, after secrets are configured:

1. Push or manually dispatch a prerelease tag.
2. Confirm both macOS package jobs import the signing certificate.
3. Confirm both macOS archives are notarization accepted before upload.
4. Confirm `checksums.txt` includes exactly one entry per release archive.
5. Install from the release URL and run `tentgent doctor`.

## Non-Goals

- Switching macOS assets from `.tar.gz` to `.zip`.
- Producing a `.pkg` installer.
- Stapling notarization tickets to archive files.
- Homebrew tap automation changes.
- Product runtime or model compatibility changes.
