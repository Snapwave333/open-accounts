import { Injectable, WritableSignal, computed, signal } from '@angular/core';

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

export interface AgentAction {
  kind: string;
  label: string;
  args: Record<string, unknown>;
  state?: 'idle' | 'running' | 'done' | 'failed';
}

export interface AgentChatMessage {
  id: string;
  sender: 'human' | 'agent' | 'error';
  author_label: string;
  timestamp: string;
  text: string;
  target_scope?: string;
  attachments: ChatAttachment[];
  actions?: AgentAction[];
}

export interface ConnectionStatus {
  id: string;
  service: string;
  kind: 'social' | 'email' | 'lm' | string;
  connected: boolean;
  detail: string;
}

export interface InboxMessage {
  message_id: string;
  account_id: string;
  platform: string;
  author_handle: string;
  interaction_type: string;
  content_text: string;
  category: string;
  sentiment_score: number;
  drafted_reply?: string | null;
  is_auto_reply_eligible: boolean;
  requires_human_approval: boolean;
  status: string;
  received_at: string;
  permalink?: string | null;
}

export interface PreflightStep {
  step: number;
  name: string;
  passed: boolean;
  details: string;
}

export interface RegistrationReport {
  account_id: string;
  platform: string;
  allocated_email: string;
  username: string;
  step_results: PreflightStep[];
  is_ready_for_automation: boolean;
  status: 'Failed' | 'AwaitingHuman' | 'Completed';
  human_task_id?: string | null;
  signup_url?: string | null;
}

export interface LmProfile {
  id: string;
  name: string;
  provider_type: string;
  endpoint?: string | null;
  model_id: string;
  api_key_secret_id?: string | null;
  max_tokens?: number | null;
  temperature?: number | null;
}

export interface AccountMetric {
  account_id: string;
  platform: string;
  views: number;
  followers?: number | null;
  posts?: number | null;
  likes?: number | null;
  is_verified_by_api: boolean;
  captured_at: string;
}

export interface CreativeJob {
  job_id: string;
  asset_id: string;
  progress_percent: number;
  current_stage: string;
  output_path?: string | null;
  status: string;
  error_message?: string | null;
  warnings?: string[];
}

export interface AgentChatReply {
  reply: string;
  model: string;
  profile_id: string;
  tokens_used?: number | null;
  proposed_actions: AgentAction[];
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
  connections = signal<ConnectionStatus[]>([]);
  lmProfiles = signal<LmProfile[]>([]);
  metrics = signal<AccountMetric[]>([]);
  chatProfile = signal<string>('nim-cascade');
  inbox = signal<InboxMessage[]>([]);
  registrations = signal<RegistrationReport[]>([]);
  lastRegistration = signal<RegistrationReport | null>(null);
  busy = signal<Set<string>>(new Set());
  agentChatMessages = signal<AgentChatMessage[]>([]);
  chatPending = signal<boolean>(false);
  isAutonomousLoopRunning = signal<boolean>(false);
  killSwitchActive = signal<boolean>(false);

  vaultUnlocked = computed(() => this.vaultStatus()?.unlocked ?? false);
  inboxNeedingReview = computed(() =>
    this.inbox().filter((m) => (m.status === 'New' || m.status === 'PendingApproval') && (m.requires_human_approval || !!m.drafted_reply))
  );
  latestMetrics = computed(() => {
    const map = new Map<string, AccountMetric>();
    for (const m of this.metrics()) {
      const current = map.get(m.account_id);
      if (!current || current.captured_at < m.captured_at) map.set(m.account_id, m);
    }
    return map;
  });
  connectedPlatforms = computed(() => new Set(this.connections().filter((c) => c.kind === 'social' && c.connected).map((c) => c.id)));
  justDidItems = computed(() => this.flow().just_did);
  activelyDoingItems = computed(() => this.flow().actively_doing);
  planToDoNextItems = computed(() => this.flow().up_next);

  private fastTimer?: ReturnType<typeof setInterval>;
  private slowTimer?: ReturnType<typeof setInterval>;
  private liveInFlight = false;
  private allInFlight = false;

  constructor() {
    if (this.backendConnected) {
      void this.refreshAll();
      this.fastTimer = setInterval(() => this.visible() && void this.refreshLive(), 5000);
      this.slowTimer = setInterval(() => this.visible() && void this.refreshAll(), 30000);
      if (typeof document !== 'undefined') {
        document.addEventListener('visibilitychange', () => {
          if (this.visible()) void this.refreshAll();
        });
      }
    } else {
      this.lastError.set(new BackendUnavailableError().message);
    }
  }

  private visible(): boolean {
    return typeof document === 'undefined' || !document.hidden;
  }

  private setIfChanged<T>(target: WritableSignal<T>, value: T) {
    if (JSON.stringify(target()) === JSON.stringify(value)) return;
    target.set(value);
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
    if (this.liveInFlight) return;
    this.liveInFlight = true;
    try {
      await this.refreshLiveNow();
    } finally {
      this.liveInFlight = false;
    }
  }

