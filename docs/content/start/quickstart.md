---
title: "Quickstart"
description: "Run Ribosome beside an agent and inspect a checked repair."
---

# Quickstart

Run the included example: find a unit error, repair the report, and let the original agent continue.

## Build

Use the Node and Rust versions pinned in the repository. You also need Git, npm, and the Rust build tools.

```sh
git clone https://github.com/TheodoreGalanos/ribosome.git
cd ribosome
npm ci --ignore-scripts
cargo build --workspace --locked
npm run build
```

This builds both components. For the full automated suite, run `npm run check` separately.

## Configure a provider

Create `.env` in the repository root. For OpenAI:

```dotenv
RIBOSOME_PROVIDER=openai
RIBOSOME_MODEL=your-model-id
OPENAI_API_KEY=your-api-key
```

Replace the placeholders with your local settings. The model must exist in the pinned Pi catalogue. Keep `.env` private. See [configuration](../reference/configuration.md) for provider options.

```sh
npm run provider:check
```

This checks that the required settings are present. The demo makes the actual provider connection.

## Run

```sh
npm run demo:attached
```

**This makes paid model calls.** The example allocates US$0.40 to the source agent and US$0.60 to maintenance using local catalogue prices. Provider charges can differ.

## Inspect the result

The seeded report treats **1 metre + 200 centimetres as 201 metres**. A successful run produces a finding, applies a checked correction to **3 metres**, and preserves the source measurements and independent cost analysis.

The demo writes a run directory under `.ribosome/attached-startup-*`. Read its `result.json` for findings, action receipts, and usage, then inspect the corrected report and its check results.

To attach during execution instead:

```sh
npm run demo:attached -- --mid-run
```

Next: [Connect an agent](../guides/connect.md).
