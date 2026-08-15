//! OAuth session bootstrap for fixture identities: host the client metadata
//! through the native ingress, run Jacquard's full PAR/DPoP authorization
//! flow, and drive the provider's authorization UI programmatically through
//! its JSON API (cookie-aware).

use jacquard::identity::JacquardResolver;
use jacquard_common::DefaultStr;
use jacquard_common::deps::fluent_uri;
use jacquard_common::deps::smol_str::SmolStr;
use jacquard_oauth::atproto::{AtprotoClientMetadata, GrantType, atproto_client_metadata};
use jacquard_oauth::authstore::MemoryAuthStore;
use jacquard_oauth::client::{CallbackValidationPolicy, OAuthClient, OAuthSession};
use jacquard_oauth::scopes::Scopes;
use jacquard_oauth::session::ClientData;
use jacquard_oauth::types::{AuthorizeOptions, CallbackParams};

use crate::bootstrap::FixtureStack;
use crate::provider::FixtureIdentity;
use crate::transport::FixtureTransport;

/// The hosted client metadata URL doubles as the OAuth client id. The host
/// uses a non-special-use TLD: the atproto OAuth profile rejects client ids
/// on local-hostname TLDs such as `.test`. Resolution stays fixture-internal
/// (fixture DNS + host resolve mappings), so the client does not use public
/// identity resolution.
pub const CLIENT_ID: &str = "https://client.jacquard-e2e.dev/client-metadata.json";
const REDIRECT_URI: &str = "https://client.jacquard-e2e.dev/oauth-callback";
/// Scopes covering `com.atproto.access`-gated endpoints such as
/// `com.atproto.space.getDelegationToken`. The space grant is required for
/// the token to carry per-space permissions at consent time.
const CLIENT_SCOPE: &str = "atproto transition:generic space:dev.jacquard.e2e.space?authority=*&action=read&action=create&action=update&action=delete";
const TRANQUIL_SCOPE: &str = "atproto rpc?aud=did:web:localhost.jacquard-e2e.test:service&lxm=test.blobby.get repo:app.bsky.feed.post blob:text/plain";

/// API prefix of the reference PDS's authorization UI, served on the same
/// origin as the authorization endpoint.
const PROVIDER_API: &str = "/@atproto/oauth-provider/~api";

pub fn client_metadata() -> AtprotoClientMetadata<SmolStr> {
    AtprotoClientMetadata {
        client_id: fluent_uri::Uri::parse(CLIENT_ID.to_string()).expect("client id is a URI"),
        client_uri: None,
        redirect_uris: vec![
            fluent_uri::Uri::parse(REDIRECT_URI.to_string()).expect("redirect URI is a URI"),
        ],
        grant_types: vec![GrantType::AuthorizationCode, GrantType::RefreshToken],
        scopes: Scopes::<SmolStr>::new(SmolStr::from(CLIENT_SCOPE))
            .expect("client scopes are valid"),
        jwks_uri: None,
        client_name: Some(SmolStr::new_static("jacquard-e2e")),
        logo_uri: None,
        tos_uri: None,
        privacy_policy_uri: None,
    }
}

