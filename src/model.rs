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

// ───────────────────────── URL <-> query params ─────────────────────────

/// Splits `base ? query # fragment` (the fragment keeps its `#`).
fn split_url(url: &str) -> (&str, &str, &str) {
    let (before_fragment, fragment) = match url.find('#') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, ""),
    };
    match before_fragment.split_once('?') {
        Some((base, query)) => (base, query, fragment),
        None => (before_fragment, "", fragment),
    }
}

/// Decodes `%XX` sequences (a `+` stays a plus). Invalid sequences are kept as typed.
pub fn percent_decode(s: &str) -> String {
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
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Only what would break the `key=value&key=value` structure is escaped.
fn escape_query_part(s: &str, is_key: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("%26"),
            '#' => out.push_str("%23"),
            '=' if is_key => out.push_str("%3D"),
            other => out.push(other),
        }
    }
    out
}

impl Request {
    /// Rewrites the URL's query string from the enabled params (the URL's base and
    /// fragment are kept). Call after the params table changed.
    pub fn url_from_params(&mut self) {
        let query = self
            .params
            .iter()
            .filter(|p| p.enabled && !p.key.trim().is_empty())
            .map(|p| {
                format!(
                    "{}={}",
                    escape_query_part(p.key.trim(), true),
                    escape_query_part(&p.value, false)
                )
            })
            .collect::<Vec<_>>()
            .join("&");

        let (base, _, fragment) = split_url(&self.url);
        let separator = if query.is_empty() { "" } else { "?" };
        self.url = format!("{base}{separator}{query}{fragment}");
    }

    /// Updates the params table from the URL's query string. Enabled rows follow the
    /// URL (in order); disabled rows and the blank row are left where they are.
    pub fn params_from_url(&mut self) {
        let (_, query, _) = split_url(&self.url);
        let mut parsed: std::collections::VecDeque<KeyValue> = query
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| {
                let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
                KeyValue {
                    enabled: true,
                    key: percent_decode(key),
                    value: percent_decode(value),
                }
            })
            .collect();

        let mut rows = Vec::with_capacity(self.params.len() + parsed.len());
        for row in self.params.drain(..) {
            if row.is_blank() || !row.enabled {
                rows.push(row);
            } else if let Some(next) = parsed.pop_front() {
                rows.push(next);
            }
        }

        // Params that are new in the URL go above the trailing blank row.
        let at = if rows.last().is_some_and(KeyValue::is_blank) {
            rows.len() - 1
        } else {
            rows.len()
        };
        for (offset, row) in parsed.into_iter().enumerate() {
            rows.insert(at + offset, row);
        }
        if !rows.last().is_some_and(KeyValue::is_blank) {
            rows.push(KeyValue::empty());
        }
        self.params = rows;
    }

    /// Brings URL and params in line (used for requests saved before they were linked,
    /// and for imported ones): the URL's own query wins, otherwise the params fill it.
    pub fn sync_query(&mut self) {
        if split_url(&self.url).1.is_empty() {
            self.url_from_params();
        } else {
            self.params_from_url();
        }
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
    /// A file sent as the raw request body.
    Binary,
}

impl BodyKind {
    pub const ALL: [BodyKind; 7] = [
        BodyKind::None,
        BodyKind::FormData,
        BodyKind::UrlEncoded,
        BodyKind::Json,
        BodyKind::Xml,
        BodyKind::Text,
        BodyKind::Binary,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BodyKind::None => "none",
            BodyKind::FormData => "form-data",
            BodyKind::UrlEncoded => "x-www-form-urlencoded",
            BodyKind::Json => "JSON",
            BodyKind::Xml => "XML",
            BodyKind::Text => "Text",
            BodyKind::Binary => "binary",
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
    OAuth2,
}

impl AuthKind {
    pub const ALL: [AuthKind; 5] = [
        AuthKind::None,
        AuthKind::Bearer,
        AuthKind::Basic,
        AuthKind::ApiKey,
        AuthKind::OAuth2,
    ];
}

impl fmt::Display for AuthKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AuthKind::None => "No Auth",
            AuthKind::Bearer => "Bearer Token",
            AuthKind::Basic => "Basic Auth",
            AuthKind::ApiKey => "API Key",
            AuthKind::OAuth2 => "OAuth 2.0",
        })
    }
}

/// How an OAuth 2.0 access token is requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OAuthGrant {
    /// Machine to machine: client id + secret.
    #[default]
    ClientCredentials,
    /// The user's name and password are exchanged for a token.
    Password,
}

impl OAuthGrant {
    pub const ALL: [OAuthGrant; 2] = [OAuthGrant::ClientCredentials, OAuthGrant::Password];
}

