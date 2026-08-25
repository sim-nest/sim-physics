use sim_lib_physics_audit::{EnergyBalanceResidual, RUNTIME_EXPORTS, SHAPES};

fn main() {
    println!("r_E = E(t1) - E(t0) - sum(port work) - sum(event transfer)");
    println!(
        "lane={} quantity={}",
        EnergyBalanceResidual::KIND,
        EnergyBalanceResidual::QUANTITY
    );
    println!(
        "shapes={} runtime-exports={}",
        SHAPES.len(),
        RUNTIME_EXPORTS.len()
    );
}
