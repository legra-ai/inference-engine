use napi_derive::napi;

#[napi]
pub fn compile_profile(selector: String) -> napi::Result<String> {
    let profile = inference_engine::compile_profile(&selector)
        .map_err(|error| napi::Error::from_reason(error.to_string()))?;
    serde_json::to_string(&profile).map_err(|error| napi::Error::from_reason(error.to_string()))
}

#[napi]
pub fn profiles() -> Vec<String> {
    // bounded: the shared catalogue has five named presets, no graph data.
    inference_engine::profiles().map(str::to_owned).collect()
}

#[napi]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
