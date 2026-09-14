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

An acquisition directory uses one profile. When its mappings change, prepare a new directory. Local and Hugging Face imports check this before reading or writing source files, and reuse checks the episode's task, family, metadata and annotations.

Publisher rewards, final-output sidecars and unclassified source fields stay in the owner files. Imported source messages remain quoted observations. A fixed prefix includes only its selected message parts, so reading an early message does not expose a later transcript through a shared artifact.

Large messages have ordered parts. An agent must read those parts before concluding that a result or check is absent. Coverage reports preserve pending tool calls and missing timestamps, which helps the owner choose experiments the episode can support.

Event sequences preserve message order. Parent links connect tool results to identified calls; adjacent independent messages have no dependency link. Curators can retain other proposed relationships as inferences.

To rebuild evidence published before the parent-link correction, assign it under a new study ID. Earlier stored events and experiment results retain their original links.

## Reuse and withdraw evidence

Imported episodes use the ordinary Ribosome store and source links. Findings and memories can cite their events, then be retrieved in a later workflow. Withdrawing an imported source removes its generated message files and makes dependent records and saved context unavailable.

The function study executes a prepared instruction through the ordinary Pi worker with experimental purpose. Its fresh cases separate subject input from the independent judge's answers. The system comparison adds retry, critique and caretaker controls under shared case budgets. See [Experiments](experiments.md).

## Read the first results

The twelve-episode development investigation used six AEC episodes and six Nebius episodes. A curator completed a source-linked investigation and extracted a path instruction from inspected EDK2 code and tests. The ordinary Pi executor tested it on two fresh tasks, twice each, alongside a baseline. The model and output allowance also changed during development, so completion cannot be attributed to budget alone.

The instruction respected an applicability boundary but failed its intended function. It preserved the already correct incompatible result in both trials and failed both applicable path-resolution trials. It was not admitted. The final contrast rejected applying the path definition to the two GeoPandas test excerpts it inspected.

The pilot is closed with those outcomes. E6, the broader system comparison, is deferred until a candidate demonstrates its local function. Read the [expanded results](../../../examples/offline-lab/expanded-results.md) for the case outcomes, access fixes and remaining research questions, and the [first pilot](../../../examples/offline-lab/results.md) for the earlier attempts.


The [agent-strategy follow-up](../../../examples/offline-lab/behavior-results.md) investigates an observed warning repair and tests its instruction on fresh configuration tasks. Discovery, a nearby contrast and extraction completed. After a first study exposed an ambiguous recipient field, an explicit-contract repeat passed all eight executions: the candidate matched baseline on these local cases. Both attempts, the instruction and source excerpts are available for inspection.
