import { Component, HostListener, computed, inject, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { AccountRecord, ChatAttachment, EngagementQuota, TauriService } from './services/tauri.service';

export type NavTab = 'home' | 'flow' | 'chat' | 'accounts' | 'vault';

const ALL_SCOPE = 'All accounts';
const TEXT_ATTACHMENT_LIMIT = 64 * 1024;
const TEXT_MIME = /^(text\/|application\/(json|xml|javascript|x-yaml|yaml|csv))/;

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [CommonModule, FormsModule],
  templateUrl: './app.html',
  styleUrl: './app.scss',
})
export class App {
  tauri = inject(TauriService);

  readonly allScope = ALL_SCOPE;
  activeTab = signal<NavTab>('home');

  newAccountPlatform = signal<string>('instagram');
  newAccountUsername = signal<string>('');
  newAccountEmail = signal<string>('');

  vaultPasswordInput = signal<string>('');
  vaultError = signal<string>('');
  copiedSecretId = signal<string | null>(null);
  newSecretLabel = signal<string>('');
  newSecretType = signal<string>('OAuthToken');
  newSecretValue = signal<string>('');
  newSecretAccount = signal<string>('');

  chatInput = signal<string>('');
  chatTargetScope = signal<string>(ALL_SCOPE);
  stagedAttachments = signal<ChatAttachment[]>([]);
  isDragOver = signal<boolean>(false);

  chatScopes = computed(() => [
    ALL_SCOPE,
    ...this.tauri
      .accounts()
      .filter((a) => a.platform !== 'email')
      .map((a) => `${a.username} (${a.platform})`),
  ]);

  platformSummary = computed(() => {
    const names = Array.from(new Set(this.tauri.accounts().map((a) => a.platform)));
    return names.length ? names.map((n) => n.charAt(0).toUpperCase() + n.slice(1)).join(' · ') : 'No accounts yet';
  });

  brandKeys = computed(() => Array.from(new Set(this.tauri.accounts().map((a) => this.getAccountBrandKey(a)))).slice(0, 6));

  quotaSummary = computed(() => {
    const busiest = [...this.tauri.quotas()]
      .filter((q: EngagementQuota) => q.cap > 0)
      .sort((a, b) => b.used / b.cap - a.used / a.cap)
      .slice(0, 3);
    return busiest.length
      ? busiest.map((q) => `${q.account_handle} ${q.used}/${q.cap} ${q.action}`).join(' · ')
      : 'No engagement yet today';
  });

  pendingInboxCount = computed(() => this.tauri.planToDoNextItems().filter((i) => i.kind === 'inbox').length);

  setTab(tab: NavTab) {
    this.activeTab.set(tab);
  }

  @HostListener('window:keydown', ['$event'])
  handleGlobalShortcuts(event: KeyboardEvent) {
    const tag = (event.target as HTMLElement)?.tagName?.toLowerCase();
    if (tag === 'input' || tag === 'textarea' || tag === 'select' || event.ctrlKey || event.metaKey || event.altKey) {
      return;
    }
    const tabs: Record<string, NavTab> = { '1': 'home', '2': 'flow', '3': 'chat', '4': 'accounts', '5': 'vault' };
    const tab = tabs[event.key];
    if (tab) this.setTab(tab);
  }

  getAccountBrandKey(acc: AccountRecord): string {
    const u = (acc.username || '').toLowerCase();
    const p = (acc.platform || '').toLowerCase();
    if (u.includes('gmail.com')) return 'gmail';
    if (u.includes('outlook.com') || u.includes('hotmail.com')) return 'outlook';
    if (p === 'paypal' || u.includes('paypal')) return 'paypal';
    if (p === 'shopify' || u.includes('myshopify')) return 'shopify';
    return p;
  }

  formatTime(iso?: string | null): string {
    if (!iso) return '—';
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? '—' : d.toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' });
  }

  onChatFilesSelected(event: Event) {
    const input = event.target as HTMLInputElement;
    if (input.files?.length) {
      this.processFiles(Array.from(input.files));
      input.value = '';
    }
  }

