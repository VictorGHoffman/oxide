//! Turning a request into a `curl` command, and a pasted `curl` command into a request.

use crate::model::{
    ApiKeyLocation, AuthKind, BodyKind, KeyValue, Method, Request,
};

// ───────────────────────────── export ─────────────────────────────

/// `'text'` for a POSIX shell (works in bash, zsh and PowerShell's curl.exe calls).
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn enabled(rows: &[KeyValue]) -> impl Iterator<Item = &KeyValue> {
    rows.iter()
        .filter(|r| r.enabled && !r.key.trim().is_empty())
}

/// The `curl` command for `req` (variables should already be replaced).
/// `oauth_token` is the saved access token of an OAuth 2.0 request, when there is one.
pub fn export(req: &Request, oauth_token: Option<&str>) -> String {
    let url = crate::http::build_url(req)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| req.url.trim().to_string());

    let mut parts: Vec<String> = vec!["curl".into()];
    match req.method {
        Method::Get => {}
        Method::Head => parts.push("--head".into()),
        other => {
            parts.push("-X".into());
            parts.push(other.to_string().to_uppercase());
        }
    }
    parts.push(quote(&url));

    let mut headers: Vec<(String, String)> = enabled(&req.headers)
        .map(|h| (h.key.trim().to_string(), h.value.clone()))
        .collect();

    match req.auth_kind {
        AuthKind::None => {}
        AuthKind::Bearer => {
            if !req.bearer_token.trim().is_empty() {
                headers.push((
                    "Authorization".into(),
                    format!("Bearer {}", req.bearer_token.trim()),
                ));
            }
        }
        AuthKind::OAuth2 => headers.push((
            "Authorization".into(),
            format!("Bearer {}", oauth_token.unwrap_or("YOUR_ACCESS_TOKEN")),
        )),
        AuthKind::ApiKey => {
            if req.api_key_in == ApiKeyLocation::Header && !req.api_key_name.trim().is_empty() {
                headers.push((req.api_key_name.trim().to_string(), req.api_key_value.clone()));
            }
        }
        AuthKind::Basic => {}
    }

    let has_content_type = |headers: &[(String, String)]| {
        headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
    };
    let default_type = match req.body_kind {
        BodyKind::Json => Some("application/json"),
        BodyKind::Xml => Some("application/xml"),
        BodyKind::Text => Some("text/plain"),
        BodyKind::Binary => Some("application/octet-stream"),
        _ => None,
    };
    if let Some(kind) = default_type {
        if !has_content_type(&headers) {
            headers.push(("Content-Type".into(), kind.into()));
        }
    }

    for (name, value) in &headers {
        parts.push("-H".into());
        parts.push(quote(&format!("{name}: {value}")));
    }

    if req.auth_kind == AuthKind::Basic && !req.basic_user.is_empty() {
        parts.push("-u".into());
        parts.push(quote(&format!("{}:{}", req.basic_user, req.basic_password)));
    }

    let cookies = enabled(&req.cookies)
        .map(|c| format!("{}={}", c.key.trim(), c.value.trim()))
        .collect::<Vec<_>>()
        .join("; ");
    if !cookies.is_empty() {
        parts.push("-b".into());
        parts.push(quote(&cookies));
    }

    match req.body_kind {
        BodyKind::None => {}
        BodyKind::Json | BodyKind::Xml | BodyKind::Text => {
            if !req.body_text.is_empty() {
                parts.push("--data-raw".into());
                parts.push(quote(&req.body_text));
            }
        }
        BodyKind::Binary => {
            if !req.body_file.trim().is_empty() {
                parts.push("--data-binary".into());
                parts.push(quote(&format!("@{}", req.body_file.trim())));
            }
        }
        BodyKind::UrlEncoded => {
            for row in enabled(&req.url_encoded) {
                parts.push("--data-urlencode".into());
                parts.push(quote(&format!("{}={}", row.key.trim(), row.value)));
            }
        }
        BodyKind::FormData => {
            for row in enabled(&req.form_data) {
                parts.push("--form-string".into());
                parts.push(quote(&format!("{}={}", row.key.trim(), row.value)));
            }
        }
    }

    parts.join(" ")
}

// ───────────────────────────── import ─────────────────────────────

/// Whether `text` looks like the start of a curl command.
pub fn looks_like_curl(text: &str) -> bool {
    let lower = text.trim_start().to_ascii_lowercase();
    lower.starts_with("curl ") || lower.starts_with("curl.exe ")
}

