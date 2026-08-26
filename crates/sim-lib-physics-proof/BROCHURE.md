# sim-lib-physics-proof

In one line: Orthogonal numerical refinement and certified model verdicts for physical studies.

## What it gives you

Orthogonal refinement plans, event-exact output meshes, independent numerical gates, continuation diagnoses, and immutable verdict records. Definite threshold claims require certified intervals; ordinary estimates remain unresolved. A plausible estimate cannot be promoted into a proof. Space, time, event, and solver refinements stay independently visible. Failed certification returns actionable diagnosis instead of false certainty. This crate sits above physical models and numerical solvers. It evaluates their evidence and emits certified or explicitly unresolved verdicts without owning either model construction or execution. The contract keeps inputs, outputs, limits, and refusal cases explicit, so callers can compose the capability without acquiring unrelated host, transport, or product authority. Stable records make the result suitable for tests, inspection, and deterministic integration.

## Why you will be glad

- The public contract makes supported behavior, limits, and typed failures visible before integration.
- One owning crate prevents neighboring libraries from growing competing copies of the same policy.
- Deterministic records and checked tests keep adapters reviewable when implementations evolve.

## Where it fits

Within SIM, sim-lib-physics-proof owns only the focused contract described above. Adjacent runtime libraries, platform adapters, codecs, and user surfaces can build around it while retaining their own policy. That boundary keeps the kernel small, avoids competing implementations, and lets this capability evolve without forcing unrelated components to change.
