---
id: 0001
title: Record decisions in docs/decisions
status: accepted
date: 2026-11-19
issue: —
supersedes: —
superseded-by: —
signed-by: —
---

## Context
People and agents change this repo. Code shows what was built, not why, so intent gets lost and the same questions are decided again — or undone by someone who did not know.

## Options
1. Keep decisions in chat and PR threads — kind: decline
2. A decisions section in the README — kind: smaller
3. One short record per decision in `docs/decisions/`, with status, options and "Revisit if" — kind: full

Recommendation: 3, because records can be linked, superseded and checked by agents before they build.

## Decision
Option 3, using `TEMPLATE.md`. An `accepted` record binds until a newer record supersedes it.

## Consequences
Every hard-to-reverse change starts as a `proposed` record. Writing one takes ~10 minutes; changing a decision means a new record, never an edit of the old one's Decision.

## Revisit if
records go stale (code and records disagree for months) or nobody reads them before building.
