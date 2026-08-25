# sim-lib-physics-runtime

The loadable composition boundary for SIM physics. It projects Cards for the
enabled core, power, audit, proof, influence, study, and findings layers through
the ordinary kernel `Lib` contract. Domain behavior remains in those layer
crates; this crate only assembles their public runtime surface.

The default feature is `core`. Higher layers are explicit and one-way: `power`,
`audit`, `proof`, `influence`, `study`, `findings`, and `full`.
