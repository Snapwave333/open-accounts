import { computed, Injectable, signal } from '@angular/core';

// ---------------------------------------------------------------------------
// Backend shapes (mirrors the Rust structs; everything here is live data)
// ---------------------------------------------------------------------------

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
  profile_metadata?: Record<string, unknown>;
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

export interface StickAgentPairing {
  openaccounts_agent: string;
  openaccounts_title: string;
  stick_agents: string[];
  stick_roles: string[];
  channel: string;
}

export interface StickEpisodeSummary {
  slug: string;
  title: string;
  has_long_mp4: boolean;
  shorts_count: number;
  youtube_published: boolean;
  tiktok_posted: boolean;
  instagram_posted: boolean;
  kitchen_dish?: string | null;
  kitchen_show?: string | null;
}

export interface StickBridgeState {
  status: string;
  stick_studio_root: string;
  control_room_url: string;
  delegation: {
    comments_delegated_to_openaccounts: boolean;
    engage_delegated_to_openaccounts: boolean;
    social_agent_delegated_to_openaccounts: boolean;
  };
  pairings: StickAgentPairing[];
  messages: StickBridgeMessage[];
  latest_episodes: StickEpisodeSummary[];
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

export interface ChromeRelayStatus {
  online: boolean;
  broker_url: string;
  sessions: { name: string; run_id: string; state: string; tab_id: number | null }[];
  error: string | null;
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

export interface NovaCampaignSnapshot {
  campaign_id: string;
  brand_id: string;
  title: string;
  handle: string | null;
  account_status: string;
  gates_open: string[];
  calendar_slots: number;
  ready_posts: number;
  posted: number;
  followers: number | null;
  reference_images: number;
  photos_only: boolean;
  cost: string;
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
  influencer_root: string;
  influencer_root_exists: boolean;
  cycle_in_flight: boolean;
  current_job: string | null;
  planned_jobs: string[];
  nova?: NovaCampaignSnapshot | null;
}

export interface CampaignBrief {
  campaign_id: string;
  brand_id: string;
  title: string;
  objective: string;
  target_platforms: string[];
  target_audience: string;
  content_formats: string[];
  hashtags: string[];
  start_date: string;
  end_date: string;
  status: string;
}

export interface BrandProfile {
  brand_id: string;
  name: string;
  tagline: string;
  visual_identity: { primary_hex: string; secondary_hex: string; accent_hex: string; font_family: string };
  voice: {
    tone_adjectives: string[];
    audience_persona: string;
    writing_rules: string[];
    prohibited_words: string[];
    escalation_triggers: string[];
  };
}

export interface CampaignAccount {
  id: string;
  platform: string;
  username: string;
  status: string;
  last_active: string | null;
}

export interface CampaignAgent {
  name: string;
  role: string;
  side: 'openaccounts' | 'studio' | 'pipeline';
  channel: string;
  jobs: string[];
}

export interface CampaignIssue {
  severity: 'error' | 'blocked' | 'warning';
  title: string;
  detail: string;
  at: string | null;
  source: string;
}

export interface CampaignMetric {
  label: string;
  value: string;
  numeric: number | null;
  target: number | null;
  source: string;
  captured_at: string | null;
}

export interface CampaignActivity {
  cycle_id: string;
  at: string;
  job: string;
  status: 'done' | 'failed' | 'timed_out' | 'waiting' | 'skipped';
  summary: string;
  duration_ms: number;
}

export interface CampaignProgress {
  label: string;
  done: number;
  total: number;
}

export interface CampaignOverview {
  brief: CampaignBrief;
  brand: BrandProfile | null;
  accounts: CampaignAccount[];
  agents: CampaignAgent[];
  last_activity_at: string | null;
  last_cycle_status: string | null;
  jobs_done: number;
  jobs_failed: number;
  jobs_waiting: number;
  current_job: string | null;
  issues: CampaignIssue[];
  outstanding: string[];
  metrics: CampaignMetric[];
  progress: CampaignProgress[];
  pending_tasks: number;
}

export interface CycleSummary {
  cycle_id: string;
  started_at: string;
  finished_at: string;
  status: string;
  done: number;
  failed: number;
  waiting: number;
}

export interface CampaignDetail {
  overview: CampaignOverview;
  activities: CampaignActivity[];
  cycles: CycleSummary[];
  pending_tasks: HumanTask[];
  bridge_messages: StickBridgeMessage[];
  episodes: StickEpisodeSummary[];
  calendar: Record<string, unknown>[];
}

/** A unit of agent work for the Activity view (built only from real job reports). */
export interface FlowCardItem {
  id: string;
  title: string;
  detail: string;
  tag: 'DONE' | 'FAILED' | 'WAITING' | 'RUNNING NOW' | 'QUEUED' | 'SKIPPED';
  time_label: string;
  eta?: string;
  progress?: number;
  platform: string;
  account_handle: string;
  campaign: 'campaign_cdn' | 'campaign_nova' | 'runtime';
}

export interface UiState {
  activeView?: string;
  selectedCampaign?: string | null;
  chatDraft?: string;
  activityFilter?: string;
}

/** Which accounts a studio job acts on, for the activity cards' logo and label. */
const JOB_ACCOUNT: Record<string, { platform: string; handle: string; role: string }> = {
  'Comment sweep': { platform: 'youtube', handle: 'YouTube · TikTok · IG', role: 'Comment & Reply Agent' },
  'Social plan': { platform: 'instagram', handle: 'Instagram', role: 'Social Publishing Agent' },
  'Social publish': { platform: 'instagram', handle: 'Instagram', role: 'Social Publishing Agent' },
  'Engagement run': { platform: 'tiktok', handle: 'TikTok · IG · YouTube', role: 'Engagement Agent' },
  'Engagement stats': { platform: 'tiktok', handle: 'TikTok · IG · YouTube', role: 'Engagement Agent' },
  'Retention GC': { platform: 'studio', handle: 'Studio host', role: 'Retention / GC Agent' },
};

/** Cycle state changes by the second; accounts, vault, GC and bridge rarely do. */
const FAST_POLL_MS = 5000;
const SLOW_POLL_MS = 30000;

/** Turns engage.py's stats line (a Python dict) into "IG 4 likes · 1 follow · YT 3 likes". */
export function formatEngagement(summary: string): string | null {
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
export function enumText(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object') {
    const [key, inner] = Object.entries(value as Record<string, unknown>)[0] ?? [];
    if (key === undefined) return '';
    return typeof inner === 'string' ? inner : key;
  }
  return String(value ?? '');
}

export function localTime(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export function timeAgo(iso: string | null | undefined): string {
  if (!iso) return 'never';
  const mins = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.round(mins / 60);
  return hours < 48 ? `${hours}h ago` : `${Math.round(hours / 24)}d ago`;
}

@Injectable({
  providedIn: 'root',
})
export class TauriService {
  readonly isTauriAvailable = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  // Everything below is loaded from the Rust backend; nothing is seeded.
  stats = signal<SystemStats | null>(null);
  accounts = signal<AccountRecord[]>([]);
  campaigns = signal<CampaignOverview[]>([]);
  humanTasks = signal<HumanTask[]>([]);
  vaultUnlocked = signal<boolean>(false);
  vaultSecrets = signal<SecretMetadata[]>([]);
  killSwitchActive = signal<boolean>(false);
  isAutonomousLoopRunning = signal<boolean>(false);
  autonomyStatus = signal<AutonomyStatus | null>(null);
  recentCycles = signal<AutonomyCycleReport[]>([]);
  gcReport = signal<GarbageCollectionReport | null>(null);
  bridgeState = signal<StickBridgeState | null>(null);
  relayStatus = signal<ChromeRelayStatus | null>(null);
  justDidItems = signal<FlowCardItem[]>([]);
  activelyDoingItems = signal<FlowCardItem[]>([]);
  planToDoNextItems = signal<FlowCardItem[]>([]);
  /** Last cycle's "Engagement stats" output (today's real action counts). */
  engagementToday = signal<string | null>(null);
  uiState = signal<UiState>({});

  /** Last error from the fast poller, the slow poller, and the last user action. */
  private cycleError = signal<string | null>(null);
  private studioError = signal<string | null>(null);
  private actionError = signal<string | null>(null);
  /** What the UI shows: whichever source is currently failing. */
  backendError = computed(() => this.actionError() ?? this.cycleError() ?? this.studioError());
  /** Set once the first refresh has finished, so empty states aren't shown while loading. */
  loaded = signal<boolean>(false);

  /** Issues that need the operator: failed jobs, open tasks, halted runtime. */
  attentionCount = computed(() => {
    const tasks = this.humanTasks().length;
    const failed = this.justDidItems().filter((i) => i.tag === 'FAILED').length;
    return tasks + failed + (this.killSwitchActive() ? 1 : 0);
  });

  private fastPolling = false;
  private slowPolling = false;
  private uiSaveTimer: ReturnType<typeof setTimeout> | null = null;

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
    try {
      this.uiState.set((await this.invoke<UiState>('get_ui_state')) ?? {});
    } catch (err) {
      console.warn('No persisted UI state', err);
    }
    await this.refresh();
    setInterval(() => void this.refreshCycleState(), FAST_POLL_MS);
    setInterval(() => void this.refreshStudioData(), SLOW_POLL_MS);
    try {
      const { listen } = await import('@tauri-apps/api/event');
      // A finished cycle may have changed accounts, GC, campaigns and the bridge as well.
      await listen<AutonomyCycleReport>('autonomy://cycle', () => void this.refresh());
    } catch (err) {
      console.warn('Cannot subscribe to autonomy://cycle events', err);
    }
  }

  /** Persists a UI preference (debounced) to the backend's ui_state.json. */
  updateUiState(patch: Partial<UiState>) {
    this.uiState.update((s) => ({ ...s, ...patch }));
    if (!this.isTauriAvailable) return;
    if (this.uiSaveTimer) clearTimeout(this.uiSaveTimer);
    this.uiSaveTimer = setTimeout(() => {
      void this.invoke('set_ui_state', { value: this.uiState() }).catch((e) => console.warn('ui state not saved', e));
    }, 400);
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
        this.invoke<AutonomyCycleReport[]>('list_autonomy_cycles', { limit: 12 }),
        this.invoke<SystemStats>('get_system_stats'),
        this.invoke<boolean>('is_kill_switch_active'),
        this.invoke<Array<HumanTask & { id: unknown; challenge_type: unknown; status: unknown }>>('list_human_tasks'),
      ]);
      this.autonomyStatus.set(status);
      this.recentCycles.set(cycles);
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

