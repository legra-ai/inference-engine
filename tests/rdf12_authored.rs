//! Execute published authored data with explicit incomplete-capability reporting.
mod authored_support;
#[tokio::test]
async fn published_rdf12_authored_source_proofs() {
    let report = authored_support::run().await;
    assert_eq!(report.cases.len(), 18);
    assert_eq!(report.passed(), 8);
    assert_eq!(report.unsupported(), 10);
    assert_eq!(report.failed(), 0);
    assert_eq!(report.errors(), 0);
    println!("{}", serde_json::to_string(&report).unwrap());
}
