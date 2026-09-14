---
title: "External agent records"
description: "Import source trajectories, inspect their evidence and test discovered instructions."
---

# External agent records

Use an existing agent execution as evidence for a new investigation. Ribosome can import its messages and tool interactions, let a caretaker inspect a selected window, and let a curator propose a reusable behaviour. A supported discovery can become an instruction for a fresh local task.

The optional `ribosome-import` crate reads local JSON, JSONL, compressed JSONL and Parquet. Its network feature reads revision-pinned Hugging Face Parquet files. Included profiles cover AEC-Bench engineering rollouts and Nebius software-agent trajectories.

## Follow one execution

1. Import source rows with a profile that defines fields, joins and message decoding.
2. Inspect the coverage report: messages, tool calls, paired results, timestamps and missing evidence.
3. Assign complete episodes or fixed prefixes through a host-owned corpus.
4. Run an audit or discovery and inspect its saved evidence references.
5. Challenge a supported definition, extract an instruction and test it on fresh recipient tasks.

The [runnable offline lab](../../../examples/offline-lab/README.md) includes commands, profiles and the study-plan fields. Its [YAML profile guide](../../../examples/offline-lab/profiles/README.md) explains field mapping and how recorded tool calls can guide evidence selection. Preparation makes no model calls. Live stages share a bounded allowance and save incomplete attempts alongside completed work.

## Control what an agent can read

The profile describes the source format. The generated source lock records the revision, concrete files and selected episode identities. The study manifest decides which messages and artifact parts each investigation can read.

Publisher rewards, final-output sidecars and unclassified source fields stay in the owner files. Imported source messages remain quoted observations. A fixed prefix includes only its selected message parts, so reading an early message does not expose a later transcript through a shared artifact.

Large messages have ordered parts. An agent must read those parts before concluding that a result or check is absent. Coverage reports preserve pending tool calls and missing timestamps, which helps the owner choose experiments the episode can support.

## Reuse and withdraw evidence

Imported episodes use the ordinary Ribosome store and source links. Findings and memories can cite their events, then be retrieved in a later workflow. Withdrawing an imported source removes its generated message files and makes dependent records and saved context unavailable.

The function study executes a prepared instruction through the ordinary Pi worker with experimental purpose. Its fresh cases separate subject input from the independent judge's answers. The system comparison adds retry, critique and caretaker controls under shared case budgets. See [Experiments](experiments.md).

## Read the first results

The pilot imported six AEC episodes and six Nebius episodes. With a larger model allowance, a curator completed a source-linked investigation and extracted a path-validation instruction. The ordinary Pi executor then tested it on two fresh tasks, twice each, alongside a baseline.

The candidate passed two of four executions; baseline passed one. Both candidate successes involved preserving an already correct result for an incompatible request. Path resolution failed. The final contrast completed and rejected applying the path definition to the two GeoPandas test excerpts it inspected.

The pilot is closed with those outcomes. E6, the broader system comparison, is deferred until a candidate demonstrates its local function. Read the [expanded results](../../../examples/offline-lab/expanded-results.md) for the case outcomes, access fixes and remaining research questions, and the [first pilot](../../../examples/offline-lab/results.md) for the earlier attempts.
