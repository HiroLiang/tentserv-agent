# TUI Open Questions

Decide these before or during the first implementation slices.

1. Should `tentgent tui` auto-start the daemon when bind and token settings are
   safe, or only show the exact command to run?
2. Should daemon detach UX be `tentgent daemon start`, `tentgent daemon run
   --detach`, or both?
3. Should TUI tokens come only from `TENTGENT_DAEMON_TOKEN`, or should a local
   daemon-token file be introduced later?
4. Should provider key mutation remain CLI-only permanently, or get a separate
   guarded TUI flow after security review?
5. Should the first chat view support streaming immediately, or ship non-stream
   first and add streaming after layout settles?
6. In multi-server chat situations, should the TUI auto-pick, prompt, or require
   explicit server selection?
7. Should token display use full masking, last-three-character hints, or source
   labels only?
8. Should local chat omit cost entirely, show `$0.00 (local)`, or show a
   non-monetary runtime signal?
9. Are six visible ref characters enough in dense lists, or should the UI use
   12-character short refs everywhere?
10. Should compaction controls live only in the session chat view, or also in
    settings and session detail?
11. Should training metrics use an ASCII sparkline, a table, or both?
12. Which actions require typed confirmation rather than a simple yes/no?
13. Should provider-backed dataset actions require confirmation for every call
    or only when starting a batch?
14. Should mouse support stay out of scope for the MVP?
15. When the daemon is remote or non-loopback, should path-opening actions be
    disabled automatically?
