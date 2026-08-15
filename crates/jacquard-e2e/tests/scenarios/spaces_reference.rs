//! Provider-specific reference spaces scenario target. Gated behind the
//! `reference` feature so the lifecycle controller runs it only against the
//! reference spaces-alpha provider.

use jacquard_e2e::bootstrap::FixtureStack;
use jacquard_e2e::provider::Provider;

fn stack() -> FixtureStack {
    let stack = FixtureStack::from_env().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        stack.context.provider,
        Provider::Reference,
        "spaces scenarios target the reference provider"
    );
    stack
}

#[tokio::test]
async fn reference_spaces_lifecycle_and_member_boundary() {
    let stack = stack();
    stack
        .ensure_bootstrapped()
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    jacquard_e2e::spaces::reference_spaces_lifecycle_and_member_boundary(&stack)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
}
