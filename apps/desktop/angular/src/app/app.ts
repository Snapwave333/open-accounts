import { Component, HostListener, inject, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { AccountRecord, ChatAttachment, TauriService } from './services/tauri.service';

export type NavTab = 'home' | 'flow' | 'chat' | 'accounts' | 'vault';

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [CommonModule, FormsModule],
  templateUrl: './app.html',
  styleUrl: './app.scss',
})
export class App {
  tauri = inject(TauriService);

  activeTab = signal<NavTab>('flow');

  // Accounts
  newAccountPlatform = signal<string>('instagram');
  newAccountUsername = signal<string>('');
  newAccountEmail = signal<string>('');

  // Vault
  vaultPasswordInput = signal<string>('');
  vaultError = signal<string>('');

  // Chat
  chatInput = signal<string>('');
  chatTargetScope = signal<string>('All Accounts & Studio Agents');
  stagedAttachments = signal<ChatAttachment[]>([]);
  isDragOver = signal<boolean>(false);

  // Garbage collection
  gcRunning = signal<boolean>(false);
  cycleRunning = signal<boolean>(false);

  getAccountBrandKey(acc: AccountRecord): string {
    const u = (acc.username || '').toLowerCase();
    const p = acc.platform;
    if (u.includes('gmail.com')) return 'gmail';
    if (u.includes('outlook.com') || u.includes('hotmail.com')) return 'outlook';
    if (p === 'paypal' || u.includes('paypal')) return 'paypal';
    if (p === 'shopify' || u.includes('myshopify')) return 'shopify';
    return p;
  }

  timeAgo(iso: string | null | undefined): string {
    if (!iso) return 'never';
    const mins = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
    if (mins < 1) return 'just now';
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.round(mins / 60);
    return hours < 48 ? `${hours}h ago` : `${Math.round(hours / 24)}d ago`;
  }

  onChatFilesSelected(event: Event) {
    const input = event.target as HTMLInputElement;
    if (input.files && input.files.length > 0) {
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
    if (event.dataTransfer?.files && event.dataTransfer.files.length > 0) {
      this.processFiles(Array.from(event.dataTransfer.files));
    }
  }

  onChatPaste(event: ClipboardEvent) {
    if (event.clipboardData?.files && event.clipboardData.files.length > 0) {
      this.processFiles(Array.from(event.clipboardData.files));
    }
  }

  private processFiles(files: File[]) {
    for (const file of files) {
      const kb = file.size / 1024;
      const sizeLabel = kb >= 1024 ? `${(kb / 1024).toFixed(1)} MB` : `${Math.max(1, Math.round(kb))} KB`;
      const isImg = file.type.startsWith('image/');
      const att: ChatAttachment = {
        id: `att-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
        name: file.name,
        size_label: sizeLabel,
        mime_type: file.type || 'application/octet-stream',
        is_image: isImg,
      };
      if (isImg) {
        const reader = new FileReader();
        reader.onload = () => {
          const dataUrl = typeof reader.result === 'string' ? reader.result : undefined;
          this.stagedAttachments.update((prev) => [...prev, { ...att, data_url: dataUrl }]);
        };
        reader.readAsDataURL(file);
      } else {
        this.stagedAttachments.update((prev) => [...prev, att]);
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
    if (!text && atts.length === 0) return;

    this.chatInput.set('');
    this.stagedAttachments.set([]);
    await this.tauri.sendAgentChatMessage(text, this.chatTargetScope(), atts);
  }

  @HostListener('window:keydown', ['$event'])
  handleGlobalShortcuts(event: KeyboardEvent) {
    const tag = (event.target as HTMLElement)?.tagName?.toLowerCase();
    const isInput = tag === 'input' || tag === 'textarea' || tag === 'select';
    if (!isInput && !event.ctrlKey && !event.metaKey && !event.altKey) {
      if (event.key === '1') this.setTab('home');
      else if (event.key === '2') this.setTab('flow');
      else if (event.key === '3') this.setTab('chat');
      else if (event.key === '4') this.setTab('accounts');
      else if (event.key === '5') this.setTab('vault');
    }
  }

  setTab(tab: NavTab) {
    this.activeTab.set(tab);
  }

  async addAccount() {
    const username = this.newAccountUsername().trim();
    if (!username) return;
    const email = this.newAccountEmail().trim();
    const id = await this.tauri.act<string>('create_account', {
      platform: this.newAccountPlatform(),
      username,
      email: email || null,
    });
    if (id) {
      this.newAccountUsername.set('');
      this.newAccountEmail.set('');
    }
  }

  async runCycleNow() {
    this.cycleRunning.set(true);
    try {
      await this.tauri.runCycleNow();
    } finally {
      this.cycleRunning.set(false);
    }
  }

  async runGarbageCollection() {
    this.gcRunning.set(true);
    try {
      await this.tauri.runGarbageCollection();
    } finally {
      this.gcRunning.set(false);
    }
  }

  async unlockVault() {
    if (!this.vaultPasswordInput()) return;
    try {
      await this.tauri.invoke('unlock_vault', { password: this.vaultPasswordInput() });
      this.vaultPasswordInput.set('');
      this.vaultError.set('');
    } catch (e: unknown) {
      this.vaultError.set(`Failed to unlock vault: ${e instanceof Error ? e.message : String(e)}`);
    }
    await this.tauri.refresh();
  }

  async lockVault() {
    await this.tauri.act('lock_vault');
  }

  async toggleKillSwitch() {
    await this.tauri.toggleKillSwitch();
  }

  async toggleMarketingLoop() {
    await this.tauri.toggleMarketingLoop();
  }
}
