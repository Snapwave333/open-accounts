import { Component, computed, inject, output, signal } from '@angular/core';
import { TauriService } from '../services/tauri.service';
import { CeoChatService } from '../services/ceo-chat.service';
import { BrandLogoComponent } from '../shared/brand-logo.component';
import { LocalWhenPipe, TimeAgoPipe } from '../shared/time-ago.pipe';
import type { NavView } from '../app';

interface AttentionItem {
  severity: 'error' | 'blocked' | 'warning';
  title: string;
  detail: string;
  view: NavView;
  when?: string | null;
}

/** The command center: what needs the operator, what the agents are doing, campaign and account health. */
@Component({
  selector: 'app-overview',
  standalone: true,
  imports: [BrandLogoComponent, TimeAgoPipe, LocalWhenPipe],
  templateUrl: './overview.component.html',
})
export class OverviewComponent {
  tauri = inject(TauriService);
  ceo = inject(CeoChatService);
  navigate = output<NavView>();

  cycleRunning = signal(false);

  attention = computed<AttentionItem[]>(() => {
    const items: AttentionItem[] = [];
    if (this.tauri.killSwitchActive()) {
      items.push({ severity: 'error', title: 'Emergency kill switch is active', detail: 'No agent job runs until it is reset.', view: 'system' });
    }
    for (const t of this.tauri.humanTasks()) {
      items.push({ severity: 'blocked', title: t.title, detail: t.instructions, view: 'needs-you', when: t.created_at });
    }
    if (this.ceo.pending()) {
      const p = this.ceo.pending()!;
      items.push({ severity: 'blocked', title: `CEO is waiting for your go-ahead: ${p.tool}`, detail: `Requested via ${p.channel} ${this.fmtParams(p.params)}`, view: 'ceo', when: p.requested_at });
    }
    for (const c of this.tauri.campaigns()) {
      for (const i of c.issues) {
        if (i.source === 'Needs You') continue; // already listed from the human tasks
        items.push({ severity: i.severity, title: `${this.shortTitle(c.brief.title)}: ${i.title}`, detail: i.detail, view: 'campaigns', when: i.at });
      }
    }
    const relay = this.tauri.relayStatus();
    if (relay && !relay.online) {
      items.push({ severity: 'warning', title: 'Chrome relay is offline', detail: relay.error ?? 'Publishing and engagement jobs that drive Chrome will wait.', view: 'system' });
    }
    const order = { error: 0, blocked: 1, warning: 2 };
    return items.sort((a, b) => order[a.severity] - order[b.severity]);
  });

  lastCycle = computed(() => this.tauri.autonomyStatus()?.last_cycle ?? this.tauri.recentCycles()[0] ?? null);

  lastCycleCounts = computed(() => {
    const c = this.lastCycle();
    if (!c) return null;
    const ran = c.jobs.filter((j) => !j.skipped);
    return {
      ran: ran.length,
      ok: ran.filter((j) => j.success).length,
      failed: c.jobs.filter((j) => !j.success).length,
      waiting: c.jobs.filter((j) => j.skipped && j.exit_code === 3).length,
      deferred: c.jobs.filter((j) => j.skipped && j.exit_code !== 3).length,
    };
  });

  cyclesToday = computed(() => {
    const today = new Date().toDateString();
    return this.tauri.recentCycles().filter((c) => new Date(c.finished_at).toDateString() === today);
  });

  recentDone = computed(() => this.tauri.justDidItems().slice(0, 8));

  socialAccounts = computed(() => this.tauri.accounts().filter((a) => !['email', 'paypal'].includes(a.platform)));

  latestCeoMessage = computed(() => {
    const m = this.ceo.messages();
    for (let i = m.length - 1; i >= 0; i--) if (m[i].role === 'ceo') return m[i];
    return null;
  });

  shortTitle(t: string): string {
    return t.split(' (')[0].split(' - ')[0];
  }

  fmtParams(p: Record<string, unknown>): string {
    const entries = Object.entries(p ?? {});
    return entries.length ? `(${entries.map(([k, v]) => `${k}=${String(v)}`).join(', ')})` : '';
  }

  progressPct(done: number, total: number): number {
    return total > 0 ? Math.round((done / total) * 100) : 0;
  }

  async runCycleNow() {
    this.cycleRunning.set(true);
    try {
      await this.tauri.runCycleNow();
    } finally {
      this.cycleRunning.set(false);
    }
  }
}
