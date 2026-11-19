//! Apidog import: the per-request `*.http.yaml` files Apidog keeps on disk.
//!
//! Each file holds one request (`method`, `url`, `parameters`, `requestBody`,
//! `auth`). A picked folder becomes a collection whose sub-folders become
//! folders; a handful of picked files become a collection named after the
//! folder they live in. What Oxide cannot represent is skipped and reported
//! back through [`postman::Summary::warnings`].

use std::path::Path;

use serde_json::Value;

use crate::model::*;
use crate::postman::{self, Counter, Summary};

/// A folder of Apidog files, read from disk.
#[derive(Debug, Clone, Default)]
pub struct Dir {
    pub name: String,
    /// `(file name, file contents)`.
    pub files: Vec<(String, String)>,
    pub dirs: Vec<Dir>,
}

/// Whether a file name looks like an Apidog request file.
pub fn is_apidog_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".yaml") || lower.ends_with(".yml")
}

/// Reads `path` and its sub-folders, keeping only the YAML files.
/// Hidden entries are skipped; folders without any request file are dropped.
pub fn read_dir(path: &Path) -> Result<Dir, String> {
    fn walk(path: &Path, depth: usize) -> Result<Option<Dir>, String> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut dir = Dir {
            name,
            ..Dir::default()
        };
        let entries = std::fs::read_dir(path)
            .map_err(|e| format!("could not read {} ({e}).", path.display()))?;
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if file_name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if depth < 32 {
                    if let Some(sub) = walk(&entry.path(), depth + 1)? {
                        dir.dirs.push(sub);
                    }
                }
            } else if is_apidog_file(&file_name) {
                let bytes = std::fs::read(entry.path())
                    .map_err(|e| format!("could not read {file_name} ({e})."))?;
                let text = String::from_utf8(bytes)
                    .map_err(|_| format!("{file_name} is not a UTF-8 text file."))?;
                dir.files.push((file_name, text));
            }
        }

        Ok((!dir.files.is_empty() || !dir.dirs.is_empty()).then_some(dir))
    }

    walk(path, 0)?.ok_or_else(|| "the folder has no Apidog request files (.yaml).".into())
}

// ───────────────────────────── import ─────────────────────────────

/// Adds `dir` to `tree` as a new collection. Nothing is added when no file
/// in it is an Apidog request.
pub fn import(tree: &mut Tree, dir: &Dir) -> Result<Summary, String> {
    // Parse everything first so a bad import leaves the tree untouched.
    let mut counter = Counter {
        requests: 0,
        folders: 0,
        warnings: Vec::new(),
    };
    let parsed = parse_dir(dir, &mut counter);
    if counter.requests == 0 {
        return Err(counter
            .warnings
            .first()
            .cloned()
            .unwrap_or_else(|| "no Apidog requests were found.".into()));
    }

    let name = dir.name.trim();
    let name = if name.is_empty() { "Apidog Import" } else { name };

    let collection = tree.add_collection();
    if let Some(node) = tree.get_mut(collection) {
        node.name = name.to_string();
    }
    add_to_tree(tree, collection, parsed, &mut counter);

    if let Some(node) = tree.get_mut(collection) {
        node.expanded = true;
        postman::collapse_folders(&mut node.children);
    }

    Ok(Summary {
        collection,
        name: name.to_string(),
        requests: counter.requests,
        folders: counter.folders,
        warnings: counter.warnings,
    })
}

enum Entry {
    Folder(String, Vec<Entry>),
    Request(String, Request),
}

fn parse_dir(dir: &Dir, counter: &mut Counter) -> Vec<Entry> {
    let mut out = Vec::new();
    for sub in &dir.dirs {
        let children = parse_dir(sub, counter);
        if !children.is_empty() {
            out.push(Entry::Folder(sub.name.clone(), children));
        }
    }
    for (file_name, content) in &dir.files {
        match parse_file(file_name, content, counter) {
            Ok(Some(entry)) => {
                counter.requests += 1;
                out.push(entry);
            }
            Ok(None) => counter.warn("YAML files that are not Apidog requests were skipped"),
            Err(e) => counter.warn(&format!("{file_name}: {e}")),
        }
    }
    out
}

fn add_to_tree(tree: &mut Tree, parent: Id, entries: Vec<Entry>, counter: &mut Counter) {
    for entry in entries {
        match entry {
            Entry::Folder(name, children) => {
                let Some(folder) = tree.add_folder(parent) else {
                    continue;
                };
                if let Some(node) = tree.get_mut(folder) {
                    node.name = name;
                }
                counter.folders += 1;
                add_to_tree(tree, folder, children, counter);
            }
            Entry::Request(name, request) => {
                let Some(id) = tree.add_request(parent) else {
                    continue;
                };
                if let Some(node) = tree.get_mut(id) {
                    node.name = name;
                    if let Some(slot) = node.request_mut() {
                        *slot = request;
                    }
                }
            }
        }
    }
}

