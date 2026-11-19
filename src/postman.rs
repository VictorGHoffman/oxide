//! Postman Collection v2.1 import and export.
//!
//! Import also accepts v2.0 files (the item structure is the same). What Oxide
//! cannot represent (scripts, file uploads, GraphQL, ...) is skipped
//! and reported back through [`Summary::warnings`].

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

use crate::model::*;

pub const SCHEMA: &str = "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

/// What an import produced.
pub struct Summary {
    pub collection: Id,
    pub name: String,
    pub requests: usize,
    pub folders: usize,
    /// Things in the file that Oxide cannot represent (deduplicated).
    pub warnings: Vec<String>,
}

pub(crate) struct Counter {
    pub(crate) requests: usize,
    pub(crate) folders: usize,
    pub(crate) warnings: Vec<String>,
}

impl Counter {
    pub(crate) fn warn(&mut self, message: &str) {
        if !self.warnings.iter().any(|w| w == message) {
            self.warnings.push(message.to_string());
        }
    }
}

// ───────────────────────────── helpers ─────────────────────────────

/// Strings, numbers and booleans as text; anything else (or nothing) as "".
pub(crate) fn text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

pub(crate) fn items<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn is_true(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// A `{ key, value, disabled }` row. Rows without a key are dropped.
fn kv(row: &Value) -> Option<KeyValue> {
    let key = text(row.get("key"));
    if key.is_empty() {
        return None;
    }
    Some(KeyValue {
        enabled: !is_true(row, "disabled"),
        key,
        value: text(row.get("value")),
    })
}

/// Key/value tables always end with one blank row to type into.
fn finish(table: &mut Vec<KeyValue>) {
    if !table.last().is_some_and(KeyValue::is_blank) {
        table.push(KeyValue::empty());
    }
}

fn percent_decode(s: &str) -> String {
    fn hex(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }

    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn split_query(raw: &str) -> (&str, &str) {
    // Drop a #fragment first: it is never sent to the server.
    let raw = raw.split('#').next().unwrap_or(raw);
    raw.split_once('?').unwrap_or((raw, ""))
}

fn parse_query(query: &str, out: &mut Vec<KeyValue>) {
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        out.push(KeyValue {
            enabled: true,
            key: percent_decode(key),
            value: percent_decode(value),
        });
    }
}

fn join_parts(value: Option<&Value>, separator: &str) -> String {
    match value {
        Some(Value::Array(parts)) => parts
            .iter()
            .map(|part| match part {
                Value::Object(o) => text(o.get("value")),
                other => text(Some(other)),
            })
            .collect::<Vec<_>>()
            .join(separator),
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

fn build_raw(url: &Value) -> String {
    let protocol = text(url.get("protocol"));
    let host = join_parts(url.get("host"), ".");
    let port = text(url.get("port"));
    let path = join_parts(url.get("path"), "/");

    let mut out = String::new();
    if !protocol.is_empty() {
        out.push_str(&protocol);
        out.push_str("://");
    }
    out.push_str(&host);
    if !port.is_empty() {
        out.push(':');
        out.push_str(&port);
    }
    if !path.is_empty() {
        out.push('/');
        out.push_str(&path);
    }
    out
}

// ───────────────────────────── import ─────────────────────────────

/// Parses a Postman collection and adds it to `tree` as a new collection.
/// Nothing is added when the file is not a valid collection.
pub fn import(tree: &mut Tree, source: &str) -> Result<Summary, String> {
    let source = source.trim_start_matches('\u{feff}');
    let root: Value =
        serde_json::from_str(source).map_err(|e| format!("not a valid JSON file ({e})."))?;

    let (Some(info), Some(children)) = (
        root.get("info").filter(|i| i.is_object()),
        root.get("item").and_then(Value::as_array),
    ) else {
        return Err(if root.get("requests").is_some() {
            "this looks like a Postman v1 collection. Only v2.0 and v2.1 are supported.".into()
        } else {
            "this file is not a Postman collection.".into()
        });
    };

    let name = text(info.get("name"));
    let name = name.trim();
    let name = if name.is_empty() {
        "Imported Collection"
    } else {
        name
    };

    let collection = tree.add_collection();
    if let Some(node) = tree.get_mut(collection) {
        node.name = name.to_string();
    }

    let mut counter = Counter {
        requests: 0,
        folders: 0,
        warnings: Vec::new(),
    };

    // Collection variables come along.
    let mut variables: Vec<KeyValue> = items(&root, "variable").iter().filter_map(kv).collect();
    finish(&mut variables);
    if let Some(node) = tree.get_mut(collection) {
        node.variables = variables;
    }
    if !items(&root, "event").is_empty() {
        counter.warn("scripts (pre-request and tests) are ignored");
    }

    import_items(tree, collection, children, root.get("auth"), &mut counter);

    // Show the collection open, with its folders closed.
    if let Some(node) = tree.get_mut(collection) {
        node.expanded = true;
        collapse_folders(&mut node.children);
    }

    Ok(Summary {
        collection,
        name: name.to_string(),
        requests: counter.requests,
        folders: counter.folders,
        warnings: counter.warnings,
    })
}

pub(crate) fn collapse_folders(nodes: &mut [Node]) {
    for node in nodes {
        node.expanded = false;
        collapse_folders(&mut node.children);
    }
}

fn import_items(
    tree: &mut Tree,
    parent: Id,
    entries: &[Value],
    inherited_auth: Option<&Value>,
    counter: &mut Counter,
) {
    for entry in entries {
        let name = text(entry.get("name")).trim().to_string();

        if !items(entry, "event").is_empty() {
            counter.warn("scripts (pre-request and tests) are ignored");
        }

        if let Some(children) = entry.get("item").and_then(Value::as_array) {
            let Some(folder) = tree.add_folder(parent) else {
                continue;
            };
            if let Some(node) = tree.get_mut(folder) {
                node.name = if name.is_empty() {
                    "Folder".to_string()
                } else {
                    name
                };
            }
            counter.folders += 1;

            let auth = entry.get("auth").or(inherited_auth);
            import_items(tree, folder, children, auth, counter);
        } else if let Some(request) = entry.get("request") {
            let Some(id) = tree.add_request(parent) else {
                continue;
            };
            let parsed = parse_request(request, inherited_auth, counter);
            if let Some(node) = tree.get_mut(id) {
                node.name = if name.is_empty() {
                    "Request".to_string()
                } else {
                    name
                };
                if let Some(slot) = node.request_mut() {
                    *slot = parsed;
                }
            }
            counter.requests += 1;
        }
    }
}

fn parse_request(value: &Value, inherited_auth: Option<&Value>, counter: &mut Counter) -> Request {
    let mut req = Request::default();
    req.params.clear();
    req.headers.clear();
    req.cookies.clear();
    req.form_data.clear();
    req.url_encoded.clear();

    // A request may be given as just a URL string.
    if let Some(url) = value.as_str() {
        set_url_from_string(&mut req, url);
        finish_all(&mut req);
        req.sync_query();
        return req;
    }

    // Method.
    if let Some(method) = value.get("method").and_then(Value::as_str) {
        req.method = match method.to_ascii_uppercase().as_str() {
            "GET" => Method::Get,
            "POST" => Method::Post,
            "PUT" => Method::Put,
            "PATCH" => Method::Patch,
            "DELETE" => Method::Delete,
            "OPTIONS" => Method::Options,
            "HEAD" => Method::Head,
            other => {
                counter.warn(&format!(
                    "method {other} is not supported (imported as GET)"
                ));
                Method::Get
            }
        };
    }

    // URL.
    match value.get("url") {
        Some(Value::String(s)) => set_url_from_string(&mut req, s),
        Some(url) if url.is_object() => {
            let raw = match url.get("raw").and_then(Value::as_str) {
                Some(raw) => raw.to_string(),
                None => build_raw(url),
            };
            let (base, raw_query) = split_query(&raw);
            req.url = base.to_string();

            if url.get("query").and_then(Value::as_array).is_some() {
                // The structured list also holds the disabled params.
                req.params.extend(items(url, "query").iter().filter_map(kv));
            } else {
                parse_query(raw_query, &mut req.params);
            }
        }
        _ => {}
    }

    // Headers. A `Cookie` header becomes rows of the Cookies tab.
    for header in items(value, "header") {
        let Some(row) = kv(header) else { continue };
        if row.key.trim().eq_ignore_ascii_case("cookie") {
            for pair in row.value.split(';') {
                let pair = pair.trim();
                if pair.is_empty() {
                    continue;
                }
                let (name, cookie) = pair.split_once('=').unwrap_or((pair, ""));
                req.cookies.push(KeyValue {
                    enabled: row.enabled,
                    key: name.trim().to_string(),
                    value: cookie.trim().to_string(),
                });
            }
        } else {
            req.headers.push(row);
        }
    }

    // Body.
    if let Some(body) = value.get("body").filter(|b| b.is_object()) {
        if !is_true(body, "disabled") {
            match body.get("mode").and_then(Value::as_str) {
                Some("raw") => {
                    req.body_text = text(body.get("raw"));
                    let language = body
                        .pointer("/options/raw/language")
                        .and_then(Value::as_str)
                        .unwrap_or("text");
                    req.body_kind = match language {
                        "json" => BodyKind::Json,
                        "xml" => BodyKind::Xml,
                        _ => BodyKind::Text,
                    };
                }
                Some("urlencoded") => {
                    req.body_kind = BodyKind::UrlEncoded;
                    req.url_encoded
                        .extend(items(body, "urlencoded").iter().filter_map(kv));
                }
                Some("formdata") => {
                    req.body_kind = BodyKind::FormData;
                    for row in items(body, "formdata") {
                        if row.get("type").and_then(Value::as_str) == Some("file") {
                            counter.warn("file uploads in form-data are skipped");
                            continue;
                        }
                        if let Some(row) = kv(row) {
                            req.form_data.push(row);
                        }
                    }
                }
                Some("graphql") => counter.warn("GraphQL bodies are not supported"),
                Some("file") => {
                    req.body_kind = BodyKind::Binary;
                    req.body_file = text(body.pointer("/file/src"));
                }
                _ => {}
            }
        }
    }

    // Auth: the request's own, otherwise the one inherited from its folder/collection.
    if let Some(auth) = value.get("auth").or(inherited_auth) {
        apply_auth(&mut req, auth, counter);
    }

    finish_all(&mut req);
    req.sync_query();
    req
}

pub(crate) fn set_url_from_string(req: &mut Request, raw: &str) {
    let (base, query) = split_query(raw);
    req.url = base.to_string();
    parse_query(query, &mut req.params);
}

pub(crate) fn finish_all(req: &mut Request) {
    finish(&mut req.params);
    finish(&mut req.headers);
    finish(&mut req.cookies);
    finish(&mut req.form_data);
    finish(&mut req.url_encoded);
}

pub(crate) fn apply_auth(req: &mut Request, auth: &Value, counter: &mut Counter) {
    let Some(kind) = auth.get("type").and_then(Value::as_str) else {
        return;
    };

    // v2.1 stores `[{ key, value }]`, v2.0 stores a plain object.
    let param = |key: &str| -> String {
        match auth.get(kind) {
            Some(Value::Array(list)) => list
                .iter()
                .find(|p| p.get("key").and_then(Value::as_str) == Some(key))
                .map(|p| text(p.get("value")))
                .unwrap_or_default(),
            Some(Value::Object(map)) => text(map.get(key)),
            _ => String::new(),
        }
    };

    match kind {
        "bearer" => {
            req.auth_kind = AuthKind::Bearer;
            req.bearer_token = param("token");
        }
        "basic" => {
            req.auth_kind = AuthKind::Basic;
            req.basic_user = param("username");
            req.basic_password = param("password");
        }
        "apikey" => {
            req.auth_kind = AuthKind::ApiKey;
            req.api_key_name = param("key");
            req.api_key_value = param("value");
            req.api_key_in = if param("in") == "query" {
                ApiKeyLocation::Query
            } else {
                ApiKeyLocation::Header
            };
        }
        "oauth2" => {
            let grant = param("grant_type");
            if grant.is_empty() || grant == "client_credentials" || grant == "password_credentials" {
                req.auth_kind = AuthKind::OAuth2;
                req.oauth_grant = if grant == "password_credentials" {
                    OAuthGrant::Password
                } else {
                    OAuthGrant::ClientCredentials
                };
                req.oauth_token_url = param("accessTokenUrl");
                req.oauth_client_id = param("clientId");
                req.oauth_client_secret = param("clientSecret");
                req.oauth_scope = param("scope");
                req.oauth_username = param("username");
                req.oauth_password = param("password");
                req.oauth_client_auth = if param("client_authentication") == "body" {
                    OAuthClientAuth::Body
                } else {
                    OAuthClientAuth::BasicHeader
                };
            } else {
                counter.warn(&format!("OAuth 2.0 grant \"{grant}\" is not supported"));
            }
        }
        "noauth" => req.auth_kind = AuthKind::None,
        other => counter.warn(&format!("auth type \"{other}\" is not supported")),
    }
}

// ───────────────────────────── export ─────────────────────────────

/// Serializes a collection (and everything inside it) as Postman Collection v2.1.
pub fn export(collection: &Node) -> String {
    let value = json!({
        "info": {
            "_postman_id": new_id(&collection.name),
            "name": collection.name,
            "schema": SCHEMA,
        },
        "item": export_items(&collection.children),
        "variable": live(&collection.variables)
            .map(|v| {
                let mut entry = json!({ "key": v.key.trim(), "value": v.value, "type": "default" });
                if !v.enabled {
                    entry["disabled"] = Value::Bool(true);
                }
                entry
            })
            .collect::<Vec<Value>>(),
    });
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

fn export_items(nodes: &[Node]) -> Vec<Value> {
    nodes
        .iter()
        .filter_map(|node| match &node.kind {
            Kind::Request(req) => Some(json!({
                "name": node.name,
                "request": export_request(req),
                "response": [],
            })),
            Kind::Folder => Some(json!({
                "name": node.name,
                "item": export_items(&node.children),
            })),
            Kind::Collection => None,
        })
        .collect()
}

fn live(rows: &[KeyValue]) -> impl Iterator<Item = &KeyValue> {
    rows.iter().filter(|r| !r.key.trim().is_empty())
}

fn row(kv: &KeyValue, with_type: bool) -> Value {
    let mut map = Map::new();
    map.insert("key".into(), Value::from(kv.key.trim()));
    map.insert("value".into(), Value::from(kv.value.as_str()));
    if with_type {
        map.insert("type".into(), Value::from("text"));
    }
    if !kv.enabled {
        map.insert("disabled".into(), Value::Bool(true));
    }
    Value::Object(map)
}

fn export_request(req: &Request) -> Value {
    let mut out = Map::new();

    if let Some(auth) = export_auth(req) {
        out.insert("auth".into(), auth);
    }
    out.insert("method".into(), Value::from(req.method.as_str()));
    out.insert("header".into(), Value::Array(export_headers(req)));
    if let Some(body) = export_body(req) {
        out.insert("body".into(), body);
    }
    out.insert("url".into(), export_url(req));

    Value::Object(out)
}

fn export_headers(req: &Request) -> Vec<Value> {
    let mut out: Vec<Value> = live(&req.headers).map(|h| row(h, true)).collect();

    // Postman has no cookies tab: enabled cookies travel in one `Cookie`
    // header, disabled ones as disabled `Cookie` headers (so nothing is lost).
    let cookie = |c: &KeyValue| format!("{}={}", c.key.trim(), c.value.trim());

    let enabled: Vec<String> = live(&req.cookies)
        .filter(|c| c.enabled)
        .map(cookie)
        .collect();
    if !enabled.is_empty() {
        out.push(json!({ "key": "Cookie", "value": enabled.join("; "), "type": "text" }));
    }
    for disabled in live(&req.cookies).filter(|c| !c.enabled) {
        out.push(json!({
            "key": "Cookie",
            "value": cookie(disabled),
            "type": "text",
            "disabled": true,
        }));
    }
    out
}

fn export_body(req: &Request) -> Option<Value> {
    match req.body_kind {
        BodyKind::None => None,
        BodyKind::Json | BodyKind::Xml | BodyKind::Text => {
            let language = match req.body_kind {
                BodyKind::Json => "json",
                BodyKind::Xml => "xml",
                _ => "text",
            };
            Some(json!({
                "mode": "raw",
                "raw": req.body_text,
                "options": { "raw": { "language": language } },
            }))
        }
        BodyKind::Binary => Some(json!({
            "mode": "file",
            "file": { "src": req.body_file },
        })),
        BodyKind::UrlEncoded => {
            let rows: Vec<Value> = live(&req.url_encoded).map(|r| row(r, true)).collect();
            Some(json!({ "mode": "urlencoded", "urlencoded": rows }))
        }
        BodyKind::FormData => {
            let rows: Vec<Value> = live(&req.form_data).map(|r| row(r, true)).collect();
            Some(json!({ "mode": "formdata", "formdata": rows }))
        }
    }
}

fn auth_param(key: &str, value: &str) -> Value {
    json!({ "key": key, "value": value, "type": "string" })
}

fn export_auth(req: &Request) -> Option<Value> {
    match req.auth_kind {
        AuthKind::None => None,
        AuthKind::Bearer => Some(json!({
            "type": "bearer",
            "bearer": [auth_param("token", &req.bearer_token)],
        })),
        AuthKind::Basic => Some(json!({
            "type": "basic",
            "basic": [
                auth_param("username", &req.basic_user),
                auth_param("password", &req.basic_password),
            ],
        })),
        AuthKind::OAuth2 => {
            let grant = match req.oauth_grant {
                OAuthGrant::ClientCredentials => "client_credentials",
                OAuthGrant::Password => "password_credentials",
            };
            let client_auth = match req.oauth_client_auth {
                OAuthClientAuth::BasicHeader => "header",
                OAuthClientAuth::Body => "body",
            };
            let mut params = vec![
                auth_param("grant_type", grant),
                auth_param("accessTokenUrl", &req.oauth_token_url),
                auth_param("clientId", &req.oauth_client_id),
                auth_param("clientSecret", &req.oauth_client_secret),
                auth_param("scope", &req.oauth_scope),
                auth_param("client_authentication", client_auth),
                auth_param("addTokenTo", "header"),
            ];
            if req.oauth_grant == OAuthGrant::Password {
                params.push(auth_param("username", &req.oauth_username));
                params.push(auth_param("password", &req.oauth_password));
            }
            Some(json!({ "type": "oauth2", "oauth2": params }))
        }
        AuthKind::ApiKey => {
            let location = match req.api_key_in {
                ApiKeyLocation::Header => "header",
                ApiKeyLocation::Query => "query",
            };
            Some(json!({
                "type": "apikey",
                "apikey": [
                    auth_param("key", &req.api_key_name),
                    auth_param("value", &req.api_key_value),
                    auth_param("in", location),
                ],
            }))
        }
    }
}

/// Escapes the few characters that would break a query string.
fn encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '&' => out.push_str("%26"),
            '#' => out.push_str("%23"),
            '+' => out.push_str("%2B"),
            other => out.push(other),
        }
    }
    out
}

fn export_url(req: &Request) -> Value {
    // The URL text holds the enabled params as its query string; Postman wants them apart.
    let full = req.url.trim();
    let base = full.split(['?', '#']).next().unwrap_or(full);
    let params: Vec<&KeyValue> = live(&req.params).collect();

    let query = params
        .iter()
        .filter(|p| p.enabled)
        .map(|p| format!("{}={}", encode_query(p.key.trim()), encode_query(&p.value)))
        .collect::<Vec<_>>()
        .join("&");

    let raw = if query.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{query}")
    };

    // Split the base into the parts Postman keeps next to `raw`.
    let without_query = base;
    let (protocol, rest) = match without_query.split_once("://") {
        Some((protocol, rest)) => (Some(protocol), rest),
        None => (None, without_query),
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
            (host, Some(port))
        }
        _ => (authority, None),
    };

    let mut url = Map::new();
    url.insert("raw".into(), Value::from(raw));
    if let Some(protocol) = protocol {
        url.insert("protocol".into(), Value::from(protocol));
    }
    if !host.is_empty() {
        let parts: Vec<Value> = host.split('.').map(Value::from).collect();
        url.insert("host".into(), Value::Array(parts));
    }
    if let Some(port) = port {
        url.insert("port".into(), Value::from(port));
    }
    let segments: Vec<Value> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(Value::from)
        .collect();
    if !segments.is_empty() {
        url.insert("path".into(), Value::Array(segments));
    }
    if !params.is_empty() {
        let query: Vec<Value> = params.iter().map(|p| row(p, false)).collect();
        url.insert("query".into(), Value::Array(query));
    }

    Value::Object(url)
}