/// Write the wire-format client metadata document the ingress serves at
/// `CLIENT_ID`, derived from Jacquard's own metadata conversion. Idempotent;
/// the ingress reads the file per request.
pub fn ensure_client_metadata_file(stack: &FixtureStack) -> Result<(), String> {
    let path = stack.files.root.join("client-metadata.json");
    if path.exists() {
        return Ok(());
    }
    let wire = atproto_client_metadata(&client_metadata(), &None)
        .map_err(|e| format!("convert client metadata: {e}"))?;
    let json = serde_json::to_string_pretty(&wire)
        .map_err(|e| format!("serialize client metadata: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("write client metadata: {e}"))?;
    Ok(())
}

/// Build an `OAuthClient` over the fixture resolver and in-memory auth store.
/// The store must live as long as the flow: `start_auth` persists PAR state
/// that `callback` consumes through it.
pub fn oauth_client(
    stack: &FixtureStack,
) -> OAuthClient<JacquardResolver<FixtureTransport>, MemoryAuthStore> {
    OAuthClient::new(
        MemoryAuthStore::new(),
        ClientData {
            keyset: None,
            config: client_metadata(),
        },
        stack.transport.as_ref().clone(),
    )
}

/// Run the complete authorization flow for `identity` without a browser:
/// PAR through Jacquard's client, JSON sign-in and consent through the
/// provider's cookie-aware UI API, then Jacquard's callback/code exchange.
pub async fn oauth_session(
    stack: &FixtureStack,
    identity: &FixtureIdentity,
) -> Result<OAuthSession<JacquardResolver<FixtureTransport>, MemoryAuthStore>, String> {
    ensure_client_metadata_file(stack)?;

    let password =
        std::fs::read_to_string(stack.files.root.join("reference").join("admin-password"))
            .map_err(|e| format!("read account password: {e}"))?
            .trim()
            .to_string();

    let client = oauth_client(stack);
    let authorize_url = client
        .start_auth(
            identity.handle,
            AuthorizeOptions::<String> {
                redirect_uri: None,
                scopes: Scopes::new(CLIENT_SCOPE.to_string())
                    .map_err(|e| format!("parse client scopes: {e}"))?,
                prompt: None,
                state: Some("jacquard-e2e".to_string()),
            },
        )
        .await
        .map_err(|e| stack.sanitize_error("oauth PAR", e))?;

    let session = drive_ui_consent(stack, &authorize_url, identity, &password).await?;

    let params = CallbackParams {
        code: session.code,
        state: Some(session.state),
        iss: session.iss,
    };
    let oauth = client.callback(params).await.map_err(|e| {
        // Never include the code or state in diagnostics.
        stack.sanitize_error("oauth callback", e)
    })?;
    Ok(oauth)
}

/// The Tranquil OAuth session bootstrap: a loopback (native) client — the
/// published Tranquil image's OAuth metadata fetcher only reaches global
/// hosts, so a web client id on the fixture ingress is unreachable; the
/// loopback path builds client metadata locally on the provider with no
/// fetch at all. Authorization is driven through Tranquil's JSON submit
/// API (no cookies or CSRF, unlike the reference's SPA).
pub type FixtureOAuthClient = OAuthClient<JacquardResolver<FixtureTransport>, MemoryAuthStore>;

pub struct TranquilAuthorization {
    pub client: FixtureOAuthClient,
    pub callback: CallbackParams,
}

pub async fn tranquil_oauth_authorization(
    stack: &FixtureStack,
    identity: &FixtureIdentity,
) -> Result<TranquilAuthorization, String> {
    let password =
        std::fs::read_to_string(stack.files.root.join("tranquil").join("admin-password"))
            .map_err(|e| format!("read account password: {e}"))?
            .trim()
            .to_string();

    let client_id = "http://localhost?redirect_uri=http%3A%2F%2F127.0.0.1%2Foauth-callback&scope=atproto%20rpc%3Faud%3Ddid%3Aweb%3Alocalhost.jacquard-e2e.test%3Aservice%26lxm%3Dtest.blobby.get%20repo%3Aapp.bsky.feed.post%20blob%3Atext%2Fplain";
    let redirect_uri = "http://127.0.0.1/oauth-callback";
    let metadata = AtprotoClientMetadata {
        client_id: fluent_uri::Uri::parse(client_id.to_string())
            .map_err(|e| format!("loopback client id: {e:?}"))?,
        client_uri: None,
        redirect_uris: vec![
            fluent_uri::Uri::parse(redirect_uri.to_string())
                .map_err(|e| format!("loopback redirect URI: {e:?}"))?,
        ],
        grant_types: vec![GrantType::AuthorizationCode, GrantType::RefreshToken],
        scopes: Scopes::<SmolStr>::new(SmolStr::new_static(TRANQUIL_SCOPE))
            .map_err(|e| format!("parse loopback scopes: {e}"))?,
        jwks_uri: None,
        client_name: Some(SmolStr::new_static("jacquard-e2e")),
        logo_uri: None,
        tos_uri: None,
        privacy_policy_uri: None,
    };
    let client = OAuthClient::new(
        MemoryAuthStore::new(),
        ClientData {
            keyset: None,
            config: metadata,
        },
        stack.transport.as_ref().clone(),
    )
    .with_callback_validation(CallbackValidationPolicy::AllowMissingIssuer);

    let authorize_url = client
        .start_auth(
            identity.did,
            AuthorizeOptions::<String> {
                redirect_uri: None,
                scopes: Scopes::new(TRANQUIL_SCOPE.to_string())
                    .map_err(|e| format!("parse client scopes: {e}"))?,
                prompt: None,
                state: Some("jacquard-e2e".to_string()),
            },
        )
        .await
        .map_err(|e| stack.sanitize_error("oauth PAR (tranquil)", e))?;

    // Tranquil's authorize endpoint takes the credentials as a JSON body;
    // the request_uri rides the authorize URL's query.
    let request_uri = authorize_url
        .split("request_uri=")
        .nth(1)
        .and_then(|rest| rest.split('&').next())
        .map(|raw| urldecode(raw))
        .ok_or("authorize URL missing request_uri")?;

    let base = stack.context.coordinates.provider_url.trim_end_matches('/');
    let http = stack.cookie_client()?;
    let submit = http
        .post(format!("{base}/oauth/authorize"))
        .header("accept", "application/json")
        .json(&serde_json::json!({
            "request_uri": request_uri,
            "username": identity.handle,
            "password": password,
            "remember_device": false,
        }))
        .send()
        .await
        .map_err(|e| format!("authorize submit: {e}"))?;
    if !submit.status().is_success() {
        let body = submit.text().await.unwrap_or_default();
        let excerpt: String = body.chars().filter(|c| !c.is_control()).take(200).collect();
        return Err(format!("authorize submit failed: {excerpt}"));
    }
    let body: serde_json::Value = submit
        .json()
        .await
        .map_err(|e| format!("decode authorize response: {e}"))?;
    let mut location = body
        .get("redirect_uri")
        .and_then(|v| v.as_str())
        .ok_or("authorize response missing redirect_uri")?
        .to_string();

    // A consent hop may be required; its response carries the code.
    if location.contains("/oauth/consent") {
        let consent = http
            .post(format!("{base}/oauth/authorize/consent"))
            .header("accept", "application/json")
            .json(&serde_json::json!({
                "request_uri": request_uri,
                "approved_scopes": [
                    "atproto",
                    "repo:app.bsky.feed.post",
                    "blob:text/plain",
                    "rpc?aud=did:web:localhost.jacquard-e2e.test:service&lxm=test.blobby.get"
                ],
                "remember": false,
            }))
            .send()
            .await
            .map_err(|e| format!("consent submit: {e}"))?;
        if !consent.status().is_success() {
            let body = consent.text().await.unwrap_or_default();
            let excerpt: String = body.chars().filter(|c| !c.is_control()).take(200).collect();
            return Err(format!("consent submit failed: {excerpt}"));
        }
        let body: serde_json::Value = consent
            .json()
            .await
            .map_err(|e| format!("decode consent response: {e}"))?;
        location = body
            .get("redirect_uri")
            .and_then(|v| v.as_str())
            .ok_or("consent response missing redirect_uri")?
            .to_string();
    }

    let params = extract_callback_params(&location)?;
    Ok(TranquilAuthorization {
        client,
        callback: CallbackParams {
            code: params.code,
            state: Some(params.state),
            iss: params.iss,
        },
    })
}

struct UiConsent {
    code: DefaultStr,
    state: DefaultStr,
    iss: Option<DefaultStr>,
}

/// Drive the provider's authorization UI over its JSON API: fetch the
/// authorize page (establishing the UI session cookies), sign in with the
/// fixture password, and consent for the identity's DID. Returns the code
/// and state extracted from the consent redirect URL.
async fn drive_ui_consent(
    stack: &FixtureStack,
    authorize_url: &str,
    identity: &FixtureIdentity,
    password: &str,
) -> Result<UiConsent, String> {
    // A cookie-aware clone of the fixture transport's reqwest client: same
    // CA, per-host resolve mappings, no redirects, no proxies. Only the
    // authorize page origin is ever contacted, by construction.
    let http = stack.cookie_client()?;

    let page = http
        .get(authorize_url)
        // The authorize UI requires browser-style Fetch Metadata headers.
        .header("accept", "text/html")
        .header("sec-fetch-site", "same-origin")
        .header("sec-fetch-mode", "navigate")
        .header("sec-fetch-dest", "document")
        .header("sec-fetch-user", "?1")
        .header("upgrade-insecure-requests", "1")
        .send()
        .await
        .map_err(|e| format!("fetch authorize page: {e}"))?;
    // A 303 here means the provider already has a completed authorization
    // for this account+client and redirects straight to the callback; the
    // code/state ride its Location header.
    if page.status() == http::StatusCode::FOUND || page.status() == http::StatusCode::SEE_OTHER {
        let location = page
            .headers()
            .get(http::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or("authorize redirect missing location")?
            .to_string();
        eprintln!("oauth driver: authorize 303 -> {location}");
        return extract_callback_params(&location);
    }
    if !page.status().is_success() {
        return Err(format!(
            "authorize page returned {}",
            page.status().as_u16()
        ));
    }
    let set_cookies = page
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    // Drain the body so cookies from the response are fully applied.
    let _ = page.text().await;

    let authorize_origin = origin_of(authorize_url)?;
    let api = format!("{authorize_origin}{PROVIDER_API}");

    // The UI API requires the CSRF token issued with the authorize page
    // (cookie `csrf-token`, echoed back as `x-csrf-token`).
    let csrf = set_cookies
        .iter()
        .find_map(|c| c.strip_prefix("csrf-token="))
        .map(|v| v.split(';').next().unwrap_or(v).to_string())
        .ok_or("authorize page set no csrf-token cookie")?;

    let sign_in = http
        .post(format!("{api}/sign-in"))
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin")
        .header("sec-fetch-mode", "same-origin")
        .header("sec-fetch-dest", "empty")
        .header("referer", authorize_url)
        .header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "locale": "en",
            "username": identity.handle,
            "password": password,
            "remember": true,
        }))
        .send()
        .await
        .map_err(|e| format!("sign-in request: {e}"))?;
    if !sign_in.status().is_success() {
        // The response body never contains the password, but keep only a
        // short excerpt for the diagnostic.
        let body = sign_in.text().await.unwrap_or_default();
        let excerpt: String = body.chars().filter(|c| !c.is_control()).take(200).collect();
        return Err(format!("sign-in failed: {excerpt}"));
    }
    let signed_in: serde_json::Value = sign_in
        .json()
        .await
        .map_err(|e| format!("decode sign-in response: {e}"))?;
    let account_did = signed_in
        .pointer("/account/did")
        .and_then(|v| v.as_str())
        .ok_or("sign-in response missing account.did")?
        .to_string();
    if account_did != identity.did {
        return Err(format!(
            "sign-in returned a different DID than the fixture identity"
        ));
    }

    let consent = http
        .post(format!("{api}/consent"))
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin")
        .header("sec-fetch-mode", "same-origin")
        .header("sec-fetch-dest", "empty")
        .header("referer", authorize_url)
        .header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "did": account_did }))
        .send()
        .await
        .map_err(|e| format!("consent request: {e}"))?;
    if !consent.status().is_success() {
        let body = consent.text().await.unwrap_or_default();
        let excerpt: String = body.chars().filter(|c| !c.is_control()).take(200).collect();
        return Err(format!("consent failed: {excerpt}"));
    }
    let consented: serde_json::Value = consent
        .json()
        .await
        .map_err(|e| format!("decode consent response: {e}"))?;
    let url = consented
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or("consent response missing redirect url")?;
    if std::env::var("JACQUARD_E2E_DEBUG").as_deref() == Ok("1") {
        eprintln!("oauth driver: consent redirect: {url}");
    }
    extract_callback_params(url)
}

