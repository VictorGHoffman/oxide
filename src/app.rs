//! Application state, messages and the `update` logic.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use iced::widget::{operation, scrollable, text_editor};
use iced::{event, keyboard, mouse, window, Event, Point, Size, Subscription, Task, Theme};

use crate::apidog;
use crate::backup;
use crate::certs::{self, CertKind, ClientCert};
use crate::curl;
use crate::htmlview;
use crate::http;
use crate::jsontree;
use crate::media;
use crate::model::*;
use crate::oauth;
use crate::postman;
use crate::settings::{self, Settings};
use crate::storage;
use crate::theme;
use crate::vars;

pub const RENAME_ID: &str = "oxide-rename-input";
pub const TABS_ID: &str = "oxide-tabs";
/// Width of a tab plus the gap before the next one (used to turn a drag distance into a position).
/// How many undo steps are kept per request.
const HISTORY_LIMIT: usize = 100;
/// Edits closer together than this count as one undo step (typing a word, ...).
const HISTORY_MERGE: Duration = Duration::from_millis(700);

pub const TAB_STRIDE: f32 = 196.0;
pub const SEARCH_ID: &str = "oxide-search";
pub const PALETTE_ID: &str = "oxide-palette";
/// Rows shown at once in the command palette.
pub const PALETTE_ROWS: usize = 10;

// ───────────────────────────── UI enums ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Params,
    Body,
    Headers,
    Cookies,
    Auth,
    Script,
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
    /// JSON as a collapsible tree.
    Tree,
    /// HTML drawn as a page.
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvList {
    Params,
    Headers,
    Cookies,
    FormData,
    UrlEncoded,
    /// The variables of the open Variables dialog (not part of a request).
    Vars,
}

/// Which set of variables the dialog is editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarScope {
    Global,
    Collection(Id),
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
    ScriptAction(text_editor::Action),
    /// Tab pressed in the body editor / script editor: indent.
    BodyTab,
    ScriptTab,
    /// Shift+Tab: take one indentation step back.
    BodyUntab,
    ScriptUntab,
    AuthKind(AuthKind),
    BearerToken(String),
    BasicUser(String),
    BasicPassword(String),
    ApiKeyName(String),
    ApiKeyValue(String),
    ApiKeyIn(ApiKeyLocation),
    BodyFile(String),
    OAuthGrant(OAuthGrant),
    OAuthTokenUrl(String),
    OAuthClientId(String),
    OAuthClientSecret(String),
    OAuthScope(String),
    OAuthUsername(String),
    OAuthPassword(String),
    OAuthClientAuth(OAuthClientAuth),
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
    ImportCollection,
    ImportApidogFolder,
    ExportCollection,
    EditVariables,
    ShowInFolder,
    CopyCurl,
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
    ToggleSidebar,
    FocusSearch,
    Undo,
    Redo,
    Send,
    ImportCollection,
    ImportApidogFolder,
    Palette,
    Settings,
    Up,
    Down,
}

/// A draggable border between two panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Divider {
    /// Between the sidebar and the request area.
    Sidebar,
    /// Between the request editor and the response.
    Response,
}

/// A divider being dragged: where the cursor and the panel size were when it started.
pub struct Drag {
    pub divider: Divider,
    pub start: Point,
    pub size: f32,
}

/// The files a certificate entry points to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertFile {
    Pfx,
    Cert,
    Key,
    Ca,
}

#[derive(Debug, Clone, Copy)]
pub enum PdfNav {
    First,
    Previous,
    Next,
    Last,
}

#[derive(Debug, Clone)]
pub enum CertField {
    Enabled(bool),
    Host(String),
    Kind(CertKind),
    Passphrase(String),
    Path(CertFile, String),
}

#[derive(Debug, Clone)]
pub enum Message {
    TabPressed(Id),

    // Sidebar search
    SearchChanged(String),

    // Variables
    OpenVariables(VarScope),
    CloseVariables,

    // Command palette
    OpenPalette,
    ClosePalette,
    PaletteQuery(String),
    PaletteRun,
    PaletteRunAt(usize),

    // Settings
    OpenSettings(SettingsTab),
    CloseSettings,
    SettingTimeout(String),
    SettingVerifyTls(bool),
    SettingRedirects(bool),

    // Backup
    ExportBackup,
    RestoreBackup,
    /// The text of the backup file that was picked (`None` when cancelled).
    BackupPicked(Result<Option<String>, String>),

    // Certificates
    CertAdd,
    CertRemove(usize),
    CertEdit(usize, CertField),
    CertBrowse(usize, CertFile),
    CertPicked(usize, CertFile, Option<String>),

    // Layout
    ToggleTheme,
    SetTheme(theme::Choice),
    ToggleSidebar,
    ToggleResponseLayout,
    DragStart(Divider),
    DragEnd,

    // Import / export
    /// Everything picked to import; empty when cancelled.
    ImportPicked(Result<Vec<ImportSource>, String>),
    /// `Some(file name)` once written, `None` when the dialog was cancelled.
    ExportDone(Result<Option<String>, String>),
    DismissNotice,
    /// The clock that hides the status bar message once it is old enough.
    NoticeTick,

    // Sidebar
    SelectNode(Id),
    OpenPinned(Id),
    HoverNode(Option<Id>),
    /// The mouse moved over a row while dragging: `(row, y inside the row)`.
    NodeDragOver(Id, f32),
    NodeContext(Id),
    SidebarContext,
    NewCollection,
    RenameChanged(String),
    RenameSubmit,

    // Tabs
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
    /// The result, plus the OAuth 2.0 token that was used (when there is one).
    Response(Id, u64, Result<http::Response, String>, Option<oauth::Token>),
    PickBodyFile,
    BodyFilePicked(Id, Option<String>),
    OAuthFetch,
    OAuthFetched(Id, Result<oauth::Token, String>),
    OAuthClear,

    // Response viewer
    ResponseTabChanged(ResponseTab),
    BodyViewChanged(BodyView),
    ToggleJsonNode(String),
    JsonExpandAll,
    JsonCollapseAll,
    OpenLink(String),
    OpenInBrowser,
    /// Zoom an image / PDF preview: `true` = in, `false` = out.
    MediaZoom(bool),
    /// Image preview: shrink to fit the panel.
    MediaFit,
    /// Image / PDF preview: real size.
    MediaActual,
    PdfPage(PdfNav),
    /// Open the image or PDF with the system's default program.
    OpenMedia,
    /// Save the body of the response to a file.
    SaveResponse,
    ResponseSaved(Result<Option<String>, String>),
    /// Copy the active request as a curl command.
    CopyCurl,
    ResponseAction(text_editor::Action),
    CopyBody,
}

// ───────────────────────────── State ─────────────────────────────

pub struct Tab {
    pub request_id: Id,
    /// `false` = preview tab (italic), replaced by the next single click.
    pub pinned: bool,
}

/// A tab being dragged to a new position.
pub struct TabDrag {
    pub id: Id,
    pub start_x: f32,
    pub start_index: usize,
    pub moved: bool,
}

/// A sidebar move that Ctrl+Z / Ctrl+Y can undo / redo.
pub struct MoveStep {
    pub seq: u64,
    pub before: Vec<NodePosition>,
    pub after: Vec<NodePosition>,
    /// The selection after the move: `(primary, others)`.
    pub selection: (Id, Vec<Id>),
}

/// A sidebar node being dragged to a new place.
pub struct NodeDrag {
    /// The row that was pressed.
    pub id: Id,
    /// Every node being moved (the outermost selected nodes, in tree order).
    pub ids: Vec<Id>,
    /// The press landed on an item of a multi-selection: if it turns out to be a plain
    /// click (no drag), the selection collapses to that item on release.
    pub collapse_on_click: bool,
    pub start: Point,
    /// Becomes `true` once the mouse has moved far enough to count as a drag.
    pub active: bool,
    /// Where it would land if released now.
    pub target: Option<(Id, DropZone)>,
    /// Whether the node was an open folder before the press (pressing toggles it).
    pub was_expanded: bool,
}

/// Height of a sidebar row; the vertical position inside it picks the drop zone.
pub const ROW_HEIGHT: f32 = 30.0;

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

/// The pages of the settings window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Appearance,
    Requests,
    Certificates,
    Data,
    Shortcuts,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 5] = [
        SettingsTab::Appearance,
        SettingsTab::Requests,
        SettingsTab::Certificates,
        SettingsTab::Data,
        SettingsTab::Shortcuts,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettingsTab::Appearance => "Appearance",
            SettingsTab::Requests => "Requests",
            SettingsTab::Certificates => "Certificates",
            SettingsTab::Data => "Data & backup",
            SettingsTab::Shortcuts => "Shortcuts",
        }
    }
}

