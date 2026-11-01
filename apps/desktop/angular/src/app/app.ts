import { Component, inject, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import {
  AccountCreationPreflightReport,
  PlatformKnowledgeProfile,
  TauriService,
} from './services/tauri.service';

export type NavTab =
  | 'dashboard'
  | 'accounts'
  | 'multi-accounts'
  | 'preflight'
  | 'platform-intelligence'
  | 'policy-monitor'
  | 'email-identity'
  | 'automations'
  | 'human-tasks'
  | 'browser'
  | 'vault'
  | 'providers'
  | 'activity'
  | 'settings';

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [CommonModule, FormsModule],
  templateUrl: './app.html',
  styleUrl: './app.scss',
})
export class App {
  tauri = inject(TauriService);
  activeTab = signal<NavTab>('dashboard');

  // Platform Intelligence state
  selectedPlatformId = signal<string>('instagram');
  platformSearch = signal<string>('');
  platformsList = signal<PlatformKnowledgeProfile[]>([]);

  // 10-Step Preflight State
  preflightPlatform = signal<string>('instagram');
  preflightCategory = signal<string>('Brand');
  preflightUsername = signal<string>('nova_brand_official');
  preflightEmail = signal<string>('nova_ops@openaccounts.io');
  isPreflightRunning = signal<boolean>(false);
  preflightReport = signal<AccountCreationPreflightReport | null>(null);

  // Email Identity Form
  newEmailPrefix = signal<string>('brand_actor');
  newEmailPlatform = signal<string>('instagram');

  // Vault Master Password
  vaultPasswordInput = signal<string>('');
  vaultError = signal<string>('');

  // Add Account Modal
  showAddAccountModal = signal<boolean>(false);
  newAccPlatform = signal<string>('instagram');
  newAccUsername = signal<string>('');
  newAccEmail = signal<string>('');

  constructor() {
    this.loadPlatformProfiles();
  }

  async loadPlatformProfiles() {
    const list = await this.tauri.invoke<PlatformKnowledgeProfile[]>('list_platform_profiles');
    if (list && list.length > 0) {
      this.platformsList.set(list);
    }
  }

  setTab(tab: NavTab) {
    this.activeTab.set(tab);
  }

  get selectedPlatform(): PlatformKnowledgeProfile | undefined {
    return this.platformsList().find((p) => p.id === this.selectedPlatformId());
  }

  get filteredPlatforms(): PlatformKnowledgeProfile[] {
    const term = this.platformSearch().toLowerCase();
    if (!term) return this.platformsList();
    return this.platformsList().filter(
      (p) => p.name.toLowerCase().includes(term) || p.id.toLowerCase().includes(term)
    );
  }

  async runPreflightWorkflow() {
    this.isPreflightRunning.set(true);
    try {
      const report = await this.tauri.invoke<AccountCreationPreflightReport>(
        'execute_10_step_account_preflight',
        {
          platform: this.preflightPlatform(),
          category_str: this.preflightCategory(),
          desired_username: this.preflightUsername() || undefined,
          preferred_email: this.preflightEmail() || undefined,
        }
      );
      this.preflightReport.set(report);
    } catch (err: any) {
      console.error('Preflight error:', err);
    } finally {
      this.isPreflightRunning.set(false);
    }
  }

  async runPolicyCheckCycle() {
    await this.tauri.invoke('run_policy_monitor_cycle');
  }

  async simulatePolicyChangeAlert() {
    await this.tauri.invoke('simulate_policy_change', {
      platform_id: this.selectedPlatformId(),
      section: 'Bot & Automation Policy',
      new_summary: 'Third-party browser automation requires immediate human credential approval.',
    });
  }

  async allocateNewEmailIdentity() {
    await this.tauri.invoke('allocate_email_identity', {
      prefix: this.newEmailPrefix(),
      platform: this.newEmailPlatform(),
    });
  }

  async handleAddAccount() {
    if (!this.newAccUsername()) return;
    await this.tauri.invoke('create_account', {
      platform: this.newAccPlatform(),
      username: this.newAccUsername(),
      email: this.newAccEmail() || undefined,
    });
    this.newAccUsername.set('');
    this.newAccEmail.set('');
    this.showAddAccountModal.set(false);
  }

  async unlockVault() {
    if (!this.vaultPasswordInput()) return;
    try {
      await this.tauri.invoke('unlock_vault', { password: this.vaultPasswordInput() });
      this.vaultPasswordInput.set('');
      this.vaultError.set('');
    } catch (e: any) {
      this.vaultError.set('Failed to unlock vault: ' + (e?.message || e));
    }
  }

  async lockVault() {
    await this.tauri.invoke('lock_vault');
  }

  async toggleKillSwitch() {
    await this.tauri.toggleKillSwitch();
  }
}
