# Configure a trajectory import

A YAML profile describes where source records live and how to decode their messages. The same importer can read a local file or acquire Hugging Face Parquet files. Preparation makes no model calls.

## Choose a starting profile

| Profile | Input and message format |
| --- | --- |
| [local-chat.yaml](local-chat.yaml) | Local JSONL rows with a `messages` array. Includes a small synthetic fixture. |
| [local-encoded-chat.yaml](local-encoded-chat.yaml) | Local JSONL rows with a JSON-encoded `conversation` string. |
| [aec-release.yaml](aec-release.yaml) | Hugging Face artifact, rollout and task tables, joined before decoding embedded JSONL. |
| [nebius-openhands.yaml](nebius-openhands.yaml) | A Hugging Face Parquet file with a `trajectory` message array. |

Run the local example from the repository root:

```sh
cargo build -p ribosome-import --locked
target/debug/ribosome-import prepare examples/offline-lab/profiles/local-chat.yaml .ribosome/profile-example
```

Read `episodes/episode-0000.json` in the output directory for decoded messages and coverage. `report.json` records accepted episodes, duplicates and quarantine reasons. `source-lock.json` records the profile and selected sources.

For Hugging Face or local Parquet, build with `--features remote`. The [lab guide](../README.md#import-the-sample) has the acquisition commands. Local input paths are relative to the profile file; Hugging Face paths identify files in the dataset repository.

## Adapt the mapping

Copy the closest profile and give it your own `id`. Set its input files and update the field pointers. This excerpt from `local-chat.yaml` maps one source row:

```yaml
assemble:
  base: episodes
  as: source
  joins: []
episode:
  id: /source/id
  task: /source/task_id
  family: /source/family
  decoder:
    name: chat-messages-v1
    field: /source/messages
    value_encoding: array
```

`assemble.as` places the row under `source`, so `/source/messages` selects its `messages` field. If your row uses `conversation`, change that pointer. Choose `array`, `json_string` or `jsonl_string` to match the field's encoding.

| Section | What it controls |
| --- | --- |
| `input` | Local or Hugging Face acquisition, files and table encoding. |
| `assemble` | The base table, namespaces and joins. AEC joins trials by `trial_id` and task information by `task_id`. |
| `episode` | Episode identity, task, family and message decoder. Task and family groups help keep related episodes in the same study split. |
| `metadata` | Extra source fields saved in the private episode file for the owner. |
| `annotations` | Publisher outcomes and other labels saved separately for owner or evaluator use. |
| `policy` | Supported handling of unknown content, missing results and malformed episodes. The schema fixes these values to the implemented behaviours. |

Use an existing decoder when the message objects have the supported `role`, `content`, `tool_calls` and `tool_call_id` fields. A different message structure requires decoder code and a fixture. The profile selects a message field and its encoding; the decoder interprets the objects inside it.

AEC's profile also selects a conversation fallback when its trajectory is missing, null, empty or header-only. A malformed nonempty trajectory produces a quarantine reason. Inspect that reason before changing the mapping.

`prepare` validates the profile against the [schema](../../../crates/ribosome-import/profile.schema.json), decodes the source and writes its report. Start with a small local fixture when adapting a profile. Keep the existing supported policy values and use a new output directory when changing an acquired profile. Both local and remote preparation reject a changed mapping before acquisition. Reuse checks task, family, metadata and annotations as well as the messages.

## Use tool information during review

The decoder preserves message roles, tool names, original arguments, parsed argument objects where available, call IDs and result IDs. Coverage reports count paired results and list pending calls. These describe what the source recorded. Published events use `sequence` for order. A matched tool result names its calling message as a parent; adjacent independent messages have no dependency parent.

For evidence published before this parent-link correction, create assignments with a new study ID to rebuild the event graph. Earlier stored events and experiment results retain their original links.

A host can use those fields to find relevant interactions before creating a study assignment. This example lists the `read_total` calls in the synthetic episode and locates their recorded results:

```sh
node --input-type=module - .ribosome/profile-example/episodes/episode-0000.json read_total <<'JS'
import { readFileSync } from 'node:fs';
const episode = JSON.parse(readFileSync(process.argv[2], 'utf8'));
const toolName = process.argv[3];
const messages = episode.decoded.messages;
const calls = messages.flatMap(message =>
  message.tool_calls.filter(call => call.name === toolName).map(call => ({
    source_index: message.source_index,
    call_id: call.id,
    arguments: call.parsed_arguments ?? call.arguments,
    result_source_index: call.id == null ? null : messages.find(result =>
      result.role === 'tool' && result.tool_call_id === call.id
    )?.source_index ?? null,
  }))
);
console.log(JSON.stringify(calls, null, 2));
JS
```

For a source that records a tool named `snap`, replace `read_total` with `snap`. Use the returned positions to choose a coherent window containing the request, any available result and relevant surrounding messages. Save that window in the [study manifest](../README.md#assign-and-review-evidence).

When assigned, short decoded messages appear in `external_message` event payloads. Long messages have a preview and ordered `message_parts` artifacts containing the full decoded message, including its tool fields. A curator can inspect this evidence when proposing or challenging a behaviour. A caretaker can use it to support a finding. Extra profile metadata and publisher annotations remain in the owner files; adding a metadata field does not expose it to those agents.

Matching a tool name can select evidence or prompt a review. The agent determines what the interaction means and whether a change is justified; the host supplies permission for execution. The import profile currently has no rule that dispatches an action when a tool name matches.

For a live harness, [attachment configuration](../../../docs/content/guides/connect.md#communication-in-practice) already selects maintenance events by kind, such as `tool.completed`. A tool-specific condition such as “review after `snap`” belongs in host scheduling code. YAML action routing would be an additional capability, separate from this source mapping.
