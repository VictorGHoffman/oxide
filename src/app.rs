//! Application state, messages and the `update` logic.

use std::collections::{HashMap, HashSet};

use iced::widget::{operation, scrollable, text_editor};
use iced::{event, keyboard, mouse, window, Event, Point, Size, Subscription, Task, Theme};

use crate::http;
use crate::model::*;
use crate::storage;

pub const RENAME_ID: &str = "oxide-rename-input";
pub const TABS_ID: &str = "oxide-tabs";

// ───────────────────────────── UI enums ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Params,
    Body,
    Headers,
    Cookies,
    Auth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseTab {
    Body,
    Headers,
    Cookies,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyView {
    Pretty,
    Raw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvList {
    Params,
    Headers,
    Cookies,
    FormData,
    UrlEncoded,
}

/// Every edit that can happen on the request shown in the active tab.
#[derive(Debug, Clone)]
pub enum Edit {
    Method(Method),
    Url(String),
    KvToggle(KvList, usize, bool),
    KvKey(KvList, usize, String),
    KvValue(KvList, usize, String),
    KvRemove(KvList, usize),
    BodyKind(BodyKind),
    BodyAction(text_editor::Action),
    AuthKind(AuthKind),
    BearerToken(String),
    BasicUser(String),
    BasicPassword(String),
    ApiKeyName(String),
    ApiKeyValue(String),
    ApiKeyIn(ApiKeyLocation),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    NewRequest,
    NewFolder,
    NewCollection,
    Rename,
    Duplicate,
    Delete,
    CloseTab,
    CloseOthers,
    CloseAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuTarget {
    Node(Id),
    Tab(Id),
    Empty,
}

#[derive(Debug, Clone, Copy)]
pub struct ContextMenu {
    pub target: MenuTarget,
    pub position: Point,
}

#[derive(Debug, Clone, Copy)]
pub enum Shortcut {
    NewRequest,
    NewFolder,
    Delete,
    Duplicate,
    Rename,
    CloseTab,
    GoToTab(usize),
    Escape,
    Save,
    Confirm,
    FocusNext,
    FocusPrevious,
}

#[derive(Debug, Clone)]
pub enum Message {
    // Sidebar
    SelectNode(Id),
    OpenPinned(Id),
    HoverNode(Option<Id>),
    NodeContext(Id),
    SidebarContext,
    NewCollection,
    RenameChanged(String),
    RenameSubmit,

    // Tabs
    ActivateTab(Id),
    PinTab(Id),
    CloseTab(Id),
    TabContext(Id),
    TabsScrolled(mouse::ScrollDelta),

    // Delete confirmation
    ConfirmDelete,
    CancelDelete,

    // Context menu / shortcuts
    MenuAction(MenuAction),
    CloseMenu,
    Shortcut(Shortcut),

    // Window-level events
    ModifiersChanged(keyboard::Modifiers),
    CursorMoved(Point),
    WindowResized(Size),

    // Request editor
    Edit(Edit),
    SectionChanged(Section),
    Send,
    Cancel,
    Response(Id, u64, Result<http::Response, String>),

    // Response viewer
    ResponseTabChanged(ResponseTab),
    BodyViewChanged(BodyView),
    ResponseAction(text_editor::Action),
    CopyBody,
}

// ───────────────────────────── State ─────────────────────────────

pub struct Tab {
    pub request_id: Id,
    /// `false` = preview tab (italic), replaced by the next single click.
    pub pinned: bool,
}

pub struct Renaming {
    pub id: Id,
    pub text: String,
}

pub enum ResponseState {
    Idle,
    Sending,
    Cancelled,
    Failed(String),
    Done(Box<http::Response>),
}

/// Per-request UI state that is not part of the saved request definition.
pub struct Runtime {
    pub section: Section,
    pub body_editor: text_editor::Content,
    pub state: ResponseState,
    pub token: u64,
    pub abort: Option<iced::task::Handle>,
    pub response_tab: ResponseTab,
    pub body_view: BodyView,
    pub response_editor: text_editor::Content,
}

impl Runtime {
    fn new(body_text: &str) -> Self {
        Self {
            section: Section::Params,
            body_editor: text_editor::Content::with_text(body_text),
            state: ResponseState::Idle,
            token: 0,
            abort: None,
            response_tab: ResponseTab::Body,
            body_view: BodyView::Pretty,
            response_editor: text_editor::Content::new(),
        }
    }
}

pub struct Oxide {
    pub tree: Tree,
    pub selected: Option<Id>,
    pub hovered: Option<Id>,
    pub renaming: Option<Renaming>,
    pub tabs: Vec<Tab>,
    pub active: Option<Id>,
    pub runtime: HashMap<Id, Runtime>,
    pub menu: Option<ContextMenu>,
    pub cursor: Point,
    pub window_size: Size,
    /// Last saved version of every request (what is on disk).
    pub saved: HashMap<Id, Request>,
    /// Nodes waiting for the user to confirm their deletion.
    pub confirm_delete: Option<Vec<Id>>,
    pub save_error: Option<String>,
    /// Selected nodes besides the primary one (`selected`).
    pub extra: HashSet<Id>,
    /// Where a Shift+click range starts.
    pub anchor: Option<Id>,
    pub modifiers: keyboard::Modifiers,
    last_session: storage::Session,
}

impl Oxide {
    pub fn new() -> (Self, Task<Message>) {
        let (tree, save_error) = match storage::load() {
            Ok(Some(tree)) => (tree, None),
            Ok(None) => (starter_tree(), None),
            Err(e) => (starter_tree(), Some(e)),
        };
        let selected = tree.first_collection();

        let mut app = Self {
            tree,
            selected,
            hovered: None,
            renaming: None,
            tabs: Vec::new(),
            active: None,
            runtime: HashMap::new(),
            menu: None,
            cursor: Point::ORIGIN,
            window_size: Size::new(1280.0, 800.0),
            saved: HashMap::new(),
            confirm_delete: None,
            save_error,
            extra: HashSet::new(),
            anchor: selected,
            modifiers: keyboard::Modifiers::default(),
            last_session: storage::Session::default(),
        };
        app.sync_saved();
        // On startup only the path down to the active tab is expanded; the
        // restored session re-expands it (see `activate`).
        collapse_all(&mut app.tree.roots);
        app.restore_session();
        app.last_session = app.current_session();
        (app, Task::none())
    }

    // ───────────────────────── session ─────────────────────────

    fn current_session(&self) -> storage::Session {
        storage::Session {
            tabs: self
                .tabs
                .iter()
                .map(|t| storage::SessionTab {
                    id: t.request_id,
                    pinned: t.pinned,
                })
                .collect(),
            active: self.active,
        }
    }

    /// Reopens the tabs that were open last time (skipping deleted requests).
    fn restore_session(&mut self) {
        let Some(session) = storage::load_session() else {
            return;
        };
        for tab in session.tabs {
            let exists = self.tree.request(tab.id).is_some();
            let duplicate = self.tabs.iter().any(|t| t.request_id == tab.id);
            if exists && !duplicate {
                self.ensure_runtime(tab.id);
                self.tabs.push(Tab {
                    request_id: tab.id,
                    pinned: tab.pinned,
                });
            }
        }
        let active = session
            .active
            .filter(|id| self.tabs.iter().any(|t| t.request_id == *id))
            .or_else(|| self.tabs.last().map(|t| t.request_id));
        if let Some(id) = active {
            self.activate(id);
        }
    }

    /// Called after every message: the open tabs are written whenever they
    /// change, so they survive any way of closing the app (or a crash).
    fn sync_session(&mut self) {
        let session = self.current_session();
        if session != self.last_session {
            if let Err(e) = storage::save_session(&session) {
                self.save_error = Some(e);
            }
            self.last_session = session;
        }
    }

    // ───────────────────────── selection ─────────────────────────

    pub fn is_selected(&self, id: Id) -> bool {
        self.selected == Some(id) || self.extra.contains(&id)
    }

    /// Selected nodes in tree order.
    fn selected_ids(&self) -> Vec<Id> {
        let mut out = Vec::new();
        collect_ids(&self.tree.roots, false, &mut out);
        out.retain(|id| self.is_selected(*id));
        out
    }

    fn select_only(&mut self, id: Id) {
        self.selected = Some(id);
        self.extra.clear();
        self.anchor = Some(id);
    }

    /// Shift+click: everything visible between the anchor and `id`.
    fn select_range(&mut self, id: Id) {
        let mut visible = Vec::new();
        collect_ids(&self.tree.roots, true, &mut visible);

        let from = self
            .anchor
            .and_then(|a| visible.iter().position(|v| *v == a));
        let to = visible.iter().position(|v| *v == id);

        match (from, to) {
            (Some(a), Some(b)) => {
                let (lo, hi) = (a.min(b), a.max(b));
                self.extra = visible[lo..=hi].iter().copied().collect();
                self.extra.remove(&id);
                self.selected = Some(id);
            }
            _ => self.select_only(id),
        }
    }

    /// Ctrl/Cmd+click: add or remove a single node.
    fn toggle_selection(&mut self, id: Id) {
        if self.is_selected(id) {
            if self.selected == Some(id) {
                let promoted = self.extra.iter().next().copied();
                if let Some(p) = promoted {
                    self.extra.remove(&p);
                }
                self.selected = promoted;
            } else {
                self.extra.remove(&id);
            }
        } else {
            if let Some(prev) = self.selected {
                self.extra.insert(prev);
            }
            self.selected = Some(id);
            self.anchor = Some(id);
        }
    }

    /// The nodes an action applies to. Nested selections (a folder and one
    /// of its children) are reduced to the outermost nodes.
    fn action_targets(&self, target: Option<Id>) -> Vec<Id> {
        let ids = match target {
            Some(t) if self.is_selected(t) => self.selected_ids(),
            Some(t) => vec![t],
            None => {
                let selected = self.selected_ids();
                if selected.is_empty() {
                    self.active.into_iter().collect()
                } else {
                    selected
                }
            }
        };
        let set: HashSet<Id> = ids.iter().copied().collect();
        ids.into_iter()
            .filter(|id| self.tree.get(*id).is_some())
            .filter(|id| !self.tree.ancestors(*id).iter().any(|a| set.contains(a)))
            .collect()
    }

    // ───────────────────────── saving ─────────────────────────

    /// Requests that have unsaved edits (their live content differs from disk).
    pub fn is_dirty(&self, id: Id) -> bool {
        match (self.tree.request(id), self.saved.get(&id)) {
            (Some(current), Some(saved)) => current != saved,
            _ => false,
        }
    }

    /// New requests count as saved as they are; deleted ones are forgotten.
    fn sync_saved(&mut self) {
        let mut live = HashSet::new();
        for (id, req) in self.tree.requests() {
            live.insert(id);
            self.saved.entry(id).or_insert_with(|| req.clone());
        }
        self.saved.retain(|id, _| live.contains(id));
    }

    /// Writes the tree (with each request's *saved* content) to disk.
    fn persist(&mut self) {
        self.sync_saved();
        let snapshot = self.tree.snapshot(&self.saved);
        self.save_error = storage::save(&snapshot).err();
    }

    /// Ctrl+S: save the request in the active tab.
    fn save_active(&mut self) {
        if let Some(id) = self.active {
            if let Some(req) = self.tree.request(id).cloned() {
                self.saved.insert(id, req);
                self.persist();
            }
        }
    }

    pub fn title(&self) -> String {
        match self.active.and_then(|id| self.tree.get(id)) {
            Some(node) => {
                let mark = if self.is_dirty(node.id) { "* " } else { "" };
                format!("{mark}{} — Oxide", node.name)
            }
            None => "Oxide".to_string(),
        }
    }

    pub fn theme(&self) -> Theme {
        crate::theme::oxide()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        event::listen_with(handle_event)
    }

    // ───────────────────────── tabs ─────────────────────────

    fn ensure_runtime(&mut self, id: Id) {
        if self.runtime.contains_key(&id) {
            return;
        }
        if let Some(req) = self.tree.request(id) {
            let rt = Runtime::new(&req.body_text);
            self.runtime.insert(id, rt);
        }
    }

    fn activate(&mut self, id: Id) {
        self.ensure_runtime(id);
        self.active = Some(id);
        self.select_only(id);
        self.tree.expand_ancestors(id);
    }

    /// Single click: reuse the preview tab (or open one) unless the request
    /// is already open.
    fn open_preview(&mut self, id: Id) {
        if !self.tabs.iter().any(|t| t.request_id == id) {
            let tab = Tab {
                request_id: id,
                pinned: false,
            };
            match self.tabs.iter().position(|t| !t.pinned) {
                Some(pos) => self.tabs[pos] = tab,
                None => self.tabs.push(tab),
            }
        }
        self.activate(id);
    }

    /// Double click / editing: make the tab permanent.
    fn open_pinned(&mut self, id: Id) {
        match self.tabs.iter_mut().find(|t| t.request_id == id) {
            Some(tab) => tab.pinned = true,
            None => self.tabs.push(Tab {
                request_id: id,
                pinned: true,
            }),
        }
        self.activate(id);
    }

    fn pin(&mut self, id: Id) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.request_id == id) {
            tab.pinned = true;
        }
    }

    fn close_tab(&mut self, id: Id) {
        let Some(pos) = self.tabs.iter().position(|t| t.request_id == id) else {
            return;
        };
        self.tabs.remove(pos);

        if self.active == Some(id) {
            let next = self
                .tabs
                .get(pos)
                .or_else(|| pos.checked_sub(1).and_then(|p| self.tabs.get(p)))
                .map(|t| t.request_id);
            match next {
                Some(next) => self.activate(next),
                None => self.active = None,
            }
        }
    }

    fn close_other_tabs(&mut self, keep: Id) {
        self.tabs.retain(|t| t.request_id == keep);
        if self.tabs.is_empty() {
            self.active = None;
        } else {
            self.activate(keep);
        }
    }

    fn close_all_tabs(&mut self) {
        self.tabs.clear();
        self.active = None;
    }

    // ───────────────────────── tree actions ─────────────────────────

    /// Where "New Request" / "New Folder" should create things when no
    /// explicit target is given.
    fn default_container(&self) -> Option<Id> {
        self.selected
            .and_then(|id| self.tree.container_for(id))
            .or_else(|| self.active.and_then(|id| self.tree.container_for(id)))
            .or_else(|| self.tree.first_collection())
    }

    fn container_for_target(&self, target: Option<Id>) -> Option<Id> {
        target
            .and_then(|id| self.tree.container_for(id))
            .or_else(|| self.default_container())
    }

    fn commit_rename(&mut self) {
        if let Some(r) = self.renaming.take() {
            let name = r.text.trim();
            if !name.is_empty() {
                let mut changed = false;
                if let Some(node) = self.tree.get_mut(r.id) {
                    changed = node.name != name;
                    node.name = name.to_string();
                }
                if changed {
                    self.persist();
                }
            }
        }
    }

    fn start_rename(&mut self, id: Id) -> Task<Message> {
        let Some(node) = self.tree.get(id) else {
            return Task::none();
        };
        self.renaming = Some(Renaming {
            id,
            text: node.name.clone(),
        });
        self.tree.expand_ancestors(id);
        Task::batch([
            operation::focus(RENAME_ID),
            operation::select_all(RENAME_ID),
        ])
    }

    fn delete_node(&mut self, id: Id) {
        let Some(removed) = self.tree.remove(id) else {
            return;
        };
        for rid in removed.request_ids() {
            self.close_tab(rid);
            if let Some(rt) = self.runtime.remove(&rid) {
                if let Some(handle) = rt.abort {
                    handle.abort();
                }
            }
        }
        if self.renaming.as_ref().is_some_and(|r| r.id == id) {
            self.renaming = None;
        }
        let tree = &self.tree;
        self.extra.retain(|id| tree.get(*id).is_some());
        if self.selected.is_some_and(|s| tree.get(s).is_none()) {
            self.selected = None;
        }
        if self.selected.is_none() {
            self.selected = self.extra.iter().next().copied();
            if let Some(s) = self.selected {
                self.extra.remove(&s);
            }
        }
        if self.anchor.is_some_and(|a| self.tree.get(a).is_none()) {
            self.anchor = self.selected;
        }
        self.hovered = None;
    }

    fn perform(&mut self, action: MenuAction, target: Option<Id>) -> Task<Message> {
        let node_target = target.or(self.selected).or(self.active);

        match action {
            MenuAction::NewRequest => {
                if let Some(container) = self.container_for_target(target) {
                    if let Some(id) = self.tree.add_request(container) {
                        self.open_pinned(id);
                        self.persist();
                    }
                }
            }
            MenuAction::NewFolder => {
                if let Some(container) = self.container_for_target(target) {
                    if let Some(id) = self.tree.add_folder(container) {
                        self.selected = Some(id);
                        self.persist();
                    }
                }
            }
            MenuAction::NewCollection => {
                self.selected = Some(self.tree.add_collection());
                self.persist();
            }
            MenuAction::Rename => {
                if let Some(id) = node_target {
                    return self.start_rename(id);
                }
            }
            MenuAction::Duplicate => {
                let targets = self.action_targets(target);
                let copies: Vec<Id> = targets
                    .into_iter()
                    .filter_map(|id| self.tree.duplicate(id))
                    .collect();
                if let Some((&last, rest)) = copies.split_last() {
                    self.selected = Some(last);
                    self.extra = rest.iter().copied().collect();
                    self.anchor = Some(last);
                    self.persist();
                }
            }
            MenuAction::Delete => {
                // Never delete directly: ask first.
                let targets = self.action_targets(target);
                if !targets.is_empty() {
                    self.commit_rename();
                    self.confirm_delete = Some(targets);
                }
            }
            MenuAction::CloseTab => {
                if let Some(id) = target.or(self.active) {
                    self.close_tab(id);
                }
            }
            MenuAction::CloseOthers => {
                if let Some(id) = target.or(self.active) {
                    self.close_other_tabs(id);
                }
            }
            MenuAction::CloseAll => self.close_all_tabs(),
        }
        Task::none()
    }

    // ───────────────────────── request editing ─────────────────────────

    /// Returns `true` when the edit actually changed the request.
    fn apply_edit(&mut self, id: Id, edit: Edit) -> bool {
        let Some(req) = self.tree.request_mut(id) else {
            return false;
        };

        match edit {
            Edit::Method(m) => req.method = m,
            Edit::Url(url) => req.url = url,
            Edit::KvToggle(list, i, on) => {
                if let Some(row) = table_mut(req, list).get_mut(i) {
                    row.enabled = on;
                }
            }
            Edit::KvKey(list, i, key) => {
                let table = table_mut(req, list);
                if let Some(row) = table.get_mut(i) {
                    row.key = key;
                }
                keep_trailing_blank(table);
            }
            Edit::KvValue(list, i, value) => {
                let table = table_mut(req, list);
                if let Some(row) = table.get_mut(i) {
                    row.value = value;
                }
                keep_trailing_blank(table);
            }
            Edit::KvRemove(list, i) => {
                let table = table_mut(req, list);
                if i < table.len() {
                    table.remove(i);
                }
                keep_trailing_blank(table);
            }
            Edit::BodyKind(kind) => req.body_kind = kind,
            Edit::BodyAction(action) => {
                let is_edit = action.is_edit();
                if let Some(rt) = self.runtime.get_mut(&id) {
                    rt.body_editor.perform(action);
                    if is_edit {
                        req.body_text = rt.body_editor.text();
                    }
                }
                return is_edit;
            }
            Edit::AuthKind(kind) => req.auth_kind = kind,
            Edit::BearerToken(v) => req.bearer_token = v,
            Edit::BasicUser(v) => req.basic_user = v,
            Edit::BasicPassword(v) => req.basic_password = v,
            Edit::ApiKeyName(v) => req.api_key_name = v,
            Edit::ApiKeyValue(v) => req.api_key_value = v,
            Edit::ApiKeyIn(v) => req.api_key_in = v,
        }
        true
    }

    // ───────────────────────── update ─────────────────────────

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle_message(message);
        self.sync_session();
        task
    }

    fn handle_message(&mut self, message: Message) -> Task<Message> {
        match message {
            // ── Sidebar ──
            Message::SelectNode(id) => {
                self.commit_rename();
                self.menu = None;

                if self.modifiers.shift() && self.anchor.is_some() {
                    self.select_range(id);
                } else if self.modifiers.command() {
                    self.toggle_selection(id);
                } else {
                    self.select_only(id);

                    let is_request = self.tree.get(id).is_some_and(|n| !n.is_container());
                    if is_request {
                        self.open_preview(id);
                    } else if let Some(node) = self.tree.get_mut(id) {
                        node.expanded = !node.expanded;
                    }
                }
            }
            Message::OpenPinned(id) => {
                if self.modifiers.shift() || self.modifiers.command() {
                    return Task::none();
                }
                let is_request = self.tree.get(id).is_some_and(|n| !n.is_container());
                if is_request {
                    self.open_pinned(id);
                } else if let Some(node) = self.tree.get_mut(id) {
                    // The two presses of a double click toggle twice; this
                    // third toggle makes the net effect a single toggle.
                    node.expanded = !node.expanded;
                }
            }
            Message::HoverNode(id) => self.hovered = id,
            Message::NodeContext(id) => {
                self.commit_rename();
                // Right-clicking inside a multi-selection keeps it, so the
                // menu acts on all selected items.
                if !self.is_selected(id) {
                    self.select_only(id);
                }
                self.menu = Some(ContextMenu {
                    target: MenuTarget::Node(id),
                    position: self.cursor,
                });
            }
            Message::SidebarContext => {
                self.commit_rename();
                self.menu = Some(ContextMenu {
                    target: MenuTarget::Empty,
                    position: self.cursor,
                });
            }
            Message::NewCollection => {
                self.commit_rename();
                self.selected = Some(self.tree.add_collection());
                self.persist();
            }
            Message::RenameChanged(text) => {
                if let Some(r) = &mut self.renaming {
                    r.text = text;
                }
            }
            Message::RenameSubmit => self.commit_rename(),

            // ── Tabs ──
            Message::ActivateTab(id) => {
                self.commit_rename();
                self.menu = None;
                self.activate(id);
            }
            Message::TabsScrolled(delta) => {
                // The horizontal scrollable ignores the vertical wheel axis, so
                // translate it: wheel down/up scrolls the tabs right/left.
                let (x, y) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (x * 60.0, y * 60.0),
                    mouse::ScrollDelta::Pixels { x, y } => (x, y),
                };
                if x == 0.0 && y != 0.0 {
                    return operation::scroll_by(
                        TABS_ID,
                        scrollable::AbsoluteOffset { x: -y, y: 0.0 },
                    );
                }
            }
            Message::PinTab(id) => self.pin(id),
            Message::CloseTab(id) => self.close_tab(id),
            Message::TabContext(id) => {
                self.menu = Some(ContextMenu {
                    target: MenuTarget::Tab(id),
                    position: self.cursor,
                });
            }

            // ── Menu & shortcuts ──
            Message::MenuAction(action) => {
                let target = self.menu.take().map(|m| m.target);
                return match target {
                    Some(MenuTarget::Node(id)) | Some(MenuTarget::Tab(id)) => {
                        self.perform(action, Some(id))
                    }
                    Some(MenuTarget::Empty) | None => self.perform(action, None),
                };
            }
            Message::ConfirmDelete => {
                if let Some(ids) = self.confirm_delete.take() {
                    for id in ids {
                        self.delete_node(id);
                    }
                    self.persist();
                }
            }
            Message::CancelDelete => self.confirm_delete = None,
            Message::CloseMenu => self.menu = None,
            Message::Shortcut(shortcut) => {
                self.menu = None;
                return self.handle_shortcut(shortcut);
            }

            // ── Window events ──
            Message::ModifiersChanged(m) => self.modifiers = m,
            Message::CursorMoved(p) => self.cursor = p,
            Message::WindowResized(size) => self.window_size = size,

            // ── Request editor ──
            Message::Edit(edit) => {
                if let Some(id) = self.active {
                    if self.apply_edit(id, edit) {
                        self.pin(id); // editing a preview tab keeps it open
                    }
                }
            }
            Message::SectionChanged(section) => {
                if let Some(rt) = self.active.and_then(|id| self.runtime.get_mut(&id)) {
                    rt.section = section;
                }
            }
            Message::Send => return self.send_active(),
            Message::Cancel => {
                if let Some(rt) = self.active.and_then(|id| self.runtime.get_mut(&id)) {
                    if let Some(handle) = rt.abort.take() {
                        handle.abort();
                    }
                    if matches!(rt.state, ResponseState::Sending) {
                        rt.state = ResponseState::Cancelled;
                    }
                }
            }
            Message::Response(id, token, result) => {
                if let Some(rt) = self.runtime.get_mut(&id) {
                    if rt.token == token && matches!(rt.state, ResponseState::Sending) {
                        rt.abort = None;
                        match result {
                            Ok(resp) => {
                                rt.body_view = BodyView::Pretty;
                                rt.response_editor =
                                    text_editor::Content::with_text(&resp.body_pretty);
                                rt.state = ResponseState::Done(Box::new(resp));
                            }
                            Err(e) => rt.state = ResponseState::Failed(e),
                        }
                    }
                }
            }

            // ── Response viewer ──
            Message::ResponseTabChanged(tab) => {
                if let Some(rt) = self.active.and_then(|id| self.runtime.get_mut(&id)) {
                    rt.response_tab = tab;
                }
            }
            Message::BodyViewChanged(view) => {
                if let Some(rt) = self.active.and_then(|id| self.runtime.get_mut(&id)) {
                    rt.body_view = view;
                    if let ResponseState::Done(resp) = &rt.state {
                        let text = match view {
                            BodyView::Pretty => &resp.body_pretty,
                            BodyView::Raw => &resp.body_raw,
                        };
                        rt.response_editor = text_editor::Content::with_text(text);
                    }
                }
            }
            Message::ResponseAction(action) => {
                // Read-only: allow selecting/copying/scrolling, ignore edits.
                if !action.is_edit() {
                    if let Some(rt) = self.active.and_then(|id| self.runtime.get_mut(&id)) {
                        rt.response_editor.perform(action);
                    }
                }
            }
            Message::CopyBody => {
                if let Some(rt) = self.active.and_then(|id| self.runtime.get(&id)) {
                    if let ResponseState::Done(resp) = &rt.state {
                        let text = match rt.body_view {
                            BodyView::Pretty => resp.body_pretty.clone(),
                            BodyView::Raw => resp.body_raw.clone(),
                        };
                        return iced::clipboard::write(text);
                    }
                }
            }
        }
        Task::none()
    }

    fn handle_shortcut(&mut self, shortcut: Shortcut) -> Task<Message> {
        // The delete dialog is modal: only Esc / Enter do anything.
        if self.confirm_delete.is_some() {
            match shortcut {
                Shortcut::Escape => self.confirm_delete = None,
                Shortcut::Confirm => return self.update(Message::ConfirmDelete),
                _ => {}
            }
            return Task::none();
        }

        // While a name is being edited, only Escape is meaningful.
        if self.renaming.is_some() {
            if matches!(shortcut, Shortcut::Escape) {
                self.renaming = None;
            }
            return Task::none();
        }

        match shortcut {
            Shortcut::NewRequest => self.perform(MenuAction::NewRequest, None),
            Shortcut::NewFolder => self.perform(MenuAction::NewFolder, None),
            Shortcut::Delete => self.perform(MenuAction::Delete, None),
            Shortcut::Duplicate => self.perform(MenuAction::Duplicate, None),
            Shortcut::Rename => self.perform(MenuAction::Rename, None),
            Shortcut::CloseTab => self.perform(MenuAction::CloseTab, None),
            Shortcut::GoToTab(index) => {
                if let Some(id) = self.tabs.get(index).map(|t| t.request_id) {
                    self.activate(id);
                }
                Task::none()
            }
            Shortcut::Escape => {
                self.extra.clear();
                Task::none()
            }
            Shortcut::Confirm => Task::none(),
            Shortcut::Save => {
                self.save_active();
                Task::none()
            }
            Shortcut::FocusNext => operation::focus_next(),
            Shortcut::FocusPrevious => operation::focus_previous(),
        }
    }

    fn send_active(&mut self) -> Task<Message> {
        let Some(id) = self.active else {
            return Task::none();
        };
        let Some(request) = self.tree.request(id).cloned() else {
            return Task::none();
        };
        self.ensure_runtime(id);
        let Some(rt) = self.runtime.get_mut(&id) else {
            return Task::none();
        };
        if matches!(rt.state, ResponseState::Sending) {
            return Task::none();
        }

        rt.token += 1;
        let token = rt.token;
        rt.state = ResponseState::Sending;

        let (task, handle) = Task::perform(http::send(request), move |r| {
            Message::Response(id, token, r)
        })
        .abortable();
        rt.abort = Some(handle);
        self.pin(id);
        task
    }
}