/// Splits a command line like a POSIX shell would (quotes, escapes, line continuations).
fn split_words(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            '\\' | '^' | '`' => {
                match chars.get(i + 1) {
                    // A line continuation (also what is left once a paste lost its newlines).
                    None => {}
                    Some('\n' | '\r') => {}
                    Some(next) if next.is_whitespace() => {
                        if started && c == '\\' {
                            // `plain\ word`: an escaped space belongs to the word
                            word.push(*next);
                            i += 1;
                        }
                    }
                    Some(next) if c == '\\' => {
                        word.push(*next);
                        started = true;
                        i += 1;
                    }
                    Some(_) => {
                        word.push(c);
                        started = true;
                    }
                }
            }
            '\'' => {
                started = true;
                i += 1;
                while i < chars.len() && chars[i] != '\'' {
                    word.push(chars[i]);
                    i += 1;
                }
            }
            '"' => {
                started = true;
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < chars.len() && "\"\\$`".contains(chars[i + 1]) {
                        i += 1;
                    }
                    word.push(chars[i]);
                    i += 1;
                }
            }
            '$' if chars.get(i + 1) == Some(&'\'') => {
                // $'…' with C-style escapes
                started = true;
                i += 2;
                while i < chars.len() && chars[i] != '\'' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                        match chars[i] {
                            'n' => word.push('\n'),
                            't' => word.push('\t'),
                            'r' => word.push('\r'),
                            other => word.push(other),
                        }
                    } else {
                        word.push(chars[i]);
                    }
                    i += 1;
                }
            }
            _ => {
                word.push(c);
                started = true;
            }
        }
        i += 1;
    }
    if started {
        words.push(word);
    }
    words
}

/// Options that are followed by a value we do not use.
fn skips_value(option: &str) -> bool {
    matches!(
        option,
        "-o" | "--output"
            | "-m"
            | "--max-time"
            | "--connect-timeout"
            | "-x"
            | "--proxy"
            | "-U"
            | "--proxy-user"
            | "-w"
            | "--write-out"
            | "-E"
            | "--cert"
            | "--key"
            | "--cacert"
            | "--capath"
            | "-T"
            | "--upload-file"
            | "--retry"
            | "--retry-delay"
            | "--retry-max-time"
            | "-K"
            | "--config"
            | "-c"
            | "--cookie-jar"
            | "-D"
            | "--dump-header"
            | "--resolve"
            | "--interface"
            | "--limit-rate"
            | "-r"
            | "--range"
            | "-z"
            | "--time-cond"
            | "--max-redirs"
            | "--proto"
            | "--http-version-not-used"
    )
}

enum Data {
    /// `-d`, `--data-raw`: sent as written.
    Raw(String),
    /// `--data-urlencode`: encoded when sent.
    Encoded(String),
    /// `--data-binary @file` / `-d @file`.
    File(String),
}

fn parse_method(text: &str) -> Result<Method, String> {
    Ok(match text.trim().to_ascii_uppercase().as_str() {
        "GET" => Method::Get,
        "POST" => Method::Post,
        "PUT" => Method::Put,
        "PATCH" => Method::Patch,
        "DELETE" => Method::Delete,
        "OPTIONS" => Method::Options,
        "HEAD" => Method::Head,
        other => return Err(format!("The method {other} is not supported.")),
    })
}

fn row(key: &str, value: &str) -> KeyValue {
    KeyValue {
        enabled: true,
        key: key.to_string(),
        value: value.to_string(),
    }
}

fn with_blank(mut rows: Vec<KeyValue>) -> Vec<KeyValue> {
    rows.push(KeyValue::empty());
    rows
}