/// A random-looking UUID-shaped id for `info._postman_id`.
fn new_id(seed: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let mut first = DefaultHasher::new();
    (seed, nanos, 1u8).hash(&mut first);
    let mut second = DefaultHasher::new();
    (seed, nanos, 2u8).hash(&mut second);
    let (x, y) = (first.finish(), second.finish());

    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (x >> 32) as u32,
        (x >> 16) & 0xffff,
        x & 0xfff,
        0x8000 | ((y >> 48) & 0x3fff),
        y & 0xffff_ffff_ffff
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "info": { "name": "Demo" },
        "auth": { "type": "bearer", "bearer": [ { "key": "token", "value": "abc" } ] },
        "item": [
            { "name": "Users", "item": [
                { "name": "List", "request": {
                    "method": "GET",
                    "header": [
                        { "key": "Accept", "value": "application/json" },
                        { "key": "X-Off", "value": "1", "disabled": true }
                    ],
                    "url": {
                        "raw": "https://api.test/users?page=2&off=1",
                        "query": [
                            { "key": "page", "value": "2" },
                            { "key": "off", "value": "1", "disabled": true }
                        ]
                    }
                } }
            ] },
            { "name": "Create", "request": {
                "method": "POST",
                "body": { "mode": "raw", "raw": "{}", "options": { "raw": { "language": "json" } } },
                "url": "https://api.test/users"
            } }
        ]
    }"#;

    #[test]
    fn imports_folders_requests_and_inherited_auth() {
        let mut tree = Tree::new();
        let summary = import(&mut tree, SAMPLE).unwrap();
        assert_eq!(summary.requests, 2);
        assert_eq!(summary.folders, 1);

        let collection = tree.get(summary.collection).unwrap();
        assert_eq!(collection.name, "Demo");

        let list = collection.children[0].children[0].request().unwrap();
        assert_eq!(list.url, "https://api.test/users?page=2");
        assert_eq!(list.params.len(), 3); // page, off (disabled), blank row
        assert!(list.params[0].enabled && !list.params[1].enabled);
        assert!(list.headers[0].enabled && !list.headers[1].enabled);
        assert_eq!(list.auth_kind, AuthKind::Bearer);
        assert_eq!(list.bearer_token, "abc");

        let create = collection.children[1].request().unwrap();
        assert_eq!(create.method, Method::Post);
        assert_eq!(create.body_kind, BodyKind::Json);
        assert_eq!(create.body_text, "{}");
    }

    #[test]
    fn rejects_files_that_are_not_collections() {
        let mut tree = Tree::new();
        assert!(import(&mut tree, "{\"values\": []}").is_err());
        assert!(import(&mut tree, "not json").is_err());
        assert!(tree.roots.is_empty());
    }

    #[test]
    fn export_then_import_round_trips_a_request() {
        let mut tree = Tree::new();
        let collection = tree.add_collection();
        let id = tree.add_request(collection).unwrap();
        {
            let req = tree.request_mut(id).unwrap();
            req.method = Method::Put;
            req.url = "https://x.test:8080/a/b".into();
            req.params = vec![
                KeyValue {
                    enabled: true,
                    key: "a".into(),
                    value: "1 2".into(),
                },
                KeyValue::empty(),
            ];
            req.cookies = vec![
                KeyValue {
                    enabled: true,
                    key: "sid".into(),
                    value: "xyz".into(),
                },
                KeyValue::empty(),
            ];
            req.body_kind = BodyKind::Json;
            req.body_text = "{\"k\": 1}".into();
            req.auth_kind = AuthKind::ApiKey;
            req.api_key_name = "k".into();
            req.api_key_value = "v".into();
            req.api_key_in = ApiKeyLocation::Query;
        }
        tree.request_mut(id).unwrap().url_from_params();
        let original = tree.request(id).unwrap().clone();

        let json = export(tree.get(collection).unwrap());
        let mut other = Tree::new();
        let summary = import(&mut other, &json).unwrap();

        let back = other.get(summary.collection).unwrap().children[0]
            .request()
            .unwrap();
        assert_eq!(back, &original);
    }

    #[test]
    fn binary_body_and_oauth_round_trip() {
        let mut tree = Tree::new();
        let collection = tree.add_collection();
        let id = tree.add_request(collection).unwrap();
        {
            let req = tree.request_mut(id).unwrap();
            req.body_kind = BodyKind::Binary;
            req.body_file = "/tmp/data.bin".into();
            req.auth_kind = AuthKind::OAuth2;
            req.oauth_grant = OAuthGrant::Password;
            req.oauth_token_url = "https://auth.test/token".into();
            req.oauth_client_id = "id".into();
            req.oauth_client_secret = "secret".into();
            req.oauth_scope = "read write".into();
            req.oauth_username = "bob".into();
            req.oauth_password = "pw".into();
            req.oauth_client_auth = OAuthClientAuth::Body;
        }
        let original = tree.request(id).unwrap().clone();

        let json = export(tree.get(collection).unwrap());
        let mut other = Tree::new();
        let summary = import(&mut other, &json).unwrap();
        let back = other.get(summary.collection).unwrap().children[0]
            .request()
            .unwrap();
        assert_eq!(back, &original);
    }
}
