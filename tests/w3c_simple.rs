//! Pinned W3C RDF 1.2 semantic cases, with an independent test-only syntax adapter.
mod w3c_support;

#[tokio::test]
async fn w3c_rdf12_simple_entailment() {
    let report = w3c_support::run_simple().await;
    assert_eq!(
        report.passed, 18,
        "the pinned supported partition must not shrink"
    );
    assert_eq!(
        report.unsupported_datatypes,
        ["opaque-literal"],
        "additional datatype recognition must be explicit"
    );
}
