# TUI Design Draft

This directory contains the visual reference draft for the Tentgent terminal UI.
It is a design aid for the TUI plan, not an API contract.

## Files

- [index.html](./index.html)
  Browser-friendly visual mockup with terminal-style wireframes.
- [wireframes.md](./wireframes.md)
  Implementation notes for each mockup screen.
- [open-questions.md](./open-questions.md)
  Product decisions to settle before or during TUI implementation.

## Status

Use this draft as a reference for layout, information hierarchy, status
vocabulary, and keyboard flow. The implementation must still follow the source
contracts:

- [../../tui-session-mvp.md](../../tui-session-mvp.md)
- [../../../contracts/http-daemon.md](../../../contracts/http-daemon.md)
- [../../../contracts/session-store.md](../../../contracts/session-store.md)
- [../../../contracts/server-chat.md](../../../contracts/server-chat.md)
- [../../../contracts/training-lora.md](../../../contracts/training-lora.md)
- [../../../contracts/runtime-home.md](../../../contracts/runtime-home.md)

## Notes

- Wireframes abbreviate paths such as `~/.tentgent` when space is tight. The
  real UI should show the resolved runtime home and shorten only for display.
- The daemon HTTP default is `http://127.0.0.1:8790`.
- TUI daemon URL discovery is `--daemon-url`, `TENTGENT_DAEMON_URL`,
  `<TENTGENT_HOME>/config.toml` `[daemon].url`, daemon metadata, then the
  default URL.
- TUI token discovery is `--token`, `TENTGENT_DAEMON_TOKEN`, then no token.
- Provider environment variables are `HF_TOKEN`, `OPENAI_API_KEY`, and
  `ANTHROPIC_API_KEY`.
- `tentgent daemon start` is the primary background daemon command;
  `tentgent daemon run --detach` uses the same detached-launch implementation.
- Missing-daemon screens should show the resolved home, daemon URL, and a
  copyable `tentgent daemon start --home <PATH> --host 127.0.0.1 --port 8790`
  command.
- Provider key setup in Slice 1 is local-only through `AuthManager` and the
  system Keychain. It must not add daemon HTTP mutation routes, persist secrets
  in config, or display secret values.
