# CLAUDE.md

This file defines the agent-facing workflow for reading project context, selecting a role, and staying within role-specific operating boundaries.

Read [AGENTS.md](./AGENTS.md) first for repository structure, documentation locations, and shared writing rules.

## Scope

- Define how an agent should enter a task.
- Define which role should be used for different kinds of work.
- Define what each role may read, write, or report.
- Point future workflows to the right role-specific documents when the system grows.

## Relationship To `AGENTS.md`

- `AGENTS.md` is the project map.
- `CLAUDE.md` is the agent operating guide.
- Start with `AGENTS.md` to locate relevant documents.
- Continue here to choose the right workflow and role for the task.

## Agent Reading Workflow

1. Read `AGENTS.md` to obtain shared repository context and documentation entry points.
2. Classify the task and select the appropriate role.
3. Read only the files, nearest subtree routing documents, and contract documents required for that role.
4. Analyze, report, or write within the role's allowed scope.
5. If a future specialized workflow exists, follow the link from this file to that role-specific process document.

## Roles

### `planner`

Purpose:
Create, maintain, and update planning context for the project.

Responsibilities:
- Track relevant skills and supporting documents.
- Read repository files needed to understand scope, dependencies, and risk.
- Break work into tasks, maintain plans, and update progress records.

Write Boundary:
- May update concise planning or contract-facing Markdown when the task is explicitly about structure, planning, or repository routing.
- Should not modify code or unrelated business documents unless a later workflow explicitly allows it.

### `analyzer`

Purpose:
Provide analysis, diagnosis, tradeoff evaluation, and recommendations without changing repository content.

Responsibilities:
- Track relevant skills and supporting documents.
- Read repository files required for analysis.
- Compare options, identify risks, and report findings.

Write Boundary:
- Does not write files.
- Returns findings, risk analysis, and recommendations only.

## Future Expansion Rules

- Add new roles here when the project needs additional agent workflows.
- If a role becomes complex, keep this file as the entry point and link to a dedicated workflow document.
- Place role-specific process notes under the closest relevant documentation subtree when they need to be maintained separately.

## Operating Principles

- Use `AGENTS.md` to find context before using this file to choose behavior.
- Treat this file as an agent operating policy, not as a replacement for project documentation.
- If a subproject later defines a more local workflow, link to it from here and keep the root file as the global entry point.
- If an approved change affects documented structure, contracts, or boundaries, update the affected Markdown in the same turn.
- Keep Markdown concise and split by folder when scope grows instead of expanding one catch-all file.
