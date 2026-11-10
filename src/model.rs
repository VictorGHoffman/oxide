//! Data model: the collection tree and the request definition.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

pub type Id = u64;

// ───────────────────────────── Request ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Method {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Head,
}

impl Method {
    pub const ALL: [Method; 7] = [
        Method::Get,
        Method::Post,
        Method::Put,
        Method::Patch,
        Method::Delete,
        Method::Options,
        Method::Head,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Options => "OPTIONS",
            Method::Head => "HEAD",
        }
    }

    /// Short label used in the sidebar and in tabs.
    pub fn short(self) -> &'static str {
        match self {
            Method::Delete => "DEL",
            Method::Options => "OPT",
            other => other.as_str(),
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyValue {
    pub enabled: bool,
    pub key: String,
    pub value: String,
}

impl KeyValue {
    pub fn empty() -> Self {
        Self {
            enabled: true,
            key: String::new(),
            value: String::new(),
        }
    }

    pub fn is_blank(&self) -> bool {
        self.key.is_empty() && self.value.is_empty()
    }
}

/// A key/value table always keeps one trailing blank row to type into.
pub fn blank_table() -> Vec<KeyValue> {
    vec![KeyValue::empty()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BodyKind {
    #[default]
    None,
    FormData,
    UrlEncoded,
    Json,
    Xml,
    Text,
}

impl BodyKind {
    pub const ALL: [BodyKind; 6] = [
        BodyKind::None,
        BodyKind::FormData,
        BodyKind::UrlEncoded,
        BodyKind::Json,
        BodyKind::Xml,
        BodyKind::Text,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BodyKind::None => "none",
            BodyKind::FormData => "form-data",
            BodyKind::UrlEncoded => "x-www-form-urlencoded",
            BodyKind::Json => "JSON",
            BodyKind::Xml => "XML",
            BodyKind::Text => "Text",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AuthKind {
    #[default]
    None,
    Bearer,
    Basic,
    ApiKey,
}

impl AuthKind {
    pub const ALL: [AuthKind; 4] = [
        AuthKind::None,
        AuthKind::Bearer,
        AuthKind::Basic,
        AuthKind::ApiKey,
    ];
}

impl fmt::Display for AuthKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AuthKind::None => "No Auth",
            AuthKind::Bearer => "Bearer Token",
            AuthKind::Basic => "Basic Auth",
            AuthKind::ApiKey => "API Key",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

impl ApiKeyLocation {
    pub const ALL: [ApiKeyLocation; 2] = [ApiKeyLocation::Header, ApiKeyLocation::Query];
}

impl fmt::Display for ApiKeyLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ApiKeyLocation::Header => "Header",
            ApiKeyLocation::Query => "Query Params",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub params: Vec<KeyValue>,
    pub headers: Vec<KeyValue>,
    pub cookies: Vec<KeyValue>,

    pub body_kind: BodyKind,
    pub body_text: String,
    pub form_data: Vec<KeyValue>,
    pub url_encoded: Vec<KeyValue>,

    pub auth_kind: AuthKind,
    pub bearer_token: String,
    pub basic_user: String,
    pub basic_password: String,
    pub api_key_name: String,
    pub api_key_value: String,
    pub api_key_in: ApiKeyLocation,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            method: Method::Get,
            url: String::new(),
            params: blank_table(),
            headers: blank_table(),
            cookies: blank_table(),
            body_kind: BodyKind::None,
            body_text: String::new(),
            form_data: blank_table(),
            url_encoded: blank_table(),
            auth_kind: AuthKind::None,
            bearer_token: String::new(),
            basic_user: String::new(),
            basic_password: String::new(),
            api_key_name: String::new(),
            api_key_value: String::new(),
            api_key_in: ApiKeyLocation::Header,
        }
    }
}

/// Number of enabled, non-empty rows (used for the little counters on the tabs).
pub fn active_count(rows: &[KeyValue]) -> usize {
    rows.iter()
        .filter(|r| r.enabled && !r.key.is_empty())
        .count()
}

// ───────────────────────────── Tree ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Kind {
    Collection,
    Folder,
    Request(Box<Request>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Id,
    pub name: String,
    pub expanded: bool,
    pub kind: Kind,
    pub children: Vec<Node>,
}

impl Node {
    pub fn is_container(&self) -> bool {
        !matches!(self.kind, Kind::Request(_))
    }

    pub fn request(&self) -> Option<&Request> {
        match &self.kind {
            Kind::Request(r) => Some(r),
            _ => None,
        }
    }

    pub fn request_mut(&mut self) -> Option<&mut Request> {
        match &mut self.kind {
            Kind::Request(r) => Some(r),
            _ => None,
        }
    }

    /// Ids of every request in this subtree (including the node itself).
    pub fn request_ids(&self) -> Vec<Id> {
        let mut out = Vec::new();
        self.collect_requests(&mut out);
        out
    }

    fn collect_requests(&self, out: &mut Vec<Id>) {
        if matches!(self.kind, Kind::Request(_)) {
            out.push(self.id);
        }
        for child in &self.children {
            child.collect_requests(out);
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Tree {
    pub roots: Vec<Node>,
    next_id: Id,
}

fn find(nodes: &[Node], id: Id) -> Option<&Node> {
    for node in nodes {
        if node.id == id {
            return Some(node);
        }
        if let Some(found) = find(&node.children, id) {
            return Some(found);
        }
    }
    None
}

fn find_mut(nodes: &mut [Node], id: Id) -> Option<&mut Node> {
    for node in nodes.iter_mut() {
        if node.id == id {
            return Some(node);
        }
        if let Some(found) = find_mut(&mut node.children, id) {
            return Some(found);
        }
    }
    None
}

fn find_parent(nodes: &[Node], parent: Option<Id>, id: Id) -> Option<Option<Id>> {
    for node in nodes {
        if node.id == id {
            return Some(parent);
        }
        if let Some(found) = find_parent(&node.children, Some(node.id), id) {
            return Some(found);
        }
    }
    None
}

/// The list of nodes that live directly under `parent` (`None` = the root list).
fn siblings_mut(roots: &mut Vec<Node>, parent: Option<Id>) -> Option<&mut Vec<Node>> {
    match parent {
        None => Some(roots),
        Some(pid) => find_mut(roots, pid).map(|n| &mut n.children),
    }
}

fn reassign_ids(node: &mut Node, next: &mut Id) {
    node.id = *next;
    *next += 1;
    for child in &mut node.children {
        reassign_ids(child, next);
    }
}

/// "New Request" → "New Request_1" → (duplicating again) "New Request_2" …
/// A trailing `_<number>` on the source name is ignored so copies of copies
/// keep counting instead of stacking suffixes.
fn unique_copy_name(siblings: &[Node], name: &str) -> String {
    let base = match name.rfind('_') {
        Some(pos)
            if pos > 0
                && !name[pos + 1..].is_empty()
                && name[pos + 1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            &name[..pos]
        }
        _ => name,
    };

    (1..)
        .map(|n| format!("{base}_{n}"))
        .find(|candidate| !siblings.iter().any(|s| &s.name == candidate))
        .expect("infinite iterator")
}

impl Tree {
    pub fn new() -> Self {
        Self {
            roots: Vec::new(),
            next_id: 1,
        }
    }

    fn alloc(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn get(&self, id: Id) -> Option<&Node> {
        find(&self.roots, id)
    }

    pub fn get_mut(&mut self, id: Id) -> Option<&mut Node> {
        find_mut(&mut self.roots, id)
    }

    pub fn request(&self, id: Id) -> Option<&Request> {
        self.get(id).and_then(Node::request)
    }

    pub fn request_mut(&mut self, id: Id) -> Option<&mut Request> {
        self.get_mut(id).and_then(Node::request_mut)
    }

    /// `Some(None)` = root level, `Some(Some(p))` = child of `p`, `None` = unknown id.
    pub fn parent_of(&self, id: Id) -> Option<Option<Id>> {
        find_parent(&self.roots, None, id)
    }

    /// The collection/folder that new items should be created in when `id`
    /// is the current context: a container is its own context, a request
    /// uses its parent.
    pub fn container_for(&self, id: Id) -> Option<Id> {
        let node = self.get(id)?;
        if node.is_container() {
            Some(id)
        } else {
            self.parent_of(id).flatten()
        }
    }

    /// Every request in the tree, depth first.
    pub fn requests(&self) -> Vec<(Id, &Request)> {
        fn walk<'a>(nodes: &'a [Node], out: &mut Vec<(Id, &'a Request)>) {
            for node in nodes {
                if let Some(req) = node.request() {
                    out.push((node.id, req));
                }
                walk(&node.children, out);
            }
        }
        let mut out = Vec::new();
        walk(&self.roots, &mut out);
        out
    }

    /// A copy of the tree where every request holds its *saved* content, so
    /// unsaved edits never reach the disk until the user presses Ctrl+S.
    pub fn snapshot(&self, saved: &HashMap<Id, Request>) -> Tree {
        fn apply(nodes: &mut [Node], saved: &HashMap<Id, Request>) {
            for node in nodes {
                let id = node.id;
                if let Some(req) = node.request_mut() {
                    if let Some(version) = saved.get(&id) {
                        *req = version.clone();
                    }
                }
                apply(&mut node.children, saved);
            }
        }
        let mut copy = self.clone();
        apply(&mut copy.roots, saved);
        copy
    }

    pub fn first_collection(&self) -> Option<Id> {
        self.roots.first().map(|n| n.id)
    }

    /// All ancestors of `id`, from the closest to the farthest.
    pub fn ancestors(&self, id: Id) -> Vec<Id> {
        let mut out = Vec::new();
        let mut current = id;
        while let Some(Some(parent)) = self.parent_of(current) {
            out.push(parent);
            current = parent;
        }
        out
    }

    pub fn expand_ancestors(&mut self, id: Id) {
        for ancestor in self.ancestors(id) {
            if let Some(node) = self.get_mut(ancestor) {
                node.expanded = true;
            }
        }
    }

    pub fn add_collection(&mut self) -> Id {
        let id = self.alloc();
        self.roots.push(Node {
            id,
            name: "New Collection".into(),
            expanded: true,
            kind: Kind::Collection,
            children: Vec::new(),
        });
        id
    }

    pub fn add_folder(&mut self, container: Id) -> Option<Id> {
        self.add_child(container, "New Folder", Kind::Folder)
    }

    /// Requests can only be created inside a collection or a folder.
    pub fn add_request(&mut self, container: Id) -> Option<Id> {
        self.add_child(container, "New Request", Kind::Request(Box::default()))
    }

    fn add_child(&mut self, container: Id, name: &str, kind: Kind) -> Option<Id> {
        if !self.get(container)?.is_container() {
            return None;
        }
        let id = self.alloc();
        let parent = self.get_mut(container)?;
        parent.expanded = true;
        parent.children.push(Node {
            id,
            name: name.into(),
            expanded: true,
            kind,
            children: Vec::new(),
        });
        Some(id)
    }

    pub fn remove(&mut self, id: Id) -> Option<Node> {
        let parent = self.parent_of(id)?;
        let list = siblings_mut(&mut self.roots, parent)?;
        let idx = list.iter().position(|n| n.id == id)?;
        Some(list.remove(idx))
    }

    /// Deep-copies a node right after the original, with fresh ids and a
    /// `_1`, `_2`, … suffixed name.
    pub fn duplicate(&mut self, id: Id) -> Option<Id> {
        let parent = self.parent_of(id)?;
        let mut next = self.next_id;

        let list = siblings_mut(&mut self.roots, parent)?;
        let idx = list.iter().position(|n| n.id == id)?;

        let mut copy = list[idx].clone();
        copy.name = unique_copy_name(list, &copy.name);
        reassign_ids(&mut copy, &mut next);
        let new_id = copy.id;
        list.insert(idx + 1, copy);

        self.next_id = next;
        Some(new_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_names_count_up() {
        let mut t = Tree::new();
        let c = t.add_collection();
        let r = t.add_request(c).unwrap();
        let a = t.duplicate(r).unwrap();
        let b = t.duplicate(r).unwrap();
        let c2 = t.duplicate(a).unwrap();
        assert_eq!(t.get(a).unwrap().name, "New Request_1");
        assert_eq!(t.get(b).unwrap().name, "New Request_2");
        assert_eq!(t.get(c2).unwrap().name, "New Request_3");
    }

    #[test]
    fn requests_only_inside_containers() {
        let mut t = Tree::new();
        let c = t.add_collection();
        let r = t.add_request(c).unwrap();
        assert!(t.add_request(r).is_none());
        assert_eq!(t.container_for(r), Some(c));
    }

    #[test]
    fn duplicating_a_folder_gives_children_new_ids() {
        let mut t = Tree::new();
        let c = t.add_collection();
        let f = t.add_folder(c).unwrap();
        let r = t.add_request(f).unwrap();
        let f2 = t.duplicate(f).unwrap();
        let r2 = t.get(f2).unwrap().children[0].id;
        assert_ne!(r, r2);
        assert_eq!(t.get(f2).unwrap().name, "New Folder_1");
    }
}
