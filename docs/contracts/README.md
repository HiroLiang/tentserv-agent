# Contracts

Use this directory for concise interface documents that define stable boundaries between repository components.

## Scope

- Describe contracts between Rust entry points and the Python daemon.
- Describe backend routing rules, adapter lifecycle rules, and request or response shapes when they become stable enough to document.
- Describe runtime-home conventions, environment-variable overrides, and stable storage boundaries.
- Describe provider auth storage and resolution rules when secrets cross process boundaries.
- Describe model-store identity, deduplication, and import or pull boundaries when model management behavior changes.
- Keep each document focused on one interface or one boundary.

## Expansion Rules

- If this directory grows, split by subsystem instead of collecting unrelated notes in one file.
- Add a subfolder plus its own `README.md` when one contract area becomes too large to scan quickly.
- Keep documents concise and update them in the same change that modifies the corresponding boundary.
