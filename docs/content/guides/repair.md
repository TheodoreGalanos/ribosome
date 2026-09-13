---
title: "Repair an artifact"
description: "Give maintenance temporary control of selected writes and require checks."
---

# Repair an artifact

A finding is advice. A repair additionally requires artifact access, permitted tools, mandatory checks, and control over competing writes.

## Choose the level of access

| Grant mode | Allowed work |
| --- | --- |
| `observe` | Read evidence and return findings. |
| `sandbox` | Work in a separate copy of permitted files. |
| `apply` | Request live changes through the host's checks and permissions. |

The local `sandbox` mode uses a copied workspace and trusted registered tools. Choose an appropriately isolated executor when your application needs to run untrusted code.

## Coordinate shared writes

Enable `attachment.allow_coordinated_writes` in the host configuration and `coordinatedWrites` when attaching. Use an `apply` grant with `writable_paths`, registered tools, and nonempty `required_checks`.

Wrap **every external writer from the start** in the same coordinator:

```ts
import { WriteCoordinator } from '@ribosome/agents/attachments';

const writer = new WriteCoordinator();

// Inside each application tool that changes the workspace:
await writer.run(() => updateArtifact());

// At an application-controlled repair boundary:
await attachment.finish();
await attachment.repair(writer);
await attachment.finish();
```

`attachment` is your configured connection; `updateArtifact` is an application-owned operation.

## Check the outcome

The caretaker investigates, edits a branch, and requests checked application. The host enforces its mandatory checks even when the proposal omits them.

Inspect the feedback, actual artifact, and check receipts to establish whether the repair succeeded. `repair()` resolves when writer ownership has been settled. Each local application operation applies the selected artifact.

Checks record the artifact version and properties they validate. After an input changes, affected results become stale. A fresh authorized check can restore support for the properties it covers; downstream results remain stale until their own required checks establish support.

## On interruption

A timeout or lost connection leaves the writer stopped. Inspect the handoff and effect receipts before reconciling with the original coordinator. When an action outcome was captured, recovery finishes its bookkeeping without repeating the action. Unknown outcomes require owner inspection and settlement.

See [Operations](../reference/operations.md) for settlement and fresh checks. The reference path supports one coordinated attachment per workspace.
