//! OAuth 2.0 access tokens: requesting, refreshing and remembering them.
//!
//! Supported grants: Client Credentials and Password. When a request uses OAuth 2.0 the
//! token is fetched automatically before sending, and renewed (with the refresh token when
//! the server gave one, otherwise by asking again) once it has expired.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http;
use crate::model::{AuthKind, OAuthClientAuth, OAuthGrant, Request};

/// A token is renewed this many seconds before it really expires.
const EXPIRY_MARGIN: u64 = 30;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Token {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix time (seconds) when the token stops working; `None` = no expiry was announced.
    pub expires_at: Option<u64>,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Token {
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .is_some_and(|at| now() + EXPIRY_MARGIN >= at)
    }

    /// Seconds left (negative once expired), when the expiry is known.
    pub fn seconds_left(&self) -> Option<i64> {
        self.expires_at.map(|at| at as i64 - now() as i64)
    }
}

/// Everything needed to ask for a token (variables already replaced).
#[derive(Debug, Clone)]
pub struct Config {
    pub grant: OAuthGrant,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scope: String,
    pub username: String,
    pub password: String,
    pub client_auth: OAuthClientAuth,
}

impl Config {
    pub fn from_request(req: &Request) -> Self {
        Self {
            grant: req.oauth_grant,
            token_url: req.oauth_token_url.trim().to_string(),
            client_id: req.oauth_client_id.clone(),
            client_secret: req.oauth_client_secret.clone(),
            scope: req.oauth_scope.trim().to_string(),
            username: req.oauth_username.clone(),
            password: req.oauth_password.clone(),
            client_auth: req.oauth_client_auth,
        }
    }
}

/// Asks the server for a token: with the refresh token when one is given, otherwise with
/// the configured grant.
pub async fn fetch(config: &Config, refresh_token: Option<&str>) -> Result<Token, String> {
    if config.token_url.is_empty() {
        return Err("Enter the Access Token URL.".into());
    }

    let mut form: Vec<(&str, String)> = Vec::new();
    match refresh_token {
        Some(refresh) => {
            form.push(("grant_type", "refresh_token".into()));
            form.push(("refresh_token", refresh.to_string()));
        }
        None => match config.grant {
            OAuthGrant::ClientCredentials => {
                form.push(("grant_type", "client_credentials".into()));
            }
            OAuthGrant::Password => {
                form.push(("grant_type", "password".into()));
                form.push(("username", config.username.clone()));
                form.push(("password", config.password.clone()));
            }
        },
    }
    if !config.scope.is_empty() {
        form.push(("scope", config.scope.clone()));
    }

    let client = reqwest::Client::builder()
        .user_agent(concat!("Oxide/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let mut builder = client
        .post(&config.token_url)
        .header(reqwest::header::ACCEPT, "application/json");

    if !config.client_id.is_empty() {
        match config.client_auth {
            OAuthClientAuth::BasicHeader => {
                builder = builder.basic_auth(&config.client_id, Some(&config.client_secret));
            }
            OAuthClientAuth::Body => {
                form.push(("client_id", config.client_id.clone()));
                if !config.client_secret.is_empty() {
                    form.push(("client_secret", config.client_secret.clone()));
                }
            }
        }
    }

    let response = builder
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("Could not reach the token URL: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Could not read the token response: {e}"))?;

    if !status.is_success() {
        return Err(describe_failure(status.as_u16(), &body));
    }
    parse_response(&body, refresh_token)
}

/// The token to use for a request: the saved one while it is valid, otherwise a renewed one.
pub async fn ensure(config: &Config, current: Option<Token>) -> Result<Token, String> {
    let Some(current) = current else {
        return fetch(config, None).await;
    };
    if !current.is_expired() {
        return Ok(current);
    }
    if let Some(refresh) = current.refresh_token.as_deref() {
        // A refresh token the server no longer accepts is not fatal: ask again from scratch.
        if let Ok(token) = fetch(config, Some(refresh)).await {
            return Ok(token);
        }
    }
    fetch(config, None).await
}

/// Sends `request`, getting (or renewing) an OAuth 2.0 token first when the request asks for
/// it. The token that was used comes back so the caller can remember it.
pub async fn send(
    mut request: Request,
    current: Option<Token>,
) -> (Result<http::Response, String>, Option<Token>) {
    if request.auth_kind != AuthKind::OAuth2 {
        return (http::send(request).await, None);
    }

    let config = Config::from_request(&request);
    let token = match ensure(&config, current).await {
        Ok(token) => token,
        Err(e) => return (Err(format!("Could not get an OAuth 2.0 token.\n{e}")), None),
    };

    request.auth_kind = AuthKind::Bearer;
    request.bearer_token = token.access_token.clone();
    (http::send(request).await, Some(token))
}

/// Reads a token endpoint's JSON answer. `old_refresh` is kept when the server does not
/// send a new refresh token.
pub fn parse_response(body: &str, old_refresh: Option<&str>) -> Result<Token, String> {
    let json: Value = serde_json::from_str(body)
        .map_err(|_| format!("The token response is not JSON: {}", snippet(body)))?;

    let access_token = json
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| format!("The response has no access_token: {}", snippet(body)))?
        .to_string();

    let refresh_token = json
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .or_else(|| old_refresh.map(str::to_string));

    // `expires_in` is a number of seconds (some servers send it as text).
    let expires_in = json.get("expires_in").and_then(|v| match v {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|f| f as u64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    });

    Ok(Token {
        access_token,
        refresh_token,
        expires_at: expires_in.map(|secs| now() + secs),
    })
}