// ───────────────────────── helpers ─────────────────────────

fn table_mut(req: &mut Request, list: KvList) -> &mut Vec<KeyValue> {
    match list {
        KvList::Params => &mut req.params,
        KvList::Headers => &mut req.headers,
        KvList::Cookies => &mut req.cookies,
        KvList::FormData => &mut req.form_data,
        KvList::UrlEncoded => &mut req.url_encoded,
    }
}

fn keep_trailing_blank(table: &mut Vec<KeyValue>) {
    while table.len() > 1 && table[table.len() - 1].is_blank() && table[table.len() - 2].is_blank()
    {
        table.pop();
    }
    if !table.last().is_some_and(KeyValue::is_blank) {
        table.push(KeyValue::empty());
    }
}

/// Translates raw window events into messages.
fn handle_event(event: Event, status: event::Status, _window: window::Id) -> Option<Message> {
    match event {
        Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => Some(Message::ModifiersChanged(m)),
        Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Message::CursorMoved(position))
        }
        Event::Window(window::Event::Resized(size)) => Some(Message::WindowResized(size)),
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            modifiers,
            ..
        }) => {
            let shortcut = shortcut_for(&key, physical_key, modifiers)?;
            // Widgets that are typing (text inputs, editors) capture their keys,
            // except Escape, which must still be able to cancel a rename.
            if status == event::Status::Captured && !matches!(shortcut, Shortcut::Escape) {
                return None;
            }
            Some(Message::Shortcut(shortcut))
        }
        _ => None,
    }
}

