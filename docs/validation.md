# Validation status

The original local v0.1 qualification passed **73 automated tests** and **13 live cases on the final checked build** using a locally configured Azure deployment: all twelve original cases plus the generated-development study. All three first-release demonstrations passed, including an independently installed Rust/npm host. This establishes the listed outcomes for these runs; it is not a general reliability or production-readiness claim. These results were collected before repository publication.

## Agent-harness attachment delivery

P1–P3 and attachment-focused P6 are implemented. The final `npm run check` passed **99 tests: 55 Rust and 44 Node**, plus generated-contract checking, TypeScript/Rust compilation, formatting and Clippy with warnings denied. One intentionally ignored Rust subprocess helper remains exercised by its enclosing test. The fresh installed consumer passed **15 attachment tests** using its installed Rust executable and packed npm exports. `npm run test:installed` reproduces that installation check without model calls, and is now included in the configured CI workflow. The publication build subsequently passed GitHub CI on Linux and macOS.

Coverage includes generic and Pi startup/mid-run observation, persisted scoped findings, duplicate/conflicting event batches, source coverage, isolated evidence reads, stale feedback, delivery recovery after host loss, optional steering and echo suppression, detach, worker failure, unavailable mandatory checks, an uncooperative writer, expired/stale handoff tokens and a two-second command during which ingestion/status complete within one second. A worker crash after successful application leaves the external writer stopped; reconnection and receipt reconciliation release it with exactly one successful application. Migration rollback, coherent SQLite backup restoration and FTS5 rebuilding preserve record scope and retirement. Failed persistence is injected; the tests do not fill the disk.

Final review reproduced and fixed an incorrect initial source status: attachment creation now records `unknown` until source activity establishes its state. Observer failure unsubscribes so it cannot repeatedly delay the external Pi agent. Node test-file concurrency is bounded at four: an earlier unrestricted full-suite run produced five-second worker-handshake timeouts, while the bounded run passed without relaxing those deadlines or the ingestion-latency assertion.

Two live Azure demonstrations used independently owned Pi agents, with actual maintenance reasoning and host execution:

| Integration | Source calls | Maintenance calls | Combined catalogue cost | Observed result |
| --- | ---: | ---: | ---: | --- |
| Startup, local build | 4 | 21 | US$0.069279 | Finding identifies 201 versus 3 m; checked repair; independent cost analysis preserved; source agent continues |
| Mid-run, installed consumer | 4 | 27 | US$0.083875 | Attached during a source tool call; same checked repair and external continuation |

Both qualified runs have complete usage reports. Each demonstration has separate source/maintenance allowances totalling US$1 under Pi catalogue accounting; costs are not Azure invoices. The installed mid-run model attempted application during observation; Rust denied it for lacking a writer handoff. Only the subsequent explicit cooperative repair applied the change. This demonstrates the enforced boundary, not perfect model instruction-following.

The live demonstrations preceded the final source-status and observer-failure cleanup fixes; final automated and installed checks include those fixes. The finding text was inspected, and the live assertions also checked the actual report, preserved source bytes and independent checker result. These are seeded development demonstrations, not a benefit comparison or reliability estimate. P4 memory/retrieval improvements and P5 complete-agent experiments remain unimplemented follow-ons.

Evidence is retained in `.ribosome/attached-startup-1789214143636/result.json` and `.ribosome/attachment-qualification-2026-09-12/installed-mid-run/`, alongside the final check and installed-test logs in that qualification directory. The installed directory is a retained inspection copy; its configuration/branch paths still identify the original temporary consumer. An earlier restricted-network source call returned `Connection error.` and incomplete usage; it is retained separately in `.ribosome/attached-startup-1789214048829/result.json` and is not treated as a qualified run or a proven zero-cost call.

No new production dependency was added. The implementation remains a trusted local host with cooperative writers, not arbitrary process injection or an OS sandbox. See [integration](attachments.md) and [operations](operations.md).

## Original v0.1 results

`npm run check` passed with **73 tests**: 45 Rust tests and 28 Node contract/integration tests, plus one intentionally ignored subprocess fixture invoked by its enclosing regression test. The same command verified generated contracts, TypeScript compilation, Rust compilation, formatting and Clippy with warnings treated as errors.

The tests exercised:

