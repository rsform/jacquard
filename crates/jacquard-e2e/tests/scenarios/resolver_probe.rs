//! Manual probe against a kept (--keep) stack: run with the
//! JACQUARD_E2E_* environment variables the lifecycle controller exports.

use jacquard_common::types::string::{Did, Handle};
use jacquard_e2e::bootstrap::FixtureStack;
use jacquard_e2e::provider::ProviderContext;
use jacquard_identity::resolver::IdentityResolver;

#[tokio::test]
async fn resolver_probe() {
    let context = ProviderContext::from_env().unwrap_or_else(|e| panic!("{e}"));
    let stack = FixtureStack::from_context(context).unwrap_or_else(|e| panic!("{e}"));
    let resolver = stack.resolver();

    let did = Did::new(stack.context.identity.did).expect("fixture DID");
    let _doc = resolver
        .resolve_did_doc(&did)
        .await
        .unwrap_or_else(|e| panic!("resolve_did_doc failed: {e:?}"));
    println!("resolved doc ok");

    let handle = Handle::new(stack.context.identity.handle).expect("fixture handle");
    let did_from_handle = resolver
        .resolve_handle(&handle)
        .await
        .unwrap_or_else(|e| panic!("resolve_handle failed: {e}"));
    println!("handle -> {did_from_handle}");
}
