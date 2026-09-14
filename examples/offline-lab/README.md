# Experiments with external agent records

Import engineering and software-agent records, assign evidence to a caretaker or curator, and inspect the saved findings. A supported discovery can become a prepared instruction for fresh local tasks.

The pilot uses six AEC-Bench episodes and six Nebius episodes. It is closed with [recorded results](expanded-results.md) for discovery, contrast, extraction and an eight-execution function study. The candidate preserved the already-correct incompatible result in both repetitions but failed both applicable path-resolution tasks. It was not admitted. E6, the whole-system comparison, is deferred after the candidate's negative function result. The [first pilot](results.md) retains the earlier attempts under its smaller budget.

## Import the sample

Start with the [YAML profile guide](profiles/README.md) to choose a source, map its fields and inspect decoded tool calls. It includes a local example and explains how tool information reaches curation and review.

Run from the repository root:

```sh
cargo build -p ribosome-import --features remote --locked
cargo build -p ribosome-cli --locked
npm run build
target/debug/ribosome-import prepare examples/offline-lab/profiles/aec-release.yaml .ribosome/offline-lab/aec
target/debug/ribosome-import prepare examples/offline-lab/profiles/nebius-openhands.yaml .ribosome/offline-lab/nebius
node examples/offline-lab/lab.mjs prepare .ribosome/offline-lab
```

These commands make no model calls. The importer resolves the dataset revision, reads the required Parquet ranges, joins tables, and saves six episodes from each source. Repeating acquisition in the same directory uses its saved revision. Local and remote acquisition reject a changed profile before reading sources or writing caches. Use a new acquisition directory for a changed mapping. Reuse also checks task, family, metadata and annotations alongside decoded evidence.

For local JSON, JSONL or gzip-compressed JSONL, build without `--features remote`. Local Parquet also uses the remote feature because that feature contains the Parquet dependency. The [local profile](profiles/local-chat.yaml) and [encoded-message profile](profiles/local-encoded-chat.yaml) are runnable examples.

Save limits in a JSON file and pass its path as the final argument to `prepare`. For example, `limits.json` can contain:

```json
{
  "episodes": 6,
  "row_bytes": 16777216,
  "messages": 4000,
  "table_rows": 100000,
  "download_bytes": 268435456
}
```

```sh
target/debug/ribosome-import prepare examples/offline-lab/profiles/local-chat.yaml .ribosome/profile-example limits.json
```

The importer examines the first `episodes` base rows in file order. Quarantine and duplicate rows count toward that limit. It reports fewer accepted episodes when some rows fail. Dimension tables must fit `table_rows`; required join rows are never silently dropped. The download limit covers aggregate acquired bytes, and also bounds decoded JSONL/Parquet data per file.

## Read what was imported

| File | Contents |
| --- | --- |
| `aec/source-lock.json`, `nebius/source-lock.json` | Source revision, concrete files and cached ranges, selected source IDs, profile and decoder versions. |
| `aec/episodes/`, `nebius/episodes/` | Decoded messages, coverage, owner metadata, publisher annotations and raw joined rows. |
| `coverage.json` | Message counts, paired and pending calls, timestamps and supported modalities. |
| `study.json` | Episode splits and the exact message windows assigned to each investigation. |
| `assignments/` | Registered corpus references and the owner configuration. |
| `state/ribosome.db` | Ribosome's ordinary events, records, runs, usage and source links. |
| `workspace/evidence/` | Message parts that an assigned agent can read. |
| `report.json`, `private-*.json` | Attempt status, saved record IDs, budget totals and private diagnostics. |

Keep this directory private. Publisher outcomes and final-output sidecars remain in the owner episode files. A source system message is quoted evidence. Imported tool responses describe what the source recorded; new Ribosome executions have their own receipts.

Large messages have a preview and ordered `message_parts`. Read the parts before claiming that a message omits a result. Tool results may themselves contain excerpts produced by the original harness; the importer preserves those limitations.

