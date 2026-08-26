# sim-lib-physics-audit

In one line: Independent stored-energy audits with typed residual and uncertainty lanes.

## What it gives you

Endpoint stored-energy evaluation and immutable energy-balance records with separate transfer, residual, uncertainty, and evidence lanes. Audits remain independent of the solver that produced the trajectory. Missing energy cannot hide inside one aggregate error number. Uncertainty stays explicit rather than being mistaken for residual. Evidence references make every verdict traceable. This crate consumes physics-core study records and numerical results, then produces audit evidence. It owns neither the model nor the solver. The contract keeps inputs, outputs, limits, and refusal cases explicit, so callers can compose the capability without acquiring unrelated host, transport, or product authority. Stable records make the result suitable for tests, inspection, and deterministic integration.

## Why you will be glad

- The public contract makes supported behavior, limits, and typed failures visible before integration.
- One owning crate prevents neighboring libraries from growing competing copies of the same policy.
- Deterministic records and checked tests keep adapters reviewable when implementations evolve.

## Where it fits

Within SIM, sim-lib-physics-audit owns only the focused contract described above. Adjacent runtime libraries, platform adapters, codecs, and user surfaces can build around it while retaining their own policy. That boundary keeps the kernel small, avoids competing implementations, and lets this capability evolve without forcing unrelated components to change.
