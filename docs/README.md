# Ribosome documentation

Read the guides below on GitHub, or open [index.html](index.html) locally for the browsable documentation with search and a page outline.

## Start

- [Introduction](content/index.md)
- [Quickstart](content/start/quickstart.md)
- [Installation](content/start/installation.md)

## Core

- [How it works](content/core/how-it-works.md)
- [Biological inspiration](content/core/inspiration.md)
- [Behavioral motifs](content/core/motifs.md)

## Guides

- [Connect an agent](content/guides/connect.md)
- [Runtime and storage](content/guides/runtime.md)
- [Repair an artifact](content/guides/repair.md)
- [Discover a behavior](content/guides/discovery.md)
- [Reuse a procedure](content/guides/reuse.md)
- [Project memory](content/guides/memory.md)

## Evaluation

- [Experiments](content/evaluation/experiments.md)
- [Current support](content/evaluation/support.md)

## Reference

- [Configuration](content/reference/configuration.md)
- [Operations](content/reference/operations.md)
- [Detailed references](content/reference/further-reading.md)

The detailed [connector](attachments.md), [protocol](protocol.md), [operations](operations.md), and [validation](validation.md) documents retain integration contracts and recorded evidence. The guides above are the main reading path.

## Edit and preview

Edit pages in `docs/content/`. Each page has a quoted `title` and `description` in frontmatter and one H1. List each page once in `docs/navigation.json`. Keep links relative so they work in GitHub's Markdown view as well as the generated site.

From the repository root, after `npm ci --ignore-scripts`:

```sh
npm run docs:check
npm run docs:build
```

The check renders all pages and validates navigation, repository links, and Markdown heading targets. The build writes `docs/index.html`; regenerate it with each documentation change. Marked is a development dependency used to render Markdown. The layout and browser behavior are in `docs/site/`.

The generated HTML contains every guide page and the search index. Open it directly in a browser; reading the guides and searching works offline. Links to detailed repository references and external sources use GitHub or the cited website and need a connection.

For a local HTTP preview, serve the `docs` directory with any static file server. Check navigation, search, code copying, page anchors, and a narrow viewport after changing the template or browser script.
