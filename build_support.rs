//! Build-time authority identity for source and normalized Cargo manifests.

/// Derive the catalogue identity from the exact manifest dependency pin.
pub fn rule_catalogue(manifest: &str) -> Result<String, String> {
    let table: toml::Table = manifest
        .parse()
        .map_err(|e: toml::de::Error| e.to_string())?;
    let dependency = table
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .and_then(|dependencies| dependencies.get("rdf-reasoning"))
        .ok_or("missing rdf-reasoning dependency")?;
    let requirement = match dependency {
        toml::Value::String(version) => Some(version.as_str()),
        toml::Value::Table(dependency) => dependency.get("version").and_then(toml::Value::as_str),
        _ => None,
    }
    .ok_or("rdf-reasoning requires an explicit version")?;
    let version = requirement
        .strip_prefix('=')
        .filter(|version| !version.is_empty() && version.trim() == *version)
        .ok_or("rdf-reasoning must use a canonical exact version pin")?;
    Ok(format!("rdf-reasoning/{version}"))
}
