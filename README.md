# Ribosome

**A companion library for maintaining, repairing, and reusing agent work.**

Ribosome puts maintenance agents alongside your existing agents and workflows. They inspect results, investigate problems, help repair artifacts, and preserve project knowledge and useful procedures for later work.

Your application supplies its agents, tools, and definition of success. You choose when maintenance runs, which evidence it can read, and what it may change.

[Read the documentation](docs/README.md) · [Quickstart](docs/content/start/quickstart.md) · [Connect an agent](docs/content/guides/connect.md)

## See it work

The attachment example starts with a report that adds **1 metre and 200 centimetres** and presents the result as **201 metres**.

Ribosome observes the application-owned agent and investigates the report. When the application hands over control of writes, a caretaker can repair a working copy, run the required checks, and apply the corrected result: **3 metres**. The original measurements and independent cost analysis are preserved, and the application's agent continues.

Use the Node and Rust versions pinned in [`.node-version`](.node-version) and [`rust-toolchain.toml`](rust-toolchain.toml):

```sh
git clone https://github.com/TheodoreGalanos/ribosome.git
cd ribosome
npm ci --ignore-scripts
cargo build --workspace --locked
npm run build
```

Create a private root `.env` using the provider settings in [`.env.example`](.env.example), then run:

```sh
npm run provider:check
npm run demo:attached
```

The configuration check makes no model call. The demo uses your model provider with local allowances of US$0.40 for the source agent and US$0.60 for maintenance. Provider charges can differ. Read the generated `result.json`, artifact, and check receipts to assess the result.

Use `npm run demo:attached -- --mid-run` to try attachment during execution. The [quickstart](docs/content/start/quickstart.md) explains setup and output. `npm run check` runs the full automated suite separately.

## What you can build

**Maintenance beside an agent.** Connect a Pi agent at startup or during execution, or send events from another harness through the generic interface. Receive findings and optionally steer the agent at its next boundary.

**Checked repairs.** Permit maintenance to edit selected files, require application checks, and coordinate shared writes. Recorded action outcomes let interrupted work resume; fresh checks establish which results are valid after a change.

**Project memory.** Save episodes, failure knowledge, temporary conditions, and procedural guidance with their evidence and applicability. Later runs can search that knowledge. Source withdrawal is checked before subsequent model requests, including resumed work.

**Behavior discovery and reuse.** Ask a curator to investigate selected execution evidence and describe a useful local function. Prepare instructions, evaluate them, and execute an admitted version on recipient inputs with fresh tool observations and checks.

**Whole-agent experiments.** Compare an ordinary agent, retries, critique, caretaker help, and prepared behavior on host-owned cases. Record task success, cost, incomplete cases, and the context in which a candidate is accepted for reuse.

## How it runs

The TypeScript package uses [Pi agent-core](https://github.com/earendil-works/pi/tree/main/packages/agent) for maintenance reasoning and model calls. Rust owns local SQLite state, retrieval, work scheduling, permissions, effect execution, recovery, and shared budgets.

Three profiles cover the work: a **caretaker** investigates and repairs artifacts; a **curator** investigates behavior and consolidates memory; an **experimenter** proposes studies and interprets their results.

Start a run explicitly, connect an event subscription, or let an active maintenance agent request permitted follow-up work. Your application controls how discovery, extraction, evaluation, and reuse are connected. [Runtime and storage](docs/content/guides/runtime.md) explains the process and persistence model.

## Status

Ribosome is an early-stage local library installed from source. The R1–R7 implementation includes installed-consumer checks for attachment, recovery, memory withdrawal, and prepared execution.

Live learning trials have produced mixed and incomplete results. The whole-agent development comparison remained inconclusive, and the installed learning example stopped at discovery without an executable candidate. General learned transfer and a benefit over the controls remain open evaluation questions. [Current support](docs/content/evaluation/support.md) summarizes the evidence, with details in [validation](docs/validation.md).

Use trusted registered tools and meaningful task checks. Shared-file repairs require cooperating writers. For untrusted code, provide an appropriately isolated executor.

## Documentation

| Guide | What it helps you do |
| --- | --- |
| [Connect an agent](docs/content/guides/connect.md) | Send events, handle findings, and integrate another harness. |
| [Runtime and storage](docs/content/guides/runtime.md) | Start maintenance, persist work, and share resources. |
| [Repair an artifact](docs/content/guides/repair.md) | Coordinate writes and establish whether a repair worked. |
| [Discover a behavior](docs/content/guides/discovery.md) | Supply evidence and inspect a curator's investigation. |
| [Reuse a procedure](docs/content/guides/reuse.md) | Prepare and invoke behavior on a new task. |
| [Project memory](docs/content/guides/memory.md) | Save, retrieve, revise, and withdraw knowledge. |
| [Experiments](docs/content/evaluation/experiments.md) | Test a function or compare complete agent workflows. |
| [Detailed references](docs/content/reference/further-reading.md) | Find contracts, operating procedures, and validation evidence. |

To browse the documentation locally, open [docs/index.html](docs/index.html). To edit it, change the Markdown pages and run `npm run docs:build`. The [documentation index](docs/README.md) also works directly on GitHub.

The design grew out of agent evaluation work in AEC-Bench and draws on biological proofreading, repair, and adaptation. [Biological inspiration](docs/content/core/inspiration.md) explains those origins and their use in the library.
