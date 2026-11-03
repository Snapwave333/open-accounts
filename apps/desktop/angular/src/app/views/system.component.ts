import { Component, inject, signal } from '@angular/core';
import { DecimalPipe } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { TauriService } from '../services/tauri.service';
import { LocalWhenPipe, TimeAgoPipe } from '../shared/time-ago.pipe';

/** Runtime configuration, the vault, the Chrome relay, garbage collection and the studio bridge. */
@Component({
  selector: 'app-system',
  standalone: true,
  imports: [FormsModule, DecimalPipe, TimeAgoPipe, LocalWhenPipe],
  templateUrl: './system.component.html',
})
export class SystemComponent {
  tauri = inject(TauriService);

  vaultPassword = signal('');
  vaultError = signal('');
  gcRunning = signal(false);

  dispatchTo = signal('OpenAccounts · ContentStrategist');
  dispatchSubject = signal('');
  dispatchBody = signal('');
  dispatchDirective = signal(true);
  dispatching = signal(false);
  dispatchResult = signal<string | null>(null);

  async unlockVault() {
    if (!this.vaultPassword()) return;
    try {
      await this.tauri.invoke('unlock_vault', { password: this.vaultPassword() });
      this.vaultPassword.set('');
      this.vaultError.set('');
    } catch (e: unknown) {
      this.vaultError.set(`Failed to unlock vault: ${e instanceof Error ? e.message : String(e)}`);
    }
    await this.tauri.refresh();
  }

  async lockVault() {
    await this.tauri.act('lock_vault');
  }

  async runGc() {
    this.gcRunning.set(true);
    try {
      await this.tauri.runGarbageCollection();
    } finally {
      this.gcRunning.set(false);
    }
  }

  async dispatch() {
    const subject = this.dispatchSubject().trim();
    const body = this.dispatchBody().trim();
    if (!subject || !body) return;
    this.dispatching.set(true);
    this.dispatchResult.set(null);
    try {
      const sent = await this.tauri.sendBridgeMessage(this.dispatchTo(), subject, body, this.dispatchDirective());
      if (sent) {
        this.dispatchResult.set(`Delivered as ${sent.id}${this.dispatchDirective() ? ' and filed as a CEO directive' : ''}.`);
        this.dispatchSubject.set('');
        this.dispatchBody.set('');
      }
    } finally {
      this.dispatching.set(false);
    }
  }
}