/// The command palette (Ctrl+K).
pub struct Palette {
    pub query: String,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub enum PaletteAction {
    Run(Message),
    Open(Id),
}

#[derive(Debug, Clone)]
pub struct PaletteItem {
    pub label: String,
    /// Where a request lives, or what a command does.
    pub detail: String,
    pub keys: Option<&'static str>,
    pub action: PaletteAction,
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
    pub script_editor: text_editor::Content,
    /// What the post-response script did the last time it ran: `(is_error, text)`.
    pub script_output: Vec<(bool, String)>,
    /// Earlier versions of the request (most recent last) for Ctrl+Z.
    pub undo: Vec<Request>,
    /// Versions undone, for Ctrl+Y.
    pub redo: Vec<Request>,
    /// When each `undo` / `redo` entry was recorded (see [`Oxide::history_seq`]), so
    /// Ctrl+Z can tell whether a request edit or a sidebar move is the most recent.
    pub undo_seq: Vec<u64>,
    pub redo_seq: Vec<u64>,
    pub last_edit: Option<Instant>,
    /// Result of the last "Get new token": `(is_error, text)`.
    pub oauth_status: Option<(bool, String)>,
    pub oauth_busy: bool,
    /// The response parsed as JSON (for the tree view) and which nodes are open.
    pub json: Option<serde_json::Value>,
    pub json_open: HashSet<String>,
    /// The response parsed as an HTML page (for the preview), when it is one.
    pub html: Option<Vec<htmlview::Block>>,
    /// The response as an image or PDF (for the preview), when it is one.
    pub preview: Option<Result<media::Preview, String>>,
}

impl Runtime {
    fn new(body_text: &str, script: &str) -> Self {
        Self {
            section: Section::Params,
            body_editor: text_editor::Content::with_text(body_text),
            state: ResponseState::Idle,
            token: 0,
            abort: None,
            response_tab: ResponseTab::Body,
            body_view: BodyView::Pretty,
            response_editor: text_editor::Content::new(),
            script_editor: text_editor::Content::with_text(script),
            script_output: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            undo_seq: Vec::new(),
            redo_seq: Vec::new(),
            last_edit: None,
            oauth_status: None,
            oauth_busy: false,
            json: None,
            json_open: HashSet::new(),
            html: None,
            preview: None,
        }
    }
}

/// A short message about an import/export shown at the bottom of the sidebar.
pub struct Notice {
    pub text: String,
    pub error: bool,
}

/// How long a status bar message stays before it goes away.
const NOTICE_DURATION: Duration = Duration::from_secs(5);

pub const MIN_SIDEBAR: f32 = 180.0;
pub const MAX_SIDEBAR: f32 = 600.0;
pub const MIN_RESPONSE_WIDTH: f32 = 260.0;
pub const MIN_RESPONSE_HEIGHT: f32 = 120.0;

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
    pub notice: Option<Notice>,
    /// When `notice` was shown.
    pub notice_at: Instant,
    /// Index into [`theme::ENTRIES`].
    /// Text typed in the sidebar search box (empty = show everything).
    pub search: String,
    pub tab_drag: Option<TabDrag>,
    pub node_drag: Option<NodeDrag>,
    pub theme_index: usize,
    /// Global variables (shared by all collections).
    pub globals: Vec<KeyValue>,
    /// The Variables dialog, when open.
    pub vars_dialog: Option<VarScope>,
    /// Client certificates (saved in certificates.json) and whether their dialog is open.
    pub certs: Vec<ClientCert>,
    /// Whether each certificate's files could be read (same order as `certs`).
    pub cert_status: Vec<Result<(), String>>,
    /// The settings window, on the page that is showing.
    pub settings_tab: Option<SettingsTab>,
    pub settings: Settings,
    /// What is typed in the timeout box.
    pub timeout_text: String,
    pub palette: Option<Palette>,
    pub sidebar_visible: bool,
    pub sidebar_width: f32,
    /// `true` = response docked on the right, `false` = below the editor.
    pub response_right: bool,
    pub response_height: f32,
    pub response_width: f32,
    pub drag: Option<Drag>,
    /// OAuth 2.0 tokens by request id (saved in tokens.json).
    pub tokens: HashMap<Id, oauth::Token>,
    /// Selected nodes besides the primary one (`selected`).
    pub extra: HashSet<Id>,
    /// Where a Shift+click range starts.
    pub anchor: Option<Id>,
    pub modifiers: keyboard::Modifiers,
    /// Sidebar moves that can be undone / redone (most recent last).
    pub move_undo: Vec<MoveStep>,
    pub move_redo: Vec<MoveStep>,
    /// Counter that orders every undo step (request edits and sidebar moves).
    pub history_seq: u64,
    last_session: storage::Session,
}

