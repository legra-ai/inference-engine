//! Execute published authored data with explicit incomplete-capability reporting.
mod authored_support;
#[tokio::test]
async fn published_rdf12_authored_context_proofs() {
    let report = authored_support::run().await;
    println!("{}", serde_json::to_string(&report).unwrap());
    assert_eq!(report.cases.len(), 18);
    assert_eq!(report.passed(), 12);
    assert_eq!(report.unsupported(), 6);
    assert_eq!(report.failed(), 0);
    assert_eq!(report.errors(), 0);
}
