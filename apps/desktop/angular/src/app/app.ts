import { Component, computed, effect, HostListener, inject, signal } from '@angular/core';
import { TauriService } from './services/tauri.service';
import { CeoChatService } from './services/ceo-chat.service';
import { OverviewComponent } from './views/overview.component';
import { ActivityComponent } from './views/activity.component';
import { CampaignsComponent } from './views/campaigns.component';
import { CeoChatComponent } from './views/ceo-chat.component';
import { NeedsYouComponent } from './views/needs-you.component';
import { AccountsComponent } from './views/accounts.component';
import { SystemComponent } from './views/system.component';

export type NavView = 'overview' | 'activity' | 'campaigns' | 'ceo' | 'needs-you' | 'accounts' | 'system';

interface NavItem {
  id: NavView;
  label: string;
  hint: string;
  key: string;
}

export const NAV_ITEMS: NavItem[] = [
  { id: 'overview', label: 'Overview', hint: 'Command center', key: '1' },
  { id: 'activity', label: 'Activity', hint: 'Agent runs and queue', key: '2' },
  { id: 'campaigns', label: 'Campaigns', hint: 'Lifecycle and accounts', key: '3' },
  { id: 'ceo', label: 'CEO Chat', hint: 'Talk to V. Sterling', key: '4' },
  { id: 'needs-you', label: 'Needs You', hint: 'Human gates', key: '5' },
  { id: 'accounts', label: 'Accounts', hint: 'Managed identities', key: '6' },
  { id: 'system', label: 'System', hint: 'Vault, relay, bridge', key: '7' },
];

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [
    OverviewComponent,
    ActivityComponent,
    CampaignsComponent,
    CeoChatComponent,
    NeedsYouComponent,
    AccountsComponent,
    SystemComponent,
  ],
  templateUrl: './app.html',
})
export class App {
  tauri = inject(TauriService);
  ceo = inject(CeoChatService);

  nav = NAV_ITEMS;
  activeView = signal<NavView>('overview');
  /** Set once the persisted view has been applied, so the default does not flash. */
  private restored = false;
  now = signal<number>(Date.now());

  constructor() {
    setInterval(() => this.now.set(Date.now()), 1000);
    effect(() => {
      const saved = this.tauri.uiState().activeView as NavView | undefined;
      if (!this.restored && saved && this.nav.some((n) => n.id === saved)) {
        this.activeView.set(saved);
      }
      if (this.tauri.loaded()) this.restored = true;
    });
  }

  statusLabel = computed(() => {
    if (this.tauri.killSwitchActive()) return 'HALTED';
    const s = this.tauri.autonomyStatus();
    if (!s) return 'CONNECTING';
    if (s.paused) return 'PAUSED';
    if (s.cycle_in_flight) return 'RUNNING';
    if (s.loop_running) return 'AUTOPILOT';
    return 'STOPPED';
  });

  statusTone = computed(() => {
    const l = this.statusLabel();
    if (l === 'HALTED') return 'danger';
    if (l === 'PAUSED' || l === 'STOPPED') return 'warn';
    if (l === 'CONNECTING') return 'muted';
    return 'ok';
  });

  nextCycleIn = computed(() => {
    const s = this.tauri.autonomyStatus();
    const now = this.now();
    if (!s) return '';
    if (s.cycle_in_flight) return s.current_job ? `running ${s.current_job}` : 'cycle running';
    if (this.tauri.killSwitchActive()) return 'kill switch engaged';
    if (s.paused) return 'loop paused';
    if (!s.next_cycle_at) return 'first cycle pending';
    const secs = Math.max(0, Math.round((new Date(s.next_cycle_at).getTime() - now) / 1000));
    const m = Math.floor(secs / 60);
    const sec = secs % 60;
    return `next cycle in ${m}:${sec.toString().padStart(2, '0')}`;
  });

  badge(id: NavView): number | null {
    switch (id) {
      case 'needs-you':
        return this.tauri.humanTasks().length || null;
      case 'campaigns': {
        const issues = this.tauri.campaigns().reduce((n, c) => n + c.issues.filter((i) => i.severity !== 'warning').length, 0);
        return issues || null;
      }
      case 'ceo':
        return this.ceo.pending() ? 1 : null;
      case 'accounts':
        return this.tauri.accounts().length || null;
      default:
        return null;
    }
  }

  setView(view: NavView) {
    this.activeView.set(view);
    this.tauri.updateUiState({ activeView: view });
  }

  @HostListener('window:keydown', ['$event'])
  handleGlobalShortcuts(event: KeyboardEvent) {
    const tag = (event.target as HTMLElement)?.tagName?.toLowerCase();
    const isInput = tag === 'input' || tag === 'textarea' || tag === 'select';
    if (isInput || !(event.ctrlKey || event.metaKey) || event.altKey) return;
    const item = this.nav.find((n) => n.key === event.key);
    if (item) {
      event.preventDefault();
      this.setView(item.id);
    }
  }
}
