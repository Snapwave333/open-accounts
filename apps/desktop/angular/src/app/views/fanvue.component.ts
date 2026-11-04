import { Component, computed, inject, signal } from '@angular/core';
import { FanvuePolicyStatus, FanvueWeeklyReport, TauriService } from '../services/tauri.service';

interface DayGroup {
  date: string;
  weekday: string;
  slots: Array<{
    id: string;
    time: string;
    kind: string;
    tier: string;
    window: string;
    pillar: string;
    theme: string;
    status: string;
    ipSafe: boolean;
    images: number | null;
    price: string | null;
  }>;
}

const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

@Component({
  selector: 'app-fanvue',
  standalone: true,
  template: `
    <header class="view-header">
      <div>
        <h1>Fanvue</h1>
        <p class="view-sub">
          Nova's paid surface: spicy sets, PPV drops and subscriber care. Instagram keeps the
          20-image carousel funnel; what fans want spicier lives here. Two platforms, one
          character identity and one compliance gate.
        </p>
      </div>
      <div class="view-actions">
        <button class="btn btn-primary" (click)="refresh()" [disabled]="loading()">
          {{ loading() ? 'Refreshing…' : 'Refresh' }}
        </button>
      </div>
    </header>

    <div class="fanvue-grid">
      <!-- Publishing authorization / approval queue -->
      <section class="panel">
        <div class="panel-header">
          <h2>Approval queue</h2>
          <span class="badge">{{ openGates().length }}</span>
        </div>
        <div class="flow-list">
          @for (gate of openGates(); track gate.id) {
            <article class="flow-card">
              <div class="flow-top">
                <span class="tag tag-warn">GATE</span>
              </div>
              <div class="flow-title">{{ gate.title }}</div>
              <p class="flow-detail">{{ gate.instructions }}</p>
              <div class="flow-foot">Approve in the Needs You tab (Ctrl+5)</div>
            </article>
          } @empty {
            <div class="empty">All Fanvue gates handled. Publishing runs on cadence.</div>
          }
        </div>
      </section>

      <!-- Compliance -->
      <section class="panel">
        <div class="panel-header">
          <h2>Compliance gate</h2>
          <span class="tag tag-ok">ACTIVE</span>
        </div>
        <div class="policy-body">
          <p class="flow-detail">
            Every caption, message and prompt is audited against Fanvue's published policies
            (Community Guidelines, AUP, Creator Terms, API policy) before upload — fail closed.
            The weekly agent re-reads the raw documents and re-distills the ruleset.
          </p>
          <dl class="policy-meta">
            <dt>Rules enforced</dt>
            <dd>{{ policy()?.rules ?? '—' }}</dd>
            <dt>Last agent review</dt>
            <dd>{{ policy()?.reviewed_at ?? '—' }}</dd>
            <dt>Reviewer</dt>
            <dd class="policy-reviewer">{{ policy()?.reviewed_by ?? '—' }}</dd>
          </dl>
          @if (policy()?.last_review_notes) {
            <p class="flow-detail policy-notes">{{ policy()?.last_review_notes }}</p>
          }
        </div>
      </section>

      <!-- Weekly report -->
      <section class="panel">
        <div class="panel-header">
          <h2>Weekly report</h2>
          <span class="badge">{{ report()?.published?.posts ?? 0 }} posts</span>
        </div>
        <div class="policy-body">
          <dl class="policy-meta">
            <dt>Earnings (7d)</dt>
            <dd>{{ earningsLabel() }}</dd>
            <dt>Subscribers</dt>
            <dd>{{ subscribersLabel() }}</dd>
            <dt>PPV releases</dt>
            <dd>{{ report()?.published?.ppv_releases ?? 0 }}</dd>
            <dt>Platform-verified</dt>
            <dd>{{ report()?.published?.verified ?? 0 }} / {{ report()?.published?.posts ?? 0 }}</dd>
            <dt>Inventory</dt>
            <dd>{{ report()?.inventory?.total ?? 0 }} assets ({{ report()?.inventory?.by_status?.['available'] ?? 0 }} available)</dd>
          </dl>
          <p class="flow-detail">Full numbers: data/fanvue_weekly_report.md — the report job runs daily and reads the real API.</p>
        </div>
      </section>
    </div>

    <!-- Calendar -->
    <section class="panel">
      <div class="panel-header">
        <h2>Publishing calendar</h2>
        <span class="badge">{{ upcoming().length }} upcoming</span>
      </div>
      <div class="fanvue-days">
        @for (day of upcoming(); track day.date) {
          <div class="fanvue-day">
            <div class="fanvue-day-head">{{ day.weekday }} · {{ day.date }}</div>
            @for (slot of day.slots; track slot.id) {
              <div class="fanvue-slot">
                <span class="fanvue-time">{{ slot.time }}</span>
                <span class="tag" [class.tag-ok]="slot.kind === 'feed'" [class.tag-warn]="slot.kind === 'ppv'">
                  {{ slot.kind === 'ppv' ? 'PPV' : slot.tier }}
                </span>
                @if (slot.ipSafe) {
                  <span class="tag tag-muted" title="IP-safe parody: suggests the look, never names the character">IP-safe</span>
                }
                <span class="fanvue-theme" [title]="slot.theme">{{ slot.theme }}</span>
                <span class="tag tag-muted">{{ slot.status }}</span>
                @if (slot.price) {
                  <span class="fanvue-price">{{ slot.price }}</span>
                }
              </div>
            }
          </div>
        } @empty {
          <div class="empty">No calendar yet — the "Nova fanvue calendar" job builds it on its next cycle.</div>
        }
      </div>
    </section>

    <!-- Published -->
    <section class="panel">
      <div class="panel-header">
        <h2>Published</h2>
        <span class="badge">{{ posts().length }}</span>
      </div>
      <div class="flow-list">
        @for (post of posts(); track post.at) {
          <article class="flow-card">
            <div class="flow-top">
              <span class="tag" [class.tag-ok]="post.verified" [class.tag-warn]="!post.verified">
                {{ post.verified ? 'VERIFIED' : post.result.toUpperCase() }}
              </span>
              <span class="tag tag-muted">{{ post.kind }}</span>
            </div>
            <div class="flow-title">{{ post.slot }}</div>
            <p class="flow-detail">{{ post.images ?? 1 }} image(s){{ post.price_cents ? ' · ' + ppvLabel(post.price_cents) : '' }}</p>
            <div class="flow-foot">{{ post.at }}</div>
          </article>
        } @empty {
          <div class="empty">Nothing published yet. The publisher needs the authorization gate and a ready slot.</div>
        }
      </div>
    </section>
  `,
  styles: [`
    .fanvue-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 16px; margin-bottom: 16px; }
    .policy-body { padding: 12px 16px 16px; }
    .policy-meta { display: grid; grid-template-columns: auto 1fr; gap: 4px 16px; margin: 10px 0 0; font-size: 13px; }
    .policy-meta dt { color: var(--text-dim, #8b8b9e); }
    .policy-meta dd { margin: 0; text-align: right; }
    .policy-reviewer { word-break: break-word; }
    .policy-notes { margin-top: 10px; white-space: pre-wrap; }
    .fanvue-days { display: grid; gap: 10px; padding: 12px 16px 16px; max-height: 420px; overflow-y: auto; }
    .fanvue-day-head { font-size: 12px; text-transform: uppercase; letter-spacing: 0.08em; opacity: 0.65; margin-bottom: 6px; }
    .fanvue-slot { display: flex; align-items: center; gap: 8px; padding: 6px 0; border-bottom: 1px solid var(--border, #ffffff14); font-size: 13px; }
    .fanvue-time { font-variant-numeric: tabular-nums; opacity: 0.85; min-width: 52px; }
    .fanvue-theme { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .fanvue-price { color: var(--accent, #7c5cff); font-variant-numeric: tabular-nums; }
  `],
})
export class FanvueComponent {
  tauri = inject(TauriService);
  loading = signal(false);
  policyStatus = signal<Record<string, unknown> | null>(null);
  weeklyReport = signal<Record<string, unknown> | null>(null);

