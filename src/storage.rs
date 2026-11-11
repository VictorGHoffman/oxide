//! Persistence: `<Documents>/Oxide/collections.json`.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::{Id, KeyValue, Tree};
use crate::oauth::Token;

pub fn file_path() -> Option<PathBuf> {
    let documents =
        dirs::document_dir().or_else(|| dirs::home_dir().map(|h| h.join("Documents")))?;
    Some(documents.join("Oxide").join("collections.json"))
}

/// `Ok(None)` when nothing has been saved yet.
///
/// If the file exists but cannot be parsed it is moved to `collections.json.bak`
/// (so the next save cannot destroy it) and an error is returned.
pub fn load() -> Result<Option<Tree>, String> {
    let Some(path) = file_path() else {
        return Err("Could not find the Documents folder.".into());
    };
    if !path.exists() {
        return Ok(None);
    }

    let text =
        fs::read_to_string(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    match serde_json::from_str::<Tree>(&text) {
        Ok(tree) => Ok(Some(tree)),
        Err(e) => {
            let backup = path.with_extension("json.bak");
            let _ = fs::rename(&path, &backup);
            Err(format!(
                "Could not parse collections.json ({e}). The old file was kept as {}.",
                backup.display()
            ))
        }
    }
}

/// Writes atomically (temp file + rename) so a crash cannot leave a half-written file.
pub fn save(tree: &Tree) -> Result<(), String> {
    let path = file_path().ok_or("Could not find the Documents folder.")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }

    let json = serde_json::to_string_pretty(tree).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("Could not write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("Could not save {}: {e}", path.display()))
}

// ───────────────────────────── global variables ─────────────────────────────

fn globals_path() -> Option<PathBuf> {
    file_path().map(|p| p.with_file_name("globals.json"))
}

/// The global variables (empty when none were saved yet or the file is unreadable).
pub fn load_globals() -> Vec<KeyValue> {
    globals_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_globals(globals: &[KeyValue]) -> Result<(), String> {
    let path = globals_path().ok_or("Could not find the Documents folder.")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(globals).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("Could not write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("Could not save {}: {e}", path.display()))
}

// ───────────────────────────── certificates ─────────────────────────────

fn certs_path() -> Option<PathBuf> {
    file_path().map(|p| p.with_file_name("certificates.json"))
}

pub fn load_certs() -> Vec<crate::certs::ClientCert> {
    certs_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_certs(certs: &[crate::certs::ClientCert]) -> Result<(), String> {
    let path = certs_path().ok_or("Could not find the Documents folder.")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(certs).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("Could not write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("Could not save {}: {e}", path.display()))
}

// ───────────────────────────── OAuth 2.0 tokens ─────────────────────────────

fn tokens_path() -> Option<PathBuf> {
    file_path().map(|p| p.with_file_name("tokens.json"))
}

/// The OAuth 2.0 tokens obtained so far, by request id.
pub fn load_tokens() -> HashMap<Id, Token> {
    tokens_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_tokens(tokens: &HashMap<Id, Token>) -> Result<(), String> {
    let path = tokens_path().ok_or("Could not find the Documents folder.")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(tokens).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("Could not write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("Could not save {}: {e}", path.display()))
}

// ───────────────────────────── session (open tabs) ─────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTab {
    pub id: Id,
    pub pinned: bool,
}

/// Sizes and positions of the panels (whole pixels, so sessions compare cheaply).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layout {
    pub sidebar_visible: bool,
    pub sidebar_width: u32,
    /// `true` = response docked on the right.
    pub response_right: bool,
    pub response_width: u32,
    pub response_height: u32,
}

/// What was open when the app was last used.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub tabs: Vec<SessionTab>,
    pub active: Option<Id>,
    /// Name of the selected theme.
    #[serde(default)]
    pub theme: Option<String>,
    /// Panel sizes and the response dock position.
    #[serde(default)]
    pub layout: Option<Layout>,
}

fn session_path() -> Option<PathBuf> {
    file_path().map(|p| p.with_file_name("session.json"))
}

pub fn load_session() -> Option<Session> {
    let text = fs::read_to_string(session_path()?).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_session(session: &Session) -> Result<(), String> {
    let path = session_path().ok_or("Could not find the Documents folder.")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(session).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("Could not write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("Could not save {}: {e}", path.display()))
}
