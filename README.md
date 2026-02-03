# Dieah

Dieah is a multi‑agent orchestration desktop app built with Rust + Tauri. It connects to an existing gateway, surfaces agents, and provides a fast, local‑first chat workspace with memory tools.

## Features
- Multi‑agent workspace with per‑agent chat
- Topic tabs per agent (separate session keys)
- Streaming responses with smooth token rendering
- Thinking blocks + tool call visualization
- Markdown + code blocks with one‑click copy
- Live context gauge (base + “actual” with memory boost)
- Auto‑summary when context hits 90%
- Memory injection before every send (local service)
- Gateway connect + token handling + auto‑reconnect
- Auto‑restore last agent on launch
- Installed skills list + curated marketplace view

## What’s in this repo
- `dieah-main/` — desktop app (Tauri + Rust backend + UI)
- `dieah-memory/` — optional local memory service
- `.env.example` — optional overrides (not required)

## System requirements
- Rust (stable)
- Tauri prerequisites (platform toolchain)
- A running gateway + CLI on the same machine

## Quick start (local dev)

### 1) Start the UI server
The app expects a frontend server on port `1420`.

```bash
cd dieah-main/ui
python3 -m http.server 1420
```

### 2) (Optional) Start the memory service
```bash
cd dieah-memory
cargo run
```

### 3) Start the desktop app
```bash
cd dieah-main/src-tauri
cargo tauri dev
```

You should see the app launch into **Overview**.

## Connect to an existing agent (first‑time)
1) Open **Agent Config** from the left sidebar.
2) Select your profile in the profile dropdown.
3) Get a tokenized dashboard URL from the CLI:
   - Default profile:
     ```bash
     openclaw dashboard --no-open
     ```
   - Named profile:
     ```bash
     openclaw --profile <profile> dashboard --no-open
     ```
4) In **Agent Config → Gateway**, click **Use profile token** (or paste the token manually).
5) Click **Connect**. Once connected, the config will collapse into **Agents Live**.
6) Click **Agents** in the left sidebar, then select your agent.
7) Start a chat in the center panel. Use **+** to open new topic tabs.

Once connected successfully, the app will auto‑reconnect on next launch.

## Create a new agent
1) Open **Agent Config**.
2) Click **New Agent** (this reopens the config panel).
3) In the **Agents** view, click **+** to create a new agent.

## Memory service (recommended)
1) Start `dieah-memory` (see step 2 above).
2) Go to **Settings → Memory & Context**.
3) Enable memory and set the URL (default: `http://127.0.0.1:8420`).
4) Save settings. The chat view will now show the **Actual Context** bar and auto‑summary behavior.

## Skills tab
- **Curated** is the default (recommended set).
- **Show All** exposes the full catalog once the marketplace API is wired.
- **Installed** shows what the gateway currently has enabled.

## Troubleshooting
- **Gateway token mismatch**: Use the CLI `dashboard --no-open` command for the active profile and paste the token.
- **No agents listed**: Confirm the gateway is running, then click **Refresh** in Agent Config.
- **Blank Dock icon**: Quit the app fully and relaunch after icon generation.
- **UI not loading**: Confirm the UI server is running on port `1420`.

## Optional environment overrides
You can set overrides with `.env` if you want, but it is not required.
Use `.env.example` as a template and keep `.env` untracked.

## License
MIT
