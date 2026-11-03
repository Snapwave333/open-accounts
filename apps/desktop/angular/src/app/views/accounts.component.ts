import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { AccountRecord, TauriService } from '../services/tauri.service';
import { BrandLogoComponent } from '../shared/brand-logo.component';
import { LocalWhenPipe, TimeAgoPipe } from '../shared/time-ago.pipe';

/** The account registry (SQLite), grouped by the campaign each identity serves. */
@Component({
  selector: 'app-accounts',
  standalone: true,
  imports: [FormsModule, BrandLogoComponent, TimeAgoPipe, LocalWhenPipe],
  templateUrl: './accounts.component.html',
})
export class AccountsComponent {
  tauri = inject(TauriService);

  platform = signal('instagram');
  username = signal('');
  email = signal('');
  adding = signal(false);
  showAdd = signal(false);

  groups = computed(() => {
    const all = this.tauri.accounts();
    const nova = all.filter((a) => this.tauri.isNovaAccount(a));
    const rest = all.filter((a) => !this.tauri.isNovaAccount(a));
    const social = rest.filter((a) => !['email', 'paypal'].includes(a.platform));
    const ops = rest.filter((a) => ['email', 'paypal'].includes(a.platform));
    return [
      { title: 'Clearly Drawn News', sub: 'campaign_cdn · social handles and store', items: social },
      { title: 'Nova Vexley', sub: 'campaign_nova · AI influencer', items: nova },
      { title: 'Operations', sub: 'inbox watch and payout', items: ops },
    ].filter((g) => g.items.length);
  });

  metaText(a: AccountRecord): string {
    const m = a.profile_metadata ?? {};
    const parts: string[] = [];
    if (m['integration']) parts.push(`via ${String(m['integration'])}`);
    if (m['source']) parts.push(`from ${String(m['source'])}`);
    if (m['provider']) parts.push(String(m['provider']));
    if (m['store_url']) parts.push(String(m['store_url']));
    return parts.join(' · ');
  }

  async add() {
    const username = this.username().trim();
    if (!username) return;
    this.adding.set(true);
    try {
      const id = await this.tauri.act<string>('create_account', {
        platform: this.platform(),
        username,
        email: this.email().trim() || null,
      });
      if (id) {
        this.username.set('');
        this.email.set('');
        this.showAdd.set(false);
      }
    } finally {
      this.adding.set(false);
    }
  }
}
