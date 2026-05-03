# TUI Wireframes

The canonical visual draft is [index.html](./index.html). This file records the
implementation intent in a form that is easier for agents to scan.

## Common Shell

- Keep daemon URL, daemon state, auth summary, and time visible in the header.
- Use a stable left navigation column for the eight primary sections.
- Use the right pane for the selected list, detail, chat, log, or action form.
- Keep contextual key hints in the footer.
- Confirm destructive actions explicitly.
- Use dynamic tables for lists and summaries, selected-row styling for focus,
  and radio-style controls (`●` selected, `○` unselected) for option sets.
- Show keyboard navigation explicitly: `↑`/`↓` move, Enter selects, Escape backs
  out, `r` refreshes, and destructive actions require confirmation.
- Recompute layout on resize. Prefer hiding nonessential columns and collapsing
  detail panels before wrapping dense table text.

## Screens

### Bootstrap / Daemon Down

When daemon HTTP is unreachable, the landing screen is a setup/action screen,
not a dashboard. It should show resolved home, resolved daemon URL/source,
token source, the start command, and local settings. The canonical visual draft
is `index.html`; `index_v1.html` is archived for history only. Enabled actions:

- `Start daemon`
- daemon URL/token setup
- provider auth setup by explicit provider selection
- settings
- quit

Disabled sections should remain visible only as disabled menu entries or
secondary hints. The TUI must not read Keychain or call auth routes repeatedly
from this screen. Provider Keychain checks happen only after the user enters a
specific provider setup/check/set/remove flow.

### Dashboard / Status

The daemon-running landing screen is a function menu plus monitoring dashboard
summary. Show runtime home, daemon URL, doctor summary, auth summary, inventory
counts, warnings, and quick actions. Live dashboard data should come from daemon
HTTP. The dashboard should not block normal menu navigation.

### Start Daemon

Selecting `Start daemon` should update the app to a stable starting state and
then replace the screen with the resulting Bootstrap or Operator state. The TUI
may show a compact progress table with phases such as resolving home, spawning
detached daemon, polling `/healthz`, and ready/failed. It must not stream raw
shell output, clear the terminal, scroll content, or leave partial frames
visible. The detailed stdout/stderr daemon logs stay in the log files and can be
opened or copied as paths.

### List + Inspect

Use this pattern for models, adapters, datasets, sessions, and train plans:
filterable list on the left of the content area, selected item details on the
right, summary row below the list.

### Servers

Show server state, kind, model/provider, pid/port, health, logs, chat entry, and
safe stop/remove actions. Running/stopped/starting/failed states should be
visually distinct without relying on color alone.

### Session Chat

Show session title/ref, selected server, selected adapter, bounded message
history, compaction status, and sticky input. Surface session-specific failures
such as `session_busy`, `server_not_running`, and `session_compaction_failed`.

### Dataset Tools

Show dataset list/detail plus actions for validate, template, export, diff,
synth, and eval. Provider-backed synth/eval must require explicit confirmation
and should show artifact paths rather than raw provider output.

### Training

Show LoRA plans, runs, status, phase, metrics summary, and log/metrics entry
points. The first pass should observe and launch existing plan/run APIs rather
than inventing new training state.

### Settings

Show resolved path settings, daemon URL/token source, provider auth status,
`.env` guidance, version, and Python runtime. Most fields should be read-only
until there is a concrete config mutation API.

## Compact Fallback

The design includes a `100 x 30` fallback. The implementation should degrade by
collapsing detail panels and hiding nonessential columns before wrapping text.
