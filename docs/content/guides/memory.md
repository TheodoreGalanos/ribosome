---
title: "Project memory"
description: "Save experience with its evidence and retrieve it in later project work."
---

# Project memory

Memory makes selected experience available to later workflows in the same client and project. A curator decides what to retain; Rust stores and retrieves it within the configured scope.

## What to retain

| Kind | Example |
| --- | --- |
| Working | A temporary tool outage or an open investigation. |
| Episodic | What failed in one run and what was tried. |
| Aggregated | A tentative pattern supported by several episodes. |
| Procedural | A reference to a candidate or evaluated implementation. |
| Failure | Recognition guidance, counterexamples, and possible responses. |

Keep the strength of the claim clear. An episode describes an observation. An aggregated memory describes a pattern and the evidence supporting it.

## Structure a memory

A `Memory` record stores `kind`, `content`, and `applicability`, plus references to its supporting evidence. It can include counterexamples, possible responses, regression cases, and an expiry time.

`conflicts` names records that contradict it. `supersedes` names memories it replaces. These fields contain record IDs; explain uncertainty in the content and provenance. The surrounding record supplies scope, version, and source relationships.

The `memory@1` operator searches existing records, reads evidence, and saves this structured knowledge. Your application can request consolidation after a run or when enough related evidence has accumulated.

## Retrieve it later

Agents search stored text and exact metadata within their allowed scope. Search supports matching all terms or any terms, relevance ranking with SQLite FTS5 BM25, and bounded pages. Use `order: "relevance"` and `query_mode: "any_terms"` when those choices suit the question.

Search identifies candidates for the agent to assess. The agent reads applicability, evidence, and conflicts before using a memory. Procedural reuse also checks the implementation's capabilities and contextual admission. See [Reuse a procedure](reuse.md).

## Keep context current

Ribosome retains source references for retrieved material, tool results, and summaries. Before the next model request, including after restart, it checks that those sources are still available. Retired, deleted, expired, or inaccessible source material is excluded from subsequent context.

Retirement withdraws access to a memory and dependent material. Deletion also schedules cleanup of managed copies, including saved context and summaries. An operator can inspect and retry cleanup jobs. Apply your retention policy separately to external logs and backups.

Long runs use retained tool results and bounded continuation state. Pi can summarize older context through metered model calls; the summary keeps its source relationships so withdrawal still applies.

For storage paths, see [Runtime and storage](runtime.md). For cleanup and backups, see the [operations reference](../../operations.md#rebuild-search-and-handle-failed-persistence).
