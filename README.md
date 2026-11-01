# OpenAccounts — LM-Agnostic Account Automation Platform

OpenAccounts is a desktop application designed to create, manage, secure, and automate online accounts through a unified console. It is built as a provider-independent orchestration engine where the user controls which language model powers each task, while maintaining offline security, strict permission gating, and human-in-the-loop checkpoints.

---

## 🏛️ Core Product Architecture

```
                    ┌───────────────────────────────────────────────┐
                    │               Angular Desktop UI              │
                    │   Dashboard · Accounts · Creator · Automations │
                    │    Approvals · Chrome Relay · Vault · Models  │
                    └───────────────────────┬───────────────────────┘
                                            │ Tauri 2 IPC
                    ┌───────────────────────▼───────────────────────┐
                    │                Rust Core Engine               │
                    │  Orchestration · Permissions · Task Scheduler │
                    │     Audit Trail · Checkpointing · Event Bus   │
                    └───┬─────────────┬─────────────┬─────────────┬─┘
                        │             │             │             │
        ┌───────────────▼─┐   ┌───────▼───────┐   ┌─▼───────────┐ ┌▼──────────────┐
        │   LM Gateway    │   │ Chrome Relay  │   │ Encrypted   │ │  Human Gate   │
        │ Ollama, Gemini, │   │ Snapwave333   │   │ Vault       │ │ CAPTCHAs, 2FA │
        │ OpenAI, Claude, │   │ Native Host,  │   │ Argon2id +  │ │ Legal Consent │
        │ NIM, OpenRouter │   │ Tab Leases    │   │ AES-256-GCM │ │ Manual Review │
        └─────────────────┘   └───────────────┘   └─────────────┘ └───────────────┘
```

---

## 📦 Workspace Architecture & Crates

| Crate / Module | Path | Purpose |
| :--- | :--- | :--- |
| **`openaccounts-core`** | `crates/core` | Common domain types, error handling (`OpenAccountsError`), IDs, risk levels, and actions. |
| **`openaccounts-database`** | `crates/database` | SQLite persistence pool, automated migrations (`accounts`, `automations`, `audit_logs`, `human_tasks`). |
| **`openaccounts-credential-vault`** | `crates/credential-vault` | Encrypted offline credential store utilizing **Argon2id** key derivation and **AES-256-GCM** cipher with memory zeroization. |
| **`openaccounts-lm-gateway`** | `crates/lm-gateway` | Provider-agnostic gateway supporting **Ollama, OpenAI-compatible, Google Gemini, Anthropic, NVIDIA NIM, and OpenRouter**. |
| **`openaccounts-browser-abstraction`** | `crates/browser-abstraction` | Common trait interface (`BrowserProvider`) for headless, embedded, and Chrome Relay sessions. |
| **`openaccounts-chrome-relay-adapter`** | `crates/chrome-relay-adapter` | First-class integration adapter for **Snapwave333/opencode-chrome-relay** with tab leases & origin fencing. |
| **`openaccounts-human-intervention`** | `crates/human-intervention` | Human Gate state machine: pauses automation at CAPTCHAs, Terms of Service, and 2FA prompts for user takeover. |
| **`openaccounts-permission-engine`** | `crates/permission-engine` | Granular permission checking by account, platform, action type, and risk level (`Allowed`, `ApprovalRequired`, `Denied`). |
| **`openaccounts-automation-engine`** | `crates/automation-engine` | Workflow runtime, rule executor, audit logger, and **Emergency Global Kill Switch**. |
| **`openaccounts-workflow-runtime`** | `crates/workflow-runtime` | Step graphs, checkpoint persistence, and fault-tolerant resumption. |
| **`openaccounts-scheduler`** | `crates/scheduler` | Interval and cron-based task scheduling with active-window restrictions. |
| **`openaccounts-audit-engine`** | `crates/audit-engine` | Tamper-evident execution journal logging execution IDs, timestamps, and outcomes. |
| **`openaccounts-account-registry`** | `crates/account-registry` | Account CRUD, metadata store, and platform mapping. |
| **`openaccounts-account-creator`** | `crates/account-creator` | Guided and Autonomous account registration workflows with credential generation. |
| **`openaccounts-platform-connectors`** | `crates/platform-connectors` | Official API clients for X v2, YouTube Data v3, Reddit OAuth and Instagram Graph (likes, follows, replies, publishing, inbox, metrics). |
| **`openaccounts-desktop`** | `apps/desktop/src-tauri` | Tauri 2 application providing native IPC command handlers for the Angular UI. |
| **`angular-ui`** | `apps/desktop/angular` | Modern desktop console built with Angular 22 standalone components and responsive dark theme. |

---

## 🚀 Getting Started

### Prerequisites
- **Node.js**: v20+ (v24 tested)
- **Rust & Cargo**: 1.80+ (1.99 tested)
- **Tauri CLI**: 2.x

### Running the Application

1. **Development Mode (Browser & Web Mock)**:
   ```bash
   cd apps/desktop/angular
   npm start
   ```
   The browser-only dev server renders the UI shell but shows a "backend not running" banner: every screen reads live data from the Rust engine, so use native mode below for real data.

