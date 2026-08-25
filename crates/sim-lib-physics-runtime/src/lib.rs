#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Loadable composition for the SIM physics stack.
//!
//! This crate contains no solver, audit, proof, study, or finding behavior. It
//! projects the independently selectable layer-owned Shapes and operations as
//! runtime cards through SIM's existing [`sim_kernel::Lib`] contract.

use sim_kernel::{
    AbiVersion, Cx, Export, Lib, LibManifest, LibTarget, Linker, LoadCx, Result, Symbol, Version,
};

/// Stable host-loader identity for the composed physics stack.
pub const HOST_ID: &str = "lib/physics";

/// Thin host-registered library that projects the enabled physics layers.
pub struct PhysicsRuntimeLib;

impl Lib for PhysicsRuntimeLib {
    fn manifest(&self) -> LibManifest {
        LibManifest {
            id: Symbol::qualified("sim", "physics"),
            version: Version(env!("CARGO_PKG_VERSION").to_owned()),
            abi: AbiVersion { major: 0, minor: 1 },
            target: LibTarget::HostRegistered,
            requires: Vec::new(),
            capabilities: Vec::new(),
            exports: surface_symbols()
                .into_iter()
                .map(|symbol| Export::Value { symbol })
                .collect(),
        }
    }

    fn load(&self, cx: &mut LoadCx, linker: &mut Linker<'_>) -> Result<()> {
        for (symbol, layer, kind) in surface_rows() {
            let values = [symbol.to_string(), layer.to_owned(), kind.to_owned()]
                .into_iter()
                .map(|value| cx.factory().string(value))
                .collect::<Result<Vec<_>>>()?;
            linker.value(symbol, cx.factory().list(values)?)?;
        }
        Ok(())
    }
}

/// Installs the enabled physics layer cards exactly once.
pub fn install_physics_runtime(cx: &mut Cx) -> Result<()> {
    let id = PhysicsRuntimeLib.manifest().id;
    if cx.registry().lib(&id).is_none() {
        cx.load_lib(&PhysicsRuntimeLib)?;
    }
    Ok(())
}

/// Returns the stable runtime symbols projected by the enabled layers.
pub fn surface_symbols() -> Vec<Symbol> {
    surface_rows()
        .into_iter()
        .map(|(symbol, _layer, _kind)| symbol)
        .collect()
}

fn surface_rows() -> Vec<(Symbol, &'static str, &'static str)> {
    let mut rows = Vec::new();
    extend(&mut rows, "core", "shape", sim_lib_physics_core::SHAPES);
    #[cfg(feature = "power")]
    {
        extend(&mut rows, "power", "shape", sim_lib_physics_power::SHAPES);
        extend(
            &mut rows,
            "power",
            "operation",
            sim_lib_physics_power::RUNTIME_EXPORTS,
        );
    }
    #[cfg(feature = "audit")]
    {
        extend(&mut rows, "audit", "shape", sim_lib_physics_audit::SHAPES);
        extend(
            &mut rows,
            "audit",
            "operation",
            sim_lib_physics_audit::RUNTIME_EXPORTS,
        );
    }
    #[cfg(feature = "proof")]
    extend(
        &mut rows,
        "proof",
        "operation",
        &["physics/certified-verdict", "physics/refine"],
    );
    #[cfg(feature = "influence")]
    extend(
        &mut rows,
        "influence",
        "operation",
        &["physics/audit-influence", "physics/prepare-selection"],
    );
    #[cfg(feature = "study")]
    for symbol in sim_lib_physics_study::study_surface_symbols() {
        rows.push((symbol, "study", "operation"));
    }
    #[cfg(feature = "findings")]
    extend(
        &mut rows,
        "findings",
        "operation",
        &[
            "physics/open-finding",
            "physics/append-finding",
            "physics/browse-findings",
        ],
    );
    rows
}

fn extend(
    rows: &mut Vec<(Symbol, &'static str, &'static str)>,
    layer: &'static str,
    kind: &'static str,
    names: &[&str],
) {
    rows.extend(names.iter().map(|name| (parse_symbol(name), layer, kind)));
}

fn parse_symbol(name: &str) -> Symbol {
    name.split_once('/').map_or_else(
        || Symbol::new(name),
        |(namespace, local)| Symbol::qualified(namespace, local),
    )
}
