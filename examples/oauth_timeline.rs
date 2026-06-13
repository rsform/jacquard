use clap::Parser;
use jacquard::api::app_bsky::feed::get_timeline::GetTimeline;
use jacquard::client::{Agent, FileAuthStore};
#[cfg(not(feature = "loopback"))]
use jacquard::common::deps::fluent_uri::Uri;
use jacquard::common::session::SessionHint;
use jacquard::oauth::atproto::AtprotoClientMetadata;
use jacquard::oauth::client::OAuthClient;
#[cfg(not(feature = "loopback"))]
use jacquard::oauth::client::OAuthResumeOrLogin;
#[cfg(feature = "loopback")]
use jacquard::oauth::loopback::LoopbackConfig;
use jacquard::oauth::scopes::Scopes;
use jacquard::oauth::types::AuthorizeOptions;
#[cfg(not(feature = "loopback"))]
use jacquard::oauth::types::CallbackParams;
use jacquard::xrpc::XrpcClient;
use miette::IntoDiagnostic;
use std::io::{BufRead, Write, stdin, stdout};

#[derive(Parser, Debug)]
#[command(author, version, about = "Jacquard - OAuth loopback demo")]
struct Args {
    /// Optional handle, DID, or PDS URL used to start or resume OAuth.
    input: Option<String>,

    /// Path to auth store file (will be created if missing)
    #[arg(long, default_value = "/tmp/jacquard-oauth-session.json")]
    store: String,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();

    // File-backed auth store shared by OAuthClient and session registry.
    let store = FileAuthStore::new(&args.store);
    let client_data = jacquard_oauth::session::ClientData {
        keyset: None,
        // Default sets normal localhost redirect URIs and "atproto transition:generic" scopes.
        // The localhost helper will ensure you have at least "atproto" and will fix urls.
        config: AtprotoClientMetadata::default_localhost(),
    };

    // Build an OAuth client (this is reusable, and can create multiple sessions).
    let oauth = OAuthClient::new(store, client_data, reqwest::Client::new());
    let hint = SessionHint::from_optional_input(args.input.as_deref());
    // The atproto docs include a scope string builder for choosing permissions:
    // https://atproto.com/guides/scope-builder. In Jacquard code, use typed
    // helpers when possible so scope NSIDs stay tied to endpoint types.
    let timeline_scopes = Scopes::builder()
        .atproto()
        .rpc_request_aud::<GetTimeline>("did:web:api.bsky.app#bsky_appview")?
        .build()?;

    #[cfg(feature = "loopback")]
    let session = match oauth
        .resume_or_login_with_local_server(
            &hint,
            AuthorizeOptions::default().with_scopes(timeline_scopes.clone()),
            LoopbackConfig::default(),
        )
        .await?
    {
        Some(session) => session,
        None => {
            let input = prompt_login_input(&args.store)?;
            oauth
                .login_with_local_server(
                    input,
                    AuthorizeOptions::default().with_scopes(timeline_scopes.clone()),
                    LoopbackConfig::default(),
                )
                .await?
        }
    };

    #[cfg(not(feature = "loopback"))]
    let session = match oauth
        .resume_or_start_auth(
            &hint,
            AuthorizeOptions::default().with_scopes(timeline_scopes.clone()),
        )
        .await
    {
        Ok(OAuthResumeOrLogin::Resumed(session)) => session,
        Ok(OAuthResumeOrLogin::LoginUrl(auth_url)) => finish_manual_oauth(&oauth, auth_url).await?,
        Ok(OAuthResumeOrLogin::NeedsInput) => {
            let input = prompt_login_input(&args.store)?;
            match oauth
                .resume_or_start_auth_for(
                    input,
                    AuthorizeOptions::default().with_scopes(timeline_scopes.clone()),
                )
                .await?
            {
                OAuthResumeOrLogin::Resumed(session) => session,
                OAuthResumeOrLogin::LoginUrl(auth_url) => {
                    finish_manual_oauth(&oauth, auth_url).await?
                }
                OAuthResumeOrLogin::NeedsInput => {
                    miette::bail!("login input must be a handle, DID, or PDS URL")
                }
            }
        }
        Err(err) => {
            return Err(err).into_diagnostic();
        }
    };

