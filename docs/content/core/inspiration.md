---
title: "Biological inspiration"
description: "The ideas behind proofreading, repair, and reuse in Ribosome."
---

# Biological inspiration

A [ribosome](https://www.genome.gov/genetics-glossary/Ribosome) reads messenger RNA and assembles a protein. Ribosome the library borrows more broadly from biological expression, repair, and adaptation.

## The useful analogy

An execution trace records behavior in one situation: the inputs, decisions, actions, and observed results.

A reusable procedure can be drawn from that experience, but it needs testing before we rely on it elsewhere.

| Inspiration | In Ribosome |
| --- | --- |
| Proofreading | Investigate a transition before its result is relied on. |
| Excision repair | Replace affected work and check the result. |
| Recombination | Adapt a prepared procedure to a new task. |
| Chaperoning | Help an artifact meet its consumer's requirements. |
| Regulation | Adjust a strategy to current conditions. |
| Immune memory | Retain evidence about recurring failures and useful responses. |
| Regeneration | Restore support for a result after its inputs change. |

Maintenance profiles use these capabilities as part of their work.

## Other roots

The design grew out of agent evaluation and evolution work in [AEC-Bench](https://github.com/TheodoreGalanos/aec-bench). Ribosome runs independently of it.

Its inventory also draws on [MAP-Elites](https://arxiv.org/abs/1504.04909): retain useful alternatives for different conditions. Ribosome's archive keeps evaluated implementations with the evidence and conditions that support each choice.

Next: [Behavioral motifs](motifs.md).
