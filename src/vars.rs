//! Variables (`{{name}}`) and the tiny post-response script language.
//!
//! A script is a list of assignments, one per line:
//!
//! ```text
//! # collection variable (the default)
//! token = body.data.access_token
//! # global variable
//! global.user_id = body.user.id
//! # text can be joined with +
//! auth = "Bearer " + body.token
//! session = cookie.sid
//! ```
//!
//! Sources: `body`, `body.path.to[0].field` (JSON), `header.Name`, `cookie.Name`,
//! `status`, `time` (milliseconds), `url`, `"text"` and `{{other_variable}}`.

use serde_json::Value;

use crate::http::Response;
use crate::model::*;

/// Where a variable lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Shared by every collection.
    Global,
    /// Only for the collection the request belongs to (wins over global ones).
    Collection,
}

pub struct Assignment {
    pub scope: Scope,
    pub name: String,
    pub value: String,
}

#[derive(Default)]
pub struct Outcome {
    pub assignments: Vec<Assignment>,
    /// Problems found while running (bad syntax, value not found, ...).
    pub errors: Vec<String>,
}

pub type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

// ───────────────────────────── substitution ─────────────────────────────

/// Replaces every `{{name}}` that `lookup` knows. Unknown ones are left untouched.
pub fn substitute(text: &str, lookup: Lookup) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };

        let name = after[..end].trim();
        match lookup(name) {
            Some(value) => out.push_str(&value),
            None => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// The names still referenced as `{{name}}` in `text`.
pub fn names_in(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let name = after[..end].trim();
        if !name.is_empty() && !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
        rest = &after[end + 2..];
    }
    names
}

/// A piece of text: plain, or a `{{variable}}` reference.
pub enum Piece<'a> {
    Text(&'a str),
    Variable { raw: &'a str, name: &'a str },
}

/// Splits `text` into plain text and `{{variable}}` references (in order, nothing lost).
pub fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut rest = text;

    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        if start > 0 {
            out.push(Piece::Text(&rest[..start]));
        }
        out.push(Piece::Variable {
            raw: &rest[start..start + 2 + end + 2],
            name: after[..end].trim(),
        });
        rest = &after[end + 2..];
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest));
    }
    out
}

/// A copy of `req` with every variable replaced.
pub fn resolve(req: &Request, lookup: Lookup) -> Request {
    let sub = |s: &str| substitute(s, lookup);
    let rows = |rows: &[KeyValue]| -> Vec<KeyValue> {
        rows.iter()
            .map(|r| KeyValue {
                enabled: r.enabled,
                key: sub(&r.key),
                value: sub(&r.value),
            })
            .collect()
    };

    let mut out = req.clone();
    out.url = sub(&req.url);
    out.params = rows(&req.params);
    out.headers = rows(&req.headers);
    out.cookies = rows(&req.cookies);
    out.form_data = rows(&req.form_data);
    out.url_encoded = rows(&req.url_encoded);
    out.body_text = sub(&req.body_text);
    out.bearer_token = sub(&req.bearer_token);
    out.basic_user = sub(&req.basic_user);
    out.basic_password = sub(&req.basic_password);
    out.api_key_name = sub(&req.api_key_name);
    out.api_key_value = sub(&req.api_key_value);
    out
}

// ───────────────────────────── scripts ─────────────────────────────

/// Runs `script` against a response and returns the variables it assigns.
pub fn run_script(script: &str, resp: &Response, lookup: Lookup) -> Outcome {
    let mut outcome = Outcome::default();
    let json: Option<Value> = serde_json::from_str(&resp.body_raw).ok();

    for (index, raw) in script.lines().enumerate() {
        let number = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        let line = line.strip_prefix("set ").unwrap_or(line);

        let Some((target, expr)) = line.split_once('=') else {
            outcome
                .errors
                .push(format!("line {number}: expected `name = value`"));
            continue;
        };

        let (scope, name) = parse_target(target.trim());
        if !valid_name(name) {
            outcome
                .errors
                .push(format!("line {number}: invalid variable name \"{name}\""));
            continue;
        }

        // Values assigned earlier in this script are visible to the next lines.
        let value = {
            let earlier = |n: &str| {
                outcome
                    .assignments
                    .iter()
                    .rev()
                    .find(|a| a.name == n)
                    .map(|a| a.value.clone())
                    .or_else(|| lookup(n))
            };
            evaluate(expr, resp, json.as_ref(), &earlier)
        };

        match value {
            Ok(value) => outcome.assignments.push(Assignment {
                scope,
                name: name.to_string(),
                value,
            }),
            Err(e) => outcome.errors.push(format!("line {number}: {e}")),
        }
    }
    outcome
}