  /** StickExplainers CDN accounts vs Nova campaign accounts (metadata.campaign_id / brand). */
  cdnAccounts(): AccountRecord[] {
    return this.accounts().filter((a) => !this.isNovaAccount(a));
  }

  novaAccounts(): AccountRecord[] {
    return this.accounts().filter((a) => this.isNovaAccount(a));
  }

  isNovaAccount(a: AccountRecord): boolean {
    const handle = (a.username || '').toLowerCase();
    if (handle.includes('novavexley') || handle.includes('nova vexley')) return true;
    let meta: Record<string, unknown> | null = null;
    const raw = a.profile_metadata as unknown;
    if (raw && typeof raw === 'object') meta = raw as Record<string, unknown>;
    else if (typeof raw === 'string') {
      try {
        meta = JSON.parse(raw) as Record<string, unknown>;
      } catch {
        meta = null;
      }
    }
    if (!meta) return false;
    return (
      meta['campaign_id'] === 'campaign_nova' ||
      String(meta['brand'] ?? '').includes('Nova Vexley') ||
      String(meta['source'] ?? '').startsWith('influencer')
    );
  }

  /** Accounts, vault, GC, relay, campaigns and the studio bridge: slow to read, slow to change. */
  async refreshStudioData() {
    if (this.slowPolling) return;
    this.slowPolling = true;
    try {
      const [accounts, vault, gc, bridge, campaigns, relay] = await Promise.all([
        this.invoke<Array<AccountRecord & { platform: unknown }>>('list_accounts'),
        this.invoke<boolean>('is_vault_unlocked'),
        this.invoke<GarbageCollectionReport | null>('get_gc_report'),
        this.invoke<StickBridgeState>('get_stick_bridge_state'),
        this.invoke<CampaignOverview[]>('list_campaign_overviews').catch((e) => {
          console.warn('campaign overviews unavailable', e);
          return [] as CampaignOverview[];
        }),
        this.invoke<ChromeRelayStatus>('get_chrome_relay_status').catch(
          (e) => ({ online: false, broker_url: '', sessions: [], error: String(e) }) as ChromeRelayStatus
        ),
      ]);
      this.accounts.set(accounts.map((a) => ({ ...a, platform: enumText(a.platform).toLowerCase() })));
      this.campaigns.set(campaigns ?? []);
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
      this.relayStatus.set(relay);
      this.studioError.set(null);
    } catch (err) {
      this.studioError.set(String(err));
    } finally {
      this.slowPolling = false;
    }
  }

