import { Component, computed, inject, signal } from '@angular/core';
import { DecimalPipe } from '@angular/common';
import { FlowCardItem, TauriService } from '../services/tauri.service';
import { BrandLogoComponent } from '../shared/brand-logo.component';
import { LocalWhenPipe } from '../shared/time-ago.pipe';

type Filter = 'all' | 'campaign_cdn' | 'campaign_nova' | 'failed';

/** Everything the runtime did, is doing, and will do next, plus the cycle ledger. */
@Component({
  selector: 'app-activity',
  standalone: true,
  imports: [BrandLogoComponent, LocalWhenPipe, DecimalPipe],
  templateUrl: './activity.component.html',
})
export class ActivityComponent {
  tauri = inject(TauriService);
  filter = signal<Filter>((this.tauri.uiState().activityFilter as Filter) ?? 'all');
  expandedCycle = signal<string | null>(null);
  cycleRunning = signal(false);

  filters: { id: Filter; label: string }[] = [
    { id: 'all', label: 'All' },
    { id: 'campaign_cdn', label: 'Clearly Drawn News' },
    { id: 'campaign_nova', label: 'Nova Vexley' },
    { id: 'failed', label: 'Failed only' },
  ];

  private apply(items: FlowCardItem[]): FlowCardItem[] {
    const f = this.filter();
    if (f === 'all') return items;
    if (f === 'failed') return items.filter((i) => i.tag === 'FAILED');
    return items.filter((i) => i.campaign === f);
  }

  done = computed(() => this.apply(this.tauri.justDidItems()));
  running = computed(() => this.apply(this.tauri.activelyDoingItems()));
  queued = computed(() => this.apply(this.tauri.planToDoNextItems()));

  setFilter(f: Filter) {
    this.filter.set(f);
    this.tauri.updateUiState({ activityFilter: f });
  }

  countFailed(c: { jobs: { success: boolean }[] }): number {
    return c.jobs.filter((j) => !j.success).length;
  }

  toggleCycle(id: string) {
    this.expandedCycle.update((cur) => (cur === id ? null : id));
  }

  jobTone(job: { success: boolean; skipped?: boolean; exit_code: number | null; timed_out: boolean }): string {
    if (job.timed_out || !job.success) return 'danger';
    if (job.skipped && job.exit_code === 3) return 'warn';
    if (job.skipped) return 'muted';
    return 'ok';
  }

  jobLabel(job: { success: boolean; skipped?: boolean; exit_code: number | null; timed_out: boolean }): string {
    if (job.timed_out) return 'timed out';
    if (!job.success) return 'failed';
    if (job.skipped && job.exit_code === 3) return 'waiting';
    if (job.skipped) return 'not due';
    return 'done';
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