  policy = computed(() => (this.policyStatus() ?? this.tauri.fanvuePolicyStatus()) as FanvuePolicyStatus | null);
  report = computed(() => (this.weeklyReport() ?? this.tauri.fanvueWeeklyReport()) as FanvueWeeklyReport | null);
  posts = computed(() => this.tauri.fanvuePosts().slice().reverse().slice(0, 20));

  constructor() {
    void this.refresh();
  }

  async refresh(): Promise<void> {
    this.loading.set(true);
    try {
      await this.tauri.refreshFanvue();
      const [policy, report] = await Promise.all([
        this.tauri.invoke<FanvuePolicyStatus>('fanvue_policy_status').catch(() => null),
        this.tauri.invoke<FanvueWeeklyReport>('fanvue_weekly_report').catch(() => null),
      ]);
      this.policyStatus.set(policy as unknown as Record<string, unknown>);
      this.weeklyReport.set(report as unknown as Record<string, unknown>);
    } finally {
      this.loading.set(false);
    }
  }

  openGates(): Array<{ id: string; title: string; instructions: string }> {
    return (this.tauri.humanTasks() as unknown as Array<Record<string, unknown>>)
      .filter((t) => String(t['id'] ?? '').includes('fanvue'))
      .map((t) => ({
        id: String(t['id']),
        title: String(t['title'] ?? t['id']),
        instructions: String(t['instructions'] ?? 'Waiting for approval in the Needs You tab.'),
      }));
  }

