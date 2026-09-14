# First external-data pilot

The [expanded campaign](expanded-results.md) continues this pilot with a larger allowance and a completed function study. The results below preserve the original 100-call attempt.

The importer loaded twelve real episodes. A live caretaker saved an audit finding and a prefix finding. Live discovery saved one proposed definition, but no grounded occurrences or completed investigation. Useful learned transfer remains unestablished.

This pilot ran locally on 13 September 2026 against the working implementation based on `12e87e74c81d38da507d545a43a80815be1d310b`. Its private reports are in `.ribosome/offline-lab/`. The [aggregate results](results/pilot.json) contain counts and source revisions without source transcripts or provider settings.

## Imported evidence

| Sample | Messages | Tool calls | Paired results | Pending calls | Original timestamps |
| --- | ---: | ---: | ---: | ---: | ---: |
| Six AEC episodes | 12 | 0 | 0 | 0 | 12 |
| Six Nebius episodes | 870 | 432 | 426 | 6 | 0 |

The AEC sample contains repeated attempts on two tasks from one line-capacitance template. Each episode has a task message and a long assistant message. This supports investigation of calculations and claims, with limited evidence about tool execution. The Nebius sample covers six software issues and contains recorded tool interactions. Each ends with one pending call in the imported representation.

All twelve episodes were accepted. Neither sample is balanced or representative: selection used the first six rows. The initial AEC discovery assignment contained all six episodes and overlapped its challenge assignment. No contrast ran on it. The current example assigns the first three episodes to discovery and the last episode to challenge, and rejects overlapping evidence before contrast.

## Live outcomes

| Stage | Observed outcome | What to investigate next |
| --- | --- | --- |
| AEC audit | One saved finding after six calls. | The caretaker read the message preview but did not read its full artifact parts. It claimed numerical results were absent although the full source contained calculations. Evidence access alone does not ensure adequate inspection. |
| Nebius discovery, first attempt | Summary completed after 64 calls; no records saved. | The curator repeatedly submitted an invented definition relation. The host rejected it. Errors now include the unavailable reference. |
| Nebius discovery, second attempt | Seven calls, then abstention; no investigation saved. | An invented optional work reference prevented submission. Work ownership is now supplied by the host. |
| Nebius discovery, third attempt | Seventeen calls; one definition saved, then budget exhaustion. | The definition describes a repository path utility. It has no supporting occurrence or investigation demonstrating a reusable agent behaviour. |
| AEC prefix | A network retry completed in five calls and saved one finding. | The finding describes the visible boundary but offers little analysis of the source task. Its discussion of absent local obligations should not be treated as evidence about the original agent. |

The initial audit attempt failed before a model call because caretaker runs could not receive corpus assignments. That path is fixed. The first prefix attempt failed because the shell sandbox could not resolve external hosts; its provider reservation remains unresolved.

The pilot reached its 100-call ceiling. Known usage was **US$0.706080**, with **US$0.070856** reserved for the unresolved connection attempt. Compaction is included. These amounts use the configured catalogue rates and are not a billing invoice.

The host correctly reported the third discovery attempt as exhausted: a conservative reservation for its next request exceeded the available budget. A saved definition by itself did not advance the pipeline to extraction.

## Library checks

- Eight importer tests passed for representation decoding, fallback, tool pairing, unknown content, duplicate identities, joins, atomic episode publication, prefix access and source withdrawal.
- Nine focused Node tests passed for tool schemas, the study setup, transcript visibility, per-run budgets and imported-memory continuation.
- The continuation check also passed using the real first Nebius episode. After interruption and source withdrawal, two subsequent provider requests excluded the retained memory and its derived interpretation. This used a scripted provider transport.
- Twenty-one existing core tests passed across corpus access, source availability and artifact context.
- Twenty-seven focused Node tests covering previously failing attachment, provider, bridge and prepared-invocation paths passed locally. The unchanged long-history regression passed separately in about 91 seconds.

Two failures produced useful regressions. Long imported messages exceeded the snapshot limit, so the importer now publishes ordered message parts. Reading a retained snapshot created a copy without a link to its parent, so withdrawing the source left the copy accessible. The copied snapshot now retains that source link, and the regression passes.

Repeated executable-content checks were also removed. Checker versions use configuration, file size and modification time. The importer records source revisions and episode identities without content hashes.

The earlier public Linux CI run had nine failing Node tests. The focused local results cover those paths, but updated Linux CI has not run because this change has not been pushed. The local long-history test remains relatively slow.

## Remaining experiments

The pipeline can prepare contrasts, extract a supported discovery, search instructions, and configure the existing function and system-benefit evaluator. This pilot did not produce the supported investigation needed to exercise those stages with learned external material.

The next experiments are a complete evidence audit with full-message inspection, a comparable plain-transcript review, and one completed discovery investigation. Only then can a real extracted instruction be tested on fresh recipients. The eight-execution function study and broader control comparison remain unexecuted. A larger task-grouped cohort is also still needed.
