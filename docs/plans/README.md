# Plans

Use this directory for active or still-open implementation plans that are too large or too cross-cutting to execute safely in one pass without a staged breakdown.

## Scope

- Record step-by-step execution plans before large runtime, server, or backend changes.
- Keep one plan per major initiative.
- Prefer short, action-oriented documents over long design essays.

## Routing Rule

- Keep each plan focused on one execution track.
- If a plan grows large, split it into subfolders with a local `README.md`.
- Update the plan when the approved execution order changes materially.
- Prefer review-sized implementation slices over one large execution step.
- When a track is being developed interactively, document the next small slice explicitly.
- Move completed plans into `archive/` so the top-level plan directory stays focused on unfinished work.

## Active Plan Index

- [packaging-install-mvp.md](./packaging-install-mvp.md)
  Active release/install track. The current execution track is the 0.3.x
  project-owned Homebrew tap distribution path: stable release readiness, tag
  assets, tap formula, install/upgrade/uninstall smoke, docs, and tap update
  automation.
- [apple-signed-cli-release.md](./apple-signed-cli-release.md)
  Next release-engineering track for GitHub Actions macOS CLI signing,
  notarization, checksums, Homebrew tap updates, and tag-driven release
  automation. This track is CLI plus daemon only; no TUI artifact is produced.
- [linux-release-support.md](./linux-release-support.md)
  Linux release/install track. The x86_64 prerelease path now has GitHub
  Release tarballs, `install.sh` support, base runtime bootstrap smoke, and
  user-facing preview docs. Optional expansion remains open for Linux arm64,
  distro packages, and heavier runtime profiles.
- [tentgent-daemon-runtime.md](./tentgent-daemon-runtime.md)
  Planned `tentgent-daemon` runtime systems for one-shot background jobs,
  bounded progress/output visibility, session-aware daemon orchestration, and
  chat-backed session compaction.
- [model-capabilities-embedding-rerank.md](./model-capabilities-embedding-rerank.md)
  Planned model capability track for embedding and rerank models. Separates
  model storage format from serving capability before adding non-chat endpoints;
  local backend work should use kernel capability-state gates.
## Recommended Order

1. Run the Apple signed CLI release track so macOS artifacts are Developer ID
   signed, notarized, checked, and tap-update ready.
2. Continue Linux optional expansion only after preview feedback and runtime
   profile readiness are clear.
3. Plan and implement model capabilities for embedding and rerank models.
4. Keep kernel capability state as the backend-readiness source for any new
   model/runtime feature.

## Deferred Plans

- No TUI redesign track is active. The product surface is CLI plus daemon REST.

## Archived Plans

- [archive/README.md](./archive/README.md)
  Router for completed plans that are kept only for historical context.
