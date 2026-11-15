#![allow(missing_docs)]

use rhusky::Rhusky;

mod build_support;

fn main() {
    let catalogue = build_support::rule_catalogue(include_str!("Cargo.toml"))
        .expect("cannot identify the compiled RDF rule authority");
    println!("cargo:rustc-env=INFERENCE_ENGINE_RULE_CATALOGUE={catalogue}");
    Rhusky::new()
        .hooks_dir(".githooks")
        .skip_in_env("CI")
        .skip_in_env("GITHUB_ACTIONS")
        .with_default_hooks()
        .install_from_build_script(env!("CARGO_MANIFEST_DIR"))
        .expect("failed to install repository Git hooks");
}
