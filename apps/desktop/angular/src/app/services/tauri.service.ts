import { computed, Injectable, signal } from '@angular/core';

export interface SystemStats {
  total_accounts: number;
  active_automations: number;
  pending_human_tasks: number;
  system_status: string;
  updated_at: string;
}

export interface AccountRecord {
  id: string;
  /** Normalised to a plain string ("instagram", "tiktok", ...). */
  platform: string;
  username: string;
  email?: string | null;
  phone?: string | null;
  status: string;
  created_at: string;
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

export interface SecretMetadata {
  id: string;
  account_id?: string | null;
  secret_type: string;
  label: string;
  created_at: string;
}

export interface StickBridgeMessage {
  id: string;
  timestamp: string;
  direction: string;
  from_agent: string;
  to_agent: string;
  channel: string;
  subject: string;
  body: string;
  status: string;
}

export interface StickBridgeState {
  status: string;
  stick_studio_root: string;
  messages: StickBridgeMessage[];
  last_synced_at: string;
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

export interface FlowCardItem {
  id: string;
  title: string;
  detail: string;
  tag: string;
  time_label: string;
  eta?: string;
  progress?: number;
  platform: string;
  account_handle: string;
}

export interface ChatAttachment {
  id: string;
  name: string;
  size_label: string;
  mime_type: string;
  is_image: boolean;
  data_url?: string;
}

export interface AgentChatMessage {
  id: string;
  sender: 'human' | 'agent';
  author_label: string;
  timestamp: string;
  text: string;
  target_scope?: string;
  attachments: ChatAttachment[];
}

export interface AutonomyJobReport {
  name: string;
  program: string;
  args: string[];
  exit_code: number | null;
  success: boolean;
  timed_out: boolean;
  skipped?: boolean;
  duration_ms: number;
  summary: string;
}

export interface AutonomyCycleReport {
  cycle_id: string;
  started_at: string;
  finished_at: string;
  status: string;
  duration_ms: number;
  jobs: AutonomyJobReport[];
  error: string | null;
}

export interface AutonomyStatus {
  loop_running: boolean;
  paused: boolean;
  interval_secs: number;
  cycles_completed: number;
  cycles_failed: number;
  last_cycle: AutonomyCycleReport | null;
  next_cycle_at: string | null;
  last_error: string | null;
  studio_root: string;
  python: string;
  studio_root_exists: boolean;
  cycle_in_flight: boolean;
  current_job: string | null;
  planned_jobs: string[];
}

/** Which accounts a studio job acts on, for the flow cards' logo and label. */
const JOB_ACCOUNT: Record<string, { platform: string; handle: string; role: string }> = {
  'Comment sweep': { platform: 'youtube', handle: 'YouTube · TikTok · IG', role: 'Comment & Reply Agent' },
  'Social plan': { platform: 'instagram', handle: 'Instagram', role: 'Social Publishing Agent' },
  'Social publish': { platform: 'instagram', handle: 'Instagram', role: 'Social Publishing Agent' },
  'Engagement run': { platform: 'tiktok', handle: 'TikTok · IG · YouTube', role: 'Engagement Agent' },
  'Engagement stats': { platform: 'tiktok', handle: 'TikTok · IG · YouTube', role: 'Engagement Agent' },
  'Retention GC': { platform: 'shopify', handle: 'Studio host', role: 'Retention / GC Agent' },
};

/** Cycle state changes by the second; accounts, vault, GC and bridge rarely do. */
const FAST_POLL_MS = 5000;
const SLOW_POLL_MS = 30000;

/** Turns engage.py's stats line (a Python dict) into "IG 4 likes · 1 follow · YT 3 likes". */
function formatEngagement(summary: string): string | null {
  let parsed: Record<string, Record<string, number>>;
  try {
    parsed = JSON.parse(summary.replace(/'/g, '"'));
  } catch {
    return null;
  }
  const labels: Record<string, string> = { instagram: 'IG', tiktok: 'TT', youtube: 'YT' };
  const parts: string[] = [];
  for (const [platform, counts] of Object.entries(parsed)) {
    if (!counts || typeof counts !== 'object') continue;
    const bits: string[] = [];
    if (counts['like']) bits.push(`${counts['like']} like${counts['like'] === 1 ? '' : 's'}`);
    if (counts['follow']) bits.push(`${counts['follow']} follow${counts['follow'] === 1 ? '' : 's'}`);
    if (counts['comment']) bits.push(`${counts['comment']} comment${counts['comment'] === 1 ? '' : 's'}`);
    if (counts['comment_draft']) bits.push(`${counts['comment_draft']} draft${counts['comment_draft'] === 1 ? '' : 's'}`);
    if (bits.length) parts.push(`${labels[platform] ?? platform} ${bits.join(', ')}`);
  }
  return parts.length ? parts.join(' · ') : 'no actions yet today';
}

/** Rust enums arrive either as "variant" or {"variant": value}; show them as text. */
function enumText(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object') {
    const [key, inner] = Object.entries(value as Record<string, unknown>)[0] ?? [];
    if (key === undefined) return '';
    return typeof inner === 'string' ? inner : key;
  }
  return String(value ?? '');
}

function localTime(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

@Injectable({
  providedIn: 'root',
})
export class TauriService {
  readonly isTauriAvailable = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  // Everything below is loaded from the Rust backend; nothing is seeded.
  stats = signal<SystemStats | null>(null);
  accounts = signal<AccountRecord[]>([]);
  humanTasks = signal<HumanTask[]>([]);
  vaultUnlocked = signal<boolean>(false);
  vaultSecrets = signal<SecretMetadata[]>([]);
  killSwitchActive = signal<boolean>(false);
  isAutonomousLoopRunning = signal<boolean>(false);
  autonomyStatus = signal<AutonomyStatus | null>(null);
  gcReport = signal<GarbageCollectionReport | null>(null);
  bridgeState = signal<StickBridgeState | null>(null);
  justDidItems = signal<FlowCardItem[]>([]);
  activelyDoingItems = signal<FlowCardItem[]>([]);
  planToDoNextItems = signal<FlowCardItem[]>([]);
  /** Last cycle's "Engagement stats" output (today's real action counts). */
  engagementToday = signal<string | null>(null);
  agentChatMessages = signal<AgentChatMessage[]>([]);

  /** Last error from the fast poller, the slow poller, and the last user action. */
  private cycleError = signal<string | null>(null);
  private studioError = signal<string | null>(null);
  private actionError = signal<string | null>(null);
  /** What the UI shows: whichever source is currently failing. */
  backendError = computed(() => this.actionError() ?? this.cycleError() ?? this.studioError());
  /** Set once the first refresh has finished, so empty states aren't shown while loading. */
  loaded = signal<boolean>(false);

  private fastPolling = false;
  private slowPolling = false;

  constructor() {
    if (this.isTauriAvailable) {
      void this.startSync();
    } else {
      this.cycleError.set('Not running inside the OpenAccounts desktop app: no backend to talk to.');
      this.loaded.set(true);
    }
  }

  async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    if (!this.isTauriAvailable) {
      throw new Error(`Cannot run '${cmd}': the OpenAccounts backend is only available in the desktop app.`);
    }
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
  }

  private async startSync() {
    await this.refresh();
    setInterval(() => void this.refreshCycleState(), FAST_POLL_MS);
    setInterval(() => void this.refreshStudioData(), SLOW_POLL_MS);
    try {
      const { listen } = await import('@tauri-apps/api/event');
      // A finished cycle may have changed accounts, GC and the bridge as well.
      await listen<AutonomyCycleReport>('autonomy://cycle', () => void this.refresh());
    } catch (err) {
      console.warn('Cannot subscribe to autonomy://cycle events', err);
    }
  }

  /** Pulls the whole visible state from the backend. */
  async refresh() {
    await Promise.all([this.refreshCycleState(), this.refreshStudioData()]);
    this.loaded.set(true);
  }

  /** The parts that change while a cycle runs. Overlapping calls are skipped. */
  async refreshCycleState() {
    if (this.fastPolling) return;
    this.fastPolling = true;
    try {
      const [status, cycles, stats, kill, tasks] = await Promise.all([
        this.invoke<AutonomyStatus>('get_autonomy_status'),
        this.invoke<AutonomyCycleReport[]>('list_autonomy_cycles', { limit: 5 }),
        this.invoke<SystemStats>('get_system_stats'),
        this.invoke<boolean>('is_kill_switch_active'),
        this.invoke<Array<HumanTask & { id: unknown; challenge_type: unknown; status: unknown }>>('list_human_tasks'),
      ]);
      this.autonomyStatus.set(status);
      this.stats.set(stats);
      this.killSwitchActive.set(kill);
      this.isAutonomousLoopRunning.set(status.loop_running && !status.paused);
      this.humanTasks.set(
        tasks.map((t) => ({
          ...t,
          id: enumText(t.id),
          challenge_type: enumText(t.challenge_type),
          status: enumText(t.status),
        }))
      );
      this.applyFlowFromRuntime(status, cycles);
      this.cycleError.set(null);
    } catch (err) {
      this.cycleError.set(String(err));
    } finally {
      this.fastPolling = false;
    }
  }

  /** Accounts, vault, GC and the studio bridge: slow to read, slow to change. */
  async refreshStudioData() {
    if (this.slowPolling) return;
    this.slowPolling = true;
    try {
      const [accounts, vault, gc, bridge] = await Promise.all([
        this.invoke<Array<AccountRecord & { platform: unknown }>>('list_accounts'),
        this.invoke<boolean>('is_vault_unlocked'),
        this.invoke<GarbageCollectionReport | null>('get_gc_report'),
        this.invoke<StickBridgeState>('get_stick_bridge_state'),
      ]);
      this.accounts.set(accounts.map((a) => ({ ...a, platform: enumText(a.platform).toLowerCase() })));
      this.vaultUnlocked.set(vault);
      this.vaultSecrets.set(
        vault
          ? (await this.invoke<Array<SecretMetadata & { id: unknown; secret_type: unknown }>>('list_vault_secrets')).map(
              (s) => ({ ...s, id: enumText(s.id), secret_type: enumText(s.secret_type) })
            )
          : []
      );
      this.gcReport.set(gc);
      this.bridgeState.set(bridge);
      this.studioError.set(null);
    } catch (err) {
      this.studioError.set(String(err));
    } finally {
      this.slowPolling = false;
    }
  }

  /** Builds Just Did / Actively Doing / Plan To Do Next from real job results only. */
  private applyFlowFromRuntime(status: AutonomyStatus, cycles: AutonomyCycleReport[]) {
    const account = (job: string) => JOB_ACCOUNT[job] ?? { platform: 'shopify', handle: 'Studio', role: 'Studio Agent' };

    const done: FlowCardItem[] = [];
    for (const cycle of cycles) {
      for (const job of cycle.jobs) {
        if (job.skipped) continue; // not due yet; the plan column shows when it next runs
        const a = account(job.name);
        done.push({
          id: `${cycle.cycle_id}-${job.name}`,
          title: `${a.role}: ${job.name}${job.success ? '' : job.timed_out ? ' (timed out)' : ' (failed)'}`,
          detail: job.summary,
          tag: job.success ? 'DONE' : 'FAILED',
          time_label: `${localTime(cycle.finished_at)} · ${(job.duration_ms / 1000).toFixed(1)}s`,
          platform: a.platform,
          account_handle: a.handle,
        });
      }
      if (cycle.jobs.length === 0 && cycle.error) {
        done.push({
          id: cycle.cycle_id,
          title: `Cycle ${cycle.status.replace(/_/g, ' ')}`,
          detail: cycle.error,
          tag: 'SKIPPED',
          time_label: localTime(cycle.finished_at),
          platform: 'shopify',
          account_handle: 'Autonomy loop',
        });
      }
    }
    this.justDidItems.set(done.slice(0, 12));

    const stats = cycles[0]?.jobs.find((j) => j.name === 'Engagement stats' && j.success);
    this.engagementToday.set(stats ? formatEngagement(stats.summary) ?? stats.summary : null);

    const planned = status.planned_jobs;
    const currentIdx = status.current_job ? planned.indexOf(status.current_job) : -1;
    if (status.current_job) {
      const a = account(status.current_job);
      this.activelyDoingItems.set([
        {
          id: `live-${status.current_job}`,
          title: `${a.role}: ${status.current_job}`,
          detail: `Running in ${status.studio_root} with ${status.python}`,
          tag: 'RUNNING NOW',
          time_label: 'Live',
          eta: `Job ${currentIdx + 1} of ${planned.length}`,
          progress: Math.round((currentIdx / Math.max(1, planned.length)) * 100),
          platform: a.platform,
          account_handle: a.handle,
        },
      ]);
    } else {
      this.activelyDoingItems.set([]);
    }

    const nextAt = status.next_cycle_at ? new Date(status.next_cycle_at) : null;
    const minsLeft = nextAt ? Math.max(0, Math.round((nextAt.getTime() - Date.now()) / 60000)) : null;
    const halted = this.killSwitchActive() || status.paused || !status.loop_running;
    const remaining = status.current_job ? planned.slice(currentIdx + 1) : planned;
    this.planToDoNextItems.set(
      remaining.map((job) => {
        const a = account(job);
        const lastRun = cycles[0]?.jobs.find((j) => j.name === job);
        return {
          id: `plan-${job}`,
          title: `${a.role}: ${job}`,
          detail: !status.studio_root_exists
            ? `Blocked: studio folder not found at ${status.studio_root}`
            : lastRun?.skipped
              ? `${lastRun.summary}. Checked again every ${Math.round(status.interval_secs / 60)} minutes.`
              : `Runs every ${Math.round(status.interval_secs / 60)}-minute cycle.`,
          tag: 'QUEUED',
          time_label: status.current_job ? 'This cycle' : 'Next cycle',
          eta: halted
            ? 'Paused'
            : status.current_job
              ? 'After current job'
              : minsLeft !== null
                ? `In ${minsLeft}m`
                : 'Starting',
          platform: a.platform,
          account_handle: a.handle,
        };
      })
    );
  }

  /** Runs an action, records its error for the UI, then re-reads backend state. */
  async act<T>(cmd: string, args?: Record<string, unknown>): Promise<T | undefined> {
    try {
      const result = await this.invoke<T>(cmd, args);
      this.actionError.set(null);
      return result;
    } catch (err) {
      this.actionError.set(`${cmd}: ${String(err)}`);
      return undefined;
    } finally {
      await this.refresh();
    }
  }

  async openExternal(url: string) {
    await this.act('open_external', { url });
  }

  async resolveTask(id: string) {
    await this.act('resolve_human_task', { task_id: id });
  }

  async toggleKillSwitch() {
    await this.act(this.killSwitchActive() ? 'reset_kill_switch' : 'trigger_kill_switch');
  }

  async toggleMarketingLoop() {
    await this.act(this.isAutonomousLoopRunning() ? 'pause_autonomous_marketing_loop' : 'start_autonomous_marketing_loop');
  }

  /** The runtime executes whole cycles, so "Run" starts a full cycle now. */
  async runCycleNow() {
    const run = this.act<AutonomyCycleReport>('run_autonomy_cycle_now');
    setTimeout(() => void this.refresh(), 300);
    return run;
  }

  async runGarbageCollection() {
    return this.act<GarbageCollectionReport>('run_garbage_collection', { apply: true });
  }

  async sendAgentChatMessage(text: string, targetScope: string, attachments: ChatAttachment[]) {
    const now = new Date().toISOString();
    this.agentChatMessages.update((prev) => [
      ...prev,
      {
        id: `chat-${Date.now()}`,
        sender: 'human',
        author_label: 'You',
        timestamp: localTime(now),
        text: text || `Attached ${attachments.length} file(s)`,
        target_scope: targetScope,
        attachments,
      },
    ]);

    const names = attachments.map((a) => `${a.name} (${a.size_label})`).join(', ');
    const body = names ? `${text}\n[Attachments named by the operator, not transferred: ${names}]` : text;

    let reply: string;
    try {
      const sent = await this.invoke<StickBridgeMessage>('send_stick_bridge_message', {
        from_agent: 'Human Operator via OpenAccounts',
        to_agent: targetScope,
        channel: 'Direct Agent Chat',
        subject: text ? text.slice(0, 64) : `Attachment note (${attachments.length} file(s))`,
        body,
        create_ceo_directive: true,
      });
      reply =
        `Written to the studio bridge as ${sent.id} and filed as a CEO directive. ` +
        `The studio's CEO agent reads active directives on its hourly brief.` +
        (names ? ' File contents are not transferred, only their names.' : '');
    } catch (err) {
      reply = `Not delivered: ${String(err)}`;
    }

    this.agentChatMessages.update((prev) => [
      ...prev,
      {
        id: `chat-reply-${Date.now()}`,
        sender: 'agent',
        author_label: 'Studio bridge',
        timestamp: localTime(new Date().toISOString()),
        text: reply,
        target_scope: targetScope,
        attachments: [],
      },
    ]);
    await this.refresh();
  }
}