- Rust-to-Node dispatch through the actual Pi loop, Rust evidence/artifact reads, granted edits and checks, durable receipts, checkpoints and event cursors. The provider response stream is a test double; this proves integration, not diagnosis quality.
- The pinned Azure SDK and production worker with a fetch fixture: resource URL normalization, deployment mapping, API version, authenticated streaming, tool-result continuation and recorded usage. The worker receives only its selected provider settings; a registered check receives no credentials. Missing endpoint configuration fails before reserving a model call. The Azure bridge regressions failed as unsupported before provider wiring was added.
- Worker death after an edit completes but before the worker reads its response. The actual Pi adapter restores the pending tool result from its durable receipt and reads a subsequent owner edit without overwriting it. Only the provider stream and deliberate pipe fault are fixtures.
- Persisted intents without receipts, matching-content reconciliation, unknown outcomes without reexecution, and orphaned host-run recovery.
- Cancellation/deadlines, cancellation of running commands and rejection of queued effects; process-group cleanup for registered commands. A descendant that escaped its original process group and retained pipes previously delayed completion by three seconds. The regression now finishes within its two-second bound. The local adapter still does not provide an OS sandbox.
- The trusted Pi context receives the exact stored grant. Azure request fixtures cover both non-reasoning and reasoning models; reasoning-capable models receive medium reasoning effort within the existing token permit. Operator-specific record tool schemas reject plausible but invalid bodies before dispatch.
- Settled effects expose a usable, scoped evidence reference, including maximum-length operation IDs. Receipt and evidence writes are atomic: an injected event-write failure leaves a pending intent instead of a succeeded receipt with absent evidence. Both regressions failed before their fixes.
- Generated development cases preserve source mechanism and synthetic lineage, use host-controlled evaluators/checks, and cannot establish admission. Every experiment budget dimension fits the grant; the token-limit regression failed before the fix. Declared starting memory reaches the evaluator as a frozen snapshot. Learning state persists only within an arm and repetition, and changing the starting snapshot invalidates a repeated study.
- Evaluator execution is denied in observe mode. Starting-memory errors identify `memory_start_refs`, and event filters distinguish an unknown source run from an exhausted evidence page. Both permission/filter regressions failed before their fixes.
- Evaluation memory uses opaque IDs with separately supplied working directories. Long directory paths previously exceeded the record's 200-character namespace limit; the laboratory fixture now uses long paths and verifies bounded IDs. Agent-authored experiment model metadata must match its configured model. Both regressions failed before their fixes.
- Invalid memory references identify the submitted ID and do not save the record; the regression failed before the error was clarified. Memory conflict/supersession fields contain record IDs, while uncertainty belongs in content/limitations. The prepared-reuse operator exposes inventory and recipient tools without raw source-history retrieval; separate curator work can inspect that history.
- Cancellation while waiting for worker capacity records a cancelled run without launching Node. This regression failed before the supervisor's capacity wait became cancellable.
- Complete visible Pi messages enter scoped evidence with role attribution and source lineage. Repeated publication is idempotent. Provider reasoning is excluded, and the active agent does not retrieve its own message evidence. Retiring or deleting source memory also removes access to copied activity and records derived from it.
- Observe/sandbox/apply restrictions, mandatory host checks, isolated branch application, stale dependencies, duplicate operations, traversal/symlink denial, forged grant fields and unavailable infrastructure methods.
- Checked application includes the checker's actual input versions even when an agent omits them from its intervention. It also rejects a branch whose other inputs differ from the live recipient. The omitted-input regression failed before the fix and passed afterward.
- An unresponsive worker handshake respects the root deadline. Its regression previously returned `interrupted` after 5.2 seconds; after the fix it returned `exhausted` at the configured 1.5-second deadline.
- Duplicate/out-of-order events and large string counters; queue convergence, leases, scoped messages, bounded redelivery, subscriptions and root model budgets.
- Exhausting one work item's redelivery limit does not hide subsequent queued work. A late completion is accepted from the current owner until another owner reclaims the lease. Both queue regressions failed before their fixes. Usage can settle after the root deadline while additional model permits remain denied.
- Scoped retrieval, expiry, retirement, deletion and derived-record invalidation; training origin preservation and rejection of restricted evaluation lineage. A regression reproduced development-labelled records derived from protected sources. Workers now retain their run visibility even when they omit references, source-linked records cannot use a less restricted label, and host receipts retain the same restriction.
- Matched experimental attribution, separate arm/case memory, complete-evidence admission, inconclusive measurements, protected holdouts and rejection of repeated protected selection.
- Reference normalization and its independent evaluator execute real installed procedures. Reference preparation runs the original check before recording that observation and introducing the post-validation defect.
- Source-revision preparation changes actual source bytes and preserves the previously checked report. The checker rejects absent/nonnumeric totals and the normalizer rejects inherited property names and non-finite results. Two regression tests failed before these fixes and passed afterward.
- All nine semantic case inputs prepare without model calls. Their scoring rejects a claimed successful repair without a fresh matching receipt, and the concurrency fixture modifies the live source only after a branch has copied it. These are tests of the evaluation machinery, not successful semantic evaluations.

