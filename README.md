# Oxide

A lightweight Postman-style HTTP client written in Rust with [iced](https://iced.rs) 0.14.

## Run

```sh
cargo run --release
```

## Shortcuts

| Action              | Shortcut        |
|---------------------|-----------------|
| New request         | Ctrl+N          |
| New folder          | Ctrl+Shift+N    |
| Save request        | Ctrl+S          |
| Duplicate           | Ctrl+D          |
| Rename              | F2              |
| Delete              | Del             |
| Go to tab *n*       | Ctrl+1 … Ctrl+9 |
| Close current tab   | Ctrl+W          |

(Cmd instead of Ctrl on macOS.) Right-click items in the sidebar or on tabs for the context menus.

## Saving

Collections live in `<Documents>/Oxide/collections.json`.

- Creating, renaming, duplicating and deleting items saves immediately.
- Edits *inside* a request (URL, params, body, auth, ...) are only written when you press **Ctrl+S**. A dot next to the request name (sidebar and tab) means it has unsaved changes.
- Deleting always asks for confirmation (Enter confirms, Esc cancels).

## Icon

`assets/oxide.ico` is compiled into the binary (window icon, via `include_bytes!`) and, on Windows, embedded into the `.exe` by `build.rs`. Replace the file with your own icon (include a 256x256 entry).

## Layout

- `src/model.rs`  collection tree + request definition (unit-tested)
- `src/http.rs`   request building / sending / response parsing
- `src/app.rs`    state, messages, `update`, keyboard shortcuts
- `src/view.rs`   every widget
- `src/storage.rs` load/save of `collections.json`
- `src/theme.rs`  the Oxide dark theme
