# TUI Session MVP

This plan defines the next terminal UI track after the HTTP daemon parity work.
The TUI should be a local operator console over the existing daemon, stores, and
session APIs, not a second implementation of runtime state.

## Current Baseline

The daemon and CLI now own the main workflow surfaces:

- managed models, adapters, datasets, servers, sessions, train plans, and train runs
- dataset validate/template/export/diff/synth/eval
- session mutation, session-aware chat, and bounded session compaction
- auth status, doctor diagnostics, daemon shutdown, and local HTTP contracts

The TUI should reuse those boundaries. It should not create another session
format, training registry, server registry, or provider runtime path.

## Scope

- Add a terminal UI entry point for local interactive use.
- Give users a browsable control plane for status, auth, doctor, stores, servers,
  sessions, dataset tools, and training plans/runs.
- Provide a session-aware chat surface that uses existing session compaction
  semantics.
- Prefer daemon HTTP APIs for live workflows and shared state.
- Use core managers only for bootstrap reads or explicit offline screens where
  HTTP is not yet available.

## Non-Goals

- Do not build a GUI.
- Do not replace the existing CLI command surface.
- Do not add multi-user collaboration.
- Do not add long-term memory, semantic retrieval, or a dataset-grade transcript
  archive.
- Do not add a separate TUI state model for stores, sessions, or runs.
- Do not add provider key mutation through daemon HTTP, config files, logs, or a
  second secret store. A guarded local TUI setup flow may reuse the existing
  `AuthManager` and system Keychain path.

## Command Surface

Planned command:

```text
tentgent tui [--home <PATH>] [--daemon-url <URL>] [--token <TOKEN>]
```

Recommended supporting daemon UX before or alongside the TUI:

```text
tentgent daemon start [--home <PATH>] [--host 127.0.0.1] [--port 8790]
tentgent daemon run --detach
```

Slice 0 fixes both commands as the supported detached daemon UX. `daemon start`
is the primary user-facing command, and `daemon run --detach` must remain a thin
entry over the same detached-launch implementation. The TUI should be able to
detect a missing daemon and show the command needed to start one.

Daemon URL discovery order for the TUI is:

1. `--daemon-url <URL>`
2. `TENTGENT_DAEMON_URL`
3. `<TENTGENT_HOME>/config.toml` `[daemon].url`
4. daemon metadata `host` and `port`
5. `http://127.0.0.1:8790`

Token discovery order is `--token <TOKEN>`, then `TENTGENT_DAEMON_TOKEN`, then
no token. No daemon token file is part of this MVP.

`config.toml` stores only non-secret preferences. It must tolerate unknown
fields, save atomically through a temp file and rename, validate `daemon.url` as
an absolute `http` or `https` URL, and never persist provider secrets or
`TENTGENT_DAEMON_TOKEN`.

## Product Shape

The first screen should be useful immediately after install:

```text
┌ Tentgent ───────────────────────────────────────────────────────────────┐
│ Home: ~/Library/Application Support/com.tentserv.tentgent   Daemon: OK  │
├───────────────┬─────────────────────────────────────────────────────────┤
│ Status        │ Doctor: ready with warnings                             │
│ Chat          │ Session workspace: choose running server and session     │
│ Models        │ Auth: OpenAI env, HF missing, Anthropic missing          │
│ Adapters      │ Models: 2  Adapters: 1  Datasets: 3  Sessions: 4         │
│ Datasets      │ Running servers: 1  Running train runs: 0                │
│ Servers       │                                                         │
│ Sessions      │ Actions: Enter inspect  n new  / search  ? help          │
│ Training      │                                                         │
│ Resources     │                                                         │
│ Settings      │                                                         │
└───────────────┴─────────────────────────────────────────────────────────┘
```

The UI should feel like a compact local operations panel, not a marketing page:
dense, keyboard-first, and explicit about which local paths, daemon URL, and
auth state are active.

## Navigation Model

Use a two-pane layout by default after the daemon is reachable:

- Left: stable navigation sections.
- Right: list, detail, chat, logs, or action form for the selected section.
- Footer: contextual key hints and destructive-action confirmation state.

Primary sections:

