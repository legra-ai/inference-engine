use pyo3::{
    exceptions::{PyRuntimeError, PyValueError},
    prelude::*,
};

#[pyfunction]
fn compile_profile(selector: &str) -> PyResult<String> {
    let profile = inference_engine::compile_profile(selector)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    serde_json::to_string(&profile).map_err(|error| PyRuntimeError::new_err(error.to_string()))
}

#[pyfunction]
fn profiles() -> Vec<&'static str> {
    // bounded: the shared catalogue has five named presets, no graph data.
    inference_engine::profiles().collect()
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add_function(wrap_pyfunction!(compile_profile, module)?)?;
    module.add_function(wrap_pyfunction!(profiles, module)?)?;
    Ok(())
}
