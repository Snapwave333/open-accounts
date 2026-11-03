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
| **`openaccounts-desktop`** | `apps/desktop/src-tauri` | Tauri 2 application providing native IPC command handlers for the Angular UI. |
| **`angular-ui`** | `apps/desktop/angular` | Command-center UI: shell + seven standalone view components (`src/app/views`), services for the runtime and the CEO chat, one design system (`styles.scss`). |

---

## 🖥️ Desktop Command Center

The dashboard is a native Tauri 2 window (Angular 22 inside WebView2, Rust backend). No browser is involved: the
Angular bundle is embedded in the executable (`custom-protocol`), the window remembers its size, position and
maximized state (`window_state.json` next to the database), and the active view, campaign selection and chat draft
persist across launches (`ui_state.json`). The database is found from the working directory, the checkout the
executable lives in, or `OPENACCOUNTS_DATA_DIR`, so launching from Explorer and from the shortcut open the same data.

Views (Ctrl+1 … Ctrl+7):

| View | What it shows | Source |
| :--- | :--- | :--- |
| **Overview** | Needs-attention list, live job, last-cycle outcome, campaign and account health, latest CEO reply | runtime status, cycle ledger, campaign overviews |
| **Activity** | Just did / Running now / Up next, filterable by campaign, plus the cycle ledger with per-job exit codes | `autonomy_cycles` (SQLite) |
| **Campaigns** | Every registered campaign: accounts, agents, progress, metrics, issues/blocked work, execution history, calendar/episodes, bridge traffic, brief and brand | `content-strategy` + `brand-manager` crates, account registry, cycle ledger, Nova `data/*.json`, studio `insights/*.json` |
| **CEO Chat** | Streamed, persistent conversation with V. Sterling; tool runs shown inline; confirm/cancel gated actions | `StickExplainers/ceo_chat.py` |
| **Needs You** | Human gates (CAPTCHAs, sign-ins, Nova launch gates) and pending CEO actions | human-intervention controller + `influencer/data/needs_you.json` |
| **Accounts** | Registry grouped by campaign | `accounts` (SQLite) |
| **System** | Runtime wiring, vault, Chrome relay, GC, studio bridge pairings and dispatch | respective engines |

### CEO communication

`StickExplainers/ceo_chat.py` is the one CEO agent. The desktop app (`apps/desktop/src-tauri/src/ceo.rs`) and the
Telegram bot (`StickExplainers/telegram_bot.py`) both call it, and both read and write the same conversation file
(`StickExplainers/insights/ceo/conversation.json`), so a thread started on Telegram continues in the app and vice
versa. Desktop turns stream: the service prints JSONL events which the Rust side forwards as `ceo://event`. Tools
that change things pause for a confirmation that either channel can give. The Telegram bot answers in plain
conversational text; inline buttons appear only for real decisions (confirm/cancel, send/skip a comment draft) and
the previous keyboard is removed when a new one is shown.

## 🚀 Getting Started

### Prerequisites
- **Node.js**: v20+ (v24 tested)
- **Rust & Cargo**: 1.80+ (1.99 tested)
- **Tauri CLI**: 2.x
- **Python**: the `py` launcher resolving to the interpreter that owns the StickExplainers dependencies

### Running the Application

1. **Native desktop (release)** — the Desktop shortcut runs this:
   ```bash
   cd apps/desktop/angular && npm run build
   cd ../../.. && cargo build --release -p openaccounts-desktop --features custom-protocol
   target/release/openaccounts-desktop.exe
   ```

2. **Native desktop (dev, hot reload)**:
   ```bash
   cd apps/desktop/src-tauri
   cargo tauri dev
   ```
   (`beforeDevCommand` starts the Angular dev server; the Tauri window loads it. Opening `http://localhost:4200`
   in a browser shows the UI without a backend and is not a supported mode.)

### Tests

```bash
cargo test --workspace                                  # Rust crates (bridge, engines)
cd ../StickExplainers && py -m pytest tests/test_ceo_chat.py -q   # shared CEO service + Telegram keyboard rules
```

## 🔒 Security Principles
- **Credential Isolation**: Models never receive raw master keys or unapproved passwords.
- **Local-First**: Vault and audit journals reside in local SQLite storage.
- **Argon2id + AES-256-GCM**: Military-grade key derivation and encryption with zeroized buffers upon lock.
- **Emergency Kill Switch**: Instant global halt halts any active browser session and workflow task queue.
