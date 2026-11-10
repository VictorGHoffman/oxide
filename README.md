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
| Duplicate           | Ctrl+D          |
| Rename              | F2              |
| Delete              | Del             |
| Go to tab *n*       | Ctrl+1 … Ctrl+9 |
| Close current tab   | Ctrl+W          |

(Cmd instead of Ctrl on macOS.) Right-click items in the sidebar or on tabs for the context menus.

## Layout

- `src/model.rs`  collection tree + request definition (unit-tested)
- `src/http.rs`   request building / sending / response parsing
- `src/app.rs`    state, messages, `update`, keyboard shortcuts
- `src/view.rs`   every widget
- `src/theme.rs`  the Oxide dark theme
