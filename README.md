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
   Open `http://localhost:4200` to interact with the full operations console with simulated Tauri IPC.

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

## 🔒 Security Principles
- **Credential Isolation**: Models never receive raw master keys or unapproved passwords.
- **Local-First**: Vault and audit journals reside in local SQLite storage.
- **Argon2id + AES-256-GCM**: Military-grade key derivation and encryption with zeroized buffers upon lock.
- **Emergency Kill Switch**: Instant global halt halts any active browser session and workflow task queue.
