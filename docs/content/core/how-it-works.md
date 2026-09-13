---
title: "How it works"
description: "Your agent does the task. Ribosome maintains the work around it."
---

# How it works

Your application supplies observations, artifact access, and permission to act. Ribosome supplies maintenance agents that use them.

## The cycle

```text
Observe → investigate → propose or act → check → retain evidence
```

The agent can request more information, try a repair, inspect the result, and change its approach. It can also conclude that no intervention is needed.

## Three profiles

| Profile | Job |
| --- | --- |
| Caretaker | Investigate problems and help repair current work. |
| Curator | Describe useful behavior and consolidate project knowledge. |
| Experimenter | Compare candidate procedures with a baseline. |

Choose the profiles your workflow needs. A caretaker can request a curator's investigation when the grant permits follow-up work.

## Who controls what?

**Your application** defines the task, permitted files and tools, and required checks. It decides whether to accept advice, allow steering, or coordinate a repair.

**TypeScript and Pi** run the maintenance agents. The agents interpret evidence and choose what to investigate or try.

**Rust** stores evidence in SQLite, schedules work, shares budgets between runs, and enforces tool permissions. It records tool outcomes and reconciles interrupted execution.

The Rust host supervises Node workers. They exchange JSON-RPC messages over stdin/stdout. [Runtime and storage](../guides/runtime.md) explains what persists and how work continues after interruption.

## Ways to use it

Attach to an existing workflow for ongoing observations and feedback. Request a repair at an application-controlled boundary. Or run a curator over retained evidence after the task ends.

Pi is the included connector. Other harnesses send events and receive feedback through the generic interface while keeping their own agent loop.

Next: [Connect an agent](../guides/connect.md). For the design's origin, read [Biological inspiration](inspiration.md).
