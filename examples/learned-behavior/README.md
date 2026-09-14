# Discover and test a behavior from an installed consumer

This example uses only the installed `@ribosome/agents` exports and a Rust executable. Copy `demo.mjs` into an npm consumer with the packed package installed. It does not import repository helpers, Python or another agent framework.

```sh
node demo.mjs --prepare ./learning-trial
RIBOSOME_CLI=/absolute/path/to/ribosome node --env-file=.env demo.mjs --run ./learning-trial
```

Configure the provider and model using the existing environment settings. Preparation makes no model calls. The live run has one shared ceiling of **160 calls and US$1**, including discovery, extraction and four planned recipient evaluations. It refuses to overwrite a trial or silently restart a used allowance. A model task that lacks a candidate remains incomplete; an infrastructure failure also exits nonzero. Inspect the retained report before deciding on further work.

Preparation executes file operations for four authored source episodes. The curator receives raw input/result/check observations, including a benign episode and a locally checked result in a globally failed execution. The starting definition inventory is empty. The host supplies no candidate policy or name-based solution selector.

To start from an existing trajectory file or Hugging Face dataset, use the [offline lab's YAML profiles](../offline-lab/profiles/README.md). They preserve recorded messages and tool interactions for assignment to a curator, who can use that evidence to propose a behaviour.

If the curator saves a structured definition, grounded occurrence and investigation, a separate extraction run can save executable instructions. Rust pins that implementation and the ordinary baseline for two recipient cases: supported reconciliation and conflicting evidence where leaving an already unresolved result unchanged is correct. Subject agents use the production Pi worker and granted artifact tools; the offline judge observes actual output and preserved independent work. It cannot repair a result. Related authored recipients remain **development evidence**, not independent protected transfer.

The private trial directory contains the configuration, source observations, records, database, diagnostics and `report.json`. Do not publish it wholesale. The repository's `scripts/qualification-evidence.mjs` selects sanitized result fields for a handoff bundle.

The observed R7 installed trial saved an inconclusive investigation but no candidate. It therefore did not run extraction or recipient evaluation. Seven calls cost US$0.017447 with no unknown usage. This is an attempted capability demonstration, not successful learned transfer. Installed provider fixtures separately verify the invocation, fresh-check and abstention mechanisms.

## Revise and reuse a known instruction

`lifecycle.mjs` follows one instruction from a faulty decision through revision, testing, memory and later use. It starts with an authored rule: when rows share an ID, keep the first value. A conflicting pair exposes the problem.

1. Run two recipient cases against the faulty instruction and an ordinary worker.
2. Ask an experimenter to inspect the evaluation records and save a corrected procedure.
3. Run both cases again and request admission under the host's acceptance policy.
4. Save procedural memory linked to the accepted instruction.
5. Let a fresh agent search memory and the usable inventory, inspect new rows and propose a binding. The host executes that selection through `ImplementationInvocation`.
6. Introduce rows whose equal numbers have different units. Inspect the result. A supported unresolved response or abstention completes reconsideration. A failed check leads to withdrawal of the instruction and its linked memory.

Each recipient works in a branch. The independent checker observes row values and the preserved cost field. Saved records, effects, usage and outcomes remain in the trial directory. Model choices determine whether a live trial reaches each step; `report.json` records where it stopped.

Copy both `lifecycle.mjs` and `demo.mjs` into an installed consumer, or run them from this directory. Configure the provider before preparation:

```sh
node --env-file=.env lifecycle.mjs --prepare ./lifecycle-trial
RIBOSOME_CLI=/absolute/path/to/ribosome node --env-file=.env lifecycle.mjs --run ./lifecycle-trial
```

The trial has one shared ceiling of **400 calls and US$3**, including eight planned evaluations, revision and later use. Preparation sets a one-hour deadline. A used trial keeps its report and allowance; prepare a new directory for a separate experiment.

The repository also checks this complete path with authored provider responses:

```sh
cargo build -p ribosome-cli --locked
npm run build
node --test tests/integration/lifecycle.test.mjs
```

The provider fixture chooses the procedure and tool calls. Real Pi workers and Rust execute the effects, evaluation, admission, retrieval and withdrawal. In this check, the faulty candidate passes one of two cases and the correction passes both. A later recipient succeeds with its cost field preserved. The units counterexample fails; withdrawal removes the instruction and its memory, and Rust refuses a later production invocation. These results verify the library's behavior across the cycle. Live reasoning quality can be measured with the same example by using the configured production worker.
