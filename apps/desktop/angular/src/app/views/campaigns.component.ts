import { Component, computed, effect, inject, signal } from '@angular/core';
import { DecimalPipe } from '@angular/common';
import { CampaignDetail, CampaignOverview, TauriService } from '../services/tauri.service';
import { BrandLogoComponent } from '../shared/brand-logo.component';
import { LocalWhenPipe, TimeAgoPipe } from '../shared/time-ago.pipe';

type DetailTab = 'execution' | 'accounts' | 'agents' | 'calendar' | 'bridge' | 'brief';

/** Every registered campaign with its accounts, agents, execution state, issues and metrics. */
@Component({
  selector: 'app-campaigns',
  standalone: true,
  imports: [BrandLogoComponent, TimeAgoPipe, LocalWhenPipe, DecimalPipe],
  templateUrl: './campaigns.component.html',
})
export class CampaignsComponent {
  tauri = inject(TauriService);

  selectedId = signal<string | null>(this.tauri.uiState().selectedCampaign ?? null);
  detail = signal<CampaignDetail | null>(null);
  detailError = signal<string | null>(null);
  loadingDetail = signal(false);
  tab = signal<DetailTab>('execution');

  tabs: { id: DetailTab; label: string }[] = [
    { id: 'execution', label: 'Execution history' },
    { id: 'accounts', label: 'Accounts' },
    { id: 'agents', label: 'Agents' },
    { id: 'calendar', label: 'Calendar' },
    { id: 'bridge', label: 'Coordination' },
    { id: 'brief', label: 'Brief & brand' },
  ];

  selected = computed<CampaignOverview | null>(() => {
    const id = this.selectedId();
    return this.tauri.campaigns().find((c) => c.brief.campaign_id === id) ?? null;
  });

  private refreshTimer: ReturnType<typeof setInterval>;

  constructor() {
    effect(() => {
      // Default to the first campaign once the list is loaded.
      const list = this.tauri.campaigns();
      if (!this.selectedId() && list.length) this.select(list[0].brief.campaign_id);
    });
    effect(() => {
      const id = this.selectedId();
      if (id) void this.loadDetail(id);
    });
    this.refreshTimer = setInterval(() => {
      const id = this.selectedId();
      if (id && !this.loadingDetail()) void this.loadDetail(id, true);
    }, 20000);
  }

  select(id: string) {
    this.selectedId.set(id);
    this.tab.set('execution');
    this.tauri.updateUiState({ selectedCampaign: id });
  }

  async loadDetail(id: string, silent = false) {
    if (!silent) this.loadingDetail.set(true);
    try {
      this.detail.set(await this.tauri.loadCampaignDetail(id));
      this.detailError.set(null);
    } catch (err) {
      this.detailError.set(String(err));
    } finally {
      this.loadingDetail.set(false);
    }
  }

  pct(done: number, total: number): number {
    return total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
  }

  metricPct(m: { numeric: number | null; target: number | null }): number | null {
    if (m.numeric === null || !m.target) return null;
    return Math.min(100, Math.round((m.numeric / m.target) * 100));
  }

  slotText(s: Record<string, unknown>, key: string): string {
    const v = s[key];
    if (v === null || v === undefined) return '—';
    if (Array.isArray(v)) return v.length ? v.join(' ') : '—';
    return String(v);
  }

  healthTone(c: CampaignOverview): string {
    if (c.jobs_failed > 0 || c.issues.some((i) => i.severity === 'error')) return 'danger';
    if (c.jobs_waiting > 0 || c.issues.some((i) => i.severity === 'blocked')) return 'warn';
    if (c.last_cycle_status) return 'ok';
    return 'muted';
  }

  healthLabel(c: CampaignOverview): string {
    if (c.jobs_failed > 0) return `${c.jobs_failed} failed`;
    if (c.jobs_waiting > 0) return `${c.jobs_waiting} waiting`;
    if (c.last_cycle_status) return 'healthy';
    return 'no run yet';
  }
}