fn parse_target(target: &str) -> (Scope, &str) {
    if let Some(name) = target.strip_prefix("global.") {
        (Scope::Global, name.trim())
    } else if let Some(name) = target.strip_prefix("collection.") {
        (Scope::Collection, name.trim())
    } else {
        (Scope::Collection, target)
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Evaluates `a + b + "c"`: every part is a source or a quoted text.
fn evaluate(
    expr: &str,
    resp: &Response,
    json: Option<&Value>,
    lookup: Lookup,
) -> Result<String, String> {
    let parts = split_plus(expr);
    if parts.iter().all(|p| p.trim().is_empty()) {
        return Err("nothing to assign".into());
    }

    let mut out = String::new();
    for part in parts {
        out.push_str(&evaluate_part(part.trim(), resp, json, lookup)?);
    }
    Ok(out)
}

/// Splits on `+` signs that are not inside quotes.
fn split_plus(expr: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut quote: Option<char> = None;
    let mut start = 0;

    for (i, c) in expr.char_indices() {
        match (quote, c) {
            (None, '"') | (None, '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '+') => {
                parts.push(&expr[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&expr[start..]);
    parts
}

fn evaluate_part(
    part: &str,
    resp: &Response,
    json: Option<&Value>,
    lookup: Lookup,
) -> Result<String, String> {
    // "text" or 'text' (may contain {{variables}})
    for quote in ['"', '\''] {
        if part.len() >= 2 && part.starts_with(quote) && part.ends_with(quote) {
            return Ok(substitute(&part[1..part.len() - 1], lookup));
        }
    }
    if part.starts_with("{{") {
        let value = substitute(part, lookup);
        return if value.contains("{{") {
            Err(format!("variable {part} is not defined"))
        } else {
            Ok(value)
        };
    }

    match part {
        "status" => return Ok(resp.status.to_string()),
        "time" => return Ok(resp.elapsed.as_millis().to_string()),
        "url" => return Ok(resp.final_url.clone()),
        "body" => return Ok(resp.body_raw.clone()),
        _ => {}
    }

    if let Some(path) = part
        .strip_prefix("body.")
        .or_else(|| part.strip_prefix("body["))
    {
        // `body[0]` keeps its bracket: put it back.
        let path = if part.starts_with("body[") {
            format!("[{path}")
        } else {
            path.to_string()
        };
        let json = json.ok_or("the response body is not JSON")?;
        return json_path(json, &path)
            .map(to_text)
            .ok_or_else(|| format!("{part} was not found in the response"));
    }
    if let Some(name) = part.strip_prefix("header.") {
        return resp
            .header(name.trim())
            .map(str::to_string)
            .ok_or_else(|| format!("header \"{}\" was not found", name.trim()));
    }
    if let Some(name) = part.strip_prefix("cookie.") {
        let name = name.trim();
        return resp
            .cookies
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.value.clone())
            .ok_or_else(|| format!("cookie \"{name}\" was not set by the response"));
    }
    if part.parse::<f64>().is_ok() {
        return Ok(part.to_string());
    }

    Err(format!(
        "don't know what \"{part}\" is (use body.…, header.…, cookie.…, status, time or \"text\")"
    ))
}

/// `a.b[0].c` / `a["key"]` over a JSON value.
fn json_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let bytes = path.as_bytes();
    let mut current = root;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'.' => i += 1,
            b'[' => {
                let end = path[i..].find(']')? + i;
                let inner = path[i + 1..end].trim();
                current = match inner.parse::<usize>() {
                    Ok(n) => current.get(n)?,
                    Err(_) => current.get(inner.trim_matches(|c| c == '"' || c == '\''))?,
                };
                i = end + 1;
            }
            _ => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' {
                    i += 1;
                }
                current = current.get(&path[start..i])?;
            }
        }
    }
    Some(current)
}

fn to_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn substitutes_known_variables_only() {
        let lookup = |n: &str| (n == "host").then(|| "api.test".to_string());
        assert_eq!(
            substitute("https://{{ host }}/{{missing}}", &lookup),
            "https://api.test/{{missing}}"
        );
        assert_eq!(substitute("{{open", &none), "{{open");
    }

    #[test]
    fn pieces_keep_all_the_text() {
        let text = "a {{x}} b{{ y }}{{z";
        let joined: String = pieces(text)
            .iter()
            .map(|p| match p {
                Piece::Text(t) => *t,
                Piece::Variable { raw, .. } => *raw,
            })
            .collect();
        assert_eq!(joined, text);
        assert!(matches!(pieces(text)[1], Piece::Variable { name: "x", .. }));
    }

    #[test]
    fn json_paths() {
        let v: Value = serde_json::from_str(r#"{"a":{"b":[{"c":7},{"c":"x"}]},"k y":1}"#).unwrap();
        assert_eq!(to_text(json_path(&v, "a.b[0].c").unwrap()), "7");
        assert_eq!(to_text(json_path(&v, "a.b[1].c").unwrap()), "x");
        assert_eq!(to_text(json_path(&v, r#"["k y"]"#).unwrap()), "1");
        assert!(json_path(&v, "a.z").is_none());
    }

    #[test]
    fn splits_plus_outside_quotes() {
        assert_eq!(
            split_plus(r#""a+b" + body.x"#),
            vec![r#""a+b" "#, " body.x"]
        );
    }
}
