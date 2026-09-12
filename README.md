# Ribosome

**A companion library for maintaining, repairing, and reusing agentic work.**

Ribosome puts tool-using maintenance agents alongside your existing agents and workflows. They inspect what happened, investigate suspected mistakes, help repair work within the limits you set, and preserve useful procedures and project knowledge for later use.

Your application keeps its agents, tools, and definition of success. Ribosome helps maintain the work they produce as it changes and passes between workflows.

The project is an early-stage local library built with TypeScript, Pi, and Rust. State is kept locally; model calls go to the provider you configure. The packages are currently installed from this repository rather than a published npm or crates.io release.

## Why Ribosome?

An agent checks a report, then changes it before handing it off. Two workers produce results using different assumptions. A failed task contains a useful procedure worth keeping. A later change to the source makes part of yesterday's answer unreliable.

Ribosome helps investigate these problems, establish what remains valid, and repair the affected work. It can also preserve a useful procedure from an execution for evaluation and later reuse.

### A concrete example

The included attachment demonstration starts with a report that adds **1 metre and 200 centimetres** and presents the result as **201 metres**.

Ribosome observes the application-owned agent, investigates the report, and returns a finding. When the application explicitly hands over control of its writes, a maintenance agent can repair a separate copy, run the required checks, and apply the corrected result: **3 metres**. The original measurements and an independent cost analysis are preserved, and the application's agent can continue.

See the [attachment example](examples/attached-agent/demo.mjs) and [integration guide](docs/attachments.md).

## Get started

### Build and check

Use the Node and Rust versions pinned in [`.node-version`](.node-version) and [`rust-toolchain.toml`](rust-toolchain.toml), with npm and Rust tooling installed.

```sh
git clone https://github.com/TheodoreGalanos/ribosome.git
cd ribosome
npm ci --ignore-scripts
npm run check
```

This builds the components and runs automated checks without paid model calls.

### Try it beside an agent

Create a root `.env` with your provider configuration. For OpenAI, replace these placeholders with a model ID available in the pinned Pi catalogue and your API key:

```dotenv
RIBOSOME_PROVIDER=openai
RIBOSOME_MODEL=your-model-id
OPENAI_API_KEY=your-api-key
```

Anthropic and Azure OpenAI Responses are also supported. [`.env.example`](.env.example) documents the Azure settings. Do not commit credentials.

```sh
# Check that the required configuration is present; no model call is made.
npm run provider:check

# Observe an application-owned Pi agent and demonstrate cooperative repair.
npm run demo:attached

# Attach while the source agent is already running.
npm run demo:attached -- --mid-run
```

These demos make paid model calls, with a **US$1 local budget per run**. Provider charges may differ.

Other [examples and evaluation cases](tests/evaluations/README.md) cover failed-run procedure reuse, scoped memory, source changes, and situations where a repair should not proceed.

## What it does today

### Understand problems in context

A maintenance agent can inspect execution events, read the artifacts it has access to, request additional evidence, and distinguish a suspected problem from a supported finding. Findings retain evidence references and uncertainty so the application can decide how to respond.

For example, a check that has not happened yet may be normal during execution and a problem at handoff.

### Help repair work without discarding everything

With the appropriate permissions, Ribosome can work in a separate copy of the permitted files, edit the affected material, run registered checks, and request that the checked changes be applied to the live workspace.

The application controls what can change and which checks are mandatory. Observation alone does not grant permission to edit. Shared-workspace repairs require cooperation from the application's writers.

### Preserve useful behavior from both successful and failed runs

Curator agents can propose **behavioral motifs**: meaningful patterns such as verifying an artifact before handoff or reconciling assumptions before combining results.

Ribosome distinguishes a motif's description, an observed occurrence, and a reusable implementation. That lets a useful local procedure survive even when the enclosing task failed for another reason.

The current implementation supports agent-authored motif records, reusable instruction records, and registered procedures. Its end-to-end reuse demonstration evaluates and reuses host-registered procedures. It does **not yet establish general discovery and transfer of newly learned agent policies**.

### Carry project knowledge between workflows

Ribosome stores and retrieves memory within configured client and project scopes. Memories can describe an episode, a temporary condition, a recurring failure, a tentative generalization, or a candidate or evaluated procedure.

The point is to distinguish “this happened once” from “we have evidence that this is useful here.” An agent consolidates that knowledge; storage and search make it available to subsequent work within its allowed scope.

### Test a candidate before accepting it for reuse

The laboratory compares a candidate with a baseline using evaluators and acceptance criteria set by the application. A candidate must meet those criteria before approval for reuse, and each approval records the context in which reuse is supported.

The current examples test concrete procedures. They demonstrate the evaluation and reuse path, not a proven advantage over giving an existing agent more retries or a critique-and-revise loop. The [validation notes](docs/validation.md) describe the tests and their limitations.

## Where it fits

Ribosome is intended for developers building agent applications with inspectable work: files, reports, calculations, code changes, or other artifacts that can be checked against explicit requirements. It is especially relevant when several agents contribute to one result, when work changes after validation, or when workflows share a project over time.

There are three ways to use it:

**Alongside an existing workflow.** Connect execution observations and receive findings while your application keeps its own agent loop. Steering and cooperative repair are optional.