/// Reads a curl command. Everything that is not part of an HTTP request (progress bars,
/// output files, proxies, …) is ignored.
pub fn parse(text: &str) -> Result<Request, String> {
    let words = split_words(text);
    let mut args = words.into_iter();
    match args.next() {
        Some(first) if first.eq_ignore_ascii_case("curl") || first.eq_ignore_ascii_case("curl.exe") => {}
        _ => return Err("This is not a curl command.".into()),
    }

    let mut url: Option<String> = None;
    let mut method: Option<String> = None;
    let mut head = false;
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut data: Vec<Data> = Vec::new();
    let mut form: Vec<(String, String)> = Vec::new();
    let mut user: Option<String> = None;
    let mut cookie: Option<String> = None;
    let mut json_shortcut = false;

    while let Some(arg) = args.next() {
        // `--name=value` and `-XPOST` carry their value in the same word.
        let (option, inline): (String, Option<String>) = if arg.starts_with("--") {
            match arg.split_once('=') {
                Some((name, value)) => (name.to_string(), Some(value.to_string())),
                None => (arg.clone(), None),
            }
        } else if arg.starts_with('-') && arg.len() > 2 {
            let letter = &arg[..2];
            if matches!(letter, "-X" | "-H" | "-d" | "-F" | "-u" | "-b" | "-A" | "-e" | "-o") {
                (letter.to_string(), Some(arg[2..].to_string()))
            } else {
                // `-sSL`: several flags without values.
                continue;
            }
        } else {
            (arg.clone(), None)
        };

        let takes_value = matches!(
            option.as_str(),
            "-X" | "--request"
                | "-H"
                | "--header"
                | "-d"
                | "--data"
                | "--data-raw"
                | "--data-binary"
                | "--data-ascii"
                | "--data-urlencode"
                | "-F"
                | "--form"
                | "--form-string"
                | "-u"
                | "--user"
                | "-b"
                | "--cookie"
                | "-A"
                | "--user-agent"
                | "-e"
                | "--referer"
                | "--url"
                | "--json"
        ) || skips_value(&option);

        let value = if takes_value {
            match inline {
                Some(v) => Some(v),
                None => args.next(),
            }
        } else {
            None
        };

        match option.as_str() {
            "-X" | "--request" => method = value,
            "-H" | "--header" => {
                if let Some(v) = value {
                    if let Some((name, val)) = v.split_once(':') {
                        headers.push((name.trim().to_string(), val.trim().to_string()));
                    }
                }
            }
            "-d" | "--data" | "--data-ascii" => {
                if let Some(v) = value {
                    match v.strip_prefix('@') {
                        Some(path) => data.push(Data::File(path.to_string())),
                        None => data.push(Data::Raw(v)),
                    }
                }
            }
            "--data-raw" => {
                if let Some(v) = value {
                    data.push(Data::Raw(v));
                }
            }
            "--data-binary" => {
                if let Some(v) = value {
                    match v.strip_prefix('@') {
                        Some(path) => data.push(Data::File(path.to_string())),
                        None => data.push(Data::Raw(v)),
                    }
                }
            }
            "--data-urlencode" => {
                if let Some(v) = value {
                    data.push(Data::Encoded(v));
                }
            }
            "--json" => {
                if let Some(v) = value {
                    json_shortcut = true;
                    data.push(Data::Raw(v));
                }
            }
            "-F" | "--form" | "--form-string" => {
                if let Some(v) = value {
                    let (name, val) = v.split_once('=').unwrap_or((&v, ""));
                    form.push((name.to_string(), val.to_string()));
                }
            }
            "-u" | "--user" => user = value,
            "-b" | "--cookie" => cookie = value,
            "-A" | "--user-agent" => {
                if let Some(v) = value {
                    headers.push(("User-Agent".into(), v));
                }
            }
            "-e" | "--referer" => {
                if let Some(v) = value {
                    headers.push(("Referer".into(), v));
                }
            }
            "--url" => url = value,
            "-I" | "--head" => head = true,
            other if !other.starts_with('-') => {
                if url.is_none() {
                    url = Some(arg);
                }
            }
            _ => {}
        }
    }

    let url = url.ok_or("The command has no URL.")?;
    let mut req = Request::default();
    req.url = url.trim().to_string();

    let has_body = !data.is_empty() || !form.is_empty();
    req.method = match method {
        Some(m) => parse_method(&m)?,
        None if head => Method::Head,
        None if has_body => Method::Post,
        None => Method::Get,
    };

    // Headers: the ones with a special place in the request go there.
    let mut content_type: Option<String> = None;
    let mut header_rows = Vec::new();
    let mut cookie_rows = Vec::new();
    for (name, value) in headers {
        let lower = name.to_ascii_lowercase();
        if lower == "authorization" && value.to_ascii_lowercase().starts_with("bearer ") {
            req.auth_kind = AuthKind::Bearer;
            req.bearer_token = value[7..].trim().to_string();
        } else if lower == "cookie" {
            for pair in value.split(';') {
                if let Some((k, v)) = pair.split_once('=') {
                    cookie_rows.push(row(k.trim(), v.trim()));
                }
            }
        } else {
            if lower == "content-type" {
                content_type = Some(value.to_ascii_lowercase());
            }
            header_rows.push(row(&name, &value));
        }
    }
    if json_shortcut && content_type.is_none() {
        content_type = Some("application/json".into());
        header_rows.push(row("Content-Type", "application/json"));
    }
    if let Some(cookie) = cookie {
        for pair in cookie.split(';') {
            if let Some((k, v)) = pair.split_once('=') {
                cookie_rows.push(row(k.trim(), v.trim()));
            }
        }
    }
    if let Some(user) = user {
        let (name, password) = user.split_once(':').unwrap_or((&user, ""));
        req.auth_kind = AuthKind::Basic;
        req.basic_user = name.to_string();
        req.basic_password = password.to_string();
    }

    // Body.
    let drop_content_type = |header_rows: &mut Vec<KeyValue>, matching: &str| {
        header_rows.retain(|h| {
            !(h.key.eq_ignore_ascii_case("content-type")
                && h.value.to_ascii_lowercase().starts_with(matching))
        });
    };

    if !form.is_empty() {
        req.body_kind = BodyKind::FormData;
        req.form_data = with_blank(form.iter().map(|(k, v)| row(k, v)).collect());
        // The boundary is added when sending.
        drop_content_type(&mut header_rows, "multipart/");
    } else if !data.is_empty() {
        let only_file = data.len() == 1 && matches!(data[0], Data::File(_));
        let any_encoded = data.iter().any(|d| matches!(d, Data::Encoded(_)));

        if only_file {
            if let Data::File(path) = &data[0] {
                req.body_kind = BodyKind::Binary;
                req.body_file = path.clone();
            }
        } else if any_encoded {
            req.body_kind = BodyKind::UrlEncoded;
            let mut rows = Vec::new();
            for item in &data {
                match item {
                    Data::Encoded(v) | Data::Raw(v) => match v.split_once('=') {
                        Some((k, val)) => rows.push(row(k, val)),
                        None => rows.push(row("", v)),
                    },
                    Data::File(_) => {}
                }
            }
            req.url_encoded = with_blank(rows);
            drop_content_type(&mut header_rows, "application/x-www-form-urlencoded");
        } else {
            let joined = data
                .iter()
                .map(|d| match d {
                    Data::Raw(v) => v.clone(),
                    Data::File(path) => format!("@{path}"),
                    Data::Encoded(v) => v.clone(),
                })
                .collect::<Vec<_>>()
                .join("&");
            let trimmed = joined.trim_start();
            let kind = match content_type.as_deref() {
                Some(t) if t.contains("json") => BodyKind::Json,
                Some(t) if t.contains("xml") => BodyKind::Xml,
                Some(t) if t.starts_with("text/") => BodyKind::Text,
                Some(t) if t.contains("x-www-form-urlencoded") => BodyKind::UrlEncoded,
                Some(_) => BodyKind::Text,
                None if trimmed.starts_with('{') || trimmed.starts_with('[') => BodyKind::Json,
                None if trimmed.starts_with('<') => BodyKind::Xml,
                None if looks_like_form(&joined) => BodyKind::UrlEncoded,
                None => BodyKind::Text,
            };
            match kind {
                BodyKind::UrlEncoded => {
                    let rows = joined
                        .split('&')
                        .filter(|p| !p.is_empty())
                        .map(|pair| {
                            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                            row(&crate::model::percent_decode(k), &crate::model::percent_decode(v))
                        })
                        .collect();
                    req.url_encoded = with_blank(rows);
                    drop_content_type(&mut header_rows, "application/x-www-form-urlencoded");
                }
                BodyKind::Json => drop_content_type(&mut header_rows, "application/json"),
                BodyKind::Xml => drop_content_type(&mut header_rows, "application/xml"),
                BodyKind::Text => drop_content_type(&mut header_rows, "text/plain"),
                _ => {}
            }
            if matches!(kind, BodyKind::Json | BodyKind::Xml | BodyKind::Text) {
                req.body_text = joined;
            }
            req.body_kind = kind;
        }
    }

    req.headers = with_blank(header_rows);
    req.cookies = with_blank(cookie_rows);
    req.params_from_url();
    Ok(req)
}

