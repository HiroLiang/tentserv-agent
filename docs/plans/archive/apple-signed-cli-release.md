# Apple Signed CLI Release

Deprecated as an active plan. This standalone track was superseded on
2026-05-19 by [Capability-First Release Roadmap](./capability-first-release-roadmap.md).
Keep this file only for historical signing-plan context.

This plan defines the next release-engineering slice for signed macOS CLI
artifacts. Tentgent is CLI plus daemon REST only; this track must not produce or
test terminal UI artifacts.

## Goal

- Build macOS release artifacts from GitHub Actions.
- Sign the `tentgent` CLI with Apple Developer ID Application credentials.
- Notarize release archives or installer packages with Apple notary service.
- Publish checksums and make Homebrew tap updates repeatable.
- Keep tag-driven releases auditable and smoke-tested.

## Required Secrets

- Developer ID Application certificate exported as base64 `.p12`.
- Certificate password.
- Apple Team ID.
- Notary credentials, either App Store Connect API key material or Apple ID with
  an app-specific password.
- Homebrew tap push credentials if the workflow opens or pushes tap updates.

Secrets must live in GitHub Actions secrets and must never be written to release
logs, repository files, or `TENTGENT_HOME`.

## Workflow Shape

1. Trigger on `v*` tags and `workflow_dispatch`.
2. Build macOS arm64 and x86_64 CLI binaries, or a universal binary if the
   release pipeline chooses that shape.
3. Import the certificate into a temporary CI keychain.
4. Sign `tentgent` with identifier `com.tentserv.tentgent`.
5. Package the CLI and support files into release archives.
6. Submit notarization with `xcrun notarytool` and wait for acceptance.
7. Staple where the artifact type supports stapling; otherwise verify
   notarization and Gatekeeper behavior on the packaged artifact.
8. Generate `checksums.txt` and attach artifacts to the GitHub Release.
9. Run a smoke install on a macOS runner:
   - `tentgent --version`
   - `tentgent doctor`
   - `tentgent runtime bootstrap --print-plan`
   - confirm the former terminal UI command is not valid
10. Update the Homebrew tap through the existing checksum-driven helper or open
    a pull request for review.

## Acceptance Criteria

- A version tag produces signed and notarized macOS CLI release artifacts.
- `spctl` or an equivalent Gatekeeper verification passes for the packaged
  artifact.
- Release checksums match the uploaded artifacts.
- Homebrew formula update can be generated without hand-copying URLs or hashes.
- Release smoke confirms the CLI and daemon-only surface.

## Out Of Scope

- Reintroducing a terminal UI or GUI app.
- Automatic Python runtime bootstrap during Homebrew installation.
- Heavy local-model or training runtime smoke tests in the signing workflow.
- Public network model downloads as a required release gate.