/// Extract `code`, `state`, and `iss` from an OAuth callback redirect URL.
fn extract_callback_params(url: &str) -> Result<UiConsent, String> {
    let redirect =
        fluent_uri::Uri::parse(url).map_err(|e| format!("parse consent redirect: {e}"))?;
    let query = redirect.query().unwrap_or_default();
    let mut code = None;
    let mut state = None;
    let mut iss = None;
    for pair in query.as_str().split('&') {
        let mut it = pair.splitn(2, '=');
        let key = it.next().unwrap_or_default();
        let value = it.next().unwrap_or_default();
        let decoded = urldecode(value);
        match key {
            "code" => code = Some(decoded),
            "state" => state = Some(decoded),
            "iss" => iss = Some(decoded),
            _ => {}
        }
    }
    let code = code.ok_or("consent redirect missing code")?;
    let state = state.ok_or("consent redirect missing state")?;
    Ok(UiConsent {
        code: DefaultStr::new(&code),
        state: DefaultStr::new(&state),
        iss: iss.map(|s| DefaultStr::new(&s)),
    })
}

fn origin_of(url: &str) -> Result<String, String> {
    let parsed = fluent_uri::Uri::parse(url).map_err(|e| format!("parse authorize URL: {e}"))?;
    let scheme = parsed.scheme().as_str();
    let authority = parsed
        .authority()
        .map(|a| a.as_str())
        .ok_or("authorize URL has no authority")?;
    if !matches!(scheme, "http" | "https") {
        return Err("authorize URL is not HTTP(S)".to_string());
    }
    Ok(format!("{scheme}://{authority}"))
}

fn urldecode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