  upcoming(): DayGroup[] {
    const cal = this.tauri.fanvueCalendar();
    if (!cal?.slots) {
      return [];
    }
    const now = Date.now();
    const groups = new Map<string, DayGroup>();
    for (const slot of cal.slots) {
      const when = new Date(slot.scheduled_at).getTime();
      if (when < now - 6 * 3600_000) {
        continue;
      }
      const d = new Date(slot.scheduled_at);
      const date = slot.date ?? d.toISOString().slice(0, 10);
      let group = groups.get(date);
      if (!group) {
        group = { date, weekday: WEEKDAYS[(d.getDay() + 6) % 7], slots: [] };
        groups.set(date, group);
      }
      group.slots.push({
        id: slot.id,
        time: d.toTimeString().slice(0, 5),
        kind: slot.kind ?? (slot.unlock_price_cents ? 'ppv' : 'feed'),
        tier: slot.tier,
        window: slot.window ?? '',
        pillar: slot.pillar,
        theme: slot.theme,
        status: slot.status,
        ipSafe: !!slot.ip_safe,
        images: slot.images ?? null,
        price: slot.unlock_price_cents ? `$${(slot.unlock_price_cents / 100).toFixed(2)}` : null,
      });
    }
    return [...groups.values()].sort((a, b) => a.date.localeCompare(b.date)).slice(0, 14);
  }

  earningsLabel(): string {
    const e = this.report()?.earnings;
    if (!e?.available || e.total == null) {
      return `unavailable${e?.reason ? ` (${e.reason.slice(0, 60)})` : ''}`;
    }
    return `$${(e.total / 100).toFixed(2)}`;
  }

  subscribersLabel(): string {
    const s = this.report()?.subscribers;
    if (!s?.available) {
      return `unavailable${s?.reason ? ` (${s.reason.slice(0, 60)})` : ''}`;
    }
    return `${s.count_first_page ?? 0}+`;
  }

  ppvLabel(cents: number | null | undefined): string {
    return cents == null ? '' : `PPV $${(cents / 100).toFixed(2)}`;
  }
}
