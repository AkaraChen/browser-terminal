# Browser Terminal

A small local terminal server that lets the browser act as a terminal window manager.

The Rust backend serves a single HTML page and forwards WebSocket traffic to an
independent PTY session. The frontend uses Xterm.js, reads a `channel` query
parameter, and opens each new browser tab as a separate terminal session.

## Features

- Browser terminal UI powered by Xterm.js
- Rust WebSocket backend using Axum
- One PTY session per WebSocket connection
- `?channel=<id>` URL-based session identity
- New-tab button for opening a fresh terminal session
- Settings dialog for terminal font and font size
- Nerd Font loading via `@xterm/addon-web-fonts`
- Settings persisted in `localStorage`
- Live settings sync across open tabs via the browser `storage` event
- Window title sync from Xterm title updates
- Dynamic document icon for Claude, Codex, OpenCode, Qoder, Amp, Cline,
  Copilot, Cursor, Kilo Code, and Kimi terminal titles
- Tailwind CDN for compact page styling

## Run

```sh
cargo run
```

Then open:

```text
http://127.0.0.1:3000/?channel=main
```

The server prints Basic Auth credentials when it starts. The default username is
`admin`; if no fixed password is configured, a random password is generated for
that run.

Use a different port with:

```sh
cargo run -- --port 3100
```

Bind to a different host, or allow a separate frontend origin:

```sh
cargo run -- --host 0.0.0.0 --port 3100 --cors-origin http://localhost:5173
```

`HOST`, `PORT`, `CORS_ORIGIN`, and `DANGEROUS_ALLOW_ALL_HOST` environment
variables are also supported.

When `--cors-origin` is omitted, only loopback hosts and loopback origins on the
server port are allowed, such as `http://127.0.0.1:3000` or
`http://localhost:3000`. The same origin policy is also applied to WebSocket
handshakes.

To allow arbitrary `Host` and `Origin` headers, pass the explicit dangerous
flag:

```sh
cargo run -- --dangerous-allow-all-host
```

Basic Auth remains enabled in this mode.

Use `~/.browser-terminalrc` to pin a fixed Basic Auth password:

```text
username = admin
password = change-this-password
```

`user` is accepted as an alias for `username`, and `basic_auth_password` is
accepted as an alias for `password`.

## Notes

The server binds to `127.0.0.1` by default and is intended for local use. It
does not include TLS, so do not expose it directly to the public internet.

Shells start in the user's home directory. The PTY implementation uses
`portable-pty`, and the home directory is resolved with the cross-platform
`dirs` crate.

## Tech Stack

- Rust
- Axum
- Tokio
- portable-pty
- Xterm.js
- Xterm.js Web Fonts addon
- Tailwind CSS CDN
- LobeHub Icons static SVG CDN
- Nerd Fonts via jsDelivr
