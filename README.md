# Oxide

Oxide is a fast, lightweight desktop HTTP client in the spirit of Postman, written in Rust with [iced](https://iced.rs). It starts instantly, uses little memory, works offline and keeps everything on your machine: no account, no sign-in, no cloud sync.

Use it to build, send and inspect HTTP requests, organise them into collections, and reuse values between requests with variables.

## Features

### Requests
- Methods: GET, POST, PUT, PATCH, DELETE, OPTIONS and HEAD.
- Query params, headers and cookies edited in tables, with a checkbox to enable or disable each row.
- The URL and the params table stay in sync.
- Body types: none, form-data, x-www-form-urlencoded, JSON, XML, plain text and binary (send any file as the raw body).
- Syntax highlighting and tab indentation in the body editor.
- Multiple requests open at once in tabs, with unsaved-change markers.

### Authentication
- Bearer token, Basic auth and API key (as a header or a query parameter).
- **OAuth 2.0** with the Client Credentials and Password grants. The token is fetched automatically before the request is sent, stored, and renewed when it expires (using the refresh token when the server provides one). Client credentials can be sent as a Basic header or in the body.

### Certificates
- Add **client certificates** (mutual TLS) from the Certificates button at the bottom of the sidebar. Supports PFX / PKCS#12 (`.pfx`, `.p12`, with passphrase) and PEM (`.crt` + `.key`).
- Each certificate is tied to a host (`api.example.com`, `*.example.com`, or with a port such as `host:8443`) and is used automatically for matching requests, including OAuth 2.0 token requests.
- Optionally trust a private certificate authority (CA) for that host, useful for internal servers whose certificate is not from a public authority.
- Each entry shows right away whether its files could be read, and can be switched off without deleting it.

### cURL
- **Copy as cURL** from the right-click menu of a request or tab, or from the button above the request's tabs. Variables are replaced by their values.
- **Paste a cURL command into the URL box** and Oxide fills in the whole request: method, URL, query params, headers, cookies, body (JSON, XML, text, form, file) and Bearer or Basic auth. Multi-line commands copied from documentation or browser dev tools work too.

### Collections
- Collections, folders and requests in a sidebar tree.
- Drag and drop to reorder items or move them between folders, with a drop indicator while dragging.
- Rename, duplicate and delete from the right-click menu or with keyboard shortcuts.
- Show the data file's folder from the right-click menu.
- Import and export **Postman Collection v2.1**, including binary bodies and OAuth 2.0 settings.

### Variables
- `{{name}}` placeholders work in the URL, params, headers, cookies, body and auth.
- Global variables and per-collection variables (a collection variable wins over a global one with the same name).
- Variables are coloured inside the fields, and hovering one shows its current value.
- Autocomplete: type `{{` and pick from the defined variables.
- A small post-response script fills variables from the response, for example `token = body.data.access_token`, `global.user = header.X-User-Id` or `auth = "Bearer " + body.token`. Sources are the body (JSON path), headers, cookies, status and time.

### Responses
- Pretty (formatted and highlighted), Raw, collapsible **JSON tree** (with Expand all and Collapse all) and an **HTML preview** that renders headings, paragraphs, lists, tables and links.
- Open an HTML response in your default browser.
- **Image preview** (PNG, JPEG, GIF, WebP, BMP) and **PDF preview** with page navigation. Zoom in and out, fit an image to the panel, open the file in your default program or save it to disk.
- Status, time and size at a glance, plus response headers and cookies.
- Copy the response body, or **save it to a file** (text, images, PDFs and any other download; the file name comes from the server or the URL).

### Interface
- Dark and light themes.
- Resizable panels. The response panel can be docked at the bottom or on the right.
- The window layout, divider positions and open tabs are remembered between sessions.

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

## Your data

Everything is stored locally in `<Documents>/Oxide/`:

| File               | Contents                                      |
|--------------------|-----------------------------------------------|
| `collections.json` | Collections, folders and requests             |
| `globals.json`     | Global variables                              |
| `session.json`     | Open tabs and window layout           |
| `tokens.json`      | OAuth 2.0 tokens                              |
| `certificates.json`| Certificate entries (paths and passphrases)   |

- Creating, renaming, moving, duplicating and deleting items saves immediately.
- Edits *inside* a request (URL, params, body, auth, ...) are written when you press **Ctrl+S**. A dot next to the request name (sidebar and tab) means it has unsaved changes.
- Deleting always asks for confirmation (Enter confirms, Esc cancels).
- Certificate files are not copied: Oxide keeps the path to each file, so do not move or delete them. Passphrases are saved as plain text in `certificates.json`.
- To share or back up a collection, use **Export Collection** and send the resulting Postman `.json` file.
