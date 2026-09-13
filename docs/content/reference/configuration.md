---
title: "Configuration"
description: "Configure the model, workspace, permissions, checks, and studies."
---

# Configuration

The host reads a JSON configuration. Keep provider credentials in the environment and commit only non-secret settings.

## Create a starting file

From a built checkout:

```sh
target/debug/ribosome init /absolute/path/to/workspace
```

This creates `ribosome.json` in the chosen directory. It starts in `observe` mode with empty artifact and tool permissions and a placeholder model. Edit the file before running it.

Set the workspace and state paths, choose the provider and model, and replace the prompt with the maintenance task. Grant access to the evidence and tools that task needs. The generated grant has a one-hour deadline; issue a fresh work identity and grant when starting new work after it expires.

## Host settings

| Field | Purpose |
| --- | --- |
| `workspace`, `state_dir` | Task files and local Ribosome state. |
| `node`, `worker` | Node executable and Pi-worker paths. |
| `request` | Run identity, profile, operator, prompt, provider, and model. |
| `grant` | Client/project scope, access mode, permitted paths/tools, and budget. |
| `tools` | Registered commands and the files they read, write, or validate. |
| `attachment` | Event kinds, batching, feedback expiry, and optional steering or coordinated writes. |
| `corpora` | Selected source evidence for discovery; choose one with `request.discovery_corpus`. |
| `evaluators`, `agent_evaluators` | Command evaluators or complete Pi-agent evaluation setups. |
| `cases`, `policies` | Host-owned cases and experiment acceptance rules. |

Use absolute executable paths. An installed consumer must resolve the worker from its installed package; see [Installation](../start/installation.md).

## Provider settings

The example scripts load `.env`; existing shell settings take precedence. For direct Rust commands, export provider credentials in the shell before starting the host.

Set `RIBOSOME_PROVIDER` and `RIBOSOME_MODEL` for the example scripts. In host JSON, use `request.provider` and `request.model`. The model identifies an entry in the pinned Pi catalogue.

OpenAI, Anthropic, and Azure OpenAI Responses use the environment keys documented in [`.env.example`](../../../.env.example). Azure also maps the catalogue model to your local deployment. Keep deployment names and endpoint settings in your private environment.

```sh
npm run provider:check
```

This validates the presence of required provider settings without making a model call.

## Permissions and budgets

`grant.paths` permits reads. `writable_paths` restricts writes; omitting it uses `paths` for both. `required_checks` names checks the host must run before applying a proposed change.

Budgets cover model calls, tokens, local cost accounting, actions, follow-up work, and deadlines. Child runs, compaction, and metered studies share their parent's allowance. An unknown dispatched call keeps its reservation until its usage can be resolved. [Runtime and storage](../guides/runtime.md) explains scheduling and external-agent accounting.

For full field shapes, read the [canonical schema](../../../contracts/schema.json), [CLI configuration](../../../crates/ribosome-cli/src/main.rs), and [protocol reference](../../protocol.md).