/// `Ok(None)` when the file is valid YAML but not a request.
fn parse_file(file_name: &str, content: &str, counter: &mut Counter) -> Result<Option<Entry>, String> {
    let content = content.trim_start_matches('\u{feff}');
    let root: Value =
        serde_norway::from_str(content).map_err(|e| format!("not a valid YAML file ({e})"))?;
    if !root.is_object() || (root.get("method").is_none() && root.get("url").is_none()) {
        return Ok(None);
    }

    let name = postman::text(root.get("name")).trim().to_string();
    let name = if name.is_empty() {
        request_name(file_name)
    } else {
        name
    };
    Ok(Some(Entry::Request(name, parse_request(&root, counter))))
}

/// `efetivar.http.yaml` → `efetivar`.
fn request_name(file_name: &str) -> String {
    let mut name = file_name;
    for suffix in [".yaml", ".yml", ".YAML", ".YML"] {
        name = name.strip_suffix(suffix).unwrap_or(name);
    }
    name = name.strip_suffix(".http").unwrap_or(name);
    if name.trim().is_empty() {
        "Request".to_string()
    } else {
        name.trim().to_string()
    }
}

/// A `{ name, value }` row (Postman-style `key` is accepted too).
fn row(value: &Value) -> Option<KeyValue> {
    let key = postman::text(value.get("name").or(value.get("key")));
    if key.trim().is_empty() {
        return None;
    }
    let enabled = value.get("enable").and_then(Value::as_bool).unwrap_or(true)
        && !value.get("disabled").and_then(Value::as_bool).unwrap_or(false);
    let text = match value.get("value") {
        Some(v) if !v.is_null() => postman::text(Some(v)),
        _ => postman::text(value.get("example")),
    };
    Some(KeyValue {
        enabled,
        key,
        value: text,
    })
}

fn rows<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = (&'a Value, KeyValue)> {
    postman::items(value, key)
        .iter()
        .filter_map(|r| row(r).map(|kv| (r, kv)))
}

fn parse_request(value: &Value, counter: &mut Counter) -> Request {
    let mut req = Request::default();
    req.params.clear();
    req.headers.clear();
    req.cookies.clear();
    req.form_data.clear();
    req.url_encoded.clear();

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
                counter.warn(&format!("method {other} is not supported (imported as GET)"));
                Method::Get
            }
        };
    }

    let empty = Value::Null;
    let parameters = value.get("parameters").unwrap_or(&empty);

    // URL, with `{id}` path parameters filled in.
    let mut url = postman::text(value.get("url").or(value.get("path")));
    for (_, param) in rows(parameters, "path") {
        if !param.value.is_empty() {
            url = fill_path_param(&url, &param.key, &param.value);
        }
    }
    postman::set_url_from_string(&mut req, url.trim());

    req.params.extend(rows(parameters, "query").map(|(_, kv)| kv));
    req.headers.extend(rows(parameters, "header").map(|(_, kv)| kv));
    req.cookies.extend(rows(parameters, "cookie").map(|(_, kv)| kv));

    if let Some(body) = value.get("requestBody").filter(|b| b.is_object()) {
        parse_body(&mut req, body, counter);
    }

    if let Some(auth) = value.get("auth").filter(|a| a.is_object()) {
        match auth.get("type").and_then(Value::as_str).unwrap_or("") {
            "" | "none" | "noauth" | "inherit" | "inheritance" => {}
            _ => postman::apply_auth(&mut req, auth, counter),
        }
    }

    if !postman::items(value, "preProcessors").is_empty()
        || !postman::items(value, "postProcessors").is_empty()
    {
        counter.warn("pre/post processors (scripts and assertions) are ignored");
    }

    postman::finish_all(&mut req);
    req.sync_query();
    req
}

/// Replaces `{name}` (but not `{{name}}` variables) in `url`.
fn fill_path_param(url: &str, name: &str, value: &str) -> String {
    let pattern = format!("{{{name}}}");
    let mut out = String::with_capacity(url.len());
    let mut rest = url;
    while let Some(at) = rest.find(&pattern) {
        let before = &rest[..at];
        let after = &rest[at + pattern.len()..];
        let doubled = before.ends_with('{') || after.starts_with('}');
        out.push_str(before);
        out.push_str(if doubled { &pattern } else { value });
        rest = after;
    }
    out.push_str(rest);
    out
}

