//! Turns a [`Request`] into a real HTTP call and a [`Response`] summary.

use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE, COOKIE};
use reqwest::multipart;

use crate::model::{ApiKeyLocation, AuthKind, BodyKind, KeyValue, Method, Request};

#[derive(Debug, Clone)]
pub struct SetCookie {
    pub name: String,
    pub value: String,
    pub attributes: String,
}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub status_text: String,
    pub elapsed: Duration,
    pub size: usize,
    pub version: String,
    pub remote_addr: Option<String>,
    pub final_url: String,
    pub headers: Vec<(String, String)>,
    pub cookies: Vec<SetCookie>,
    pub body_raw: String,
    pub body_pretty: String,
    pub is_binary: bool,
    /// The exact body when it is not text (images, PDFs, downloads).
    pub bytes: Option<std::sync::Arc<Vec<u8>>>,
    /// An image or PDF body, for the preview.
    pub media: Option<crate::media::Media>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn to_reqwest(method: Method) -> reqwest::Method {
    match method {
        Method::Get => reqwest::Method::GET,
        Method::Post => reqwest::Method::POST,
        Method::Put => reqwest::Method::PUT,
        Method::Patch => reqwest::Method::PATCH,
        Method::Delete => reqwest::Method::DELETE,
        Method::Options => reqwest::Method::OPTIONS,
        Method::Head => reqwest::Method::HEAD,
    }
}

fn enabled(rows: &[KeyValue]) -> impl Iterator<Item = &KeyValue> {
    rows.iter()
        .filter(|r| r.enabled && !r.key.trim().is_empty())
}

pub fn build_url(req: &Request) -> Result<reqwest::Url, String> {
    let raw = req.url.trim();
    if raw.is_empty() {
        return Err("Please enter a URL.".into());
    }

    let with_scheme = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("http://{raw}")
    };

    let mut url = reqwest::Url::parse(&with_scheme).map_err(|e| format!("Invalid URL: {e}"))?;

    let api_key_param = (req.auth_kind == AuthKind::ApiKey
        && req.api_key_in == ApiKeyLocation::Query
        && !req.api_key_name.trim().is_empty())
    .then(|| (req.api_key_name.trim(), req.api_key_value.as_str()));

    // The URL already carries the enabled params as its query string.
    if let Some((k, v)) = api_key_param {
        url.query_pairs_mut().append_pair(k, v);
    }

    Ok(url)
}

fn header_pair(name: &str, value: &str) -> Result<(HeaderName, HeaderValue), String> {
    let name = HeaderName::from_bytes(name.trim().as_bytes())
        .map_err(|_| format!("Invalid header name: \"{}\"", name.trim()))?;
    let value = HeaderValue::from_str(value.trim())
        .map_err(|_| format!("Invalid value for header \"{name}\""))?;
    Ok((name, value))
}