- Status: daemon status, doctor summary, auth status, runtime paths.
- Chat: session-aware workspace over existing daemon chat/session routes.
- Models: list, inspect, guarded import/pull/remove actions.
- Adapters: list, inspect, guarded bind/import/pull/remove actions.
- Datasets: list, inspect, guarded validate/template/export/diff/synth/eval/remove
  actions.
- Servers: list, inspect, start/stop/logs/chat entry.
- Sessions: list, inspect, messages, compact, chat resume, delete.
- Training: LoRA plan list/inspect/create and run list/inspect/logs/metrics.
- Resources: read-only local disk/process/resource monitor.
- Settings: home path, daemon URL, token source, Python/runtime paths.

The TUI has two top-level interaction modes:

- Bootstrap mode: daemon is down or auth is insufficient for live daemon
  workflows. Show a focused setup screen with `Start daemon`, daemon URL/token
  config, explicit per-provider auth setup, settings, and quit. Do not show fake
  live workflow panels.
- Operator mode: daemon is reachable. Show a menu plus monitoring dashboard
  summary. Live workflow data should come through daemon HTTP unless a setting
  is explicitly local-only.

Bootstrap mode rules:

- `Start daemon` is an explicit action. The TUI must not silently auto-start on
  launch.
- Starting should update app state to `starting`, keep the current terminal UI
  stable, then replace the whole screen with the new daemon state after the
  detached launch and readiness check finish. Do not clear the terminal, scroll
  content, or expose launch logs as in-band UI output.
- When the daemon is down, only local setup and auth setup actions are enabled.
  Store, server, session, dataset, training, and dashboard workflow sections
  remain disabled until daemon HTTP is reachable.
- Provider auth status should not imply secret reads. The default auth summary
  may show env presence and `keychain: not checked`; it should read Keychain
  only after the user enters an explicit provider setup/check/set/remove flow.
- Auto refresh must not read Keychain and must not call daemon routes that read
  Keychain. Manual auth checks may prompt.

Operator mode rules:

- The landing view is a function menu plus compact monitoring dashboard table:
  servers, sessions, stores, doctor, daemon status, and auth summary.
- Dashboard boxes are acceptable, but they are supporting UI, not blocking
  modal-like interactions.
- All live workflow reads should go through daemon HTTP in this mode.
- Resources is an Operator-mode read-only screen. It may use local runtime-home
  path helpers and OS probes for disk/process pressure, but it must not create
  cleanup, stop, delete, archive, or mutation actions.
- Settings remain available while the daemon is running. TUI/client preferences
  such as daemon URL and UI section can update immediately. Daemon bind settings
  such as host/port and process token state apply to the next daemon start and
  should be marked as requiring restart.
- Provider Keychain set/remove should take effect for future daemon workflows
  that resolve auth after the change. Already launched runtime child processes
  do not retroactively receive updated environment secrets.
- Chat uses a separate workspace only after the user selects a running server
  or starts a chat flow. The operator dashboard is not itself the chat UI.
- Chat must make context scope visible. A continued session sends recent
  persisted session messages back to the model, which is useful for follow-up
  work but can pollute unrelated topics. New-topic actions and context-size
  controls should be first-class rather than hidden behind raw session
  mechanics.

Interaction controls should preserve the visual style in the design draft:

- Use dynamic tables for inventories, status summaries, and dashboard metrics.
- Use radio/choice controls such as `●` selected and `○` unselected for option
  sets.
- Use explicit selection state and keyboard hints for `↑`/`↓`, Enter, Escape,
  refresh, and destructive confirmations.
- Resize events should recompute layout. Wide terminals may show menu plus
  dashboard table; compact terminals should collapse detail panels and hide
  nonessential columns before wrapping text.

## Design Artifacts

The current visual draft lives in
[docs/plans/tui/design/](./tui/design/README.md). Treat it as a layout and
interaction reference, not as a protocol contract.

If the visual draft diverges from HTTP/session/runtime contracts, the contracts
win. Update the design notes rather than copying stale mock data into code.

## Execution Order

### Slice 0: Daemon UX Prerequisite

Make the daemon easy to run before the TUI depends on it.

Goals:

- add a documented detached daemon start path
- define how the TUI discovers daemon URL and token
- show a friendly startup hint when no daemon is running
- keep shutdown protected by existing daemon-control auth rules
- keep `daemon start` and `daemon run --detach` on the same detached-launch
  implementation
- treat public `GET /healthz` readiness as authoritative; report `/v1/status`
  `401` as an auth warning after health succeeds
- keep idempotent start scoped to the resolved runtime home
- let detached daemon children inherit daemon configuration environment while
  preserving daemon-token sanitization for model-bound server children

Review target:

- a user can start the daemon, run `tentgent tui`, and see status without
  opening a second foreground terminal.

### Slice 1: TUI Architecture Skeleton + Local Setup Foundation

Choose the Rust TUI stack and command boundary.

Goals:

- use `ratatui` plus `crossterm` unless a concrete blocker appears
- add `tentgent tui`
- implement terminal lifecycle, app state, navigation, key handling, status, and
  settings screens
- keep rendering code separate from daemon API/client code
- use daemon HTTP first for live status, auth, and doctor data
- use core code only for bootstrap config, daemon discovery, explicit daemon
  start, and guarded local Keychain auth setup
- allow daemon start only after an explicit `Start daemon` selection; do not
  silently auto-start the daemon on launch
- derive explicit daemon start host/port from the resolved daemon URL, so
  `--daemon-url`, `TENTGENT_DAEMON_URL`, config, metadata, and default discovery
  all feed the same start target
- treat `/healthz` success plus `/v1/status` `401` as `AuthRequired`, not down
- persist only non-secret daemon URL/UI preferences to `config.toml`
- allow provider key set/remove only through local `AuthManager` and Keychain;
  never through daemon HTTP, config, logs, or UI output

Review target:

- one minimal screen shows daemon status, runtime home, auth summary, and doctor
  summary without launching model runtime work, and settings can edit local
  non-secret setup safely.

### Slice 1.1: TUI Interaction Reset

Fix the Slice 1 skeleton into the intended operator-console interaction model
before adding read-only navigator workflows.

Goals:

- split app state into Bootstrap and Operator modes
- make daemon-down landing a focused setup/action screen, not a live dashboard
- make daemon-running landing a function menu plus monitoring dashboard summary
- keep the design draft's dense terminal UI style, including dynamic tables,
  selectable rows, radio-style choices, and keyboard hints
- remove auto Keychain reads from startup and periodic refresh
- avoid calling daemon `/v1/auth` automatically from the landing refresh path if
  it would trigger Keychain prompts
- keep provider auth setup explicit and provider-scoped
- make `Start daemon` render a stable `starting` state and then replace the
  full screen with the resulting state
- ensure start/refresh interactions never clear the terminal, scroll the UI, or
  leave partial frames visible
- make settings editable while the daemon is running, distinguishing immediate
  TUI/client settings from daemon process settings that require restart
- handle terminal resize by recalculating layout instead of relying on fixed
  panel dimensions

Review target:

- a user can launch `tentgent tui`, start a missing daemon, inspect the operator
  menu/dashboard, and enter explicit auth setup without repeated Keychain
  prompts or terminal redraw artifacts.

### Slice 2: Read-Only Navigator

Build read-only browsing first.

Goals:

- list and inspect models, adapters, datasets, servers, sessions, train plans,
  and train runs
- keep Training as one menu entry with read-only Plans and Runs sub-tabs
- show bounded server logs, train run metrics/logs, and session message tails
- use local filters only; do not add pagination/query routes in this slice
- preserve previous dashboard counts when one read endpoint fails
- treat inspect/tail `404` as a stale selected item, not an auth or daemon-down
  transition
- preserve existing CLI/HTTP behavior for errors and auth

Review target:

- users can orient themselves without memorizing CLI commands.

### Slice 2.5: Local Resource Monitor

Add a read-only local resource dashboard before mutation-heavy workflows.

Goals:

- show runtime home disk usage by category: models, adapters, datasets,
  sessions, servers, logs, runtime, and training
- show daemon and managed server process resource summaries where available:
  pid, running state, RSS/memory, CPU sample, port, and log size