fn parse_body(req: &mut Request, body: &Value, counter: &mut Counter) {
    let kind = postman::text(body.get("type")).to_ascii_lowercase();
    let data = match body.get("data").or(body.get("example")) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        // Inline YAML objects/arrays: send them as JSON.
        Some(other) => serde_json::to_string_pretty(other).unwrap_or_default(),
    };

    match kind.as_str() {
        "" | "none" => {}
        k if k.contains("json") => {
            req.body_kind = BodyKind::Json;
            req.body_text = data;
        }
        k if k.contains("xml") => {
            req.body_kind = BodyKind::Xml;
            req.body_text = data;
        }
        k if k.contains("x-www-form-urlencoded") => {
            req.body_kind = BodyKind::UrlEncoded;
            req.url_encoded
                .extend(rows(body, "parameters").map(|(_, kv)| kv));
        }
        k if k.contains("form-data") => {
            req.body_kind = BodyKind::FormData;
            for (raw, kv) in rows(body, "parameters") {
                if raw.get("type").and_then(Value::as_str) == Some("file") {
                    counter.warn("file uploads in form-data are skipped");
                } else {
                    req.form_data.push(kv);
                }
            }
        }
        k if k.contains("octet-stream") || k == "binary" => {
            req.body_kind = BodyKind::Binary;
            req.body_file = data.trim().to_string();
        }
        k if k.contains("graphql") => counter.warn("GraphQL bodies are not supported"),
        _ => {
            req.body_kind = BodyKind::Text;
            req.body_text = data;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"method: post
url: "{{local}}/credito/{id}/efetivar?x=1"
parameters:
  path:
    - name: id
      value: "42"
  query:
    - name: page
      value: "2"
  header:
    - name: b-Account
      value: "1944818"
    - name: b-Token
      value: "{{access_token}}"
  cookie:
    - name: sid
      value: abc
requestBody:
  type: application/json
  data: |
    {
      "valorTransacao": 1000.00
    }
auth:
  type: bearer
  bearer:
    token: "{{access_token}}"
"#;

    fn dir(files: &[(&str, &str)]) -> Dir {
        Dir {
            name: "Credito".into(),
            files: files
                .iter()
                .map(|(n, c)| (n.to_string(), c.to_string()))
                .collect(),
            dirs: Vec::new(),
        }
    }

    #[test]
    fn imports_a_request_file() {
        let mut tree = Tree::new();
        let summary = import(&mut tree, &dir(&[("efetivar.http.yaml", SAMPLE)])).unwrap();
        assert_eq!(summary.requests, 1);

        let collection = tree.get(summary.collection).unwrap();
        assert_eq!(collection.name, "Credito");
        let node = &collection.children[0];
        assert_eq!(node.name, "efetivar");

        let req = node.request().unwrap();
        assert_eq!(req.method, Method::Post);
        assert_eq!(req.url, "{{local}}/credito/42/efetivar?x=1&page=2");
        assert_eq!(req.headers[0].key, "b-Account");
        assert_eq!(req.headers[0].value, "1944818");
        assert_eq!(req.headers[1].value, "{{access_token}}");
        assert_eq!(req.cookies[0].key, "sid");
        assert_eq!(req.body_kind, BodyKind::Json);
        assert!(req.body_text.contains("\"valorTransacao\": 1000.00"));
        assert_eq!(req.auth_kind, AuthKind::Bearer);
        assert_eq!(req.bearer_token, "{{access_token}}");
    }

    #[test]
    fn nested_folders_and_bad_files() {
        let mut root = dir(&[("a.http.yaml", "method: get\nurl: https://x.test/a\n")]);
        root.dirs.push(Dir {
            name: "Sub".into(),
            files: vec![
                ("b.yaml".into(), "method: delete\nurl: https://x.test/b\n".into()),
                ("notes.yaml".into(), "title: hello\n".into()),
            ],
            dirs: Vec::new(),
        });

        let mut tree = Tree::new();
        let summary = import(&mut tree, &root).unwrap();
        assert_eq!(summary.requests, 2);
        assert_eq!(summary.folders, 1);
        assert_eq!(summary.warnings.len(), 1);

        let collection = tree.get(summary.collection).unwrap();
        assert_eq!(collection.children[0].name, "Sub");
        let b = collection.children[0].children[0].request().unwrap();
        assert_eq!(b.method, Method::Delete);
    }

    #[test]
    fn rejects_when_nothing_is_a_request() {
        let mut tree = Tree::new();
        assert!(import(&mut tree, &dir(&[("x.yaml", "foo: [")])).is_err());
        assert!(import(&mut tree, &dir(&[("x.yaml", "a: 1")])).is_err());
        assert!(tree.roots.is_empty());
    }

    #[test]
    fn keeps_double_brace_variables() {
        assert_eq!(fill_path_param("{{id}}/{id}", "id", "7"), "{{id}}/7");
    }
}
