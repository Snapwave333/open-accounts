import { Injectable, computed, signal } from '@angular/core';

export interface SystemStats {
  total_accounts: number;
  active_automations: number;
  pending_human_tasks: number;
  system_status: string;
  updated_at: string;
}

export interface AccountRecord {
  id: string;
  platform: string;
  username: string;
  email?: string | null;
  phone?: string | null;
  status: string;
  profile_metadata?: Record<string, unknown>;
  created_at: string;
  updated_at?: string;
  last_active?: string | null;
}

export interface HumanTask {
  id: string;
  account_id?: string | null;
  automation_id?: string | null;
  challenge_type: string;
  title: string;
  instructions: string;
  page_url?: string | null;
  status: string;
  created_at: string;
}

export interface FlowCardItem {
  id: string;
  kind: string;
  ref_id?: string | null;
  title: string;
  detail: string;
  tag: string;
  time_label: string;
  eta?: string | null;
  progress?: number | null;
  platform: string;
  account_handle: string;
  can_run_now: boolean;
}

export interface ActivityFlow {
  just_did: FlowCardItem[];
  actively_doing: FlowCardItem[];
  up_next: FlowCardItem[];
}

export interface EngagementQuota {
  account_id: string;
  account_handle: string;
  platform: string;
  action: string;
  used: number;
  cap: number;
}

export interface GarbageCollectionReport {
  executed_at: string;
  applied: boolean;
  free_gb: number;
  freed_this_run_gb: number;
  freed_total_gb: number;
  items_cleaned: number;
  logs_rotated: number;
  emulator_status: string;
  orphans_status: string;
  disk_pressure: boolean;
  openaccounts_buffers_scrubbed: number;
}

export interface StickBridgeState {
  bridge_version: string;
  status: string;
  stick_studio_root: string;
  pairings: unknown[];
  messages: unknown[];
  latest_episodes: unknown[];
  last_synced_at: string;
}

export interface VaultStatus {
  initialized: boolean;
  unlocked: boolean;
  persistent: boolean;
  secret_count: number;
  failed_attempts: number;
}

export interface SecretMetadata {
  id: string;
  account_id?: string | null;
  secret_type: string;
  label: string;
  created_at: string;
}

export interface ChatAttachment {
  id: string;
  name: string;
  size_label: string;
  mime_type: string;
  is_image: boolean;
  data_url?: string;
  text_content?: string;
}

export interface AgentChatMessage {
  id: string;
  sender: 'human' | 'agent' | 'error';
  author_label: string;
  timestamp: string;
  text: string;
  target_scope?: string;
  attachments: ChatAttachment[];
}

export interface AgentChatReply {
  reply: string;
  model: string;
  profile_id: string;
  tokens_used?: number | null;
}

export class BackendUnavailableError extends Error {
  constructor() {
    super('The OpenAccounts desktop backend is not running. Launch the app with `cargo tauri dev` to use live data.');
  }
}

function errorMessage(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  try {
    return JSON.stringify(err);
  } catch {
    return String(err);
  }
}

