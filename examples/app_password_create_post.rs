use clap::Parser;
use jacquard::api::app_bsky::feed::post::Post;
use jacquard::client::credential_session::{
    CredentialLoginOptions, CredentialResumeResult, CredentialSession,
};
use jacquard::client::{Agent, AgentSessionExt, FileAuthStore};
use jacquard::common::session::SessionHint;
use jacquard::identity::JacquardResolver;
use jacquard::types::string::Datetime;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(author, version, about = "Create a simple post")]
struct Args {
    /// Optional handle, DID, or email used to resume or start app-password auth.
    identifier: Option<String>,

    /// App password, required only when no stored credential session matches.
    password: Option<String>,

    /// Post text.
    #[arg(short, long)]
    text: String,

    /// Path to auth store file (will be created if missing).
    #[arg(long, default_value = "/tmp/jacquard-app-password-session.json")]
    store: String,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    let store = Arc::new(FileAuthStore::new(&args.store));
    let resolver = Arc::new(JacquardResolver::new(
        reqwest::Client::new(),
        Default::default(),
    ));
    let session = CredentialSession::new(store, resolver);
    let hint = SessionHint::from_optional_input(args.identifier.as_deref());

    let auth = match session.resume(&hint).await? {
        CredentialResumeResult::Resumed(auth) => auth,
        CredentialResumeResult::LoginRequired(challenge) => {
            let Some(password) = args.password.as_ref() else {
                miette::bail!(
                    "no stored app-password session found in {}; pass a handle or DID and app password to log in",
                    args.store
                );
            };
            session
                .login_from_challenge(
                    challenge,
                    CredentialLoginOptions {
                        password: password.clone().into(),
                        identifier: args.identifier.clone().map(Into::into),
                        allow_takendown: None,
                        auth_factor_token: None,
                        pds: None,
                    },
                )
                .await?
        }
    };
    println!("Signed in as {}", auth.handle);

    let agent: Agent<_> = Agent::from(session);

    // Create a simple text post using the Agent convenience method.
    let post = Post::new()
        .text(args.text)
        .created_at(Datetime::now())
        .build();
    let output = agent.create_record(post, None).await?;
    println!("✓ Created post: {}", output.uri);

    Ok(())
}