impl fmt::Display for OAuthGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            OAuthGrant::ClientCredentials => "Client Credentials",
            OAuthGrant::Password => "Password",
        })
    }
}

/// Where the client id and secret travel when asking for a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OAuthClientAuth {
    #[default]
    BasicHeader,
    Body,
}

impl OAuthClientAuth {
    pub const ALL: [OAuthClientAuth; 2] = [OAuthClientAuth::BasicHeader, OAuthClientAuth::Body];
}

impl fmt::Display for OAuthClientAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            OAuthClientAuth::BasicHeader => "Basic Auth header",
            OAuthClientAuth::Body => "Request body",
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
    /// Path of the file sent when `body_kind` is `Binary`.
    pub body_file: String,

    pub auth_kind: AuthKind,
    pub bearer_token: String,
    pub basic_user: String,
    pub basic_password: String,
    pub api_key_name: String,
    pub api_key_value: String,
    pub api_key_in: ApiKeyLocation,

    pub oauth_grant: OAuthGrant,
    pub oauth_token_url: String,
    pub oauth_client_id: String,
    pub oauth_client_secret: String,
    pub oauth_scope: String,
    pub oauth_username: String,
    pub oauth_password: String,
    pub oauth_client_auth: OAuthClientAuth,

    /// Post-response script: assigns response values to variables (see `vars`).
    pub script: String,
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
            body_file: String::new(),
            auth_kind: AuthKind::None,
            bearer_token: String::new(),
            basic_user: String::new(),
            basic_password: String::new(),
            api_key_name: String::new(),
            api_key_value: String::new(),
            api_key_in: ApiKeyLocation::Header,
            oauth_grant: OAuthGrant::default(),
            oauth_token_url: String::new(),
            oauth_client_id: String::new(),
            oauth_client_secret: String::new(),
            oauth_scope: String::new(),
            oauth_username: String::new(),
            oauth_password: String::new(),
            oauth_client_auth: OAuthClientAuth::default(),
            script: String::new(),
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
    /// Variables of a collection (empty for folders and requests).
    #[serde(default)]
    pub variables: Vec<KeyValue>,
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

