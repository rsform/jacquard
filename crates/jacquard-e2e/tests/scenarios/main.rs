//! Feature-gated e2e scenario target. Never compiled or listed without the
//! `e2e` feature; the lifecycle controller (`scripts/e2e.sh`) invokes it with
//! provider-filtered nextest runs.

use jacquard_e2e::bootstrap::FixtureStack;
fn stack() -> FixtureStack {
    FixtureStack::from_env().unwrap_or_else(|e| panic!("{e}"))
}

#[tokio::test]
async fn credential_record_crud_contract() {
    let stack = stack();
    stack
        .ensure_bootstrapped()
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    jacquard_e2e::scenarios::credential_record_crud_contract(&stack)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
}

#[tokio::test]
async fn blob_record_roundtrip_contract() {
    let stack = stack();
    stack
        .ensure_bootstrapped()
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    jacquard_e2e::scenarios::blob_record_roundtrip_contract(&stack)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
}