/// `a=1&b=2` (and not a sentence that happens to contain an `=`).
fn looks_like_form(text: &str) -> bool {
    !text.is_empty()
        && !text.contains(char::is_whitespace)
        && text.split('&').all(|pair| pair.contains('=') || pair.is_empty())
}

/// Copies what a curl command defines into `target`, leaving the rest (script, OAuth 2.0
/// settings) as it was.
pub fn apply(target: &mut Request, imported: Request) {
    target.method = imported.method;
    target.url = imported.url;
    target.params = imported.params;
    target.headers = imported.headers;
    target.cookies = imported.cookies;
    target.body_kind = imported.body_kind;
    target.body_text = imported.body_text;
    target.form_data = imported.form_data;
    target.url_encoded = imported.url_encoded;
    target.body_file = imported.body_file;
    target.auth_kind = imported.auth_kind;
    target.bearer_token = imported.bearer_token;
    target.basic_user = imported.basic_user;
    target.basic_password = imported.basic_password;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_like_a_shell() {
        assert_eq!(
            split_words(r#"curl -H 'A: b c' "x\"y" $'l1\nl2' plain\ word"#),
            vec!["curl", "-H", "A: b c", "x\"y", "l1\nl2", "plain word"]
        );
        // line continuations, also after the newline was lost in a paste
        assert_eq!(split_words("curl \\\n  -v \\  url"), vec!["curl", "-v", "url"]);
    }

    #[test]
    fn imports_a_json_post() {
        let req = parse(
            r#"curl -X POST 'https://api.example.com/users?x=1' -H 'Content-Type: application/json' -H 'Authorization: Bearer abc' -d '{"name":"Ana"}'"#,
        )
        .unwrap();
        assert_eq!(req.method, Method::Post);
        assert_eq!(req.url, "https://api.example.com/users?x=1");
        assert_eq!(req.body_kind, BodyKind::Json);
        assert_eq!(req.body_text, r#"{"name":"Ana"}"#);
        assert_eq!(req.auth_kind, AuthKind::Bearer);
        assert_eq!(req.bearer_token, "abc");
        assert_eq!(req.params[0].key, "x");
        // the redundant Content-Type is not kept as a header
        assert_eq!(req.headers.len(), 1);
    }

    #[test]
    fn imports_other_shapes() {
        let req = parse("curl -sSL -u me:secret --data-urlencode 'q=a b' -d 'k=v' https://x.test/s").unwrap();
        assert_eq!(req.method, Method::Post);
        assert_eq!(req.auth_kind, AuthKind::Basic);
        assert_eq!(req.basic_user, "me");
        assert_eq!(req.body_kind, BodyKind::UrlEncoded);
        assert_eq!(req.url_encoded[0].value, "a b");

        let req = parse("curl https://x.test -b 'a=1; b=2' -I").unwrap();
        assert_eq!(req.method, Method::Head);
        assert_eq!(req.cookies[1].key, "b");

        let req = parse("curl --data-binary @/tmp/a.bin https://x.test").unwrap();
        assert_eq!(req.body_kind, BodyKind::Binary);
        assert_eq!(req.body_file, "/tmp/a.bin");

        assert!(parse("curl -X POST").is_err());
        assert!(parse("wget x").is_err());
        assert!(parse("curl -X FETCH https://x.test").is_err());
    }

    #[test]
    fn export_round_trips() {
        let mut req = Request::default();
        req.method = Method::Put;
        req.url = "https://api.example.com/items/1".into();
        req.headers = vec![row("X-Note", "it's fine"), KeyValue::empty()];
        req.auth_kind = AuthKind::Basic;
        req.basic_user = "u".into();
        req.basic_password = "p".into();
        req.body_kind = BodyKind::Json;
        req.body_text = "{\"a\": \"it's\"}".into();

        let command = export(&req, None);
        assert!(command.starts_with("curl -X PUT 'https://api.example.com/items/1'"));
        assert!(command.contains(r#"-H 'X-Note: it'\''s fine'"#));

        let back = parse(&command).unwrap();
        assert_eq!(back.method, Method::Put);
        assert_eq!(back.body_kind, BodyKind::Json);
        assert_eq!(back.body_text, req.body_text);
        assert_eq!(back.basic_user, "u");
        assert_eq!(back.headers[0].value, "it's fine");
    }
}
