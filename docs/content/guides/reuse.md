---
title: "Reuse a procedure"
description: "Prepare instructions, test their behavior, and execute them on fresh inputs."
---

# Reuse a procedure

Ribosome can turn a candidate behavior into prepared instructions that a Pi agent executes on a receiving task. The implementation carries its requirements, bindings, and limitations so the application can decide where to use it.

## Prepare and evaluate

| Stage | Result |
| --- | --- |
| Inspect source evidence | A candidate behavior and its grounding. |
| Extract an implementation | Executable instructions or a host-registered procedure. |
| Evaluate | Fresh task results from a host-controlled comparison. |
| Admit | Acceptance of that implementation version in a stated context. |
| Reuse | A new run with recipient inputs, actions, and checks. |

Use `extraction@1` to prepare behavior from a reviewed investigation. An instruction implementation uses `format: "instructions"`. Your host then schedules `execute-motif@1` with the selected implementation version and recipient bindings. Host-registered procedures use the existing action execution path.

## Find an implementation

The evidence inventory retains candidates, failed attempts, and evaluations. Query the usable inventory to find implementations admitted for the caller's context.

Search supports text relevance and exact function filters. `eligible: true` filters by available capabilities; `inventory: "usable"` also requires contextual admission. The receiving agent still checks the implementation's assumptions against the current task.

## Execute on recipient inputs

The prepared run receives the selected definition, implementation, memory, admission, and explicit recipient evidence. It reads current inputs and obtains new action and check receipts. Source relationships remain available for withdrawal checks, while the recipient's readable material is limited to the prepared selection and its own work.

Production invocation requires contextual admission. For an experiment, the host can authorize a sandbox run of a candidate before admission. Schedule additional investigation or another prepared invocation as a separately scoped run.

## Try it

After building from the [quickstart](../start/quickstart.md), prepare a learning trial:

```sh
node examples/learned-behavior/demo.mjs --prepare .ribosome/learning-trial
```

Preparation makes no model calls. With provider settings in `.env`, start the live attempt:

```sh
RIBOSOME_CLI="$PWD/target/debug/ribosome" node --env-file=.env \
  examples/learned-behavior/demo.mjs --run .ribosome/learning-trial
```

The full path shares a ceiling of 160 calls and US$1 across discovery, extraction, and four planned recipient evaluations. Provider charges can differ. Read `report.json` in the trial directory to see which stages completed. Each new trial needs a fresh directory.

The [example guide](../../../examples/learned-behavior/README.md) also explains how to copy this driver into an installed consumer. [Experiments](../evaluation/experiments.md) explains how to assess the candidate, and [Current support](../evaluation/support.md) records the observed results.
