# Ribosome

**Maintenance and learning for agent workflows.**

Ribosome adds agents that investigate problems, help repair results, and prepare useful behavior for testing and reuse. They work alongside your existing agents, at a handoff, or over the evidence from a completed run.

Your application keeps its agent loop, tools, and definition of success. You choose what Ribosome can inspect and change.

[Documentation](docs/README.md) · [Quickstart](docs/content/start/quickstart.md) · [Connect an agent](docs/content/guides/connect.md)

## What you can do

**Investigate and repair.** A report changes after validation. Two workers produce incompatible results. Ribosome can inspect the evidence, identify what needs attention, and attempt a checked repair without discarding independent work. Start with findings; enable changes where your application can coordinate them. [Repair guide →](docs/content/guides/repair.md)

**Learn from executions.** A failed run can contain a useful procedure; a successful run can contain unnecessary work. Curator agents investigate these behaviors, compare explanations and counterexamples, and prepare reusable instructions. A behavioral motif describes the function—not just a sequence of tool calls. [Discovery guide →](docs/content/guides/discovery.md)

**Remember, test, and reuse.** Keep project knowledge with its supporting evidence. Run prepared instructions on new inputs, and compare candidate behavior with ordinary execution, retries, or critique. A promising example becomes a candidate to evaluate, not an automatic rule for every future task. [Memory](docs/content/guides/memory.md) · [Reuse](docs/content/guides/reuse.md) · [Experiments](docs/content/evaluation/experiments.md)

## Why “Ribosome”?

The name comes from the cellular machinery that assembles proteins. The broader inspiration is biological maintenance: proofreading, repairing damaged material, reusing useful components, and adapting to changing conditions.

The project grew out of [AEC-Bench](https://github.com/TheodoreGalanos/aec-bench) and runs independently of it. Read more about the [biological inspiration](docs/content/core/inspiration.md).

## Where it fits

Use Ribosome with workflows that produce inspectable work: code changes, calculations, reports, or other artifacts with meaningful checks.

The included connector works with an application-owned **Pi agent**. Other harnesses can supply observations through the generic event interface. Observation, steering, and coordinated repair are separate choices; connecting an agent does not hand over control of its workspace.

Ribosome uses **TypeScript and Pi** for its agents, and **Rust** for storage, retrieval, communication, and controlled execution. State stays local; model calls use your configured provider. See [How it works](docs/content/core/how-it-works.md).

## Try it

Follow the [quickstart](docs/content/start/quickstart.md) to build from source and configure a model provider, then run:

```sh
npm run provider:check
npm run demo:attached
```

The example observes an existing agent, investigates an incorrect report, and attempts a checked repair while preserving the original inputs. **The demo makes paid model calls.**

For your own workflow, start with [Connect an agent](docs/content/guides/connect.md).

## Status

Ribosome is an early-stage library, available from source. Behavioral discovery, prepared instruction execution, and whole-agent comparisons are implemented; live learning results remain mixed and incomplete. Useful transfer and improvements over simpler workflows are still being evaluated. See [Current support](docs/content/evaluation/support.md).

Begin with observation and limited permissions. Shared-file repairs need cooperating writers, and untrusted code needs an isolated executor.
