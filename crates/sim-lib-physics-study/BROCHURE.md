# sim-lib-physics-study

In one line: Immutable physics study plans and placement-transparent sweep evidence.

## What it gives you

`sim-lib-physics-study` turns a parameter sweep into immutable reviewable data. It preserves exact boundary cases, explicit untested regions, partition separation, sampler replay evidence, placement identity, partial failures, and provider-aware comparisons. Selection requires the existing proof-backed clean input, while energy audits remain useful after selection without becoming a hidden objective. The contract keeps inputs, outputs, limits, and refusal cases explicit, so callers can compose the capability without acquiring unrelated host, transport, or product authority. Stable records make the result suitable for tests, inspection, and deterministic integration.

## Why you will be glad

- The public contract makes supported behavior, limits, and typed failures visible before integration.
- One owning crate prevents neighboring libraries from growing competing copies of the same policy.
- Deterministic records and checked tests keep adapters reviewable when implementations evolve.

## Where it fits

Within SIM, sim-lib-physics-study owns only the focused contract described above. Adjacent runtime libraries, platform adapters, codecs, and user surfaces can build around it while retaining their own policy. That boundary keeps the kernel small, avoids competing implementations, and lets this capability evolve without forcing unrelated components to change.
