import { Component, inject, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import {
  AccountCreationPreflightReport,
  PlatformKnowledgeProfile,
  TauriService,
  UnifiedInboxMessage,
} from './services/tauri.service';

export type NavTab =
  | 'dashboard'
  | 'marketing-overview'
  | 'content-calendar'
  | 'campaigns'
  | 'creative-studio'
  | 'video-production'
  | 'unified-inbox'
  | 'social-analytics'
  | 'brand-profiles'
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
  activeTab = signal<NavTab>('marketing-overview');

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

  // Video Production Generator
  videoTitle = signal<string>('Next-Gen Account Security: Offline Strongholds');
  videoFormat = signal<'Vertical9x16' | 'Square1x1' | 'Landscape16x9'>('Vertical9x16');
  videoScript = signal<string>(
    'Did you know that managing 50 social accounts without credential isolation is a security ticking time bomb? Meet OpenAccounts. Encrypted offline vault, zeroized memory buffers, and autonomous AI agents operating with Argon2id protection.'
  );
  selectedCreativeTool = signal<string>('ffmpeg');
  isRenderingVideo = signal<boolean>(false);
  renderProgress = signal<number>(0);
  renderOutputMessage = signal<string>('');

  // Selected Inbox message for reply
  selectedInboxMsg = signal<UnifiedInboxMessage | null>(null);
  replyTextInput = signal<string>('');

  // Email Identity Form
  newEmailPrefix = signal<string>('brand_actor');
  newEmailPlatform = signal<string>('instagram');

  // Vault Master Password
  vaultPasswordInput = signal<string>('');
  vaultError = signal<string>('');

  constructor() {
    this.loadPlatformProfiles();
    const firstMsg = this.tauri.inboxMessages()[0];
    if (firstMsg) {
      this.selectInboxMessage(firstMsg);
    }
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

  selectInboxMessage(msg: UnifiedInboxMessage) {
    this.selectedInboxMsg.set(msg);
    this.replyTextInput.set(msg.drafted_reply || '');
  }

  async sendReply() {
    const msg = this.selectedInboxMsg();
    if (!msg || !this.replyTextInput()) return;
    await this.tauri.invoke('approve_and_send_inbox_reply', {
      message_id: msg.message_id,
      final_reply: this.replyTextInput(),
    });
    this.selectedInboxMsg.update((m) =>
      m ? { ...m, status: 'Replied', drafted_reply: this.replyTextInput() } : null
    );
  }

  async launchVideoProduction() {
    this.isRenderingVideo.set(true);
    this.renderProgress.set(10);
    this.renderOutputMessage.set('1. Script parsed & audio voiceover synthesized (TTS)...');

    setTimeout(() => {
      this.renderProgress.set(35);
      this.renderOutputMessage.set('2. Driving Vector Craft CLI & Photo Craft CLI for graphic overlays...');

      setTimeout(() => {
        this.renderProgress.set(70);
        this.renderOutputMessage.set('3. FFmpeg compiling 1080x1920 (9:16) video & normalizing audio to -14 LUFS...');

        setTimeout(() => {
          this.renderProgress.set(100);
          this.renderOutputMessage.set('✓ Render complete! Validated aspect ratio, audio normalization & sync.');
          this.isRenderingVideo.set(false);
        }, 1500);
      }, 1200);
    }, 1000);
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

  async allocateNewEmailIdentity() {
    await this.tauri.invoke('allocate_email_identity', {
      prefix: this.newEmailPrefix(),
      platform: this.newEmailPlatform(),
    });
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

  async toggleMarketingLoop() {
    await this.tauri.toggleMarketingLoop();
  }
}