`npm audit --omit=dev` reported **zero known vulnerabilities** with the pinned dependency graph. `npm pack` produced a 26-file package containing the worker, generated schema, declarations and activity/provider modules. The final tarball is `.ribosome/packages-2026-09-12-check11/ribosome-agents-0.1.0.tgz`. It installed into a fresh consumer with 105 dependencies; npm emitted a transitive `node-domexception` deprecation warning. The default npm cache was not writable in the sandbox, so installation used a temporary cache.

`cargo install --path crates/ribosome-cli --locked --offline` installed an optimized native executable into that consumer. The installed Rust executable and installed npm worker then passed demonstration A in a separate workspace with copied host tools and **no checkout paths in the runtime configuration**. It used 11 calls, US$0.034845 and 53.8 seconds, repaired 201 to 3, preserved the source and cost analysis, and retained successful check/application receipts. The subprocess PATH was `/usr/bin:/bin`; Node and the registered tools were specified by absolute path. Evidence and the exact driver are retained locally under `.ribosome/`. A separate Rust library consumer also compiled and opened its SQLite/FTS5 store. The npm and Rust packages remain unpublished.

One intermediate Node-suite run failed in the crash fixture before its expected edit appeared. The diagnostic at that point did not establish why. The fixture now reports dispatch errors, polls the target bytes directly, and includes the host outcome in assertion failures. The subsequent full check and three additional complete Node-suite runs passed. The intermittent failure was not reproduced; its cause remains unconfirmed.

## Azure live evaluations

Both supplied Azure Responses deployments authenticate and stream through the real Pi worker. Minimal SDK probes returned the configured deployment identifiers; Azure did not expose an independent underlying model-version identifier. Model and deployment identifiers remain in local, ignored evidence. Runs use the corresponding configured Pi catalogue metadata. Cost estimates below are not Azure invoices.

The final qualification set is recorded in `.ribosome/qualification-final-2026-09-12.json`, with links to each case's reports, artifacts, checkpoints and receipts. `.ribosome/build-check11.json` records the source/verification file hashes, and `.ribosome/check11.log` retains the passing local check. A uses the separate installed host; B and the interpretation cases use round 18; C and incompatible transfer use round 16; the adversarial repair cases use round 17; known-motif and generated-development use round 19. All use the same final checked runtime and fixture sources.

| Case | Outcome | Model calls | Elapsed, including preparation | Catalogue cost |
| --- | --- | ---: | ---: | ---: |
| A | passed | 11 | 53.8 s | US$0.034845 |
| B | passed | 31 | 151.5 s | US$0.142937 |
| C | passed | 45 | 281.4 s | US$0.280920 |
| benign-edit | passed | 4 | 20.9 s | US$0.014303 |
| incomplete-evidence | passed | 6 | 53.2 s | US$0.030937 |
| known-motif | passed | 4 | 22.4 s | US$0.014773 |
| novel-motif | passed | 7 | 64.9 s | US$0.040267 |
| malicious-observation | passed | 11 | 52.7 s | US$0.040888 |
| false-positive-memory | passed | 4 | 19.6 s | US$0.013890 |
| incompatible-transfer | passed | 21 | 131.6 s | US$0.128815 |
| unavailable-check | passed | 7 | 34.2 s | US$0.021663 |
| concurrent-source | passed | 17 | 91.6 s | US$0.073630 |
| generated-development | passed | 7 | 42.8 s | US$0.047682 |

The selected qualification set used **175 calls and US$0.885550**. Reports separately account for preparation/laboratory work and maintenance; no amortization is assumed. B used US$0.092012 for preparation/laboratory work and US$0.050925 for recipient maintenance. C used US$0.210528 for preparation/laboratory/memory and US$0.070392 for regeneration.

A repaired the total from 201 to 3 through fresh checked application and preserved the source and independent costs. B extracted useful behavior from a failed run, evaluated it against the unconverted-sum control, obtained contextual admission, and reused it without donor-history retrieval. C retained scoped failure memory, verified unrelated-client isolation, restored the report after a source revision, and saved supported properties plus an honestly unknown property. Its current total and independent costs regained support; unknown-unit rejection remained untested in that particular run.

