//! Discoverable route anchors for the self-contained analytic studies.
//! The cross-layer execution specimen lives with `sim-lib-physics-study`;
//! these exact laws make the routed scientific questions independently
//! checkable inside the repository's declared Index discovery scope.

#[test]
fn switched_network_and_impact_laws_are_self_contained() {
    // Integral of 18t on [0,1/2], then 9t on [1/2,1].
    let switched_work = 9.0 / 4.0 + 27.0 / 8.0;
    assert_eq!(switched_work, 45.0 / 8.0);

    // m1=2, m2=3, u1=4, u2=-1, restitution=1/2.
    let (v1, v2) = (-0.5, 2.0);
    assert_eq!(2.0 * 4.0 + 3.0 * -1.0, 2.0 * v1 + 3.0 * v2);
    let before = 0.5 * 2.0 * 4.0_f64.powi(2) + 0.5 * 3.0 * (-1.0_f64).powi(2);
    let after = 0.5 * 2.0 * v1.powi(2) + 0.5 * 3.0 * v2.powi(2);
    assert_eq!(before - after, 45.0 / 4.0);
}
