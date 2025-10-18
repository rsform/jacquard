use clap::Parser;
use jacquard::StreamingResponse;
use jacquard::api::com_atproto::sync::get_blob::GetBlob;
use jacquard::client::Agent;
use jacquard::types::cid::Cid;
use jacquard::types::did::Did;
use jacquard::xrpc::XrpcStreamingClient;
use jacquard_oauth::authstore::MemoryAuthStore;
use jacquard_oauth::client::OAuthClient;
use jacquard_oauth::loopback::LoopbackConfig;
use n0_future::StreamExt;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Download a blob from a PDS and stream the response, then display it, if it's an image"
)]
struct Args {
    input: String,
    #[arg(short, long)]
    did: String,
    #[arg(short, long)]
    cid: String,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    let oauth = OAuthClient::with_default_config(MemoryAuthStore::new());
    let session = oauth
        .login_with_local_server(args.input, Default::default(), LoopbackConfig::default())
        .await?;

    let agent: Agent<_> = Agent::from(session);
    // Use the streaming `.download()` method with the generated API parameter struct
    let output: StreamingResponse = agent
        .download(GetBlob {
            did: Did::new_owned(args.did)?,
            cid: Cid::str(&args.cid),
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
