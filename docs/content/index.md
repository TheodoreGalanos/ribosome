---
title: "Introduction"
description: "Inspect and repair agent work, retain project knowledge, and test reusable behavior."
---

# Ribosome

Ribosome adds maintenance agents to your workflows. They inspect results, investigate problems, help repair artifacts, and prepare useful procedures for reuse.

Use it during execution, at a handoff, or after a run. Your application keeps its agent loop, tools, and acceptance rules.

## Start here

| You want to… | Read |
| --- | --- |
| See a working example | [Quickstart](start/quickstart.md) |
| Connect your own agent | [Connect an agent](guides/connect.md) |
| Understand the design | [How it works](core/how-it-works.md) |

## What it does

**Observe.** Receive findings linked to the work that produced them.

**Repair.** Give a maintenance agent permission to change selected artifacts and run your checks.

**Remember.** Save project knowledge with its evidence and conditions for use. Retrieve it in later work and withdraw it when its sources change.

**Learn and reuse.** Investigate behavior in past runs, prepare executable instructions, and test them on fresh tasks. Compare the result with an ordinary agent, retries, critique, or caretaker help.

## Get the library

Ribosome is an early-stage local library built with TypeScript/Pi and Rust. Install it from source. Model calls use your configured provider.

Start with the [quickstart](start/quickstart.md), then [connect your agent](guides/connect.md). [Current support](evaluation/support.md) records what has been tested.