function timeLabel(date = new Date()): string {
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

@Injectable({
  providedIn: 'root',
})
export class TauriService {
  readonly backendConnected = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  lastError = signal<string | null>(null);
  loading = signal<boolean>(false);

  stats = signal<SystemStats | null>(null);
  accounts = signal<AccountRecord[]>([]);
  humanTasks = signal<HumanTask[]>([]);
  flow = signal<ActivityFlow>({ just_did: [], actively_doing: [], up_next: [] });
  quotas = signal<EngagementQuota[]>([]);
  gcReport = signal<GarbageCollectionReport | null>(null);
  bridgeState = signal<StickBridgeState | null>(null);
  vaultStatus = signal<VaultStatus | null>(null);
  vaultSecrets = signal<SecretMetadata[]>([]);
  agentChatMessages = signal<AgentChatMessage[]>([]);
  chatPending = signal<boolean>(false);
  isAutonomousLoopRunning = signal<boolean>(false);
  killSwitchActive = signal<boolean>(false);

  vaultUnlocked = computed(() => this.vaultStatus()?.unlocked ?? false);
  justDidItems = computed(() => this.flow().just_did);
  activelyDoingItems = computed(() => this.flow().actively_doing);
  planToDoNextItems = computed(() => this.flow().up_next);

  private fastTimer?: ReturnType<typeof setInterval>;
  private slowTimer?: ReturnType<typeof setInterval>;

  constructor() {
    if (this.backendConnected) {
      void this.refreshAll();
      this.fastTimer = setInterval(() => void this.refreshLive(), 5000);
      this.slowTimer = setInterval(() => void this.refreshAll(), 30000);
    } else {
      this.lastError.set(new BackendUnavailableError().message);
    }
  }

  async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    if (!this.backendConnected) {
      throw new BackendUnavailableError();
    }
    const { invoke } = await import('@tauri-apps/api/core');
    try {
      return await invoke<T>(cmd, args);
    } catch (err) {
      throw new Error(errorMessage(err));
    }
  }

  private async load<T>(cmd: string, apply: (value: T) => void, args?: Record<string, unknown>): Promise<void> {
    try {
      apply(await this.invoke<T>(cmd, args));
    } catch (err) {
      this.reportError(`${cmd}: ${errorMessage(err)}`);
    }
  }

  reportError(message: string) {
    console.error(message);
    this.lastError.set(message);
  }

  clearError() {
    this.lastError.set(null);
  }

  async refreshLive(): Promise<void> {
    await Promise.all([
      this.load<SystemStats>('get_system_stats', (v) => this.stats.set(v)),
      this.load<HumanTask[]>('list_human_tasks', (v) => this.humanTasks.set(v)),
      this.load<ActivityFlow>('get_activity_flow', (v) => this.flow.set(v)),
      this.load<boolean>('is_kill_switch_active', (v) => this.killSwitchActive.set(v)),
      this.load<boolean>('is_autonomous_marketing_loop_running', (v) => this.isAutonomousLoopRunning.set(v)),
    ]);
  }

  async refreshAll(): Promise<void> {
    this.loading.set(true);
    try {
      await Promise.all([
        this.refreshLive(),
        this.load<AccountRecord[]>('list_accounts', (v) => this.accounts.set(v)),
        this.load<EngagementQuota[]>('get_engagement_quotas', (v) => this.quotas.set(v)),
        this.load<GarbageCollectionReport | null>('get_last_garbage_collection', (v) => this.gcReport.set(v)),
        this.load<StickBridgeState>('get_stick_bridge_state', (v) => this.bridgeState.set(v)),
        this.refreshVault(),
      ]);
    } finally {
      this.loading.set(false);
    }
  }

  async refreshVault(): Promise<void> {
    await this.load<VaultStatus>('get_vault_status', (v) => this.vaultStatus.set(v));
    if (this.vaultUnlocked()) {
      await this.load<SecretMetadata[]>('list_vault_secrets', (v) => this.vaultSecrets.set(v));
    } else {
      this.vaultSecrets.set([]);
    }
  }

  private async act(cmd: string, args?: Record<string, unknown>, after?: () => Promise<void>): Promise<boolean> {
    try {
      await this.invoke(cmd, args);
      this.clearError();
      await (after ? after() : this.refreshLive());
      return true;
    } catch (err) {
      this.reportError(errorMessage(err));
      return false;
    }
  }

  resolveTask(id: string) {
    return this.act('resolve_human_task', { task_id: id });
  }

  dismissTask(id: string) {
    return this.act('dismiss_human_task', { task_id: id });
  }

  toggleKillSwitch() {
    return this.act(this.killSwitchActive() ? 'reset_kill_switch' : 'trigger_kill_switch');
  }

  toggleMarketingLoop() {
    return this.act(this.isAutonomousLoopRunning() ? 'pause_autonomous_marketing_loop' : 'start_autonomous_marketing_loop');
  }

  runFlowItem(item: FlowCardItem) {
    return this.act('run_flow_item', { kind: item.kind, ref_id: item.ref_id ?? null });
  }

  createAccount(platform: string, username: string, email: string | null) {
    return this.act('create_account', { platform, username, email }, () => this.refreshAll());
  }

  async triggerGarbageCollection(): Promise<void> {
    try {
      this.gcReport.set(await this.invoke<GarbageCollectionReport>('run_garbage_collection', { apply: true }));
      this.clearError();
    } catch (err) {
      this.reportError(errorMessage(err));
    }
  }

  async unlockVault(password: string): Promise<string | null> {
    try {
      await this.invoke('unlock_vault', { password });
      await this.refreshVault();
      return null;
    } catch (err) {
      await this.refreshVault();
      return errorMessage(err);
    }
  }

  lockVault() {
    return this.act('lock_vault', undefined, () => this.refreshVault());
  }

  storeSecret(label: string, secretType: string, value: string, accountId: string | null) {
    return this.act(
      'store_vault_secret',
      { label, secret_type: secretType, value, account_id: accountId },
      () => this.refreshVault()
    );
  }

  deleteSecret(id: string) {
    return this.act('delete_vault_secret', { secret_id: id }, () => this.refreshVault());
  }

  async revealSecret(id: string): Promise<string | null> {
    try {
      return await this.invoke<string>('reveal_vault_secret', { secret_id: id });
    } catch (err) {
      this.reportError(errorMessage(err));
      return null;
    }
  }

  async sendAgentChatMessage(text: string, targetScope: string, attachments: ChatAttachment[]) {
    const humanMsg: AgentChatMessage = {
      id: `chat-${Date.now()}`,
      sender: 'human',
      author_label: 'You',
      timestamp: timeLabel(),
      text: text || `Attached ${attachments.length} file(s)`,
      target_scope: targetScope,
      attachments,
    };
    this.agentChatMessages.update((prev) => [...prev, humanMsg]);

    const history = this.agentChatMessages()
      .filter((m) => m.sender !== 'error' && m.id !== humanMsg.id)
      .slice(-12)
      .map((m) => ({ role: m.sender === 'human' ? 'user' : 'assistant', content: m.text }));

    this.chatPending.set(true);
    try {
      const reply = await this.invoke<AgentChatReply>('agent_chat', {
        message: text,
        scope: targetScope,
        history,
        attachments: attachments.map((a) => ({
          name: a.name,
          mime_type: a.mime_type,
          size_label: a.size_label,
          text_content: a.text_content ?? null,
        })),
      });
      this.agentChatMessages.update((prev) => [
        ...prev,
        {
          id: `chat-reply-${Date.now()}`,
          sender: 'agent',
          author_label: `OpenAccounts Agent · ${reply.model}`,
          timestamp: timeLabel(),
          text: reply.reply,
          target_scope: targetScope,
          attachments: [],
        },
      ]);
    } catch (err) {
      this.agentChatMessages.update((prev) => [
        ...prev,
        {
          id: `chat-error-${Date.now()}`,
          sender: 'error',
          author_label: 'Agent unavailable',
          timestamp: timeLabel(),
          text: errorMessage(err),
          attachments: [],
        },
      ]);
    } finally {
      this.chatPending.set(false);
    }
  }
}
