# Dieah Main

Production-ready app scaffold for Dieah (Tauri 2 + Rust).

## Structure
- `ui/`: Frontend assets (wizard screen currently in `ui/index.html`).
- `design/`: Design tokens (source of truth for colors, fonts, radii).
- `src-tauri/`: Rust backend + Tauri config.

## Dev (local)
1. Serve the UI:
   `python3 -m http.server 1420 --directory ui`
2. Install Tauri CLI (once):
   `cargo install tauri-cli`
3. Run the app:
   `cargo tauri dev`

Note: the UI currently uses Tailwind and Google Fonts via CDN for speed. We can bundle these locally for production.