An AEC format header is excluded from the message sequence. Missing, null, empty or header-only trajectories can use the configured conversation fallback. Malformed nonempty trajectories are quarantined. Missing tool results remain pending, and unsupported content stays owner-side with a coverage limitation.

## Assign and review evidence

The supplied preparation creates audit, discovery, challenge and prefix assignments for each source. Discovery uses the first three episodes and challenge uses the last episode. All twelve are development material. Repeated tasks and source families must stay in one split. This small sample is useful for integration work; select a broader cohort before estimating performance.

Run a bounded stage using your private provider environment:

```sh
node --env-file-if-exists=.env examples/offline-lab/lab.mjs run .ribosome/offline-lab aec-audit
node --env-file-if-exists=.env examples/offline-lab/lab.mjs run .ribosome/offline-lab nebius-discovery
node --env-file-if-exists=.env examples/offline-lab/lab.mjs run .ribosome/offline-lab aec-prefix
node --env-file-if-exists=.env examples/offline-lab/lab.mjs run .ribosome/offline-lab aec-transcript
node examples/offline-lab/lab.mjs inspect .ribosome/offline-lab
```

Each pilot shares 100 model calls, four million tokens and US$1 at the configured catalogue rates, with a 24-hour deadline. Audit and prefix runs have a 12-call cap; discovery has a 28-call cap. Compaction counts toward these allowances. `run DIRECTORY STAGE [QUESTION.md]` and `retry DIRECTORY STAGE [QUESTION.md]` can append an owner investigation question. `retry` records another attempt against the same remaining allowance. An interrupted request may retain an unresolved cost reservation.

For a larger campaign, put a complete `run_budget` in `assignments/owner-config.json`; the stage commands use it for their cap. Fund the campaign with an owner grant carrying the desired total allowance and a new grant ID. Keep earlier reports and usage in the same database. The granted total covers all investigations and recipient runs, while `run_budget` limits one investigation. A budget limit ending an investigation is an experiment outcome.

Set `model_max_output_tokens` in that config when individual responses need more room. For example, `16384` permits longer structured records and continuation summaries. The host still reserves and accounts for those tokens under the shared budget.

Inspect both the run disposition and its records. A completed summary without a saved discovery investigation is reported as incomplete. A saved `no_motif` investigation is a valid outcome. Read its reasoning and evidence before drawing a conclusion about the source.

The `-transcript` condition supplies the audit's selected messages directly in the prompt. It uses the same caretaker, tools, corpus access and configured run budget, so the comparison tests evidence presentation and retrieval. Requests are limited to 65,536 characters; larger transcripts require a smaller audit window applied to both conditions. Compare the saved findings for support, omissions, uncertainty and usage. The first pilot exhausted its shared allowance before running this control.

For your own study, write a `StudyManifest` and use:

```sh
target/debug/ribosome-import assign STUDY.json OWNER_CONFIG.json OUTPUT
```

Windows use zero-based message indexes and an exclusive end. Online windows start at zero. The importer splits long windows into groups of 128 events. Each assignment must fit the existing limits of 1,000 messages, 64 windows, 100 snapshots and a 256 KiB corpus body. Use several assignments for larger cohorts. An early prefix receives only its selected message parts.

## Challenge, extract and retrieve

These commands require a supported investigation with saved occurrences:

```sh
node --env-file-if-exists=.env examples/offline-lab/lab.mjs contrast DIRECTORY nebius
node --env-file-if-exists=.env examples/offline-lab/lab.mjs extract DIRECTORY nebius CONTRACT.json
node examples/offline-lab/lab.mjs retrieve DIRECTORY nebius QUERIES.json
```

Contrast gives a fresh curator the selected definitions and a separate evidence window. A different episode can test transfer; a later window from the same episode is a dependent development contrast. It rejects overlapping discovery and challenge events. Extraction reads the actual investigation and accepts an owner-specified `InstructionContract`. The script fills its `discovery_refs`; the curator supplies the policy and actual motif references. Define the input bindings and output obligations from the discovered function.