  onChatDragOver(event: DragEvent) {
    event.preventDefault();
    event.stopPropagation();
    this.isDragOver.set(true);
  }

  onChatDragLeave(event: DragEvent) {
    event.preventDefault();
    event.stopPropagation();
    this.isDragOver.set(false);
  }

  onChatDrop(event: DragEvent) {
    event.preventDefault();
    event.stopPropagation();
    this.isDragOver.set(false);
    if (event.dataTransfer?.files?.length) {
      this.processFiles(Array.from(event.dataTransfer.files));
    }
  }

  onChatPaste(event: ClipboardEvent) {
    if (event.clipboardData?.files?.length) {
      this.processFiles(Array.from(event.clipboardData.files));
    }
  }

  private processFiles(files: File[]) {
    for (const file of files) {
      const kb = file.size / 1024;
      const base: ChatAttachment = {
        id: `att-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        name: file.name,
        size_label: kb >= 1024 ? `${(kb / 1024).toFixed(1)} MB` : `${Math.max(1, Math.round(kb))} KB`,
        mime_type: file.type || 'application/octet-stream',
        is_image: file.type.startsWith('image/'),
      };
      const reader = new FileReader();
      if (base.is_image) {
        reader.onload = () =>
          this.stagedAttachments.update((prev) => [
            ...prev,
            { ...base, data_url: typeof reader.result === 'string' ? reader.result : undefined },
          ]);
        reader.readAsDataURL(file);
      } else if ((TEXT_MIME.test(base.mime_type) || /\.(md|txt|csv|json|ya?ml|srt|vtt)$/i.test(file.name)) && file.size <= TEXT_ATTACHMENT_LIMIT) {
        reader.onload = () =>
          this.stagedAttachments.update((prev) => [
            ...prev,
            { ...base, text_content: typeof reader.result === 'string' ? reader.result : undefined },
          ]);
        reader.readAsText(file);
      } else {
        this.stagedAttachments.update((prev) => [...prev, base]);
      }
    }
  }

  removeAttachment(id: string) {
    this.stagedAttachments.update((prev) => prev.filter((a) => a.id !== id));
  }

  async sendQuickPrompt(prompt: string) {
    this.chatInput.set(prompt);
    await this.sendChatMessage();
  }

  async sendChatMessage() {
    const text = this.chatInput().trim();
    const atts = this.stagedAttachments();
    if ((!text && atts.length === 0) || this.tauri.chatPending()) return;
    this.chatInput.set('');
    this.stagedAttachments.set([]);
    await this.tauri.sendAgentChatMessage(text, this.chatTargetScope(), atts);
  }

  async addAccount() {
    const username = this.newAccountUsername().trim();
    if (!username) return;
    const email = this.newAccountEmail().trim() || null;
    if (await this.tauri.createAccount(this.newAccountPlatform(), username, email)) {
      this.newAccountUsername.set('');
      this.newAccountEmail.set('');
    }
  }

  async unlockVault() {
    const password = this.vaultPasswordInput();
    if (!password) return;
    const error = await this.tauri.unlockVault(password);
    this.vaultPasswordInput.set('');
    this.vaultError.set(error ?? '');
  }

  async lockVault() {
    await this.tauri.lockVault();
  }

  async addSecret() {
    const label = this.newSecretLabel().trim();
    const value = this.newSecretValue();
    if (!label || !value) return;
    if (await this.tauri.storeSecret(label, this.newSecretType(), value, this.newSecretAccount() || null)) {
      this.newSecretLabel.set('');
      this.newSecretValue.set('');
    }
  }

  async copySecret(id: string) {
    const value = await this.tauri.revealSecret(id);
    if (value === null || typeof navigator === 'undefined' || !navigator.clipboard) return;
    await navigator.clipboard.writeText(value);
    this.copiedSecretId.set(id);
    setTimeout(() => this.copiedSecretId.set(null), 2000);
  }

  async deleteSecret(id: string) {
    await this.tauri.deleteSecret(id);
  }

  async toggleKillSwitch() {
    await this.tauri.toggleKillSwitch();
  }

  async toggleMarketingLoop() {
    await this.tauri.toggleMarketingLoop();
  }
}
