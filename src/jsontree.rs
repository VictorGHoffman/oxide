//! A JSON document flattened into the rows of a collapsible tree.

use std::collections::HashSet;

use serde_json::Value;

/// Separates the segments of a node path (it never appears in a key typed by a person).
const SEP: char = '\u{1}';

/// Most rows drawn at once (a huge document would make the window crawl).
pub const ROW_LIMIT: usize = 1500;
/// Open nodes remembered by "expand all" / the default state.
const OPEN_LIMIT: usize = 50_000;

pub enum Key {
    Root,
    Name(String),
    Index(usize),
}

pub enum Leaf {
    Container { array: bool, len: usize },
    Str(String),
    Number(String),
    Bool(bool),
    Null,
}

pub struct Row {
    pub depth: usize,
    /// Identifies the node (used to open / close it).
    pub path: String,
    pub key: Key,
    pub leaf: Leaf,
    pub open: bool,
}

fn child_path(parent: &str, segment: &str) -> String {
    if parent.is_empty() {
        format!("{SEP}{segment}")
    } else {
        format!("{parent}{SEP}{segment}")
    }
}

fn leaf_of(value: &Value) -> Leaf {
    match value {
        Value::Object(map) => Leaf::Container {
            array: false,
            len: map.len(),
        },
        Value::Array(list) => Leaf::Container {
            array: true,
            len: list.len(),
        },
        Value::String(s) => {
            let shown: String = s.chars().take(300).collect();
            let more = if s.chars().count() > 300 { "…" } else { "" };
            Leaf::Str(format!("{shown}{more}"))
        }
        Value::Number(n) => Leaf::Number(n.to_string()),
        Value::Bool(b) => Leaf::Bool(*b),
        Value::Null => Leaf::Null,
    }
}

/// The rows to draw: every node whose ancestors are all open. The second value tells
/// whether the list was cut at [`ROW_LIMIT`].
pub fn rows(root: &Value, open: &HashSet<String>) -> (Vec<Row>, bool) {
    let mut out = Vec::new();
    let mut truncated = false;
    walk(root, Key::Root, String::new(), 0, open, &mut out, &mut truncated);
    (out, truncated)
}

fn walk(
    value: &Value,
    key: Key,
    path: String,
    depth: usize,
    open: &HashSet<String>,
    out: &mut Vec<Row>,
    truncated: &mut bool,
) {
    if out.len() >= ROW_LIMIT {
        *truncated = true;
        return;
    }
    let is_open = open.contains(&path);
    out.push(Row {
        depth,
        path: path.clone(),
        key,
        leaf: leaf_of(value),
        open: is_open,
    });
    if !is_open {
        return;
    }

    match value {
        Value::Object(map) => {
            for (name, child) in map {
                let child_path = child_path(&path, name);
                walk(
                    child,
                    Key::Name(name.clone()),
                    child_path,
                    depth + 1,
                    open,
                    out,
                    truncated,
                );
            }
        }
        Value::Array(list) => {
            for (index, child) in list.iter().enumerate() {
                let child_path = child_path(&path, &index.to_string());
                walk(
                    child,
                    Key::Index(index),
                    child_path,
                    depth + 1,
                    open,
                    out,
                    truncated,
                );
            }
        }
        _ => {}
    }
}

fn is_container(value: &Value) -> bool {
    matches!(value, Value::Object(_) | Value::Array(_))
}

/// What is open when a response arrives: the top level and the containers right below it.
pub fn default_open(root: &Value) -> HashSet<String> {
    let mut open = HashSet::new();
    collect(root, String::new(), 0, 1, &mut open);
    open
}

/// Every container (for "Expand all").
pub fn all_open(root: &Value) -> HashSet<String> {
    let mut open = HashSet::new();
    collect(root, String::new(), 0, usize::MAX, &mut open);
    open
}

/// Only the root (for "Collapse all").
pub fn root_only() -> HashSet<String> {
    HashSet::from([String::new()])
}

fn collect(value: &Value, path: String, depth: usize, max_depth: usize, open: &mut HashSet<String>) {
    if !is_container(value) || depth > max_depth || open.len() >= OPEN_LIMIT {
        return;
    }
    open.insert(path.clone());

    let mut visit = |segment: &str, child: &Value| {
        collect(child, child_path(&path, segment), depth + 1, max_depth, open);
    };
    match value {
        Value::Object(map) => map.iter().for_each(|(k, v)| visit(k, v)),
        Value::Array(list) => list
            .iter()
            .enumerate()
            .for_each(|(i, v)| visit(&i.to_string(), v)),
        _ => {}
    }
}

/// Whether `text` looks like a JSON object/array worth showing as a tree.
pub fn parse(text: &str) -> Option<Value> {
    let start = text.trim_start();
    if !(start.starts_with('{') || start.starts_with('[')) || text.len() > 8_000_000 {
        return None;
    }
    serde_json::from_str(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        serde_json::from_str(r#"{"a":{"b":[1,2],"c":"x"},"d":null,"e":[{"f":true}]}"#).unwrap()
    }

    #[test]
    fn default_state_opens_two_levels() {
        let doc = sample();
        let open = default_open(&doc);
        let (rows, truncated) = rows(&doc, &open);
        assert!(!truncated);
        // root, a, b, c, d, e, e[0]  (b and e[0] are closed)
        let names: Vec<usize> = rows.iter().map(|r| r.depth).collect();
        assert_eq!(names, vec![0, 1, 2, 2, 1, 1, 2]);
        assert!(!rows[2].open);
    }

    #[test]
    fn collapsing_and_expanding() {
        let doc = sample();
        let (collapsed, _) = rows(&doc, &root_only());
        assert_eq!(collapsed.len(), 4); // root + a, d, e

        let (everything, _) = rows(&doc, &all_open(&doc));
        // root, a, b, 1, 2, c, d, e, e[0], f
        assert_eq!(everything.len(), 10);
    }

    #[test]
    fn big_documents_are_cut() {
        let list: Vec<Value> = (0..ROW_LIMIT * 2).map(Value::from).collect();
        let doc = Value::Array(list);
        let (rows, truncated) = rows(&doc, &all_open(&doc));
        assert_eq!(rows.len(), ROW_LIMIT);
        assert!(truncated);
    }

    #[test]
    fn only_objects_and_arrays_become_trees() {
        assert!(parse("{\"a\":1}").is_some());
        assert!(parse("  [1]").is_some());
        assert!(parse("12").is_none());
        assert!(parse("{broken").is_none());
    }
}
