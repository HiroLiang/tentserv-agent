# Session Store

This document defines the local session store boundary for daemon-backed TUI,
CLI, and external chat transcript discovery and mutation.

## Scope

- Store session metadata and transcript messages under the Tentgent runtime home.
- Keep sessions separate from `tentgent.chat.v1` training and evaluation records.
- Expose discovery and explicit session mutations through CLI and Rust daemon
  APIs.
- Defer repair, search, export, message edit/delete, attachments, and
  session-aware chat to later slices.

## Layout

Sessions live under:

```text
<TENTGENT_HOME>/sessions/<session_ref>/
  session.toml
  messages.jsonl
  session.lock
```

`session_ref` is a managed lowercase hexadecimal ref. `short_ref` is the first
12 characters. Session refs are content-independent generated refs. API lookup
accepts full refs or unique prefixes; it never accepts arbitrary filesystem
paths.

Session creation uses `<TENTGENT_HOME>/sessions/.sessions.lock`. Per-session
mutations use `<TENTGENT_HOME>/sessions/<session_ref>/session.lock`.

## Metadata

`session.toml` uses schema `tentgent.session.v1`:

```toml
schema = "tentgent.session.v1"
session_ref = "abcdefabcdef000000000000"
short_ref = "abcdefabcdef"
title = "Planning session"
created_at = "2026-05-01T00:00:00Z"
updated_at = "2026-05-01T00:10:00Z"
message_count = 2
default_server_ref = "optional-server-ref"
adapter_ref = "optional-adapter-ref"
tags = ["optional"]
```

`message_count` is stored metadata for fast listing. Message reads scan
`messages.jsonl` and may report a structured `message_count_mismatch` warning
when the stored count is stale.

Session lists sort by `updated_at` descending, then `created_at` descending, then
`session_ref` ascending.

`default_server_ref` and `adapter_ref` are validated when written. Later removal
of the referenced server or adapter can leave stale metadata; future inspect or
repair tooling may surface that.

## Messages

`messages.jsonl` uses schema `tentgent.session.message.v1`, one JSON object per
line:

```json
{"schema":"tentgent.session.message.v1","role":"user","content":"Hello","created_at":"2026-05-01T00:00:00Z","server_ref":null,"adapter_ref":null,"metadata":{}}
```

Known roles are `system`, `user`, `assistant`, and `tool`. `server_ref`,
`adapter_ref`, and `metadata` are optional provenance fields. Readers return
missing metadata as `{}`.

Malformed JSONL, unknown roles, invalid timestamps, and non-object metadata are
read failures. Errors include line numbers but do not echo message content.

## Write Semantics

Session writes are explicit local store mutations. `created_at` for messages is
assigned by Tentgent at append time; callers cannot set it in this slice.

Writers validate:

- roles: `system`, `user`, `assistant`, `tool`
- non-empty string content up to 1 MiB
- metadata object up to 64 KiB after JSON serialization
- append batch size from 1 to 100 messages
- tags: trimmed, case-sensitive, order-preserving, max 32 tags, max 64
  characters each, no duplicates after trimming

`session.toml` is written via temp file plus rename. `messages.jsonl` append and
`message_count` / `updated_at` updates happen while holding the per-session
lock. This prevents concurrent CLI and HTTP writers from interleaving, but it is
not multi-file crash atomic. If a process crashes between appending
`messages.jsonl` and renaming `session.toml`, read APIs can report the existing
`message_count_mismatch` warning.

Session removal permanently deletes the session directory. There is no trash or
recycle bin.

## Read Semantics

Reads are best-effort local file reads. This slice does not define shared read
locks, partial-write tolerance, compaction, repair, import, or export.

Missing `messages.jsonl` is not fatal. Readers return an empty message list with
a structured `messages_missing` warning.

Path fields exposed by daemon APIs are local diagnostics and may reveal local
filesystem layout. They are intended for loopback-local daemon usage.