impl Oxide {
    pub fn new() -> (Self, Task<Message>) {
        let (tree, save_error) = match storage::load() {
            Ok(Some(tree)) => (tree, None),
            Ok(None) => (starter_tree(), None),
            Err(e) => (starter_tree(), Some(e)),
        };
        let mut tree = tree;
        // Older files kept the query params apart from the URL: link them.
        tree.sync_queries();
        let selected = tree.first_collection();

        let settings = storage::load_settings();
        settings::set_active(&settings);
        let certs = storage::load_certs();
        certs::set_active(&certs);
        let cert_status = certs.iter().map(ClientCert::check).collect();

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
            notice: None,
            notice_at: Instant::now(),
            search: String::new(),
            tab_drag: None,
            node_drag: None,
            theme_index: 0,
            globals: storage::load_globals(),
            vars_dialog: None,
            certs,
            cert_status,
            settings_tab: None,
            timeout_text: settings.timeout_secs.to_string(),
            settings,
            palette: None,
            sidebar_visible: true,
            sidebar_width: 280.0,
            response_right: false,
            response_height: 320.0,
            response_width: 480.0,
            drag: None,
            tokens: storage::load_tokens(),
            extra: HashSet::new(),
            anchor: selected,
            modifiers: keyboard::Modifiers::default(),
            move_undo: Vec::new(),
            move_redo: Vec::new(),
            history_seq: 0,
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
            theme: theme::ENTRIES
                .get(self.theme_index)
                .map(|e| e.name.to_string()),
            // While a divider is being dragged the last settled layout is kept, so the
            // file is written once when the mouse is released, not on every movement.
            layout: if self.drag.is_some() {
                self.last_session.layout.clone()
            } else {
                Some(storage::Layout {
                    sidebar_visible: self.sidebar_visible,
                    sidebar_width: self.sidebar_width.round() as u32,
                    response_right: self.response_right,
                    response_width: self.response_width.round() as u32,
                    response_height: self.response_height.round() as u32,
                })
            },
        }
    }

    /// Reopens the tabs that were open last time (skipping deleted requests).
    fn restore_session(&mut self) {
        let Some(session) = storage::load_session() else {
            return;
        };
        if let Some(index) = session.theme.as_deref().and_then(theme::index_of) {
            self.theme_index = index;
        }
        if let Some(layout) = &session.layout {
            self.sidebar_visible = layout.sidebar_visible;
            self.sidebar_width = (layout.sidebar_width as f32).clamp(MIN_SIDEBAR, MAX_SIDEBAR);
            self.response_right = layout.response_right;
            self.response_width = (layout.response_width as f32).max(MIN_RESPONSE_WIDTH);
            self.response_height = (layout.response_height as f32).max(MIN_RESPONSE_HEIGHT);
        }
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
        theme::ENTRIES
            .get(self.theme_index)
            .map_or_else(theme::oxide, |e| (e.make)())
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let events = event::listen_with(handle_event);
        // A clock only while a message is showing, to hide it after a while.
        if self.notice.is_some() {
            Subscription::batch([
                events,
                iced::time::every(Duration::from_millis(500)).map(|_| Message::NoticeTick),
            ])
        } else {
            events
        }
    }

    /// Shows `notice` in the status bar; it goes away after [`NOTICE_DURATION`].
    fn show_notice(&mut self, notice: Notice) {
        self.notice = Some(notice);
        self.notice_at = Instant::now();
    }

    // ───────────────────────── tabs ─────────────────────────

    fn ensure_runtime(&mut self, id: Id) {
        if self.runtime.contains_key(&id) {
            return;
        }
        if let Some(req) = self.tree.request(id) {
            let rt = Runtime::new(&req.body_text, &req.script);
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
        self.revert_request(id);
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
        let closing: Vec<Id> = self
            .tabs
            .iter()
            .map(|t| t.request_id)
            .filter(|id| *id != keep)
            .collect();
        for id in closing {
            self.revert_request(id);
        }
        self.tabs.retain(|t| t.request_id == keep);
        if self.tabs.is_empty() {
            self.active = None;
        } else {
            self.activate(keep);
        }
    }

    fn close_all_tabs(&mut self) {
        let closing: Vec<Id> = self.tabs.iter().map(|t| t.request_id).collect();
        for id in closing {
            self.revert_request(id);
        }
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

                    // A duplicated request opens (and becomes the active tab).
                    let (selected, extra, anchor) =
                        (self.selected, self.extra.clone(), self.anchor);
                    if let Some(&request) = copies
                        .iter()
                        .rev()
                        .find(|id| self.tree.request(**id).is_some())
                    {
                        self.open_pinned(request);
                        self.selected = selected;
                        self.extra = extra;
                        self.anchor = anchor;
                    }
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
            MenuAction::ImportCollection => {
                self.commit_rename();
                self.notice = None;
                return Task::perform(pick_collections(), Message::ImportPicked);
            }
            MenuAction::ImportApidogFolder => {
                self.commit_rename();
                self.notice = None;
                return Task::perform(pick_apidog_folder(), Message::ImportPicked);
            }
            MenuAction::EditVariables => {
                if let Some(id) = node_target {
                    let root = self.tree.ancestors(id).last().copied().unwrap_or(id);
                    return self.update(Message::OpenVariables(VarScope::Collection(root)));
                }
            }
            MenuAction::ShowInFolder => {
                let result = match storage::file_path() {
                    Some(path) if path.exists() => reveal_file(&path),
                    Some(path) => match path.parent() {
                        Some(dir) if dir.exists() => open_target(&dir.display().to_string()),
                        _ => Err("Nothing has been saved yet.".to_string()),
                    },
                    None => Err("Could not find the Documents folder.".to_string()),
                };
                if let Err(e) = result {
                    self.show_notice(Notice {
                        text: format!("Could not open the folder: {e}"),
                        error: true,
                    });
                }
            }
            MenuAction::CopyCurl => {
                if let Some(id) = node_target {
                    return self.copy_curl(id);
                }
            }
            MenuAction::ExportCollection => {
                self.commit_rename();
                self.notice = None;
                // Exporting from anywhere inside a collection exports the whole collection.
                let root =
                    node_target.map(|id| self.tree.ancestors(id).last().copied().unwrap_or(id));
                let Some(collection) = root.and_then(|id| self.tree.get(id)) else {
                    return Task::none();
                };
                let file_name = format!(
                    "{}.postman_collection.json",
                    safe_file_name(&collection.name)
                );
                let json = postman::export(collection);
                return Task::perform(save_collection(file_name, json), Message::ExportDone);
            }
        }
        Task::none()
    }

    // ───────────────────────── request editing ─────────────────────────

    /// Returns `true` when the edit actually changed the request.
    fn apply_edit(&mut self, id: Id, edit: Edit) -> bool {
        let Some(req) = self.tree.request_mut(id) else {
            return false;
        };

        // Params and the URL's query string mirror each other.
        let params_changed = matches!(
            edit,
            Edit::KvToggle(KvList::Params, ..)
                | Edit::KvKey(KvList::Params, ..)
                | Edit::KvValue(KvList::Params, ..)
                | Edit::KvRemove(KvList::Params, ..)
        );

        match edit {
            Edit::Method(m) => req.method = m,
            Edit::Url(url) => {
                req.url = url;
                req.params_from_url();
            }
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
            Edit::BodyTab => {
                let Some(rt) = self.runtime.get_mut(&id) else {
                    return false;
                };
                let changed = indent(&mut rt.body_editor);
                if changed {
                    req.body_text = rt.body_editor.text();
                }
                return changed;
            }
            Edit::BodyUntab => {
                let Some(rt) = self.runtime.get_mut(&id) else {
                    return false;
                };
                let changed = unindent(&mut rt.body_editor);
                if changed {
                    req.body_text = rt.body_editor.text();
                }
                return changed;
            }
            Edit::ScriptUntab => {
                let Some(rt) = self.runtime.get_mut(&id) else {
                    return false;
                };
                let changed = unindent(&mut rt.script_editor);
                if changed {
                    req.script = rt.script_editor.text();
                }
                return changed;
            }
            Edit::ScriptTab => {
                let Some(rt) = self.runtime.get_mut(&id) else {
                    return false;
                };
                let changed = indent(&mut rt.script_editor);
                if changed {
                    req.script = rt.script_editor.text();
                }
                return changed;
            }
            Edit::ScriptAction(action) => {
                let is_edit = action.is_edit();
                if let Some(rt) = self.runtime.get_mut(&id) {
                    rt.script_editor.perform(action);
                    if is_edit {
                        req.script = rt.script_editor.text();
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
            Edit::BodyFile(v) => req.body_file = v,
            Edit::OAuthGrant(v) => req.oauth_grant = v,
            Edit::OAuthTokenUrl(v) => req.oauth_token_url = v,
            Edit::OAuthClientId(v) => req.oauth_client_id = v,
            Edit::OAuthClientSecret(v) => req.oauth_client_secret = v,
            Edit::OAuthScope(v) => req.oauth_scope = v,
            Edit::OAuthUsername(v) => req.oauth_username = v,
            Edit::OAuthPassword(v) => req.oauth_password = v,
            Edit::OAuthClientAuth(v) => req.oauth_client_auth = v,
        }
        if params_changed {
            req.url_from_params();
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
                    // Pressing an item that is part of a multi-selection keeps the
                    // selection so all of it can be dragged together.
                    let multi = self.is_selected(id) && !self.extra.is_empty();
                    let ids = if multi {
                        self.action_targets(Some(id))
                    } else {
                        self.select_only(id);
                        vec![id]
                    };
                    self.node_drag = Some(NodeDrag {
                        id,
                        ids,
                        collapse_on_click: multi,
                        start: self.cursor,
                        active: false,
                        target: None,
                        was_expanded: self.tree.get(id).is_some_and(|n| n.expanded),
                    });

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
            Message::HoverNode(id) => {
                self.hovered = id;
                if id.is_none() {
                    if let Some(drag) = &mut self.node_drag {
                        drag.target = None;
                    }
                }
            }
            Message::NodeDragOver(row, y) => self.drag_over(row, y),
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
            Message::ImportPicked(result) => match result {
                Err(e) => {
                    self.show_notice(Notice {
                        text: format!("Import failed: {e}"),
                        error: true,
                    });
                }
                Ok(files) if files.is_empty() => {}
                Ok(files) => {
                    let mut imported = Vec::new();
                    let mut failures = Vec::new();
                    for source in &files {
                        let (name, result) = match source {
                            ImportSource::Postman(name, content) => {
                                (name, postman::import(&mut self.tree, content))
                            }
                            ImportSource::Apidog(dir) => {
                                (&dir.name, apidog::import(&mut self.tree, dir))
                            }
                        };
                        match result {
                            Ok(summary) => imported.push(summary),
                            Err(e) => failures.push(format!("{name}: {e}")),
                        }
                    }
                    if let Some(last) = imported.last() {
                        let id = last.collection;
                        self.select_only(id);
                        self.persist();
                    }
                    self.show_notice(import_notice(&imported, &failures));
                }
            },
            Message::ExportDone(result) => match result {
                Ok(Some(name)) => {
                    self.show_notice(Notice {
                        text: format!("Exported to {name}."),
                        error: false,
                    });
                }
                Ok(None) => {}
                Err(e) => {
                    self.show_notice(Notice {
                        text: format!("Export failed: {e}"),
                        error: true,
                    });
                }
            },
            Message::DismissNotice => self.notice = None,
            Message::NoticeTick => {
                if self.notice_at.elapsed() >= NOTICE_DURATION {
                    self.notice = None;
                }
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
            Message::CursorMoved(p) => {
                self.cursor = p;
                self.apply_drag();
                self.apply_tab_drag();
                self.apply_node_drag();
            }
            Message::OpenVariables(scope) => {
                self.menu = None;
                self.commit_rename();
                if let Some(table) = self.variables_mut(scope) {
                    keep_trailing_blank(table);
                }
                self.vars_dialog = Some(scope);
            }
            Message::SearchChanged(text) => self.search = text,
            Message::CloseVariables => self.vars_dialog = None,
            Message::OpenPalette => {
                self.menu = None;
                self.commit_rename();
                self.palette = Some(Palette {
                    query: String::new(),
                    selected: 0,
                });
                return operation::focus(PALETTE_ID);
            }
            Message::ClosePalette => self.palette = None,
            Message::PaletteQuery(query) => {
                if let Some(palette) = &mut self.palette {
                    palette.query = query;
                    palette.selected = 0;
                }
            }
            Message::PaletteRun => {
                let index = self.palette.as_ref().map(|p| p.selected);
                if let Some(index) = index {
                    return self.run_palette(index);
                }
            }
            Message::PaletteRunAt(index) => return self.run_palette(index),
            Message::OpenSettings(tab) => {
                self.menu = None;
                self.commit_rename();
                self.settings_tab = Some(tab);
            }
            Message::CloseSettings => self.settings_tab = None,
            Message::SettingTimeout(text) => {
                let digits: String = text.chars().filter(char::is_ascii_digit).take(5).collect();
                let secs = digits.parse::<u64>().unwrap_or(0).min(settings::MAX_TIMEOUT);
                self.timeout_text = if digits.is_empty() {
                    String::new()
                } else {
                    secs.to_string()
                };
                self.settings.timeout_secs = secs;
                self.settings_changed();
            }
            Message::SettingVerifyTls(on) => {
                self.settings.verify_tls = on;
                self.settings_changed();
            }
            Message::SettingRedirects(on) => {
                self.settings.follow_redirects = on;
                self.settings_changed();
            }
            Message::ExportBackup => {
                self.commit_rename();
                let snapshot = self.tree.snapshot(&self.saved);
                match backup::export(&snapshot, &self.globals) {
                    Ok(json) => {
                        let name = format!("oxide-backup-{}.json", backup::date(oauth::now()));
                        return Task::perform(save_backup(name, json), Message::ExportDone);
                    }
                    Err(e) => {
                        self.show_notice(Notice {
                            text: format!("Export failed: {e}"),
                            error: true,
                        });
                    }
                }
            }
            Message::RestoreBackup => return Task::perform(pick_backup(), Message::BackupPicked),
            Message::BackupPicked(result) => match result {
                Ok(None) => {}
                Err(e) => {
                    self.show_notice(Notice {
                        text: format!("Restore failed: {e}"),
                        error: true,
                    });
                }
                Ok(Some(text)) => match backup::parse(&text) {
                    Err(e) => {
                        self.show_notice(Notice {
                            text: format!("Restore failed: {e}"),
                            error: true,
                        });
                    }
                    Ok(restored) => {
                        let requests = restored.tree.requests().len();
                        let collections = self.tree.merge(restored.tree);
                        for global in restored.globals.into_iter().filter(|g| !g.key.trim().is_empty()) {
                            set_variable(&mut self.globals, &global.key, global.value);
                        }
                        keep_trailing_blank(&mut self.globals);
                        self.save_variables(VarScope::Global);
                        self.persist();
                        self.show_notice(Notice {
                            text: format!(
                                "Restored {collections} collection{} ({requests} requests) and the global variables.",
                                if collections == 1 { "" } else { "s" }
                            ),
                            error: false,
                        });
                    }
                },
            },
            Message::CertAdd => {
                self.certs.push(ClientCert::default());
                self.cert_status.push(Err("Enter the host this certificate is for.".into()));
                return self.certs_changed();
            }
            Message::CertRemove(index) => {
                if index < self.certs.len() {
                    self.certs.remove(index);
                    let _ = self.cert_status.remove(index);
                    return self.certs_changed();
                }
            }
            Message::CertEdit(index, field) => {
                if let Some(cert) = self.certs.get_mut(index) {
                    match field {
                        CertField::Enabled(on) => cert.enabled = on,
                        CertField::Host(text) => cert.host = text,
                        CertField::Kind(kind) => cert.kind = kind,
                        CertField::Passphrase(text) => cert.passphrase = text,
                        CertField::Path(CertFile::Pfx, text) => cert.pfx_path = text,
                        CertField::Path(CertFile::Cert, text) => cert.cert_path = text,
                        CertField::Path(CertFile::Key, text) => cert.key_path = text,
                        CertField::Path(CertFile::Ca, text) => cert.ca_path = text,
                    }
                    self.cert_status[index] = cert.check();
                    return self.certs_changed();
                }
            }
            Message::CertBrowse(index, file) => {
                return Task::perform(pick_cert_file(file), move |path| {
                    Message::CertPicked(index, file, path)
                });
            }
            Message::CertPicked(index, file, path) => {
                if let Some(path) = path {
                    return self.update(Message::CertEdit(index, CertField::Path(file, path)));
                }
            }
            Message::ToggleTheme => self.theme_index = theme::opposite_of(self.theme_index),
            Message::SetTheme(choice) => self.theme_index = choice.0,
            Message::ToggleSidebar => self.sidebar_visible = !self.sidebar_visible,
            Message::ToggleResponseLayout => self.response_right = !self.response_right,
            Message::DragStart(divider) => {
                self.menu = None;
                let size = match divider {
                    Divider::Sidebar => self.sidebar_width,
                    Divider::Response if self.response_right => self.response_width,
                    Divider::Response => self.response_height,
                };
                self.drag = Some(Drag {
                    divider,
                    start: self.cursor,
                    size,
                });
            }
            Message::DragEnd => {
                self.drag = None;
                self.tab_drag = None;
                self.finish_node_drag();
            }
            Message::TabPressed(id) => {
                self.commit_rename();
                self.menu = None;
                self.activate(id);
                if let Some(index) = self.tabs.iter().position(|t| t.request_id == id) {
                    self.tab_drag = Some(TabDrag {
                        id,
                        start_x: self.cursor.x,
                        start_index: index,
                        moved: false,
                    });
                }
            }
            Message::WindowResized(size) => self.window_size = size,

            // ── Request editor ──
            Message::Edit(edit) if is_variables_edit(&edit) => self.edit_variables(edit),
            Message::Edit(edit) => {
                if let Some(id) = self.active {
                    if let Edit::Url(text) = &edit {
                        if self.import_curl(id, text) {
                            return Task::none();
                        }
                    }
                    let before = self.tree.request(id).cloned();
                    // A token belongs to the settings it was requested with.
                    let invalidates_token = matches!(
                        edit,
                        Edit::OAuthGrant(_)
                            | Edit::OAuthTokenUrl(_)
                            | Edit::OAuthClientId(_)
                            | Edit::OAuthClientSecret(_)
                            | Edit::OAuthScope(_)
                            | Edit::OAuthUsername(_)
                            | Edit::OAuthPassword(_)
                            | Edit::OAuthClientAuth(_)
                    );
                    if invalidates_token && self.tokens.remove(&id).is_some() {
                        self.save_tokens();
                    }
                    if self.apply_edit(id, edit) {
                        self.pin(id); // editing a preview tab keeps it open
                        self.record_history(id, before);
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
            Message::Response(id, token, result, used_token) => {
                if let Some(used) = used_token {
                    self.remember_token(id, used);
                }
                let mut succeeded = false;
                if let Some(rt) = self.runtime.get_mut(&id) {
                    if rt.token == token && matches!(rt.state, ResponseState::Sending) {
                        rt.abort = None;
                        match result {
                            Ok(resp) => {
                                rt.body_view = BodyView::Pretty;
                                rt.response_editor =
                                    text_editor::Content::with_text(&resp.body_pretty);
                                rt.json = if resp.is_binary {
                                    None
                                } else {
                                    jsontree::parse(&resp.body_raw)
                                };
                                rt.json_open = rt
                                    .json
                                    .as_ref()
                                    .map(jsontree::default_open)
                                    .unwrap_or_default();
                                rt.html = is_html(&resp).then(|| htmlview::parse(&resp.body_raw));
                                rt.preview = resp.media.as_ref().map(media::Preview::open);
                                if rt.preview.is_some() {
                                    rt.body_view = BodyView::Preview;
                                }
                                rt.state = ResponseState::Done(Box::new(resp));
                                succeeded = true;
                            }
                            Err(e) => rt.state = ResponseState::Failed(e),
                        }
                    }
                }
                if succeeded {
                    self.run_script(id);
                }
            }

            Message::PickBodyFile => {
                if let Some(id) = self.active {
                    return Task::perform(pick_body_file(), move |path| {
                        Message::BodyFilePicked(id, path)
                    });
                }
            }
            Message::BodyFilePicked(id, path) => {
                if let Some(path) = path {
                    let before = self.tree.request(id).cloned();
                    if self.apply_edit(id, Edit::BodyFile(path)) {
                        self.pin(id);
                        self.record_history(id, before);
                    }
                }
            }
            Message::OAuthFetch => return self.fetch_token(),
            Message::OAuthFetched(id, result) => {
                if let Some(rt) = self.runtime.get_mut(&id) {
                    rt.oauth_busy = false;
                    rt.oauth_status = Some(match &result {
                        Ok(_) => (false, "Token received.".to_string()),
                        Err(e) => (true, e.clone()),
                    });
                }
                if let Ok(token) = result {
                    self.remember_token(id, token);
                }
            }
            Message::OAuthClear => {
                if let Some(id) = self.active {
                    if self.tokens.remove(&id).is_some() {
                        self.save_tokens();
                    }
                    if let Some(rt) = self.runtime.get_mut(&id) {
                        rt.oauth_status = None;
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
                            BodyView::Pretty => Some(&resp.body_pretty),
                            BodyView::Raw => Some(&resp.body_raw),
                            BodyView::Tree | BodyView::Preview => None,
                        };
                        if let Some(text) = text {
                            rt.response_editor = text_editor::Content::with_text(text);
                        }
                    }
                }
            }
            Message::ToggleJsonNode(path) => {
                if let Some(rt) = self.active.and_then(|id| self.runtime.get_mut(&id)) {
                    if !rt.json_open.remove(&path) {
                        rt.json_open.insert(path);
                    }
                }
            }
            Message::JsonExpandAll => {
                if let Some(rt) = self.active.and_then(|id| self.runtime.get_mut(&id)) {
                    if let Some(json) = &rt.json {
                        rt.json_open = jsontree::all_open(json);
                    }
                }
            }
            Message::JsonCollapseAll => {
                if let Some(rt) = self.active.and_then(|id| self.runtime.get_mut(&id)) {
                    rt.json_open = jsontree::root_only();
                }
            }
            Message::OpenLink(target) => {
                // Relative links point into the page that was fetched.
                let base = self
                    .active
                    .and_then(|id| self.runtime.get(&id))
                    .and_then(|rt| match &rt.state {
                        ResponseState::Done(resp) => Some(resp.final_url.clone()),
                        _ => None,
                    });
                let full = resolve_link(base.as_deref(), &target);
                if full.starts_with("http://") || full.starts_with("https://") {
                    if let Err(e) = open_target(&full) {
                        self.show_notice(Notice {
                            text: format!("Could not open the link: {e}"),
                            error: true,
                        });
                    }
                }
            }
            Message::OpenInBrowser => {
                let html = self
                    .active
                    .and_then(|id| self.runtime.get(&id))
                    .and_then(|rt| match &rt.state {
                        ResponseState::Done(resp) => Some(resp.body_raw.clone()),
                        _ => None,
                    });
                if let Some(html) = html {
                    let path = std::env::temp_dir().join("oxide-preview.html");
                    let result = std::fs::write(&path, html)
                        .map_err(|e| e.to_string())
                        .and_then(|_| open_target(&path.display().to_string()));
                    if let Err(e) = result {
                        self.show_notice(Notice {
                            text: format!("Could not open the browser: {e}"),
                            error: true,
                        });
                    }
                }
            }
            Message::MediaZoom(up) => {
                if let Some(Ok(preview)) = self.active_preview() {
                    match preview {
                        media::Preview::Image { zoom, .. } => {
                            *zoom = Some(media::step_zoom(zoom.unwrap_or(1.0), up));
                        }
                        media::Preview::Pdf(doc) => {
                            doc.set_zoom(media::step_zoom(doc.zoom, up));
                        }
                    }
                }
            }
            Message::MediaFit => {
                if let Some(Ok(media::Preview::Image { zoom, .. })) = self.active_preview() {
                    *zoom = None;
                }
            }
            Message::MediaActual => {
                if let Some(Ok(preview)) = self.active_preview() {
                    match preview {
                        media::Preview::Image { zoom, .. } => *zoom = Some(1.0),
                        media::Preview::Pdf(doc) => doc.set_zoom(1.0),
                    }
                }
            }
            Message::PdfPage(nav) => {
                if let Some(Ok(media::Preview::Pdf(doc))) = self.active_preview() {
                    let page = match nav {
                        PdfNav::First => 0,
                        PdfNav::Previous => doc.page.saturating_sub(1),
                        PdfNav::Next => doc.page + 1,
                        PdfNav::Last => doc.pages - 1,
                    };
                    doc.go_to(page);
                }
            }
            Message::OpenMedia => {
                let media = self
                    .active
                    .and_then(|id| self.runtime.get(&id))
                    .and_then(|rt| match &rt.state {
                        ResponseState::Done(resp) => resp.media.clone(),
                        _ => None,
                    });
                if let Some(media) = media {
                    let path = std::env::temp_dir()
                        .join(format!("oxide-preview.{}", media::extension(&media.bytes)));
                    let result = std::fs::write(&path, media.bytes.as_slice())
                        .map_err(|e| e.to_string())
                        .and_then(|_| open_target(&path.display().to_string()));
                    if let Err(e) = result {
                        self.show_notice(Notice {
                            text: format!("Could not open the file: {e}"),
                            error: true,
                        });
                    }
                }
            }
            Message::SaveResponse => {
                let saved = self
                    .active
                    .and_then(|id| self.runtime.get(&id))
                    .and_then(|rt| match &rt.state {
                        ResponseState::Done(resp) => Some((
                            resp.bytes
                                .clone()
                                .unwrap_or_else(|| std::sync::Arc::new(resp.body_raw.clone().into_bytes())),
                            response_file_name(resp),
                        )),
                        _ => None,
                    });
                if let Some((bytes, name)) = saved {
                    return Task::perform(save_media(name, bytes), Message::ResponseSaved);
                }
            }
            Message::ResponseSaved(result) => match result {
                Ok(Some(name)) => {
                    self.show_notice(Notice {
                        text: format!("Saved to {name}."),
                        error: false,
                    });
                }
                Ok(None) => {}
                Err(e) => {
                    self.show_notice(Notice {
                        text: format!("Could not save the file: {e}"),
                        error: true,
                    });
                }
            },
            Message::CopyCurl => {
                if let Some(id) = self.active {
                    return self.copy_curl(id);
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
                            BodyView::Pretty | BodyView::Tree => resp.body_pretty.clone(),
                            BodyView::Raw | BodyView::Preview => resp.body_raw.clone(),
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

        // The command palette: typing goes to its box, the arrows move the selection.
        if self.palette.is_some() {
            let total = self.palette_items().len();
            match shortcut {
                Shortcut::Escape | Shortcut::Palette => self.palette = None,
                Shortcut::Confirm => {
                    let index = self.palette.as_ref().map(|p| p.selected).unwrap_or(0);
                    return self.run_palette(index);
                }
                Shortcut::Down => {
                    if let Some(p) = &mut self.palette {
                        if total > 0 {
                            p.selected = (p.selected + 1).min(total - 1);
                        }
                    }
                }
                Shortcut::Up => {
                    if let Some(p) = &mut self.palette {
                        p.selected = p.selected.saturating_sub(1);
                    }
                }
                _ => {}
            }
            return Task::none();
        }

        // The settings window is modal too (typing goes to its fields).
        if self.settings_tab.is_some() {
            match shortcut {
                Shortcut::Escape | Shortcut::Settings => self.settings_tab = None,
                Shortcut::FocusNext => return operation::focus_next(),
                Shortcut::FocusPrevious => return operation::focus_previous(),
                _ => {}
            }
            return Task::none();
        }

        // The variables dialog is modal too (typing goes to its fields).
        if self.vars_dialog.is_some() {
            match shortcut {
                Shortcut::Escape => self.vars_dialog = None,
                Shortcut::FocusNext => return operation::focus_next(),
                Shortcut::FocusPrevious => return operation::focus_previous(),
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
                if self.node_drag.as_ref().is_some_and(|d| d.active) {
                    self.node_drag = None; // cancel a drag
                    return Task::none();
                }
                if self.search.is_empty() {
                    self.extra.clear();
                } else {
                    self.search.clear();
                }
                Task::none()
            }
            Shortcut::FocusSearch => {
                if self.sidebar_visible {
                    Task::batch([
                        operation::focus(SEARCH_ID),
                        operation::select_all(SEARCH_ID),
                    ])
                } else {
                    // Show the sidebar; press Ctrl+F again to focus the box.
                    self.sidebar_visible = true;
                    Task::none()
                }
            }
            Shortcut::Confirm => Task::none(),
            Shortcut::Save => {
                self.save_active();
                Task::none()
            }
            Shortcut::Undo => {
                self.undo_redo(false);
                Task::none()
            }
            Shortcut::Redo => {
                self.undo_redo(true);
                Task::none()
            }
            Shortcut::ToggleSidebar => {
                self.sidebar_visible = !self.sidebar_visible;
                Task::none()
            }
            Shortcut::FocusNext => operation::focus_next(),
            Shortcut::FocusPrevious => operation::focus_previous(),
            Shortcut::Send => self.send_active(),
            Shortcut::ImportCollection => self.perform(MenuAction::ImportCollection, None),
            Shortcut::ImportApidogFolder => self.perform(MenuAction::ImportApidogFolder, None),
            Shortcut::Palette => self.update(Message::OpenPalette),
            Shortcut::Settings => self.update(Message::OpenSettings(SettingsTab::Appearance)),
            Shortcut::Up | Shortcut::Down => Task::none(),
        }
    }

    // ───────────────────────── variables ─────────────────────────

    /// The value of a variable for a request: its collection's variables win over the globals.
    /// Names of the enabled variables the active request can use (collection and global).
    pub fn defined_variables(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        let mut add = |rows: &[KeyValue]| {
            for row in rows
                .iter()
                .filter(|r| r.enabled && !r.key.trim().is_empty())
            {
                let name = row.key.trim().to_string();
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        };
        if let Some(root) = self
            .active
            .and_then(|id| self.tree.ancestors(id).last().copied())
            .and_then(|id| self.tree.get(id))
        {
            add(&root.variables);
        }
        add(&self.globals);
        names.sort();
        names
    }

    pub fn lookup_variable(&self, request: Id, name: &str) -> Option<String> {
        self.lookup_variable_in_scope(request, name).map(|(v, _)| v)
    }

    /// Like [`Self::lookup_variable`], also telling where the value came from.
    pub fn lookup_variable_in_scope(
        &self,
        request: Id,
        name: &str,
    ) -> Option<(String, &'static str)> {
        let find = |rows: &[KeyValue]| {
            rows.iter()
                .rev()
                .find(|r| r.enabled && r.key.trim() == name)
                .map(|r| r.value.clone())
        };
        let root = self.tree.ancestors(request).last().copied();
        root.and_then(|id| self.tree.get(id))
            .and_then(|node| find(&node.variables))
            .map(|v| (v, "collection"))
            .or_else(|| find(&self.globals).map(|v| (v, "global")))
    }

    fn variables_mut(&mut self, scope: VarScope) -> Option<&mut Vec<KeyValue>> {
        match scope {
            VarScope::Global => Some(&mut self.globals),
            VarScope::Collection(id) => self.tree.get_mut(id).map(|n| &mut n.variables),
        }
    }

    pub fn variables(&self, scope: VarScope) -> Option<&Vec<KeyValue>> {
        match scope {
            VarScope::Global => Some(&self.globals),
            VarScope::Collection(id) => self.tree.get(id).map(|n| &n.variables),
        }
    }

    /// Commands and requests that match what was typed in the palette, best first.
    pub fn palette_items(&self) -> Vec<PaletteItem> {
        let query = self
            .palette
            .as_ref()
            .map(|p| p.query.to_lowercase())
            .unwrap_or_default();
        let words: Vec<&str> = query.split_whitespace().collect();

        let command = |label: &str, detail: &str, keys: Option<&'static str>, message: Message| {
            PaletteItem {
                label: label.to_string(),
                detail: detail.to_string(),
                keys,
                action: PaletteAction::Run(message),
            }
        };
        let has_request = self.active.is_some_and(|id| self.tree.request(id).is_some());

        let mut commands = vec![
            command("New Request", "", Some("Ctrl+N"), Message::MenuAction(MenuAction::NewRequest)),
            command("New Folder", "", Some("Ctrl+Shift+N"), Message::MenuAction(MenuAction::NewFolder)),
            command("New Collection", "", None, Message::MenuAction(MenuAction::NewCollection)),
            command("Import Collection", "Postman v2.1 or Apidog .yaml files", Some("Ctrl+O"), Message::MenuAction(MenuAction::ImportCollection)),
            command("Import Apidog Folder", "Apidog folder", Some("Ctrl+Shift+O"),Message::MenuAction(MenuAction::ImportApidogFolder)),
            command("Export Collection", "", None, Message::MenuAction(MenuAction::ExportCollection)),
            command("Settings", "Theme, requests, certificates, shortcuts", Some("Ctrl+,"), Message::OpenSettings(SettingsTab::Appearance)),
            command("Backup: export or restore everything", "", None, Message::OpenSettings(SettingsTab::Data)),
            command("Global Variables", "", None, Message::OpenVariables(VarScope::Global)),
            command("Certificates", "", None, Message::OpenSettings(SettingsTab::Certificates)),
            command("Keyboard Shortcuts", "", None, Message::OpenSettings(SettingsTab::Shortcuts)),
            command("Toggle Sidebar", "", Some("Ctrl+B"), Message::ToggleSidebar),
            command("Toggle Light / Dark Theme", "", None, Message::ToggleTheme),
            command("Dock Response Right / Bottom", "", None, Message::ToggleResponseLayout),
            command("Search Requests", "", Some("Ctrl+F"), Message::Shortcut(Shortcut::FocusSearch)),
        ];
        if has_request {
            commands.splice(
                0..0,
                [
                    command("Send Request", "", Some("Ctrl+Enter"), Message::Send),
                    command("Save Request", "", Some("Ctrl+S"), Message::Shortcut(Shortcut::Save)),
                    command("Copy as cURL", "", None, Message::CopyCurl),
                    command("Duplicate", "", Some("Ctrl+D"), Message::MenuAction(MenuAction::Duplicate)),
                    command("Rename", "", Some("F2"), Message::MenuAction(MenuAction::Rename)),
                    command("Close Tab", "", Some("Ctrl+W"), Message::MenuAction(MenuAction::CloseTab)),
                ],
            );
        }

        let mut items: Vec<PaletteItem> = commands;
        for (id, request) in self.tree.requests() {
            let Some(node) = self.tree.get(id) else {
                continue;
            };
            let mut path: Vec<String> = self
                .tree
                .ancestors(id)
                .iter()
                .filter_map(|a| self.tree.get(*a).map(|n| n.name.clone()))
                .collect();
            path.reverse();
            items.push(PaletteItem {
                label: format!("{}  {}", request.method.short(), node.name),
                detail: path.join(" / "),
                keys: None,
                action: PaletteAction::Open(id),
            });
        }

        if words.is_empty() {
            return items;
        }
        let mut scored: Vec<(usize, PaletteItem)> = items
            .into_iter()
            .filter_map(|item| {
                let haystack = format!("{} {}", item.label, item.detail).to_lowercase();
                let label = item.label.to_lowercase();
                let mut score = 0;
                for word in &words {
                    let at = haystack.find(word)?;
                    // Earlier and in the name rather than the folder path = better.
                    score += at + if label.contains(word) { 0 } else { 100 };
                    if label.starts_with(word) || label.split_whitespace().any(|w| w.starts_with(word)) {
                        score = score.saturating_sub(20);
                    }
                }
                Some((score, item))
            })
            .collect();
        scored.sort_by_key(|(score, _)| *score);
        scored.into_iter().map(|(_, item)| item).collect()
    }

    /// Runs the `index`-th row of the palette and closes it.
    fn run_palette(&mut self, index: usize) -> Task<Message> {
        let item = self.palette_items().into_iter().nth(index);
        self.palette = None;
        match item.map(|i| i.action) {
            Some(PaletteAction::Run(message)) => self.update(message),
            Some(PaletteAction::Open(id)) => {
                self.open_pinned(id);
                Task::none()
            }
            None => Task::none(),
        }
    }

    /// Puts the request as a `curl` command on the clipboard.
    fn copy_curl(&mut self, id: Id) -> Task<Message> {
        let Some(request) = self.tree.request(id) else {
            return Task::none();
        };
        let request = vars::resolve(request, &|name| self.lookup_variable(id, name));
        let token = self.tokens.get(&id).map(|t| t.access_token.clone());
        let command = curl::export(&request, token.as_deref());
        self.show_notice(Notice {
            text: "Copied as cURL.".into(),
            error: false,
        });
        iced::clipboard::write(command)
    }

    /// Pasting a `curl` command into the URL box fills in the whole request.
    /// Returns `true` when that happened.
    fn import_curl(&mut self, id: Id, text: &str) -> bool {
        let pasted = self
            .tree
            .request(id)
            .is_some_and(|req| text.len() > req.url.len() + 1);
        if !pasted || !curl::looks_like_curl(text) {
            return false;
        }
        match curl::parse(text) {
            Ok(imported) => {
                let before = self.tree.request(id).cloned();
                if let Some(req) = self.tree.request_mut(id) {
                    curl::apply(req, imported);
                }
                if let Some(rt) = self.runtime.get_mut(&id) {
                    let body = self
                        .tree
                        .request(id)
                        .map(|r| r.body_text.clone())
                        .unwrap_or_default();
                    rt.body_editor = text_editor::Content::with_text(&body);
                }
                self.pin(id);
                self.record_history(id, before);
                self.show_notice(Notice {
                    text: "Imported the cURL command.".into(),
                    error: false,
                });
                true
            }
            Err(e) => {
                self.show_notice(Notice {
                    text: format!("Could not import the cURL command: {e}"),
                    error: true,
                });
                false
            }
        }
    }

    /// The image / PDF preview of the active request, when it has one.
    fn active_preview(&mut self) -> Option<&mut Result<media::Preview, String>> {
        let id = self.active?;
        self.runtime.get_mut(&id)?.preview.as_mut()
    }

    /// Saves the settings and makes them the ones used for the next requests.
    fn settings_changed(&mut self) {
        settings::set_active(&self.settings);
        if let Err(e) = storage::save_settings(&self.settings) {
            self.show_notice(Notice {
                text: format!("Could not save the settings: {e}"),
                error: true,
            });
        }
    }

    /// Saves the certificates and makes them the ones used for the next requests.
    fn certs_changed(&mut self) -> Task<Message> {
        certs::set_active(&self.certs);
        if let Err(e) = storage::save_certs(&self.certs) {
            self.show_notice(Notice {
                text: format!("Could not save the certificates: {e}"),
                error: true,
            });
        }
        Task::none()
    }

    /// Applies an edit made in the Variables dialog and saves it right away.
    fn edit_variables(&mut self, edit: Edit) {
        let Some(scope) = self.vars_dialog else {
            return;
        };
        let Some(table) = self.variables_mut(scope) else {
            return;
        };

        match edit {
            Edit::KvToggle(_, i, on) => {
                if let Some(row) = table.get_mut(i) {
                    row.enabled = on;
                }
            }
            Edit::KvKey(_, i, key) => {
                if let Some(row) = table.get_mut(i) {
                    row.key = key;
                }
                keep_trailing_blank(table);
            }
            Edit::KvValue(_, i, value) => {
                if let Some(row) = table.get_mut(i) {
                    row.value = value;
                }
                keep_trailing_blank(table);
            }
            Edit::KvRemove(_, i) => {
                if i < table.len() {
                    table.remove(i);
                }
                keep_trailing_blank(table);
            }
            _ => return,
        }
        self.save_variables(scope);
    }

    fn save_variables(&mut self, scope: VarScope) {
        match scope {
            VarScope::Global => {
                if let Err(e) = storage::save_globals(&self.globals) {
                    self.save_error = Some(e);
                }
            }
            VarScope::Collection(_) => self.persist(),
        }
    }

    /// Runs the request's post-response script on the response it just received.
    fn run_script(&mut self, id: Id) {
        let script = self
            .tree
            .request(id)
            .map(|r| r.script.clone())
            .unwrap_or_default();
        if script.trim().is_empty() {
            if let Some(rt) = self.runtime.get_mut(&id) {
                rt.script_output.clear();
            }
            return;
        }

        let outcome = {
            let Some(ResponseState::Done(resp)) = self.runtime.get(&id).map(|rt| &rt.state) else {
                return;
            };
            vars::run_script(&script, resp, &|name| self.lookup_variable(id, name))
        };

        let root = self.tree.ancestors(id).last().copied().unwrap_or(id);
        let mut output: Vec<(bool, String)> = Vec::new();
        let (mut globals_changed, mut collection_changed) = (false, false);

        for assignment in outcome.assignments {
            let preview: String = assignment.value.chars().take(60).collect();
            let preview = if assignment.value.chars().count() > 60 {
                format!("{preview}…")
            } else {
                preview
            };
            match assignment.scope {
                vars::Scope::Global => {
                    set_variable(&mut self.globals, &assignment.name, assignment.value);
                    globals_changed = true;
                    output.push((false, format!("global.{} = {preview}", assignment.name)));
                }
                vars::Scope::Collection => {
                    if let Some(node) = self.tree.get_mut(root) {
                        set_variable(&mut node.variables, &assignment.name, assignment.value);
                        collection_changed = true;
                        output.push((false, format!("{} = {preview}", assignment.name)));
                    }
                }
            }
        }
        output.extend(outcome.errors.into_iter().map(|e| (true, e)));

        if globals_changed {
            self.save_variables(VarScope::Global);
        }
        if collection_changed {
            self.persist();
        }
        if let Some(rt) = self.runtime.get_mut(&id) {
            rt.script_output = output;
        }
    }

    /// Moves the dragged tab to the slot under the cursor.
    fn apply_tab_drag(&mut self) {
        let Some(drag) = &self.tab_drag else {
            return;
        };
        let dx = self.cursor.x - drag.start_x;
        if !drag.moved && dx.abs() < 8.0 {
            return;
        }
        let (id, start) = (drag.id, drag.start_index);

        let last = self.tabs.len().saturating_sub(1) as f32;
        let target = (start as f32 + (dx / TAB_STRIDE).round()).clamp(0.0, last) as usize;
        if let Some(current) = self.tabs.iter().position(|t| t.request_id == id) {
            if current != target {
                let tab = self.tabs.remove(current);
                self.tabs.insert(target, tab);
            }
        }
        if let Some(drag) = &mut self.tab_drag {
            drag.moved = true;
        }
    }

    /// Remembers the state before an edit so Ctrl+Z can bring it back.
    fn record_history(&mut self, id: Id, before: Option<Request>) {
        let Some(before) = before else {
            return;
        };
        if self.tree.request(id).is_none_or(|now| *now == before) {
            return;
        }
        let Some(rt) = self.runtime.get_mut(&id) else {
            return;
        };

        self.history_seq += 1;
        let seq = self.history_seq;
        let burst = rt.last_edit.is_some_and(|t| t.elapsed() < HISTORY_MERGE);
        rt.last_edit = Some(Instant::now());
        rt.redo.clear();
        rt.redo_seq.clear();
        if burst && !rt.undo.is_empty() {
            // Part of the step that is already recorded: it is now the most recent.
            if let Some(last) = rt.undo_seq.last_mut() {
                *last = seq;
            }
            return;
        }
        rt.undo.push(before);
        rt.undo_seq.push(seq);
        if rt.undo.len() > HISTORY_LIMIT {
            rt.undo.remove(0);
            rt.undo_seq.remove(0);
        }
    }

    /// Ctrl+Z / Ctrl+Y: undoes or redoes whichever is more recent, an edit of the
    /// active request or a move in the sidebar.
    fn undo_redo(&mut self, redo: bool) {
        let request_seq = self.active.and_then(|id| self.runtime.get(&id)).and_then(|rt| {
            if redo { rt.redo_seq.last() } else { rt.undo_seq.last() }.copied()
        });
        let move_seq = if redo { self.move_redo.last() } else { self.move_undo.last() }
            .map(|s| s.seq);
        match (request_seq, move_seq) {
            (Some(r), Some(m)) if m > r => self.step_moves(redo),
            (None, Some(_)) => self.step_moves(redo),
            _ => self.step_history(redo),
        }
    }

    /// Records a sidebar move so it can be undone.
    fn record_move(&mut self, before: Vec<NodePosition>, after: Vec<NodePosition>, selection: (Id, Vec<Id>)) {
        self.history_seq += 1;
        self.move_redo.clear();
        self.move_undo.push(MoveStep {
            seq: self.history_seq,
            before,
            after,
            selection,
        });
        if self.move_undo.len() > HISTORY_LIMIT {
            self.move_undo.remove(0);
        }
    }

    /// Undoes (`redo == false`) or redoes the last sidebar move.
    fn step_moves(&mut self, redo: bool) {
        let step = if redo { self.move_redo.pop() } else { self.move_undo.pop() };
        let Some(mut step) = step else {
            return;
        };
        let positions = if redo { &step.after } else { &step.before };
        if self.tree.restore_positions(positions) {
            for p in positions {
                self.tree.expand_ancestors(p.id);
            }
            let (primary, others) = &step.selection;
            if self.tree.get(*primary).is_some() {
                self.select_only(*primary);
                self.extra = others.iter().copied().filter(|id| self.tree.get(*id).is_some()).collect();
            }
            self.persist();
        }
        // The step goes to the other stack, newest there.
        self.history_seq += 1;
        step.seq = self.history_seq;
        if redo {
            self.move_undo.push(step);
        } else {
            self.move_redo.push(step);
        }
    }

    /// Ctrl+Z (`redo == false`) / Ctrl+Y (`redo == true`) on the active request.
    fn step_history(&mut self, redo: bool) {
        let Some(id) = self.active else {
            return;
        };
        let Some(current) = self.tree.request(id).cloned() else {
            return;
        };
        let Some(rt) = self.runtime.get_mut(&id) else {
            return;
        };

        let target = if redo { rt.redo.pop() } else { rt.undo.pop() };
        let Some(target) = target else {
            return;
        };
        self.history_seq += 1;
        let seq = self.history_seq;
        if redo {
            rt.redo_seq.pop();
            rt.undo.push(current);
            rt.undo_seq.push(seq);
        } else {
            rt.undo_seq.pop();
            rt.redo.push(current);
            rt.redo_seq.push(seq);
        }
        rt.last_edit = None;

        // The text editors keep their own copy of the text: rebuild them when it differs.
        let rebuild = |content: &mut text_editor::Content, text: &str| {
            if content.text() != text {
                *content = text_editor::Content::with_text(text);
                content.perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
            }
        };
        rebuild(&mut rt.body_editor, &target.body_text);
        rebuild(&mut rt.script_editor, &target.script);

        if let Some(req) = self.tree.request_mut(id) {
            *req = target;
        }
        self.pin(id);
    }

    /// Drops the unsaved edits of a request (used when its tab is closed).
    fn revert_request(&mut self, id: Id) {
        if let Some(rt) = self.runtime.get_mut(&id) {
            rt.undo.clear();
            rt.redo.clear();
            rt.undo_seq.clear();
            rt.redo_seq.clear();
            rt.last_edit = None;
        }
        let Some(saved) = self.saved.get(&id).cloned() else {
            return;
        };
        let Some(req) = self.tree.request_mut(id) else {
            return;
        };
        if *req == saved {
            return;
        }
        *req = saved;

        // The text editors keep their own copy of the text: rebuild them.
        if let (Some(rt), Some(req)) = (self.runtime.get_mut(&id), self.tree.request(id)) {
            rt.body_editor = text_editor::Content::with_text(&req.body_text);
            rt.script_editor = text_editor::Content::with_text(&req.script);
        }
    }

    /// Turns a press on a sidebar row into a drag once the mouse has moved a little.
    fn apply_node_drag(&mut self) {
        let cursor = self.cursor;
        let Some(drag) = &mut self.node_drag else {
            return;
        };
        if drag.active {
            return;
        }
        let moved = (cursor.x - drag.start.x).hypot(cursor.y - drag.start.y);
        if moved < 6.0 {
            return;
        }
        drag.active = true;
        let (id, was_expanded) = (drag.id, drag.was_expanded);
        // The press toggled a folder: put it back so a drag does not fold it.
        if let Some(node) = self.tree.get_mut(id) {
            if node.is_container() {
                node.expanded = was_expanded;
            }
        }
    }

    /// The mouse is over `row`, `y` pixels from its top: work out where a drop would land.
    fn drag_over(&mut self, row: Id, y: f32) {
        let Some(drag) = &self.node_drag else {
            return;
        };
        if !drag.active {
            return;
        }
        let ids = drag.ids.clone();
        let over_container = self.tree.get(row).is_some_and(Node::is_container);
        let dragged_collection = ids.iter().any(|id| {
            self.tree
                .get(*id)
                .is_some_and(|n| matches!(n.kind, Kind::Collection))
        });

        let ratio = y / ROW_HEIGHT;
        let wanted = if over_container && !dragged_collection {
            if ratio < 0.25 {
                DropZone::Before
            } else if ratio > 0.75 {
                DropZone::After
            } else {
                DropZone::Inside
            }
        } else if ratio < 0.5 {
            DropZone::Before
        } else {
            DropZone::After
        };

        // The drop is only allowed when every dragged node can land there the same way.
        let mut zone = None;
        for id in &ids {
            match self.tree.drop_zone(*id, row, wanted) {
                Some(z) if zone.is_none_or(|prev| prev == z) => zone = Some(z),
                _ => {
                    zone = None;
                    break;
                }
            }
        }
        let target = zone.map(|zone| (row, zone));
        if let Some(drag) = &mut self.node_drag {
            drag.target = target;
        }
    }

    /// The mouse was released: drop the node where the indicator was, if anywhere.
    fn finish_node_drag(&mut self) {
        let Some(drag) = self.node_drag.take() else {
            return;
        };
        if !drag.active {
            if drag.collapse_on_click {
                self.select_only(drag.id);
            }
            return;
        }
        let Some((target, zone)) = drag.target else {
            return;
        };
        // Each node is put right before / right after the target, so walking the list
        // forwards (Before, Inside) or backwards (After) keeps their original order.
        let mut moved = Vec::new();
        let before = self.tree.positions(&drag.ids);
        let order: Vec<Id> = if zone == DropZone::After {
            drag.ids.iter().rev().copied().collect()
        } else {
            drag.ids.clone()
        };
        for id in order {
            if self.tree.move_node(id, target, zone) {
                moved.push(id);
            }
        }
        if moved.is_empty() {
            return;
        }
        for id in &moved {
            self.tree.expand_ancestors(*id);
        }
        let primary = if moved.contains(&drag.id) { drag.id } else { moved[0] };
        let others: Vec<Id> = moved.iter().copied().filter(|id| *id != primary).collect();
        self.select_only(primary);
        self.extra = others.iter().copied().collect();
        let before: Vec<NodePosition> =
            before.into_iter().filter(|p| moved.contains(&p.id)).collect();
        let after = self.tree.positions(&moved);
        self.record_move(before, after, (primary, others));
        self.persist();
    }

    /// Resizes the panel whose border is being dragged.
    fn apply_drag(&mut self) {
        let Some(drag) = &self.drag else {
            return;
        };
        let dx = self.cursor.x - drag.start.x;
        let dy = self.cursor.y - drag.start.y;
        let window = self.window_size;

        match drag.divider {
            // The sidebar grows when dragged to the right.
            Divider::Sidebar => {
                let max = (window.width - 320.0).clamp(MIN_SIDEBAR, MAX_SIDEBAR);
                self.sidebar_width = (drag.size + dx).clamp(MIN_SIDEBAR, max);
            }
            // The response grows when dragged away from the editor (left / up).
            Divider::Response if self.response_right => {
                let sidebar = if self.sidebar_visible {
                    self.sidebar_width
                } else {
                    0.0
                };
                let max = (window.width - sidebar - 320.0).max(MIN_RESPONSE_WIDTH);
                self.response_width = (drag.size - dx).clamp(MIN_RESPONSE_WIDTH, max);
            }
            Divider::Response => {
                // 280px for the editor and tabs, 25px for the status bar.
                let max = (window.height - 305.0).max(MIN_RESPONSE_HEIGHT);
                self.response_height = (drag.size - dy).clamp(MIN_RESPONSE_HEIGHT, max);
            }
        }
    }

    fn remember_token(&mut self, id: Id, token: oauth::Token) {
        if self.tokens.get(&id) != Some(&token) {
            self.tokens.insert(id, token);
            self.save_tokens();
        }
    }

    fn save_tokens(&mut self) {
        if let Err(e) = storage::save_tokens(&self.tokens) {
            self.save_error = Some(e);
        }
    }

    /// "Get new token" in the Auth tab: always asks the server (the refresh token is used
    /// when there is one and it is still accepted).
    fn fetch_token(&mut self) -> Task<Message> {
        let Some(id) = self.active else {
            return Task::none();
        };
        let Some(request) = self.tree.request(id) else {
            return Task::none();
        };
        let request = vars::resolve(request, &|name| self.lookup_variable(id, name));
        let config = oauth::Config::from_request(&request);
        let refresh = self.tokens.get(&id).and_then(|t| t.refresh_token.clone());

        self.ensure_runtime(id);
        if let Some(rt) = self.runtime.get_mut(&id) {
            if rt.oauth_busy {
                return Task::none();
            }
            rt.oauth_busy = true;
            rt.oauth_status = None;
        }
        Task::perform(
            async move {
                // Try the refresh token first, then fall back to a fresh grant.
                if let Some(refresh) = refresh.as_deref() {
                    if let Ok(token) = oauth::fetch(&config, Some(refresh)).await {
                        return Ok(token);
                    }
                }
                oauth::fetch(&config, None).await
            },
            move |result| Message::OAuthFetched(id, result),
        )
    }

    fn send_active(&mut self) -> Task<Message> {
        let Some(id) = self.active else {
            return Task::none();
        };
        let Some(request) = self.tree.request(id) else {
            return Task::none();
        };
        // {{variables}} are replaced just before sending; the saved request keeps them.
        let request = vars::resolve(request, &|name| self.lookup_variable(id, name));
        let undefined = vars::names_in(&request.url);

        self.ensure_runtime(id);
        let Some(rt) = self.runtime.get_mut(&id) else {
            return Task::none();
        };
        if matches!(rt.state, ResponseState::Sending) {
            return Task::none();
        }
        if !undefined.is_empty() {
            let list: Vec<String> = undefined.iter().map(|n| format!("{{{{{n}}}}}")).collect();
            rt.state = ResponseState::Failed(format!(
                "The URL uses undefined variable(s): {}. Define them under Variables.",
                list.join(", ")
            ));
            return Task::none();
        }

        rt.token += 1;
        let token = rt.token;
        rt.state = ResponseState::Sending;

        let current = self.tokens.get(&id).cloned();
        let (task, handle) = Task::perform(oauth::send(request, current), move |(r, used)| {
            Message::Response(id, token, r, used)
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
        // Variables are edited through `edit_variables`, never through a request.
        KvList::Vars => &mut req.params,
    }
}

/// Spaces inserted by Tab in the code editors.
const INDENT: usize = 4;

/// Inserts one indentation step at the cursor. With a selection nothing happens
/// (typing would replace the selected text). Returns whether the text changed.
fn indent(content: &mut text_editor::Content) -> bool {
    if content.selection().is_some_and(|s| !s.is_empty()) {
        return false;
    }
    for _ in 0..INDENT {
        content.perform(text_editor::Action::Edit(text_editor::Edit::Insert(' ')));
    }
    true
}

/// Removes up to one indentation step of spaces directly before the cursor
/// (what Tab inserted). Returns whether the text changed.
fn unindent(content: &mut text_editor::Content) -> bool {
    use text_editor::{Action, Edit, Motion};

    if content.selection().is_some_and(|s| !s.is_empty()) {
        return false;
    }

    // Select to the left one character at a time, as long as it is a space.
    let mut spaces = 0;
    for _ in 0..INDENT {
        content.perform(Action::Select(Motion::Left));
        let selected = content.selection().unwrap_or_default();

        if selected.chars().count() == spaces + 1 && selected.chars().all(|c| c == ' ') {
            spaces += 1;
        } else if selected.chars().count() == spaces {
            // The cursor could not move (start of the text).
            break;
        } else {
            // Not a space (or a line break): step back over it.
            content.perform(Action::Select(Motion::Right));
            break;
        }
    }

    if spaces == 0 {
        return false;
    }
    content.perform(Action::Edit(Edit::Backspace));
    true
}

fn is_variables_edit(edit: &Edit) -> bool {
    matches!(
        edit,
        Edit::KvToggle(KvList::Vars, ..)
            | Edit::KvKey(KvList::Vars, ..)
            | Edit::KvValue(KvList::Vars, ..)
            | Edit::KvRemove(KvList::Vars, ..)
    )
}

/// Sets `name` to `value`, adding the variable above the trailing blank row if needed.
fn set_variable(rows: &mut Vec<KeyValue>, name: &str, value: String) {
    if let Some(row) = rows.iter_mut().find(|r| r.key.trim() == name) {
        row.value = value;
        row.enabled = true;
    } else {
        let at = if rows.last().is_some_and(KeyValue::is_blank) {
            rows.len() - 1
        } else {
            rows.len()
        };
        rows.insert(
            at,
            KeyValue {
                enabled: true,
                key: name.to_string(),
                value,
            },
        );
    }
    keep_trailing_blank(rows);
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
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => Some(Message::DragEnd),
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
            if status == event::Status::Captured
                && !matches!(shortcut, Shortcut::Escape | Shortcut::Send)
            {
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
        keyboard::Key::Named(Named::F1) => return Some(Shortcut::Palette),
        keyboard::Key::Named(Named::Escape) => return Some(Shortcut::Escape),
        keyboard::Key::Named(Named::Enter) => {
            return Some(if modifiers.command() {
                Shortcut::Send
            } else {
                Shortcut::Confirm
            })
        }
        keyboard::Key::Named(Named::ArrowUp) => return Some(Shortcut::Up),
        keyboard::Key::Named(Named::ArrowDown) => return Some(Shortcut::Down),
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
        ('z', false) => Some(Shortcut::Undo),
        ('z', true) | ('y', false) => Some(Shortcut::Redo),
        ('b', false) => Some(Shortcut::ToggleSidebar),
        ('f', false) => Some(Shortcut::FocusSearch),
        ('o', false) => Some(Shortcut::ImportCollection),
        ('o', true) => Some(Shortcut::ImportApidogFolder),
        ('k', false) => Some(Shortcut::Palette),
        (',', false) => Some(Shortcut::Settings),
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

// ───────────────────────────── import / export helpers ─────────────────────────────

/// Something picked to import.
#[derive(Debug, Clone)]
pub enum ImportSource {
    /// `(file name, file contents)` of a Postman collection.
    Postman(String, String),
    /// Apidog request files, as one collection.
    Apidog(apidog::Dir),
}

/// Lets the user pick Postman collections and/or Apidog request files and
/// reads them. The Apidog files picked together become one collection, named
/// after the folder they are in.
async fn pick_collections() -> Result<Vec<ImportSource>, String> {
    let Some(files) = rfd::AsyncFileDialog::new()
        .set_title("Import collection")
        .add_filter("Postman collection or Apidog request", &["json", "yaml", "yml"])
        .add_filter("Postman collection", &["json"])
        .add_filter("Apidog request", &["yaml", "yml"])
        .pick_files()
        .await
    else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    let mut apidog_files = apidog::Dir::default();
    for file in files {
        let bytes = file.read().await;
        let name = file.file_name();
        let text =
            String::from_utf8(bytes).map_err(|_| format!("{name} is not a UTF-8 text file."))?;
        let text = text.trim_start_matches('\u{feff}').to_string();
        if apidog::is_apidog_file(&name) {
            if apidog_files.name.is_empty() {
                apidog_files.name = file
                    .path()
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
            }
            apidog_files.files.push((name, text));
        } else {
            out.push(ImportSource::Postman(name, text));
        }
    }
    if !apidog_files.files.is_empty() {
        out.push(ImportSource::Apidog(apidog_files));
    }
    Ok(out)
}

/// Lets the user pick a folder of Apidog request files; sub-folders become folders.
async fn pick_apidog_folder() -> Result<Vec<ImportSource>, String> {
    let Some(folder) = rfd::AsyncFileDialog::new()
        .set_title("Import Apidog folder")
        .pick_folder()
        .await
    else {
        return Ok(Vec::new());
    };
    let path = folder.path().to_path_buf();
    let dir = apidog::read_dir(&path)?;
    Ok(vec![ImportSource::Apidog(dir)])
}

/// Whether the response is an HTML page.
fn is_html(resp: &http::Response) -> bool {
    if resp.is_binary || resp.body_raw.is_empty() {
        return false;
    }
    let kind = resp
        .header("content-type")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let start: String = resp
        .body_raw
        .trim_start()
        .chars()
        .take(15)
        .collect::<String>()
        .to_lowercase();
    kind.contains("html") || start.starts_with("<!doctype html") || start.starts_with("<html")
}

/// Makes a link found in a page absolute, using the page's own address.
fn resolve_link(base: Option<&str>, link: &str) -> String {
    let link = link.trim();
    if link.contains("://") {
        return link.to_string();
    }
    base.and_then(|b| reqwest::Url::parse(b).ok())
        .and_then(|b| b.join(link).ok())
        .map(|u| u.to_string())
        .unwrap_or_else(|| link.to_string())
}

/// Opens a web address or a file with the system's default program.
fn open_target(target: &str) -> Result<(), String> {
    use std::process::Command;

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", target]);
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg(target);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(target);
        c
    };

    command.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Opens the folder that holds `path`, selecting the file where the system supports it.
fn reveal_file(path: &std::path::Path) -> Result<(), String> {
    use std::process::Command;

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("explorer");
        c.arg(format!("/select,{}", path.display()));
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg("-R").arg(path);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let dir = path.parent().unwrap_or(path);
        let mut c = Command::new("xdg-open");
        c.arg(dir);
        c
    };

    command.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// A file name for the response: the one the server suggests, else the end of the URL.
fn response_file_name(resp: &http::Response) -> String {
    // Content-Disposition: attachment; filename="report.pdf"
    let from_header = resp.header("content-disposition").and_then(|value| {
        let lower = value.to_ascii_lowercase();
        let at = lower.find("filename=")?;
        let rest = value[at + 9..].trim_start();
        let name = match rest.strip_prefix('"') {
            Some(quoted) => quoted.split('"').next()?.to_string(),
            None => rest.split(';').next()?.trim().to_string(),
        };
        let name = safe_file_name(&name);
        (!name.is_empty()).then_some(name)
    });
    if let Some(name) = from_header {
        return name;
    }

    let extension = match &resp.media {
        Some(media) => media::extension(&media.bytes),
        None => {
            let kind = resp
                .header("content-type")
                .unwrap_or_default()
                .to_ascii_lowercase();
            if kind.contains("json") {
                "json"
            } else if kind.contains("html") {
                "html"
            } else if kind.contains("xml") {
                "xml"
            } else if kind.contains("csv") {
                "csv"
            } else if kind.contains("javascript") {
                "js"
            } else if kind.contains("css") {
                "css"
            } else if kind.contains("zip") {
                "zip"
            } else if resp.is_binary {
                "bin"
            } else {
                "txt"
            }
        }
    };
    let name = reqwest::Url::parse(&resp.final_url)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(|mut s| s.next_back().map(str::to_string))
        })
        .filter(|n| !n.is_empty() && n.len() < 100);
    match name {
        Some(n) if n.rsplit_once('.').is_some_and(|(_, e)| (1..=5).contains(&e.len())) => n,
        Some(n) => format!("{n}.{extension}"),
        None => format!("response.{extension}"),
    }
}

/// Asks where to save the body of a response and writes it there.
async fn save_media(
    name: String,
    bytes: std::sync::Arc<Vec<u8>>,
) -> Result<Option<String>, String> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("Save file")
        .set_file_name(name)
        .save_file()
        .await
    else {
        return Ok(None);
    };
    file.write(&bytes).await.map_err(|e| e.to_string())?;
    Ok(Some(file.file_name()))
}

/// Lets the user pick one of the files of a certificate.
async fn pick_cert_file(file: CertFile) -> Option<String> {
    let dialog = rfd::AsyncFileDialog::new();
    let dialog = match file {
        CertFile::Pfx => dialog
            .set_title("Choose the PFX / PKCS#12 certificate")
            .add_filter("Certificate bundle", &["pfx", "p12"]),
        CertFile::Cert => dialog
            .set_title("Choose the certificate")
            .add_filter("Certificate", &["crt", "cer", "pem"]),
        CertFile::Key => dialog
            .set_title("Choose the private key")
            .add_filter("Private key", &["key", "pem"]),
        CertFile::Ca => dialog
            .set_title("Choose the CA certificate")
            .add_filter("Certificate", &["crt", "cer", "pem", "der"]),
    }
    .add_filter("All files", &["*"]);
    dialog
        .pick_file()
        .await
        .map(|file| file.path().display().to_string())
}

/// Lets the user pick the file sent as a binary body.
async fn pick_body_file() -> Option<String> {
    rfd::AsyncFileDialog::new()
        .set_title("Choose the file to send")
        .pick_file()
        .await
        .map(|file| file.path().display().to_string())
}

/// Asks where to save the collection and writes it there.
async fn save_collection(file_name: String, json: String) -> Result<Option<String>, String> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("Export collection")
        .set_file_name(file_name)
        .add_filter("Postman collection", &["json"])
        .save_file()
        .await
    else {
        return Ok(None);
    };

    file.write(json.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    Ok(Some(file.file_name()))
}

/// Asks where to save the backup and writes it there.
async fn save_backup(file_name: String, json: String) -> Result<Option<String>, String> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("Export backup")
        .set_file_name(file_name)
        .add_filter("Oxide backup", &["json"])
        .save_file()
        .await
    else {
        return Ok(None);
    };
    file.write(json.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    Ok(Some(file.file_name()))
}

/// Lets the user pick a backup file and reads it.
async fn pick_backup() -> Result<Option<String>, String> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("Restore backup")
        .add_filter("Oxide backup", &["json"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    String::from_utf8(file.read().await)
        .map(Some)
        .map_err(|_| "The file is not a text file.".to_string())
}

/// A collection name that is safe to use as a file name on every platform.
fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim();
    if cleaned.is_empty() {
        "collection".to_string()
    } else {
        cleaned.to_string()
    }
}

fn import_notice(imported: &[postman::Summary], failures: &[String]) -> Notice {
    let mut lines = Vec::new();

    for summary in imported {
        lines.push(format!(
            "Imported \"{}\": {} requests, {} folders.",
            summary.name, summary.requests, summary.folders
        ));
    }

    let mut warnings: Vec<&str> = Vec::new();
    for summary in imported {
        for warning in &summary.warnings {
            if !warnings.contains(&warning.as_str()) {
                warnings.push(warning);
            }
        }
    }
    if !warnings.is_empty() {
        lines.push(format!("Not supported: {}.", warnings.join("; ")));
    }
    for failure in failures {
        lines.push(format!("Could not import {failure}"));
    }

    Notice {
        text: lines.join("\n"),
        error: imported.is_empty(),
    }
}
