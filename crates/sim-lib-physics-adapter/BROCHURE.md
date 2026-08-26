# sim-lib-physics-adapter

In one line: Domain-neutral conformance contract for physics model adapters.

## What it gives you

Feed heterogeneous model results into one audit contract while retaining semantic quantities, explicit boundaries, provenance, influences, and solver certificates. Missing boundaries, evidence lanes, or required lumped ports are refused instead of inferred. The contract keeps inputs, outputs, limits, and refusal cases explicit, so callers can compose the capability without acquiring unrelated host, transport, or product authority. Stable records make the result suitable for tests, inspection, and deterministic integration.

## Why you will be glad

- The public contract makes supported behavior, limits, and typed failures visible before integration.
- One owning crate prevents neighboring libraries from growing competing copies of the same policy.
- Deterministic records and checked tests keep adapters reviewable when implementations evolve.

## Where it fits

Within SIM, sim-lib-physics-adapter owns only the focused contract described above. Adjacent runtime libraries, platform adapters, codecs, and user surfaces can build around it while retaining their own policy. That boundary keeps the kernel small, avoids competing implementations, and lets this capability evolve without forcing unrelated components to change.
