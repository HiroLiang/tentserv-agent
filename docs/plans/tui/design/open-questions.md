# TUI Open Questions

Decide these before or during the first implementation slices.

1. Should `tentgent tui` auto-start the daemon when bind and token settings are
   safe, or only show the exact command to run?
2. Should the first chat view support streaming immediately, or ship non-stream
   first and add streaming after layout settles?
3. In multi-server chat situations, should the TUI auto-pick, prompt, or require
   explicit server selection?
4. Should token display use full masking, last-three-character hints, or source
   labels only?
5. Should local chat omit cost entirely, show `$0.00 (local)`, or show a
   non-monetary runtime signal?
6. Are six visible ref characters enough in dense lists, or should the UI use
   12-character short refs everywhere?
7. Should compaction controls live only in the session chat view, or also in
    settings and session detail?
8. Should training metrics use an ASCII sparkline, a table, or both?
9. Which actions require typed confirmation rather than a simple yes/no?
10. Should provider-backed dataset actions require confirmation for every call
    or only when starting a batch?
11. Should mouse support stay out of scope for the MVP?
12. When the daemon is remote or non-loopback, should path-opening actions be
    disabled automatically?

## Resolved In Slice 0

- Detached daemon UX supports both `tentgent daemon start` and
  `tentgent daemon run --detach` through the same implementation.
- TUI token discovery uses `--token`, then `TENTGENT_DAEMON_TOKEN`, then no
  token. No daemon token file is part of the MVP.

## Resolved In Slice 1

- TUI daemon URL discovery includes non-secret `config.toml` `[daemon].url`
  between `TENTGENT_DAEMON_URL` and daemon metadata.
- TUI provider key setup may use guarded local `AuthManager`/Keychain calls.
  Daemon HTTP provider-secret mutation, config-file secrets, logs, and second
  secret stores remain out of scope.