- show local disk free space for the resolved runtime home
- show large files, stale logs, stale pid/process metadata, and long-running or
  stale train runs as warnings
- keep this slice read-only; cleanup, delete, stop, compact, and archive actions
  remain deferred to later mutation slices
- avoid adding a second runtime state model; derive resource rows from existing
  runtime paths, daemon status, server specs/process metadata, train run
  metadata, and OS process probes
- degrade gracefully when platform resource probes are unavailable
- add `Resources` to the Operator menu only; Bootstrap keeps workflow panels
  hidden while daemon setup is incomplete
- run directory scans off the render loop with bounded entry/time budgets,
  skip symlink traversal, tolerate unreadable paths, and mark partial scans
- use the last completed resource snapshot for dashboard cards; do not deep scan
  runtime home on normal dashboard refresh
- probe processes and disk free space without invoking a shell; CPU is
  best-effort and may be unavailable

Review target:

- users can understand local disk and process pressure from the TUI without
  leaving the operator console or running separate `du`, `ps`, `btop`, or
  `htop` commands.

### Slice 3: Session Chat Workspace

Add a session-aware chat view.

Goals:

- add `Chat` to Operator mode only, near the top after Dashboard
- choose a running server; if no server is running, show a blocked state and do
  not start a server from the TUI
- create a session through existing `POST /v1/sessions` or resume an existing
  session through existing session GET routes
- send native daemon chat through existing `POST /v1/chat` with explicit
  `server_ref`, `session_ref`, `max_session_messages`, and request messages
- keep daemon session storage as the source of truth; streaming deltas are
  transient UI only, and `ChatDone` refreshes bounded session messages
- stream by default, but never auto-retry non-stream after an ambiguous stream
  failure; show a visible manual retry action instead
- do not use `PATCH /v1/sessions/{ref}` in this slice
- keep adapter selection optional and label compatibility as unverified unless
  existing metadata proves otherwise
- prevent double submit while sending or streaming, and make `Esc`
  state-dependent: cancel in-flight work first, then navigate
- make failures explicit: stale server/session `404`, server stopped, session
  busy, compaction required/failed, stream unsupported, and target server proxy
  failure

Review target:

- users can leave and resume a compact terminal chat session without learning
  the raw session endpoints.

Known issue from Slice 3 smoke:

- A same-session greeting can keep influencing later unrelated prompts because
  Slice 3 sends the last 50 session messages by default. Small local models are
  especially prone to repeating the earlier assistant greeting before answering
  the new question. This is expected from the current session semantics, but the
  TUI does not yet make "continue session" versus "new topic" explicit enough.

### Slice 3.1: Chat Context Controls

Make chat context scope explicit before adding store/server mutations.

Goals:

- add a visible chat context mode to the workspace header and metadata pane:
  `none`, `last 2`, `last 10`, and `last 50`
- default new TUI chat sessions and resumed sessions to `last 2` unless the
  user changes it during the current TUI run
- do not persist context mode into session metadata, config, or any new store in
  this slice
- keep `n` as "new session/new topic" and make that label visible in the footer
  and chooser; it creates a new session and does not clear, rewrite, compact,
  fork, delete, or mutate the current session
- add a quick context toggle key, such as `h`, that cycles context modes without
  leaving the workspace; it must not mutate an in-flight send request
- send the selected mode as `max_session_messages` on `POST /v1/chat`; `none`
  means `max_session_messages: 0`, no prior persisted messages, only the current
  user request
- keep transcript display separate from send context: the UI may still refresh
  `GET /v1/sessions/{ref}/messages?tail=50` while sending only the selected
  context window to the model
- capture context mode, `max_session_messages`, prompt, session ref, server ref,
  adapter ref, and stream mode into the immutable send request before dispatch
- show a warning when a session has repeated greeting-like turns or when the
  transcript is long enough that prior context may dominate small local models;
  these warnings are local, bounded to the refreshed transcript tail, and never
  block send
- preserve daemon session storage as the source of truth; context mode is a TUI
  send preference, not a second transcript store
- do not add daemon routes, session schema fields, compaction actions, or
  transcript rewrites in this slice; the allowed mutation set remains session
  create plus chat send

Review target:

- a user can clearly choose between continuing a conversation and starting a new
  topic, and unrelated prompts no longer inherit earlier greeting behavior by
  accident.

### Slice 4: Store And Dataset Actions

Expose guarded store and dataset workflows through existing daemon HTTP routes.

Goals:

- add a reusable TUI action state machine for Models, Adapters, and Datasets:
  action selection, form editing, confirmation, running, result, and error
  states
- keep all mutations daemon-first; do not shell out to CLI, call core store
  managers directly, or edit runtime-home files from the TUI
- use only existing daemon routes: model/adapter pull/import/delete, adapter
  bind, dataset import/validate/template/export/diff/synth/eval/delete
- URL-encode every model, adapter, and dataset ref path segment
- require exact short-ref or full-ref typed confirmation for destructive
  remove actions
- validate only basic form shape in the TUI, such as non-empty required fields
  and absolute path fields; daemon APIs remain the source of truth for format
  and filesystem authorization
- run pull/import/synth/eval as nonblocking TUI requests with visible elapsed
  time in Slice 4; Slice 4.1 upgrades long actions to daemon-side jobs with
  progress and background tracking
- run synth/eval only after explicit provider/network-credit confirmation; do
  not read Keychain or call `/v1/auth` as part of the confirmation
- show bounded summaries and artifact/debug paths without storing raw provider
  output in renderable TUI state, logs, panic output, or test snapshots
- refresh affected navigator sections after success and mark Resource snapshots
  stale without deep-scanning runtime home immediately
- keep server lifecycle, training lifecycle, session delete/compact, and chat
  transcript mutation out of this slice

Review target:

- common dataset and store workflows are discoverable from terminal menus while
  preserving the existing HTTP contracts.

Known issue from Slice 4 smoke:

- Long-running model pull currently renders as a plain `Action Running` request
  panel with elapsed time only. It does not show file/byte progress, percent,
  speed, or ETA.
- The TUI can keep rendering while waiting for the HTTP request, but the action
  is not a true daemon-side background job. Leaving the action view only aborts
  the local TUI wait; there is no durable job id to follow from another screen.
- The UI should not expose raw method/route/debug request details as the primary
  user experience for common downloads.

### Slice 4.1: Background Action Jobs + Progress UI

Turn long-running store and dataset actions into trackable daemon-side jobs with
a polished TUI progress surface.

Goals:

- introduce a daemon job/progress contract for long-running actions such as
  model pull, adapter pull, model/adapter import when slow, dataset synth, and
  dataset eval
- preserve existing synchronous route semantics. Existing `POST
  /v1/models/pull`, `POST /v1/models/import`, `POST /v1/adapters/pull`, `POST
  /v1/adapters/import`, `POST /v1/datasets/import`, `POST
  /v1/datasets/synth`, and `POST /v1/datasets/eval` keep their original
  response shapes; async work uses explicit `/jobs` routes.
- add async routes: `POST /v1/models/pull/jobs`, `POST
  /v1/models/import/jobs`, `POST /v1/adapters/pull/jobs`, `POST
  /v1/adapters/import/jobs`, `POST /v1/datasets/import/jobs`, `POST
  /v1/datasets/synth/jobs`, and `POST /v1/datasets/eval/jobs`
- add read-only job routes: `GET /v1/jobs` and `GET /v1/jobs/{job_id}`
- persist bounded job records under the resolved runtime home. Jobs survive TUI
  navigation and TUI process exit; active jobs are marked `interrupted` after a
  daemon restart in this slice.
- keep short actions synchronous where appropriate; do not force every mutation
  through the job system
- start a long-running action and return a `job_id` quickly so the TUI can leave
  the form/result screen and continue browsing
- expose read-only job status/progress through daemon HTTP using existing auth
  rules; do not leak provider secrets, daemon tokens, raw provider output, or
  unbounded logs
- track stage, status, started/updated timestamps, bytes/files progress when
  available, percent, speed/ETA when derivable, current artifact path, warning,
  and final result/error summary
- render active jobs in the Operator dashboard or footer as a compact progress
  area, with a dedicated Jobs/Downloads detail pane if the list outgrows the
  dashboard