    // Wrap in Agent and fetch the timeline.
    let agent: Agent<_> = Agent::from(session);
    let output = agent.send(GetTimeline::new().limit(5).build()).await?;
    let timeline = output.into_output()?;
    for (i, post) in timeline.feed.iter().enumerate() {
        println!("\n{}. by {}", i + 1, post.post.author.handle);
        println!(
            "   {}",
            serde_json::to_string_pretty(&post.post.record).into_diagnostic()?
        );
    }

    Ok(())
}

fn prompt_login_input(store_path: &str) -> miette::Result<String> {
    eprintln!("No stored OAuth session was found in {store_path}.");
    read_required_line("Enter a handle, DID, or PDS URL to log in: ")
}

#[cfg(not(feature = "loopback"))]
async fn finish_manual_oauth<T, S>(
    oauth: &OAuthClient<T, S>,
    auth_url: String,
) -> miette::Result<jacquard::oauth::client::OAuthSession<T, S>>
where
    T: jacquard::oauth::resolver::OAuthResolver
        + jacquard::oauth::dpop::DpopExt
        + Send
        + Sync
        + 'static,
    S: jacquard::oauth::authstore::ClientAuthStore + Send + Sync + 'static,
{
    eprintln!("Open this URL in your browser to authorize Jacquard:");
    eprintln!("\n{auth_url}\n");
    let callback_url = read_required_line("Paste the full redirect URL after authorization: ")?;
    let params = callback_params_from_redirect(&callback_url)?;
    oauth.callback(params).await.into_diagnostic()
}

fn read_required_line(prompt: &str) -> miette::Result<String> {
    print!("{prompt}");
    stdout().flush().into_diagnostic()?;

    let mut line = String::new();
    stdin().lock().read_line(&mut line).into_diagnostic()?;
    let line = line.trim().to_owned();
    if line.is_empty() {
        miette::bail!("input must not be empty");
    }
    Ok(line)
}

#[cfg(not(feature = "loopback"))]
fn callback_params_from_redirect(callback_url: &str) -> miette::Result<CallbackParams> {
    let query = redirect_query(callback_url)?;
    serde_html_form::from_str(query).into_diagnostic()
}

#[cfg(not(feature = "loopback"))]
fn redirect_query(callback_url: &str) -> miette::Result<&str> {
    let callback_url = callback_url.trim();
    if callback_url.is_empty() {
        miette::bail!("redirect URL must not be empty");
    }

    if let Ok(uri) = Uri::parse(callback_url) {
        return uri
            .query()
            .filter(|query| !query.is_empty())
            .map(|query| query.as_str())
            .ok_or_else(|| {
                miette::miette!("redirect URL must include OAuth callback query parameters")
            });
    }

    Ok(callback_url)
}

#[cfg(all(test, not(feature = "loopback")))]
mod tests {
    use super::*;

    #[test]
    fn parses_full_redirect_url() {
        let params = callback_params_from_redirect(
            "http://127.0.0.1:8080/callback?code=abc&state=xyz&iss=https%3A%2F%2Fexample.com",
        )
        .unwrap();
        assert_eq!(params.code, "abc");
        assert_eq!(params.state.as_deref(), Some("xyz"));
        assert_eq!(params.iss.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn parses_raw_query_string() {
        let params = callback_params_from_redirect("code=abc&state=xyz").unwrap();
        assert_eq!(params.code, "abc");
        assert_eq!(params.state.as_deref(), Some("xyz"));
        assert_eq!(params.iss, None);
    }

    #[test]
    fn rejects_empty_redirect_input() {
        assert!(callback_params_from_redirect("").is_err());
    }
}