`QUERIES.json` is an array of query strings. Use alternate wording and a poor-fit query. Retrieval saves matching implementation IDs for inspection. Lexical matches still need a recipient compatibility decision. A bounded agent can reformulate a request and inspect promising records using the same search tools:

```sh
node --env-file-if-exists=.env examples/offline-lab/lab.mjs retrieve-agent DIRECTORY nebius QUERIES.json
```

This probe accepts at most three questions. Each has a 12-call, US$0.50 cap within the existing owner grant. The prompt allows three reformulated searches, then asks for a memory containing the selected definition and implementation, applicability and remaining uncertainty. Read `retrieval-agent-nebius.json` and the saved run to inspect the actual search and selection.

## Test the instruction on fresh tasks

Create a study plan after inspecting the extracted instruction. It uses the existing `AgentEvaluator`, with `ImplementationInvocation` for experimental subject execution:

```sh
node --env-file-if-exists=.env examples/offline-lab/lab.mjs study DIRECTORY PLAN.json
```

The plan has these fields:

| Field | Owner input |
| --- | --- |
| `owner_config` | Path relative to the plan, pointing to a sandbox host config using this lab's database and an explicitly bounded grant. |
| `objective` | `function` or `system_benefit`. |
| `name` | Optional lowercase study name, such as `explicit-contract`, to retain another attempt in its own report. |
| `candidate` | Saved implementation `{id, version}`. |
| `hypothesis` | The claim being tested. |
| `cases` | Existing `EvaluationCase` objects, each separating `input.subject` from `input.oracle`. |
| `evaluator` | Existing `AgentEvaluatorConfig` fields for `paths`, `writable_paths`, `tools` and an independent command `judge`. Provider/model fields come from the environment. |
| `policy` | `required_checks`, `metric`, `min_quality`, `min_improvement` and `allowed_cells`. |
| `case_budget` | A complete `Budget` shared by all stages of one case. |