- let users hide/background the action detail and continue using Models,
  Adapters, Datasets, Chat, Resources, and Settings while jobs run
- refresh affected navigator sections when a job completes, preserving
  selection by ref where possible and marking Resources stale without immediate
  deep scan
- make cancellation explicit only if the daemon can actually cancel the job;
  otherwise provide `hide/background`, not fake cancel
- improve the action UI so common workflows render as product-level progress
  cards instead of raw method/route panels
- do not add daemon-side cancellation in Slice 4.1. Job items report
  `cancellable: false`.

Review target:

- a user can start a model pull from the TUI, navigate away, keep working, and
  still see download progress and completion/failure without relying on a
  terminal command or raw request panel.

### Slice 5: Server And Training Actions

Expose runtime and training controls through existing daemon HTTP routes only.

Goals:

- add guarded TUI runtime actions for `Servers` and `Training`
- create server specs, start with bounded readiness wait, stop, and remove
  stopped server specs
- offer explicit shortcuts from selected model to server creation and selected
  dataset to LoRA plan creation
- preview/create/remove LoRA plans with Basic and Advanced form sections
- start LoRA runs with explicit local resource confirmation
- poll active run status, metrics, and bounded raw log tails without tying HTTP
  requests to render frames
- make destructive remove actions require exact short/full ref confirmation

Boundaries:

- use only existing daemon routes; do not add backend routes or schemas
- server start uses the existing start route and is not converted into a
  Slice 4.1 job
- training runs remain in the training registry and are not mirrored into the
  job registry
- do not call `/v1/auth`, read Keychain, shell out to CLI, or mutate core files
- do not show fake cancellation for server start or training runs

Review target:

- the TUI can operate the local runtime loop from model selection through a
  small training run without adding new backend semantics, while preserving
  Chat stale-server handling and bounded log/metrics reads.

### Slice 5.1: Picker-Based Runtime Forms

Replace ref-heavy runtime action forms with chooser-first workflows.

Reason:

- managed local resources are already known to Tentgent, so the TUI should let
  users select models, datasets, and plans instead of copying refs from another
  table or terminal
- text input should remain available for cloud/runtime refs and advanced
  overrides, but it should not be the primary path for local managed resources
- picker flows reduce invalid refs, make server creation and LoRA plan creation
  discoverable, and better match the TUI goal of minimizing terminal work

Goals:

- add model pickers for server creation and LoRA plan creation
- add dataset pickers for LoRA plan creation and dataset-backed training flows
- add backend and boolean option pickers where choices are finite, such as
  `backend`, `lazy_load`, `mask_prompt`, `mlx_grad_checkpoint`,
  `peft_load_in_4bit`, and `peft_load_in_8bit`
- collapse advanced LoRA overrides behind an explicit advanced section so the
  default plan form starts with only model, dataset, optional name, and backend
- keep manual text entry as an advanced fallback for cloud refs, pasted refs,
  and values that do not have a finite local list
- show selected row metadata beside picker choices, such as model format,
  source, dataset splits, tuning readiness, and plan blockers

Boundaries:

- use existing navigator/list data and existing daemon GET routes only
- do not add backend routes or schemas
- do not auto-pull models, auto-import datasets, start servers, or start runs
  from a picker
- do not read Keychain or call `/v1/auth`
- keep exact ref confirmation for destructive actions

Review target:

- a user can create a local server and a LoRA plan from managed local resources
  without manually typing or copying a model/dataset ref, while cloud/runtime
  text entry remains available as an advanced path.

## Open Questions

- Should `tentgent tui` auto-start a daemon when token and bind settings are
  safe, or should it only show the exact command to run?
Resolved in Slice 1:

- `tentgent tui` does not silently auto-start the daemon. It may start the
  daemon after an explicit `s` action through the shared detached-launch helper,
  using the resolved daemon URL host/port as the start target.

Resolved in Slice 3:

- The first Chat workspace streams by default. Non-stream fallback is available
  only as an explicit user action when the stream failure is known to be
  pre-processing or mapping-related, avoiding duplicate chat turns.
- Provider key setup may be exposed in TUI only as a guarded local
  `AuthManager`/Keychain flow. No daemon HTTP secret mutation route is added.