pub async fn send(req: Request) -> Result<Response, String> {
    let url = build_url(&req)?;

    let client = crate::certs::apply(
        reqwest::Client::builder()
            .user_agent(concat!("Oxide/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(120)),
        &url,
    )?
    .build()
    .map_err(describe_error)?;

    let mut headers = HeaderMap::new();
    for h in enabled(&req.headers) {
        let (name, value) = header_pair(&h.key, &h.value)?;
        headers.append(name, value);
    }

    // Cookies tab → a single `Cookie` header.
    let cookie_header = enabled(&req.cookies)
        .map(|c| format!("{}={}", c.key.trim(), c.value.trim()))
        .collect::<Vec<_>>()
        .join("; ");
    if !cookie_header.is_empty() {
        let value = HeaderValue::from_str(&cookie_header)
            .map_err(|_| "Invalid cookie value".to_string())?;
        headers.insert(COOKIE, value);
    }

    // Auth tab.
    match req.auth_kind {
        // OAuth 2.0 is turned into a Bearer token by `oauth::send` before we get here.
        AuthKind::None | AuthKind::Basic | AuthKind::OAuth2 => {}
        AuthKind::Bearer => {
            if !req.bearer_token.trim().is_empty() {
                let value = HeaderValue::from_str(&format!("Bearer {}", req.bearer_token.trim()))
                    .map_err(|_| "Invalid bearer token".to_string())?;
                headers.insert(AUTHORIZATION, value);
            }
        }
        AuthKind::ApiKey => {
            if req.api_key_in == ApiKeyLocation::Header && !req.api_key_name.trim().is_empty() {
                let (name, value) = header_pair(&req.api_key_name, &req.api_key_value)?;
                headers.insert(name, value);
            }
        }
    }

    let mut builder = client.request(to_reqwest(req.method), url);

    if req.auth_kind == AuthKind::Basic && !req.basic_user.is_empty() {
        builder = builder.basic_auth(&req.basic_user, Some(&req.basic_password));
    }

    // Body tab. A Content-Type typed by the user always wins.
    let user_set_content_type = headers.contains_key(CONTENT_TYPE);
    let mut default_content_type: Option<&'static str> = None;

    match req.body_kind {
        BodyKind::None => {}
        BodyKind::Json => {
            default_content_type = Some("application/json");
            builder = builder.body(req.body_text.clone());
        }
        BodyKind::Xml => {
            default_content_type = Some("application/xml");
            builder = builder.body(req.body_text.clone());
        }
        BodyKind::Text => {
            default_content_type = Some("text/plain");
            builder = builder.body(req.body_text.clone());
        }
        BodyKind::Binary => {
            let path = req.body_file.trim();
            if path.is_empty() {
                return Err("Binary body: choose a file first.".into());
            }
            let bytes = std::fs::read(path)
                .map_err(|e| format!("Could not read the body file \"{path}\": {e}"))?;
            default_content_type = Some("application/octet-stream");
            builder = builder.body(bytes);
        }
        BodyKind::UrlEncoded => {
            let pairs: Vec<(String, String)> = enabled(&req.url_encoded)
                .map(|r| (r.key.trim().to_string(), r.value.clone()))
                .collect();
            builder = builder.form(&pairs);
        }
        BodyKind::FormData => {
            let mut form = multipart::Form::new();
            for r in enabled(&req.form_data) {
                form = form.text(r.key.trim().to_string(), r.value.clone());
            }
            builder = builder.multipart(form);
        }
    }

    if let (Some(ct), false) = (default_content_type, user_set_content_type) {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static(ct));
    }

    let started = Instant::now();

    let response = builder
        .headers(headers)
        .send()
        .await
        .map_err(describe_error)?;

    let status = response.status();
    let version = format!("{:?}", response.version());
    let remote_addr = response.remote_addr().map(|a| a.to_string());
    let final_url = response.url().to_string();

    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();

    let cookies = response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(parse_set_cookie)
        .collect();

    let bytes = response.bytes().await.map_err(describe_error)?;
    let elapsed = started.elapsed();

    let (body_raw, is_binary) = match std::str::from_utf8(&bytes) {
        Ok(text) => (text.to_string(), false),
        Err(_) => (format!("<binary content, {} bytes>", bytes.len()), true),
    };

    let body_pretty = if is_binary {
        body_raw.clone()
    } else {
        serde_json::from_str::<serde_json::Value>(&body_raw)
            .ok()
            .and_then(|value| serde_json::to_string_pretty(&value).ok())
            .unwrap_or_else(|| body_raw.clone())
    };

    let shared = is_binary.then(|| std::sync::Arc::new(bytes.to_vec()));
    let media = crate::media::sniff(&bytes).map(|kind| crate::media::Media {
        kind,
        bytes: shared
            .clone()
            .unwrap_or_else(|| std::sync::Arc::new(bytes.to_vec())),
    });

    Ok(Response {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        elapsed,
        size: bytes.len(),
        version,
        remote_addr,
        final_url,
        headers,
        cookies,
        body_raw,
        body_pretty,
        is_binary,
        bytes: shared,
        media,
    })
}

fn describe_error(e: reqwest::Error) -> String {
    use std::error::Error;

    let mut message = if e.is_timeout() {
        "The request timed out.".to_string()
    } else if e.is_connect() {
        "Could not connect to the server.".to_string()
    } else {
        e.to_string()
    };

    // Append the root cause (DNS failure, TLS error, …) when there is one.
    let mut source = e.source();
    while let Some(cause) = source {
        message.push_str(&format!("\n{cause}"));
        source = cause.source();
    }
    message
}

fn parse_set_cookie(raw: &str) -> SetCookie {
    let mut parts = raw.splitn(2, ';');
    let pair = parts.next().unwrap_or("");
    let attributes = parts.next().unwrap_or("").trim().to_string();
    let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
    SetCookie {
        name: name.trim().to_string(),
        value: value.trim().to_string(),
        attributes,
    }
}

pub fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.2} s", d.as_secs_f64())
    }
}

pub fn format_size(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{:.2} MB", b / (KB * KB))
    }
}
