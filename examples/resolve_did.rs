use clap::Parser;
use jacquard::client::BasicClient;
use jacquard::types::string::Handle;
use jacquard_identity::resolver::IdentityResolver;
use miette::IntoDiagnostic;

#[derive(Parser, Debug)]
#[command(author, version, about = "Resolve a handle to its DID document")]
struct Args {
    /// Handle to resolve (e.g., alice.bsky.social)
    #[arg(default_value = "pfrazee.com")]
    handle: String,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    // Parse the handle
    let handle = Handle::new(&args.handle)?;

    // Create an unauthenticated client with identity resolver
    let client = BasicClient::unauthenticated();

    // Resolve handle to DID
    println!("Resolving handle: {}", handle);
    let did = client
        .resolve_handle(&handle)
        .await
        .map_err(|e| miette::miette!("Failed to resolve handle: {}", e))?;

    println!("DID: {}\n", did);

    // Resolve DID document
    let doc_response = client
        .resolve_did_doc(&did)
        .await
        .map_err(|e| miette::miette!("Failed to resolve DID document: {}", e))?;

    let doc = doc_response
        .parse()
        .map_err(|e| miette::miette!("Failed to parse DID document: {}", e))?;

    println!("DID Document:");
    println!("ID: {}", doc.id);

    if let Some(aka) = &doc.also_known_as {
        if !aka.is_empty() {
            println!("\nAlso Known As:");
            for handle in aka {
                println!("  - {}", handle);
            }
        }
    }

    if let Some(verification_methods) = &doc.verification_method {
        println!("\nVerification Methods:");
        for method in verification_methods {
            println!("  ID: {}", method.id);
            println!("  Type: {}", method.r#type);
            if let Some(controller) = &method.controller {
                println!("  Controller: {}", controller);
            }
            if let Some(key) = &method.public_key_multibase {
                println!("  Public Key (multibase): {}", key);
            }
            if !method.extra_data.is_empty() {
                println!("  Extra fields: {:?}", method.extra_data);
            }
            println!();
        }
    }

    if let Some(services) = &doc.service {
        println!("Services:");
        for service in services {
            println!("  ID: {}", service.id);
            println!("  Type: {}", service.r#type);
            if let Some(endpoint) = &service.service_endpoint {
                println!("  Endpoint: {:?}", endpoint);
            }
            if !service.extra_data.is_empty() {
                println!("  Extra fields: {:?}", service.extra_data);
            }
            println!();
        }
    }

    if !doc.extra_data.is_empty() {
        for (key, value) in &doc.extra_data {
            println!(
                "{}: {}",
                key,
                serde_json::to_string_pretty(value).into_diagnostic()?
            );
        }
    }

    Ok(())
}