  private async refreshLiveNow(): Promise<void> {
    await Promise.all([
      this.load<SystemStats>('get_system_stats', (v) => this.setIfChanged(this.stats, v)),
      this.load<HumanTask[]>('list_human_tasks', (v) => this.setIfChanged(this.humanTasks, v)),
      this.load<ActivityFlow>('get_activity_flow', (v) => this.setIfChanged(this.flow, v)),
      this.load<boolean>('is_kill_switch_active', (v) => this.setIfChanged(this.killSwitchActive, v)),
      this.load<boolean>('is_autonomous_marketing_loop_running', (v) => this.setIfChanged(this.isAutonomousLoopRunning, v)),
      this.load<InboxMessage[]>('list_unified_inbox', (v) => this.setIfChanged(this.inbox, v)),
      this.load<RegistrationReport[]>('list_pending_registrations', (v) => this.setIfChanged(this.registrations, v)),
    ]);
  }

  async refreshAll(): Promise<void> {
    if (this.allInFlight) return;
    this.allInFlight = true;
    this.loading.set(true);
    try {
      await Promise.all([
        this.refreshLiveNow(),
        this.load<AccountRecord[]>('list_accounts', (v) => this.setIfChanged(this.accounts, v)),
        this.load<EngagementQuota[]>('get_engagement_quotas', (v) => this.setIfChanged(this.quotas, v)),
        this.load<GarbageCollectionReport | null>('get_last_garbage_collection', (v) => this.setIfChanged(this.gcReport, v)),
        this.load<StickBridgeState>('get_stick_bridge_state', (v) => this.setIfChanged(this.bridgeState, v)),
        this.refreshVault(),
        this.refreshConnections(false),
        this.refreshProfiles(),
        this.load<AccountMetric[]>('list_social_analytics', (v) => this.setIfChanged(this.metrics, v)),
      ]);
    } finally {
      this.loading.set(false);
      this.allInFlight = false;
    }
  }

  async refreshVault(): Promise<void> {
    await this.load<VaultStatus>('get_vault_status', (v) => this.setIfChanged(this.vaultStatus, v));
    if (this.vaultUnlocked()) {
      await this.load<SecretMetadata[]>('list_vault_secrets', (v) => this.setIfChanged(this.vaultSecrets, v));
    } else {
      this.vaultSecrets.set([]);
    }
  }

  async refreshProfiles(): Promise<void> {
    await Promise.all([
      this.load<LmProfile[]>('list_lm_profiles', (v) => this.setIfChanged(this.lmProfiles, v.filter((p) => !p.id.includes('vision')))),
      this.load<string>('get_chat_lm_profile', (v) => this.setIfChanged(this.chatProfile, v)),
    ]);
  }

  setChatProfile(profileId: string) {
    return this.act('set_chat_lm_profile', { profile_id: profileId }, () => this.refreshProfiles());
  }

  registerProfile(profile: LmProfile) {
    return this.act('register_lm_profile', { profile }, () =>
      Promise.all([this.refreshProfiles(), this.refreshConnections(true)]).then(() => undefined)
    );
  }

  removeProfile(profileId: string) {
    return this.act('remove_lm_profile', { profile_id: profileId }, () =>
      Promise.all([this.refreshProfiles(), this.refreshConnections(true)]).then(() => undefined)
    );
  }

  async refreshConnections(force: boolean): Promise<void> {
    await this.load<ConnectionStatus[]>('get_connection_status', (v) => this.setIfChanged(this.connections, v), { force });
  }

  isBusy(key: string): boolean {
    return this.busy().has(key);
  }