**At an application-controlled boundary.** Ask for maintenance around a handoff, a completed artifact, or another point where the application needs stronger evidence before proceeding.

**After an execution.** Use retained evidence for motif annotation, procedure preparation, memory consolidation, or experiments where an evaluator is available.

The included adapter connects to an application-owned **Pi agent** at startup or during execution. Other frameworks can use the generic event and feedback interface. They need to supply the relevant observations and, for repair, artifact access and coordinated control. Ribosome does not attach itself to an arbitrary running process or recover history that was never supplied.

## How it works

Ribosome includes three maintenance profiles: a **caretaker** for investigation and repair, a **curator** for behavioral material and memory, and an **experimenter** for candidate comparisons. They share tools; the application chooses which profiles to use.

The TypeScript package uses [Pi agent-core](https://github.com/earendil-works/pi/tree/main/packages/agent) for the agent loop. Rust stores records, retrieves information within its allowed scope, manages communication, and controls tool execution. Your application remains responsible for its own task and acceptance requirements.

Agents decide what to investigate and try. The application determines what they may do. Recorded execution results show what actually happened.

The local setup does not require a broker, external database, hosted control service, Python runtime, or AEC-Bench installation.

## Connect your own application

With the packages installed, a configured Rust host, and an existing Pi `agent` and `task`, the observation integration looks like this:

```ts
import { AttachmentClient } from '@ribosome/agents/attachments';
import { attachPi } from '@ribosome/agents/attachments/pi';

const client = await AttachmentClient.start({
  executable: '/absolute/path/to/ribosome',
  config: '/absolute/path/to/host.json',
});

try {
  const attachment = await attachPi(client, agent, {
    executionId: task.id,
    onFeedback: feedback => console.log(feedback),
    onError: error => console.error('Ribosome observation failed:', error),
  });

  await agent.prompt(task.prompt);
  await attachment.finish();
} finally {
  await client.close();
}
```

Feedback includes references to the findings; `attachment.readRecord(id)` retrieves their contents. The [integration guide](docs/attachments.md) covers host configuration, artifact observations, content capture, other frameworks, and optional steering and repair. Supplying task-relevant evidence is part of the integration; a connection alone does not make the whole workspace visible.

<details>
<summary>Install the current build into another project</summary>

From this checkout, after building, use an existing absolute consumer directory:

```sh
cargo install --path crates/ribosome-cli --locked --root /path/to/consumer
npm pack --workspace @ribosome/agents --pack-destination /path/to/consumer
npm install --prefix /path/to/consumer /path/to/consumer/ribosome-agents-0.1.0.tgz
```

The executable is `/path/to/consumer/bin/ribosome`. In the consumer's host configuration, point `node` to the Node executable and `worker` to the installed `@ribosome/agents/worker` export. Resolve the latter with `import.meta.resolve('@ribosome/agents/worker')` and convert the file URL to a path.

`ribosome init` supplies a starting configuration, but its checkout-relative worker path must be changed for an installed package. Set the workspace, state location, model, permissions, and registered tools for your application before running.

</details>

## Where the idea came from

The name comes from [ribosomes](https://www.genome.gov/genetics-glossary/Ribosome), the cellular machinery that reads messenger RNA and assembles proteins. The broader design borrows from biological proofreading, repair, and adaptation. These ideas guide the design; the [specification](docs/specification.md) describes the proposed capabilities.

An agent trajectory records behavior in a particular situation. A useful procedure can be extracted from that experience, but whether it works elsewhere still needs to be established.

The design grew out of work on agent evaluation and evolution in [AEC-Bench](https://github.com/TheodoreGalanos/aec-bench). Ribosome is independent of that project and can be used across domains. Its inventory also draws on quality-diversity ideas such as [MAP-Elites](https://arxiv.org/abs/1504.04909): keep useful alternatives for different conditions.

## Current scope and limits

Ribosome is an **early-stage local library** that requires application oversight.

The reference implementation has automated cross-language and installed-consumer tests, plus documented live development demonstrations. Those demonstrations show specific outcomes on seeded cases. These results do not establish general reliability.

The local host runs trusted, explicitly registered tools. Copied workspaces, permission checks, and separate processes are **not an OS sandbox**. Executing hostile generated code requires an appropriately isolated host adapter. Coordinated repair also requires all relevant writers to cooperate; stopping or steering an agent is not the same as locking its workspace.

Applications should inspect findings, actual changes, and check results. Consult [validation](docs/validation.md) for the evidence and limitations, and [operations](docs/operations.md) for interruption, recovery, and storage handling.

## Documentation

| Start here | What you will find |
| --- | --- |
| [Connect an agent](docs/attachments.md) | Pi and generic integrations, feedback, steering, and cooperative repair. |
| [Examples and evaluations](tests/evaluations/README.md) | Runnable cases, configuration, and observable acceptance criteria. |
| [Validation status](docs/validation.md) | Reported results, test coverage, and limits on the claims. |
| [Operation and recovery](docs/operations.md) | Stopping, resuming, inspecting, backing up, and maintaining local state. |
| [Protocol](docs/protocol.md) | Interfaces and contracts for deeper integrations. |
| [Design specification](docs/specification.md) | The broader design and its rationale; not a substitute for implementation status. |
