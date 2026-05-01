# Session Store

This document defines the first read-only local session store boundary for
daemon-backed TUI and external chat transcript discovery.

## Scope

- Store session metadata and transcript messages under the Tentgent runtime home.
- Keep sessions separate from `tentgent.chat.v1` training and evaluation records.
- Expose read-only discovery through the Rust daemon.
- Defer session creation, append semantics, locking, repair, search, export, and
  session-aware chat to later slices.

## Layout

Sessions live under:

```text
<TENTGENT_HOME>/sessions/<session_ref>/
  session.toml
  messages.jsonl
```

`session_ref` is a managed lowercase hexadecimal ref. `short_ref` is the first
12 characters. API lookup accepts full refs or unique prefixes; it never accepts
arbitrary filesystem paths.

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

## Read Semantics

Reads are best-effort local file reads. This slice does not define locking,
partial-write tolerance, atomic append, compaction, repair, import, or export.

Missing `messages.jsonl` is not fatal. Readers return an empty message list with
a structured `messages_missing` warning.

Path fields exposed by daemon APIs are local diagnostics and may reveal local
filesystem layout. They are intended for loopback-local daemon usage.