fn describe_failure(status: u16, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body).ok().and_then(|json| {
        let error = json.get("error").and_then(Value::as_str)?;
        Some(match json.get("error_description").and_then(Value::as_str) {
            Some(description) => format!("{error}: {description}"),
            None => error.to_string(),
        })
    });
    format!(
        "The token URL answered {status}: {}",
        detail.unwrap_or_else(|| snippet(body))
    )
}

fn snippet(body: &str) -> String {
    let text: String = body.trim().chars().take(160).collect();
    if text.is_empty() {
        "(empty)".into()
    } else {
        text
    }
}

/// "expires in 59 min", "expired 3 min ago", "no expiry"
pub fn describe_expiry(token: &Token) -> String {
    let Some(left) = token.seconds_left() else {
        return "no expiry announced".into();
    };
    let span = |secs: i64| {
        let secs = secs.unsigned_abs();
        if secs < 90 {
            format!("{secs} s")
        } else if secs < 5400 {
            format!("{} min", secs / 60)
        } else {
            format!("{:.1} h", secs as f64 / 3600.0)
        }
    };
    if left >= 0 {
        format!("expires in {}", span(left))
    } else {
        format!("expired {} ago", span(left))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_token_answer() {
        let token = parse_response(
            r#"{"access_token":"abc","token_type":"Bearer","expires_in":3600,"refresh_token":"r1"}"#,
            None,
        )
        .unwrap();
        assert_eq!(token.access_token, "abc");
        assert_eq!(token.refresh_token.as_deref(), Some("r1"));
        let left = token.seconds_left().unwrap();
        assert!((3590..=3600).contains(&left));
        assert!(!token.is_expired());
    }

    #[test]
    fn keeps_the_old_refresh_token_and_accepts_text_expiry() {
        let token = parse_response(r#"{"access_token":"x","expires_in":"120"}"#, Some("old")).unwrap();
        assert_eq!(token.refresh_token.as_deref(), Some("old"));
        assert!(token.seconds_left().unwrap() <= 120);
    }

    #[test]
    fn expiry_margin_counts() {
        let soon = Token {
            access_token: "a".into(),
            refresh_token: None,
            expires_at: Some(now() + 10),
        };
        assert!(soon.is_expired());
        let never = Token {
            expires_at: None,
            ..soon.clone()
        };
        assert!(!never.is_expired());
    }

    #[test]
    fn explains_failures() {
        assert!(parse_response("nope", None).is_err());
        assert!(parse_response(r#"{"error":"x"}"#, None).is_err());
        let text = describe_failure(
            400,
            r#"{"error":"invalid_client","error_description":"bad secret"}"#,
        );
        assert!(text.contains("invalid_client: bad secret"));
    }
}