fn shortcut_for(
    key: &keyboard::Key,
    physical: keyboard::key::Physical,
    modifiers: keyboard::Modifiers,
) -> Option<Shortcut> {
    use keyboard::key::Named;

    match key.as_ref() {
        keyboard::Key::Named(Named::Delete) => return Some(Shortcut::Delete),
        keyboard::Key::Named(Named::F2) => return Some(Shortcut::Rename),
        keyboard::Key::Named(Named::Escape) => return Some(Shortcut::Escape),
        keyboard::Key::Named(Named::Enter) => return Some(Shortcut::Confirm),
        keyboard::Key::Named(Named::Tab) => {
            return Some(if modifiers.shift() {
                Shortcut::FocusPrevious
            } else {
                Shortcut::FocusNext
            })
        }
        _ => {}
    }

    if !modifiers.command() || modifiers.alt() {
        return None;
    }

    let latin = key.to_latin(physical)?.to_ascii_lowercase();
    match (latin, modifiers.shift()) {
        ('n', false) => Some(Shortcut::NewRequest),
        ('n', true) => Some(Shortcut::NewFolder),
        ('d', false) => Some(Shortcut::Duplicate),
        ('w', false) => Some(Shortcut::CloseTab),
        ('s', false) => Some(Shortcut::Save),
        (c @ '1'..='9', false) => Some(Shortcut::GoToTab(c as usize - '1' as usize)),
        _ => None,
    }
}

fn starter_tree() -> Tree {
    let mut tree = Tree::new();
    let first = tree.add_collection();
    if let Some(node) = tree.get_mut(first) {
        node.name = "My Collection".into();
    }
    tree
}

/// Ids of all nodes in tree order; with `visible_only`, children of
/// collapsed containers are skipped.
fn collect_ids(nodes: &[Node], visible_only: bool, out: &mut Vec<Id>) {
    for node in nodes {
        out.push(node.id);
        if !visible_only || node.expanded {
            collect_ids(&node.children, visible_only, out);
        }
    }
}

/// Collapses every collection and folder.
fn collapse_all(nodes: &mut [Node]) {
    for node in nodes {
        node.expanded = false;
        collapse_all(&mut node.children);
    }
}