See the [laboratory configuration](../../docs/protocol.md#whole-agent-laboratory) for the existing types. Cases should use fresh local data and include an applicable task plus a benign or incompatible task. The original dataset environment remains an offline source of observations.

A two-case function study runs baseline and candidate twice: eight planned executions. The system comparison requires an accepted function result for the same candidate and at least two recipient families. It compares ordinary execution, retry, critique, care and candidate maintenance. Every arm starts with the same worker, and subsequent stages share the case allowance. Both studies keep incomplete executions in their planned counts.

For the system comparison, preparation cost includes all preceding usage in this lab database, including development investigations and the function study. The example assigns that cost to one reuse and carries forward any unknown usage.

The script saves the experiment and private CLI configuration before execution. `function-study.json` and `system_benefit-study.json` retain the result. To inspect or resume an interrupted study, use that saved config and experiment ID with `ribosome study`. Starting a new invocation of the example does not silently replace an existing study report. To run another development comparison, save a new plan with a distinct `name`; for example, `explicit-contract` writes `function-explicit-contract-study.json`. The new run spends from the same configured grant. A named system comparison reads the function report with the same name.

The expanded pilot found an EDK2 path procedure. After inspecting an extracted candidate, prepare its fresh recipient cases with:

```sh
node examples/offline-lab/path-study.mjs prepare DIRECTORY CANDIDATE_ID
node --env-file-if-exists=.env examples/offline-lab/lab.mjs study DIRECTORY DIRECTORY/path-function-plan.json
# After an accepted function study:
node --env-file-if-exists=.env examples/offline-lab/lab.mjs study DIRECTORY DIRECTORY/path-system-plan.json
```

The applicable case supplies a directory inventory and asks for package-relative and workspace-relative paths. It includes an unrelated path with a similar prefix. The incompatible case supplies a URL request and an already correct result. Both conditions receive the same declared output shape and path semantics. The independent judge reports interface compliance, functional correctness, preserved metadata and appropriate intervention separately. If the output shape prevents assessment of the mappings, functional correctness remains unassessed. It also checks whether the agent leaves the correct result in place. These cases measure interpretation of supplied inventory evidence and result editing.

The expanded pilot completed the function study. Its candidate failed both path-resolution repetitions, so the system comparison is deferred for a later candidate with demonstrated local function. The case-level results explain which answers failed and which independent work was preserved.

## Investigate an agent strategy

The [behavior follow-up](behavior-results.md) selects an actual decision, failed attempts and subsequent checks from the imported `numpy__numpydoc-101` execution. It asks the curator to identify what the agent did. The later challenge is from the same task and tests a nearby boundary.

```sh
node examples/offline-lab/behavior-study.mjs prepare .ribosome/behavior-study .ribosome/offline-lab/nebius
node --env-file-if-exists=.env examples/offline-lab/lab.mjs run .ribosome/behavior-study nebius-discovery examples/offline-lab/prompts/agent-strategy.md
node examples/offline-lab/lab.mjs inspect .ribosome/behavior-study
node --env-file-if-exists=.env examples/offline-lab/lab.mjs contrast .ribosome/behavior-study nebius
node examples/offline-lab/warning-study.mjs contract .ribosome/behavior-study/warning-contract.json
node --env-file-if-exists=.env examples/offline-lab/lab.mjs extract .ribosome/behavior-study nebius .ribosome/behavior-study/warning-contract.json
```

Inspect the saved discovery and candidate before each next stage. A definition without a completed occurrence and investigation remains incomplete. Instructions may link to a definition, occurrence or investigation; this lab also requires a completed supporting investigation.

The fresh task repairs a small warning configuration. The recipient reads the actual warning routes and wrapper fields, observes diagnostic output through `warning-check`, changes the configuration if needed, and checks again. The checker interprets JSON with fixed code. The paired task starts with a correct context-free configuration. Both conditions receive the same explicit interface and required diagnostic behaviour.

```sh
node examples/offline-lab/warning-study.mjs prepare .ribosome/behavior-study CANDIDATE_ID
node --env-file-if-exists=.env examples/offline-lab/lab.mjs study .ribosome/behavior-study .ribosome/behavior-study/warning-function-plan.json
```

Preparation starts with a 500-call, US$5 development allowance and 80 calls per investigation. Finish discovery, contrast and extraction before preparing the function study. That preparation creates a tool-bearing grant with the remaining calls, tokens and cost, including outstanding provider reservations. Continue with that study grant; further investigations need a separately planned allowance. Each of the eight recipient executions has a 40-call, US$0.40 cap within the study total.

This task tests applying a strategy to a local configuration repair. It gives us observable diagnosis, edits and checks. Broader source-code repair and whole-system benefit require further experiments.

## Withdraw a source

```sh
target/debug/ribosome-import withdraw STUDY.json OWNER_CONFIG.json nebius episode-0000
```

Withdrawal retires the source record and removes its generated workspace message parts. Existing source tracking then excludes dependent observations, records, summaries and saved context. Owner acquisition files remain available for inspection. Reimporting that retired identity reports a conflict.

The focused continuation check can use an acquired cohort:

```sh
RIBOSOME_OFFLINE_COHORT=.ribosome/offline-lab/nebius node --test tests/integration/offline-lab.test.mjs
```

It copies evidence into a source-linked memory, interrupts a Pi execution, withdraws the source, resumes, and inspects two subsequent provider requests. The provider transport is scripted, so this check makes no paid model calls. Without the environment variable it uses the committed synthetic fixture.

## Sources and reuse

- [AEC-Bench release rollouts](https://huggingface.co/datasets/aec-bench/release-model-rollouts): engineering task, rollout and artifact tables. The inspected card did not declare a licence; inspect current terms before redistributing source rows.
- [Nebius SWE-rebench trajectories](https://huggingface.co/datasets/nebius/SWE-rebench-openhands-trajectories): software-agent interactions, published under CC-BY-4.0. Preserve source attribution when sharing derived material.

The checked-in fixtures are synthetic. Raw public-dataset rows, provider settings and episode outputs stay in the ignored local directory.
