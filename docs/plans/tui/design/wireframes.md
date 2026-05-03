# TUI Wireframes

The canonical visual draft is [index.html](./index.html). This file records the
implementation intent in a form that is easier for agents to scan.

## Common Shell

- Keep daemon URL, daemon state, auth summary, and time visible in the header.
- Use a stable left navigation column for the eight primary sections.
- Use the right pane for the selected list, detail, chat, log, or action form.
- Keep contextual key hints in the footer.
- Confirm destructive actions explicitly.

## Screens

### Dashboard / Status

The landing screen must work even when the daemon is down. Show runtime home,
daemon URL, doctor summary, auth summary, inventory counts, and warnings. When
the daemon is unreachable, show the resolved home, daemon URL, current
`tentgent daemon start --home <PATH> --host 127.0.0.1 --port 8790` command, and
a clear path to doctor diagnostics.

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