2. **Native Desktop Mode (Tauri 2 + Angular + Rust Engine)**:
   ```bash
   cd apps/desktop
   cargo tauri dev
   ```

3. **Building Production Binaries**:
   ```bash
   # Build Angular frontend
   cd apps/desktop/angular
   npm run build

   # Build Native Desktop executable (.exe)
   cd ../src-tauri
   cargo tauri build
   ```

---

## 🤖 What The Agents Do

| Capability | How it works |
| :--- | :--- |
| **Agent Chat** | Answers from your chosen LM profile using live app state (accounts, tasks, inbox, jobs, audit log). It can propose whitelisted actions (sync inbox, refresh metrics, policy check, run a job, pause/resume autopilot, research trends) that run only when you click them, respect the kill switch, and are audit-logged. |
| **Models** | Pick the chat model in Agent Chat or add Anthropic, OpenAI-compatible, Gemini, OpenRouter, Ollama or NIM profiles; profiles persist across restarts and failures list every model tried. |
| **Compose & Autopilot** | Draft captions in your brand voice, schedule posts, and approve them from Activity Flow or Needs You. The autopilot publishes only approved, due posts, never twice, and defers posts once an account hits its rolling 24h post cap. |
| **Engagement** | Inbox sync from connected platforms, LM triage with fixed legal/security escalation, reply drafting, per-account daily action caps and duplicate suppression; history and inbox persist so caps survive restarts. |
| **Guided Sign-up** | Real policy checks, vaulted generated password, real sign-up page in a DevTools-driven browser, human checkpoint, then verification-email confirmation. |
| **Needs You** | One place for CAPTCHAs, consent screens, replies to review, posts to approve and sign-ups in progress; the window title shows the waiting count. |

## 🧪 Testing

| Command | Covers |
| :--- | :--- |
| `cargo test --workspace` | Unit and integration tests for every crate, a full `AppState` boot/restart test, and IPC round-trips of the exact payloads the UI sends |
| `cd apps/desktop/angular && npm run e2e` | Builds the UI and drives it in headless Chromium against the real backend through Tauri's IPC handlers (accounts, vault, activity flow, kill switch, agent chat) |

Browser-driven tests use a local Chromium (`OPENACCOUNTS_CHROME_PATH` or Playwright's browser) and skip cleanly when none is installed; render tests use `ffmpeg`.

## 🔌 Connecting Real Services

Nothing in OpenAccounts fakes success. When a service is not connected, the action fails with a clear "not connected" error instead of pretending to work. Provide credentials through environment variables (or the Windows user environment):

| Service | Variables | Used for |
| :--- | :--- | :--- |
| NVIDIA NIM | `NVIDIA_API_KEY`, optional `OPENACCOUNTS_NIM_CASCADE` (path to a ranked `cascade.json`) | Default LM cascade for agent chat, triage and reply drafting |
| Anthropic / OpenAI / Gemini / OpenRouter | `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `OPENROUTER_API_KEY` | Additional LM profiles |
| X | `X_ACCESS_TOKEN` (OAuth 2.0 user token) | Likes, follows, reposts, replies, posts, mentions, metrics |
| YouTube | `YOUTUBE_ACCESS_TOKEN`, optional `YOUTUBE_API_KEY` | Ratings, subscriptions, comment replies, uploads, channel stats, trending |
| Reddit | `REDDIT_ACCESS_TOKEN`, `REDDIT_USER_AGENT`, optional `REDDIT_DEFAULT_SUBREDDIT` | Votes, comments, subscriptions, submissions, inbox |
| Instagram | `INSTAGRAM_ACCESS_TOKEN`, `INSTAGRAM_USER_ID` | Comment replies, Reels/feed publishing (public media URLs), insights |
| Gmail / Outlook | `GMAIL_ACCESS_TOKEN`, `OUTLOOK_ACCESS_TOKEN` | Plus-address identities and verification-email polling during sign-up |
| Browser | `OPENACCOUNTS_CHROME_PATH` | Chromium/Chrome binary for the embedded DevTools-driven browser |
| Storage | `OPENACCOUNTS_DATA_DIR`, `OPENACCOUNTS_ARTIFACTS_DIR` | SQLite database and rendered media locations |
| StickExplainers bridge | `OPENACCOUNTS_STICK_ROOT` | Studio folder for the bridge and garbage collector |

Creative rendering uses the `ffmpeg`/`ffprobe` binaries on your `PATH`; other creative tools are detected at startup and shown as unavailable when missing.

## 🔒 Security Principles
- **Credential Isolation**: Models never receive raw master keys or unapproved passwords.
- **Local-First**: Vault, human tasks, policy snapshots and the audit journal reside in local SQLite storage.
- **Tamper-Evident Audit Journal**: Every audit entry is SHA-256 hash-chained to the previous one; `verify_audit_chain` detects edits or deletions.
- **Verified Master Password**: The vault stores an encrypted verifier, so a wrong password is rejected (with back-off) instead of silently deriving a different key.
- **Argon2id + AES-256-GCM**: Military-grade key derivation and encryption with zeroized buffers upon lock.
- **Emergency Kill Switch**: Instant global halt halts any active browser session and workflow task queue.