  private async withBusy<T>(key: string, work: () => Promise<T>): Promise<T> {
    this.busy.update((s) => new Set(s).add(key));
    try {
      return await work();
    } finally {
      this.busy.update((s) => {
        const next = new Set(s);
        next.delete(key);
        return next;
      });
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

  sendInboxReply(messageId: string, reply: string) {
    return this.withBusy(`inbox-${messageId}`, () =>
      this.act('approve_and_send_inbox_reply', { message_id: messageId, final_reply: reply })
    );
  }

  archiveInbox(messageId: string) {
    return this.act('archive_inbox_message', { message_id: messageId });
  }

  async redraftInbox(messageId: string): Promise<string | null> {
    return this.withBusy(`redraft-${messageId}`, async () => {
      try {
        const draft = await this.invoke<string | null>('redraft_inbox_reply', { message_id: messageId });
        await this.refreshLive();
        return draft;
      } catch (err) {
        this.reportError(errorMessage(err));
        return null;
      }
    });
  }

  syncInbox(accountId: string) {
    return this.withBusy(`sync-${accountId}`, () => this.act('sync_inbox', { account_id: accountId }));
  }

  refreshMetrics(accountId: string) {
    return this.withBusy(`metrics-${accountId}`, () =>
      this.act('refresh_social_metrics', { account_id: accountId }, () =>
        this.load<AccountMetric[]>('list_social_analytics', (v) => this.setIfChanged(this.metrics, v))
      )
    );
  }

  deleteAccount(accountId: string) {
    return this.act('delete_account', { account_id: accountId }, () => this.refreshAll());
  }

  async startGuidedSignup(platform: string, category: string, username: string | null, email: string | null): Promise<void> {
    await this.withBusy('signup', async () => {
      try {
        const report = await this.invoke<RegistrationReport>('execute_10_step_account_preflight', {
          platform,
          category_str: category,
          desired_username: username,
          preferred_email: email,
          email_provider_id: null,
        });
        this.lastRegistration.set(report);
        this.clearError();
        await this.refreshLive();
      } catch (err) {
        this.reportError(errorMessage(err));
      }
    });
  }

  async completeRegistration(accountId: string): Promise<void> {
    await this.withBusy(`complete-${accountId}`, async () => {
      try {
        const report = await this.invoke<RegistrationReport>('complete_account_registration', {
          account_id: accountId,
          timeout_seconds: 60,
        });
        this.lastRegistration.set(report);
        this.clearError();
        await this.refreshAll();
      } catch (err) {
        this.reportError(errorMessage(err));
      }
    });
  }

  cancelRegistration(accountId: string) {
    return this.act('cancel_account_registration', { account_id: accountId });
  }

  async draftCaption(accountId: string, brief: string): Promise<string | null> {
    return this.withBusy('draft-caption', async () => {
      try {
        const text = await this.invoke<string>('draft_post_caption', { account_id: accountId, brief });
        this.clearError();
        return text;
      } catch (err) {
        this.reportError(errorMessage(err));
        return null;
      }
    });
  }

  schedulePost(accountId: string, caption: string, scheduledAt: string | null, approve: boolean, media: string[] = []) {
    return this.withBusy('schedule', () =>
      this.act('schedule_post', { account_id: accountId, caption, scheduled_at: scheduledAt, media, approve })
    );
  }

  async renderPostVideo(accountId: string, title: string, script: string, onProgress: (job: CreativeJob) => void): Promise<CreativeJob | null> {
    return this.withBusy('render', async () => {
      try {
        const jobId = await this.invoke<string>('render_post_video', { account_id: accountId, title, script, duration_seconds: 5 });
        const deadline = Date.now() + 180_000;
        while (Date.now() < deadline) {
          const job = await this.invoke<CreativeJob | null>('get_creative_job_status', { job_id: jobId });
          if (job) {
            onProgress(job);
            if (job.status === 'Completed' || job.status === 'Failed') {
              if (job.status === 'Failed') this.reportError(job.error_message ?? 'Render failed');
              await this.refreshLive();
              return job;
            }
          }
          await new Promise((r) => setTimeout(r, 500));
        }
        this.reportError('Render did not finish within 3 minutes');
        return null;
      } catch (err) {
        this.reportError(errorMessage(err));
        return null;
      }
    });
  }

  approveSlot(slotId: string) {
    return this.act('approve_calendar_slot', { slot_id: slotId });
  }

  deleteSlot(slotId: string) {
    return this.act('delete_calendar_slot', { slot_id: slotId });
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
      await Promise.all([this.refreshVault(), this.refreshConnections(true)]);
      return null;
    } catch (err) {
      await this.refreshVault();
      return errorMessage(err);
    }
  }

  lockVault() {
    return this.act('lock_vault', undefined, () => this.refreshVault());
  }

  async changeVaultPassword(current: string, next: string): Promise<string | null> {
    try {
      await this.invoke('change_vault_password', { current, new_password: next });
      await this.refreshVault();
      return null;
    } catch (err) {
      return errorMessage(err);
    }
  }

  storeSecret(label: string, secretType: string, value: string, accountId: string | null) {
    return this.act(
      'store_vault_secret',
      { label, secret_type: secretType, value, account_id: accountId },
      () => Promise.all([this.refreshVault(), this.refreshConnections(true)]).then(() => undefined)
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
          actions: (reply.proposed_actions ?? []).map((a) => ({ ...a, state: 'idle' as const })),
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

  async runAgentAction(messageId: string, index: number): Promise<void> {
    const msg = this.agentChatMessages().find((m) => m.id === messageId);
    const action = msg?.actions?.[index];
    if (!action || action.state === 'running' || action.state === 'done') return;
    const setState = (state: AgentAction['state']) =>
      this.agentChatMessages.update((list) =>
        list.map((m) =>
          m.id === messageId ? { ...m, actions: m.actions?.map((a, i) => (i === index ? { ...a, state } : a)) } : m
        )
      );
    setState('running');
    let text: string;
    let ok = true;
    try {
      text = await this.invoke<string>('execute_agent_action', {
        action: { kind: action.kind, label: action.label, args: action.args ?? {} },
      });
    } catch (err) {
      ok = false;
      text = errorMessage(err);
    }
    setState(ok ? 'done' : 'failed');
    this.agentChatMessages.update((prev) => [
      ...prev,
      {
        id: `chat-action-${Date.now()}`,
        sender: ok ? 'agent' : 'error',
        author_label: `Action · ${action.label}`,
        timestamp: timeLabel(),
        text,
        attachments: [],
      },
    ]);
    await this.refreshLive();
  }
}
