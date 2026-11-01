import { Component, inject, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { TauriService } from './services/tauri.service';

export type NavTab =
  | 'dashboard'
  | 'accounts'
  | 'creator'
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

  // Account Creator State
  creatorMode = signal<'guided' | 'autonomous'>('guided');
  creatorPlatform = signal<string>('instagram');
  creatorUsername = signal<string>('brand_ambassador_01');
  creatorEmail = signal<string>('ops@brand.io');
  generatedPassword = signal<string>('k8$Nm9#Lq2!Vx7@Z');
  creationProgress = signal<string>('Ready to initialize');
  isCreating = signal<boolean>(false);

  // Automations Filter/Config
  selectedAutomation = signal<string>('instagram-comments');
  personalityPrompt = signal<string>(
    'Professional, helpful, and concise brand representative. Escalates negative sentiment or customer complaints to human supervisor immediately.'
  );
  autoReplyThreshold = signal<number>(85);

  // Vault master password input
  vaultPasswordInput = signal<string>('');
  vaultError = signal<string>('');

  // New Account modal
  showAddAccountModal = signal<boolean>(false);
  newAccPlatform = signal<string>('instagram');
  newAccUsername = signal<string>('');
  newAccEmail = signal<string>('');

  setTab(tab: NavTab) {
    this.activeTab.set(tab);
  }

  generateNewPassword() {
    const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()_+-=';
    let result = '';
    for (let i = 0; i < 20; i++) {
      result += chars.charAt(Math.floor(Math.random() * chars.length));
    }
    this.generatedPassword.set(result);
  }

  async runAccountCreation() {
    this.isCreating.set(true);
    this.creationProgress.set('1. Connecting to Chrome Relay browser session...');
    setTimeout(() => {
      this.creationProgress.set('2. Navigating to registration portal...');
      setTimeout(() => {
        this.creationProgress.set('3. Filling credentials and checking fields...');
        setTimeout(() => {
          this.creationProgress.set('4. PAUSED: Human verification required (CAPTCHA/TOS)');
          this.isCreating.set(false);
          this.activeTab.set('human-tasks');
        }, 1500);
      }, 1200);
    }, 1000);
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