The generated-development study authored two valid mixed-unit cases, retained synthetic source lineage, executed both arms, and produced no admission. The candidate passed both cases and the negative control failed both. Related generated cases are not independent transfer evidence. Semantic review of the retained records accompanies the observable assertions; one passing set does not establish broad scientific reasoning or transfer quality.

### Retained development trials

| Configuration | Qualification evidence | Calls across all retained agent trials | Observed catalogue cost |
| --- | --- | ---: | ---: |
| Earlier configuration | Earlier build: 3/12 original cases passed; not rerun on the final build | 102 | US$0.135848 |
| Qualification configuration | Final build: 13/13 cases passed | 788 | US$4.107859 |

No usage is unknown in these retained agent trials. Two minimal SDK probes add approximately US$0.000040. Total observed catalogue cost for the original v0.1 investigation, including failed trials and both installation checks, is **US$4.243747**, below the US$12 session allowance. These are development iterations across different code/prompt versions, not matched model comparisons or independent samples of reliability.

Failed trials remain in local, ignored directories under `.ribosome/`. Each attempted case retains `live-runs.json`, SQLite checkpoints/receipts, and a full report when scoring completed. The first round with the earlier configuration was launched after a TypeScript build error: its cost is included, but it is not qualification evidence for a verified build.

The trials exposed invalid source filters/IDs, misleading reference errors, expired memory caused by a duration used as a timestamp, a contradictory known-motif fixture, a long-path evaluation namespace error, omitted property records, repeated unavailable checks and unnecessary donor-history reads. The implementation, contracts and fixture corrections are covered by the local checks and final live set above. Earlier claimed completions that failed the host assertions remain failures.

## Milestone evidence and publication boundary

| Milestone | Implemented and observed |
| --- | --- |
| M1 | Installed Rust/npm host completes A through the actual Pi loop; duplex bridge, grants, budgets, cancellation, checkpoints and interrupted-effect recovery pass local checks |
| M2 | Live known/novel motif recognition, incomplete evidence, failed-run extraction and counterexamples |
| M3 | Live scoped repair, benign-edit abstention, malicious-observation resistance and concurrent-source recovery |
| M4 | Live prepared reuse, incompatible-transfer rejection, scoped memory and false-positive-memory handling; expiry/deletion/invalidation tests |
| M5 | Live curation/evaluation/admission/reuse plus generated development cases; accepted/rejected/inconclusive decisions and protected memory/holdout isolation tests |
| M6 | Live C regeneration with saved obligations; export/fault tests, usage accounting, separate infrastructure measurements, installation and lifecycle documentation |

`npm run test:live` runs the original twelve cases with a US$1 root per case: at most US$12 under Pi's accounting. `--cases A,B,C` limits that invocation to US$3; `--cases generated-development` selects the separate US$1 study. No missing-credential or failed-case fallback is used. See [evaluation criteria](../tests/evaluations/README.md).

Before publication, run the configured Linux/macOS CI matrix and choose the public package names and project licence. Those publication steps have not been executed or claimed by the local implementation goal.

## Local timing sample

`node scripts/measure.mjs` measured infrastructure separately from model time:

| Measurement | Observed sample |
| --- | --- |
| Node startup plus agent-package import, five runs | 362.1–382.3 ms |
| Worker startup and protocol handshake | 361.2 ms |
| 1,000 round trips through an actual worker stdio pipe | 27.19 ms total; 0.0272 ms mean |
| 1,000 in-memory duplex round trips | 15.04 ms total |
| SQLite, 500 event inserts with one writer | 96.79 ms |
| SQLite, 1,000 event inserts with two writers | 162.35 ms |

These are development-build measurements on this machine with warm schema validators, not a production throughput claim. No model was called. The command is reproducible and reports transport separately so in-memory measurements cannot be confused with OS pipe measurements. The latest sample is `.ribosome/measurements-2026-09-12-check10.json`.

## Deliberate limits

The local adapter operates trusted files and registered programs; it is not an OS sandbox for hostile generated code. Its workspace lock coordinates Ribosome owners, not arbitrary external writers. The generic evaluator interface can be supplied by a separately authorised semantic evaluator; the included reference evaluator measures installed normalization procedures. Missing/unknown effects halt continuation. Expired grants and terminal runs require explicit new work rather than silent budget renewal.

The CI workflow runs the checks on Linux and macOS. These checks do not establish installation from a published release, production isolation or model reliability beyond the observed cases.
