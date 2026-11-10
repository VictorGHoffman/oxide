//! Application state, messages and the `update` logic.

use std::collections::HashMap;

use iced::widget::{operation, text_editor};
use iced::{event, keyboard, mouse, window, Event, Point, Size, Subscription, Task, Theme};

use crate::http;
use crate::model::*;

pub const RENAME_ID: &str = "oxide-rename-input";

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

    // Context menu / shortcuts
    MenuAction(MenuAction),
    CloseMenu,
    Shortcut(Shortcut),

    // Window-level events
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
}

impl Oxide {
    pub fn new() -> (Self, Task<Message>) {
        let mut tree = Tree::new();
        let first = tree.add_collection();
        if let Some(node) = tree.get_mut(first) {
            node.name = "My Collection".into();
        }

        (
            Self {
                tree,
                selected: Some(first),
                hovered: None,
                renaming: None,
                tabs: Vec::new(),
                active: None,
                runtime: HashMap::new(),
                menu: None,
                cursor: Point::ORIGIN,
                window_size: Size::new(1280.0, 800.0),
            },
            Task::none(),
        )
    }

    pub fn title(&self) -> String {
        match self.active.and_then(|id| self.tree.get(id)) {
            Some(node) => format!("{} — Oxide", node.name),
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
        self.selected = Some(id);
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
                if let Some(node) = self.tree.get_mut(r.id) {
                    node.name = name.to_string();
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
        if self.selected.is_some_and(|s| self.tree.get(s).is_none()) {
            self.selected = None;
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
                    }
                }
            }
            MenuAction::NewFolder => {
                if let Some(container) = self.container_for_target(target) {
                    if let Some(id) = self.tree.add_folder(container) {
                        self.selected = Some(id);
                    }
                }
            }
            MenuAction::NewCollection => {
                self.selected = Some(self.tree.add_collection());
            }
            MenuAction::Rename => {
                if let Some(id) = node_target {
                    return self.start_rename(id);
                }
            }
            MenuAction::Duplicate => {
                if let Some(id) = node_target {
                    if let Some(copy) = self.tree.duplicate(id) {
                        self.selected = Some(copy);
                    }
                }
            }
            MenuAction::Delete => {
                if let Some(id) = node_target {
                    self.delete_node(id);
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
        match message {
            // ── Sidebar ──
            Message::SelectNode(id) => {
                self.commit_rename();
                self.menu = None;
                self.selected = Some(id);

                let is_request = self.tree.get(id).is_some_and(|n| !n.is_container());
                if is_request {
                    self.open_preview(id);
                } else if let Some(node) = self.tree.get_mut(id) {
                    node.expanded = !node.expanded;
                }
            }
            Message::OpenPinned(id) => {
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
                self.selected = Some(id);
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
            Message::CloseMenu => self.menu = None,
            Message::Shortcut(shortcut) => {
                self.menu = None;
                return self.handle_shortcut(shortcut);
            }

            // ── Window events ──
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
            Shortcut::Escape => Task::none(),
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
    if !table.last().is_some_and(KeyValue::is_blank) {
        table.push(KeyValue::empty());
    }
}

/// Translates raw window events into messages.
fn handle_event(event: Event, status: event::Status, _window: window::Id) -> Option<Message> {
    match event {
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
        (c @ '1'..='9', false) => Some(Shortcut::GoToTab(c as usize - '1' as usize)),
        _ => None,
    }
}
