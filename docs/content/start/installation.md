---
title: "Installation"
description: "Build from source or install the current build into another project."
---

# Installation

Build the Rust executable and TypeScript package from this repository.

## Source checkout

```sh
git clone https://github.com/TheodoreGalanos/ribosome.git
cd ribosome
npm ci --ignore-scripts
cargo build --workspace --locked
npm run build
```

Use the versions in [`.node-version`](../../../.node-version) and [`rust-toolchain.toml`](../../../rust-toolchain.toml). These commands build both components. `npm run check` runs the full automated suite when you need it.

Local state uses SQLite. Model calls go to your selected provider.

## Install into another project

From a built checkout, choose an existing absolute project directory:

```sh
CONSUMER=/absolute/path/to/your-project

cargo install --path crates/ribosome-cli --locked --root "$CONSUMER"
npm pack --workspace @ribosome/agents --pack-destination "$CONSUMER"
npm install --prefix "$CONSUMER" "$CONSUMER/ribosome-agents-0.1.0.tgz"
```

The executable is `$CONSUMER/bin/ribosome`. In that project's host configuration, set `node` to its Node executable and `worker` to the installed worker entrypoint.

Resolve the worker from the consumer directory:

```sh
cd "$CONSUMER"
node --input-type=module -e \
  'import { fileURLToPath } from "node:url"; console.log(fileURLToPath(import.meta.resolve("@ribosome/agents/worker")))'
```

Use the resolved absolute worker path in the consumer's configuration.

## Verify the installation path

From the source checkout:

```sh
npm run test:installed
```

This creates a temporary consumer and exercises the installed packages with local provider fixtures. It makes no paid calls.

Next: [Configuration](../reference/configuration.md) or [Quickstart](quickstart.md).