  async loadCampaignDetail(campaignId: string): Promise<CampaignDetail> {
    return this.invoke<CampaignDetail>('get_campaign_detail', { campaignId });
  }

  /** Builds Just Did / Running / Up Next from real job results only. */
  private applyFlowFromRuntime(status: AutonomyStatus, cycles: AutonomyCycleReport[]) {
    const account = (job: string) =>
      JOB_ACCOUNT[job] ??
      (job.startsWith('Nova ')
        ? { platform: 'instagram', handle: 'Nova Vexley', role: `Nova ${job.slice(5)} agent` }
        : { platform: 'studio', handle: 'Studio', role: 'Studio Agent' });
    const campaignOf = (job: string): FlowCardItem['campaign'] =>
      job.startsWith('Nova ') ? 'campaign_nova' : 'campaign_cdn';

    const done: FlowCardItem[] = [];
    for (const cycle of cycles) {
      for (const job of cycle.jobs) {
        const waiting = job.skipped && job.exit_code === 3;
        if (job.skipped && !waiting) continue; // not due yet; the plan column shows when it next runs
        const a = account(job.name);
        done.push({
          id: `${cycle.cycle_id}-${job.name}`,
          title: `${a.role}: ${job.name}${job.success ? '' : job.timed_out ? ' (timed out)' : ' (failed)'}`,
          detail: job.summary,
          tag: waiting ? 'WAITING' : job.success ? 'DONE' : 'FAILED',
          time_label: `${localTime(cycle.finished_at)} · ${(job.duration_ms / 1000).toFixed(1)}s`,
          platform: a.platform,
          account_handle: a.handle,
          campaign: campaignOf(job.name),
        });
      }
      if (cycle.jobs.length === 0 && cycle.error) {
        done.push({
          id: cycle.cycle_id,
          title: `Cycle ${cycle.status.replace(/_/g, ' ')}`,
          detail: cycle.error,
          tag: 'SKIPPED',
          time_label: localTime(cycle.finished_at),
          platform: 'studio',
          account_handle: 'Autonomy loop',
          campaign: 'runtime',
        });
      }
    }
    this.justDidItems.set(done.slice(0, 40));

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
          detail: `Running in ${status.current_job.startsWith('Nova ') ? status.influencer_root : status.studio_root}`,
          tag: 'RUNNING NOW',
          time_label: 'Live',
          eta: `Job ${currentIdx + 1} of ${planned.length}`,
          progress: Math.round((currentIdx / Math.max(1, planned.length)) * 100),
          platform: a.platform,
          account_handle: a.handle,
          campaign: campaignOf(status.current_job),
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
          campaign: campaignOf(job),
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

  clearActionError() {
    this.actionError.set(null);
  }

  async openExternal(url: string) {
    await this.act('open_external', { url });
  }

  async resolveTask(id: string) {
    await this.act('resolve_human_task', { taskId: id });
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

  /** Writes a coordination message to the studio bridge (and optionally a CEO directive). */
  async sendBridgeMessage(toAgent: string, subject: string, body: string, createDirective: boolean) {
    return this.act<StickBridgeMessage>('send_stick_bridge_message', {
      fromAgent: 'Operator via OpenAccounts',
      toAgent,
      channel: 'Operator Dispatch',
      subject,
      body,
      createCeoDirective: createDirective,
    });
  }
}