/// Where a dragged node lands relative to the row it is dropped on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropZone {
    /// Just above the row, as a sibling.
    Before,
    /// Inside the row's collection/folder (at the end).
    Inside,
    /// Just below the row, as a sibling.
    After,
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
            variables: blank_table(),
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
            variables: Vec::new(),
        });
        Some(id)
    }

    /// Where `id` would really land when dropped on `target` with `wanted`, or `None` when
    /// that drop is not allowed: collections only reorder among collections, everything else
    /// stays inside a collection, and nothing goes into itself.
    pub fn drop_zone(&self, id: Id, target: Id, wanted: DropZone) -> Option<DropZone> {
        if id == target || self.ancestors(target).contains(&id) {
            return None;
        }
        let dragged = self.get(id)?;
        let over = self.get(target)?;

        if matches!(dragged.kind, Kind::Collection) {
            return (matches!(over.kind, Kind::Collection) && wanted != DropZone::Inside)
                .then_some(wanted);
        }
        match (&over.kind, wanted) {
            // Nothing but collections lives at the top level.
            (Kind::Collection, _) => Some(DropZone::Inside),
            (Kind::Folder, zone) => Some(zone),
            (Kind::Request(_), DropZone::Inside) => None,
            (Kind::Request(_), zone) => Some(zone),
        }
    }

    /// Moves `id` next to / into `target`. Returns whether anything moved.
    pub fn move_node(&mut self, id: Id, target: Id, wanted: DropZone) -> bool {
        let Some(zone) = self.drop_zone(id, target, wanted) else {
            return false;
        };
        if zone != DropZone::Inside && self.parent_of(target).is_none() {
            return false;
        }
        let Some(node) = self.remove(id) else {
            return false;
        };

        match zone {
            DropZone::Inside => {
                let Some(container) = self.get_mut(target) else {
                    return false;
                };
                container.expanded = true;
                container.children.push(node);
            }
            DropZone::Before | DropZone::After => {
                let parent = self.parent_of(target).unwrap_or(None);
                let Some(list) = siblings_mut(&mut self.roots, parent) else {
                    return false;
                };
                let at = list.iter().position(|n| n.id == target).unwrap_or(list.len());
                list.insert(if zone == DropZone::After { at + 1 } else { at }, node);
            }
        }
        true
    }

    pub fn remove(&mut self, id: Id) -> Option<Node> {
        let parent = self.parent_of(id)?;
        let list = siblings_mut(&mut self.roots, parent)?;
        let idx = list.iter().position(|n| n.id == id)?;
        Some(list.remove(idx))
    }

    /// Deep-copies a node right after the original, with fresh ids and a
    /// `_1`, `_2`, … suffixed name.
    /// Links every request's URL with its params (see [`Request::sync_query`]).
    pub fn sync_queries(&mut self) {
        fn walk(nodes: &mut [Node]) {
            for node in nodes {
                if let Some(req) = node.request_mut() {
                    req.sync_query();
                }
                walk(&mut node.children);
            }
        }
        walk(&mut self.roots);
    }

    /// Adds every collection of `other` next to the existing ones (new ids; a name that is
    /// already taken gets " (restored)"). Returns how many collections were added.
    pub fn merge(&mut self, other: Tree) -> usize {
        let count = other.roots.len();
        for mut node in other.roots {
            let taken = |roots: &[Node], name: &str| roots.iter().any(|n| n.name == name);
            if taken(&self.roots, &node.name) {
                let base = format!("{} (restored)", node.name);
                let mut name = base.clone();
                let mut n = 2;
                while taken(&self.roots, &name) {
                    name = format!("{base} {n}");
                    n += 1;
                }
                node.name = name;
            }
            reassign_ids(&mut node, &mut self.next_id);
            self.roots.push(node);
        }
        count
    }

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
    fn url_and_params_follow_each_other() {
        let mut r = Request::default();
        r.url = "https://x.test/a?b=1&c=2%263#top".into();
        r.params.clear();
        r.params_from_url();
        assert_eq!(r.params[0].key, "b");
        assert_eq!(r.params[1].value, "2&3");
        assert!(r.params.last().unwrap().is_blank());

        // Disabling a param removes it from the URL but keeps the row.
        r.params[0].enabled = false;
        r.url_from_params();
        assert_eq!(r.url, "https://x.test/a?c=2%263#top");
        assert_eq!(r.params.len(), 3);

        // Editing the URL keeps the disabled row where it was.
        r.url = "https://x.test/a?c=9&d=4".into();
        r.params_from_url();
        let keys: Vec<&str> = r.params.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, ["b", "c", "d", ""]);
        assert!(!r.params[0].enabled);

        // Removing everything leaves a clean URL.
        r.url = "https://x.test/a".into();
        r.params_from_url();
        r.url_from_params();
        assert_eq!(r.url, "https://x.test/a");
    }

    #[test]
    fn merging_keeps_both_sets() {
        let mut a = Tree::new();
        let first = a.add_collection();
        a.add_request(first);
        let mut b = a.clone();
        let added = a.merge(b.clone());
        assert_eq!(added, 1);
        assert_eq!(a.roots.len(), 2);
        assert_eq!(a.roots[1].name, "New Collection (restored)");
        let ids: Vec<Id> = a.roots.iter().flat_map(|r| std::iter::once(r.id).chain(r.children.iter().map(|c| c.id))).collect();
        assert_eq!(ids.len(), 4);
        assert!(ids.iter().all(|id| ids.iter().filter(|x| *x == id).count() == 1));
        a.merge(std::mem::take(&mut b));
        assert_eq!(a.roots[2].name, "New Collection (restored) 2");
    }

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
    fn moving_nodes_around() {
        let mut t = Tree::new();
        let c1 = t.add_collection();
        let c2 = t.add_collection();
        let f = t.add_folder(c1).unwrap();
        let a = t.add_request(c1).unwrap();
        let b = t.add_request(f).unwrap();

        // A request goes into a folder, then before another request.
        assert!(t.move_node(a, f, DropZone::Inside));
        assert_eq!(t.get(f).unwrap().children.last().unwrap().id, a);
        assert!(t.move_node(a, b, DropZone::Before));
        let names: Vec<Id> = t.get(f).unwrap().children.iter().map(|n| n.id).collect();
        assert_eq!(names, vec![a, b]);

        // Dropping on a collection's top/bottom edge still puts it inside.
        assert_eq!(t.drop_zone(a, c2, DropZone::After), Some(DropZone::Inside));
        assert!(t.move_node(a, c2, DropZone::After));
        assert_eq!(t.parent_of(a), Some(Some(c2)));

        // A folder cannot go into itself or its children; collections only reorder.
        assert!(!t.move_node(f, b, DropZone::Inside));
        assert!(!t.move_node(c1, f, DropZone::After));
        assert!(!t.move_node(c1, c2, DropZone::Inside));
        assert!(t.move_node(c1, c2, DropZone::After));
        assert_eq!(t.roots.iter().map(|n| n.id).collect::<Vec<_>>(), vec![c2, c1]);
        assert!(t.move_node(c1, c2, DropZone::Before));
        assert_eq!(t.roots[0].id, c1);
        // Nothing is ever lost.
        assert_eq!(t.requests().len(), 2);
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
