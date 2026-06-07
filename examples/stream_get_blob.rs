use clap::Parser;
use jacquard::StreamingResponse;
use jacquard::api::com_atproto::sync::get_blob::GetBlob;
use jacquard::client::{Agent, FileAuthStore};
use jacquard::common::session::SessionHint;
use jacquard::types::cid::Cid;
use jacquard::types::did::Did;
use jacquard::xrpc::XrpcStreamingClient;
use jacquard_oauth::client::OAuthClient;
use jacquard_oauth::loopback::LoopbackConfig;
use jacquard_oauth::types::AuthorizeOptions;
use n0_future::StreamExt;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Download a blob from a PDS and stream the response, then display it, if it's an image"
)]
struct Args {
    /// Optional handle, DID, or PDS URL used to resume or start OAuth.
    input: Option<String>,

    #[arg(short, long)]
    did: String,

    #[arg(short, long)]
    cid: String,

    /// Path to auth store file (will be created if missing).
    #[arg(long, default_value = "/tmp/jacquard-oauth-session.json")]
    store: String,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    let oauth = OAuthClient::with_default_config(FileAuthStore::new(&args.store));
    let hint = SessionHint::from_optional_input(args.input.as_deref());
    let Some(session) = oauth
        .resume_or_login_with_local_server(
            &hint,
            AuthorizeOptions::default(),
            LoopbackConfig::default(),
        )
        .await?
    else {
        miette::bail!(
            "no stored OAuth session found in {}; pass a handle, DID, or PDS URL to log in",
            args.store
        );
    };

    let agent: Agent<_> = Agent::from(session);
    // Use the streaming `.download()` method with the generated API parameter struct.
    let output: StreamingResponse = agent
        .download(GetBlob {
            did: Did::new(args.did)?,
            cid: Cid::new(args.cid.as_bytes())?,
        })
        .await?;

    let (parts, body_stream) = output.into_parts();

    println!("Parts: {:?}", parts);

    let mut buf: Vec<u8> = Vec::new();
    let mut stream = body_stream.into_inner();

    while let Some(Ok(chunk)) = stream.as_mut().next().await {
        buf.append(&mut chunk.to_vec());
    }

    if let Ok(img) = image::load_from_memory(&buf) {
        viuer::print(&img, &viuer::Config::default()).expect("Image printing failed.");
    }

    Ok(())
}
