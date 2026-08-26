# sim-lib-physics-power

In one line: Boundary-relative conjugate ports and event-split signed-work audits.

## What it gives you

Immutable conjugate-port descriptions and boundary-relative signed-work audits. Continuous work is split exactly at declared events, retained per port, and kept separate from impulse transfers. Sign conventions are tied to explicit boundaries. Event discontinuities cannot masquerade as numerical spikes. Per-port evidence makes conservation failures diagnosable. This crate adds power and work semantics to physics-core records. Solvers provide trajectories and events; audit and proof crates consume the resulting evidence. The contract keeps inputs, outputs, limits, and refusal cases explicit, so callers can compose the capability without acquiring unrelated host, transport, or product authority. Stable records make the result suitable for tests, inspection, and deterministic integration.

## Why you will be glad

- The public contract makes supported behavior, limits, and typed failures visible before integration.
- One owning crate prevents neighboring libraries from growing competing copies of the same policy.
- Deterministic records and checked tests keep adapters reviewable when implementations evolve.

## Where it fits

Within SIM, sim-lib-physics-power owns only the focused contract described above. Adjacent runtime libraries, platform adapters, codecs, and user surfaces can build around it while retaining their own policy. That boundary keeps the kernel small, avoids competing implementations, and lets this capability evolve without forcing unrelated components to change.
