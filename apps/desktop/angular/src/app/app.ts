import { ChangeDetectionStrategy, Component, HostListener, computed, effect, inject, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { AccountRecord, ChatAttachment, EngagementQuota, InboxMessage, TauriService } from './services/tauri.service';

export type NavTab = 'home' | 'flow' | 'chat' | 'accounts' | 'vault';

const ALL_SCOPE = 'All accounts';
const API_PLATFORMS = new Set(['x', 'youtube', 'reddit', 'instagram']);
const TEXT_ATTACHMENT_LIMIT = 64 * 1024;
const TEXT_MIME = /^(text\/|application\/(json|xml|javascript|x-yaml|yaml|csv))/;

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [CommonModule, FormsModule],
  templateUrl: './app.html',
  styleUrl: './app.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
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

  showModelForm = signal<boolean>(false);
  modelProvider = signal<string>('anthropic');
  modelId = signal<string>('');
  modelProfileId = signal<string>('');
  modelEndpoint = signal<string>('');
  modelKeyEnv = signal<string>('');

  composeAccount = signal<string>('');
  composeBrief = signal<string>('');
  composeCaption = signal<string>('');
  composeWhen = signal<string>('');
  composeApprove = signal<boolean>(false);

  publishableAccounts = computed(() => this.tauri.accounts().filter((a) => API_PLATFORMS.has(a.platform)));
  composeLimit = computed(() => {
    const acc = this.tauri.accounts().find((a) => a.id === this.composeAccount());
    const limits: Record<string, number> = { x: 280, instagram: 2200, youtube: 5000, reddit: 40000 };
    return acc ? limits[acc.platform] ?? 2000 : 2000;
  });

  replyDrafts = signal<Record<string, string>>({});
  signupPlatform = signal<string>('reddit');
  signupCategory = signal<string>('brand');
  signupUsername = signal<string>('');
  signupEmail = signal<string>('');
  pendingDelete = signal<string | null>(null);

  postsAwaitingApproval = computed(() =>
    this.tauri.planToDoNextItems().filter((i) => i.kind === 'slot' && i.tag === 'NEEDS APPROVAL')
  );

  needsYouCount = computed(
    () =>
      this.tauri.humanTasks().length +
      this.tauri.inboxNeedingReview().length +
      this.tauri.registrations().length +
      this.postsAwaitingApproval().length
  );

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

  trackById = (_: number, item: { id: string }) => item.id;
  trackByMessage = (_: number, item: InboxMessage) => item.message_id;
  trackByAccount = (_: number, item: { account_id: string }) => item.account_id;
  trackByService = (_: number, item: { id: string; kind: string }) => `${item.kind}:${item.id}`;

  supportsApi(acc: AccountRecord): boolean {
    return API_PLATFORMS.has(acc.platform);
  }

  compact(n?: number | null): string {
    if (n === null || n === undefined) return '—';
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
    return `${n}`;
  }

  draftFor(msg: InboxMessage): string {
    return this.replyDrafts()[msg.message_id] ?? msg.drafted_reply ?? '';
  }

  setDraft(id: string, text: string) {
    this.replyDrafts.update((d) => ({ ...d, [id]: text }));
  }

  async sendReply(msg: InboxMessage) {
    const text = this.draftFor(msg).trim();
    if (!text) return;
    if (await this.tauri.sendInboxReply(msg.message_id, text)) {
      this.replyDrafts.update((d) => {
        const { [msg.message_id]: _, ...rest } = d;
        return rest;
      });
    }
  }

  async redraft(msg: InboxMessage) {
    const draft = await this.tauri.redraftInbox(msg.message_id);
    if (draft !== null) this.setDraft(msg.message_id, draft);
  }

  async addModel() {
    const modelId = this.modelId().trim();
    if (!modelId) return;
    const provider = this.modelProvider();
    const id = this.modelProfileId().trim() || `${provider}-${modelId}`;
    const ok = await this.tauri.registerProfile({
      id,
      name: `${provider} · ${modelId}`,
      provider_type: provider,
      endpoint: this.modelEndpoint().trim() || null,
      model_id: modelId,
      api_key_secret_id: this.modelKeyEnv().trim() || null,
      max_tokens: 1024,
      temperature: 0.4,
    });
    if (ok) {
      this.modelId.set('');
      this.modelProfileId.set('');
      this.modelEndpoint.set('');
      this.modelKeyEnv.set('');
      this.showModelForm.set(false);
    }
  }

  async draftCaption() {
    const account = this.composeAccount();
    if (!account) return;
    const text = await this.tauri.draftCaption(account, this.composeBrief() || this.composeCaption());
    if (text !== null) this.composeCaption.set(text);
  }

  async schedulePost() {
    const account = this.composeAccount();
    const caption = this.composeCaption().trim();
    if (!account || !caption || caption.length > this.composeLimit()) return;
    const when = this.composeWhen() ? new Date(this.composeWhen()).toISOString() : null;
    if (await this.tauri.schedulePost(account, caption, when, this.composeApprove())) {
      this.composeBrief.set('');
      this.composeCaption.set('');
      this.composeWhen.set('');
      this.composeApprove.set(false);
    }
  }

  async startSignup() {
    await this.tauri.startGuidedSignup(
      this.signupPlatform(),
      this.signupCategory(),
      this.signupUsername().trim() || null,
      this.signupEmail().trim() || null
    );
    this.signupUsername.set('');
    this.signupEmail.set('');
  }

  async confirmDelete(acc: AccountRecord) {
    if (this.pendingDelete() !== acc.id) {
      this.pendingDelete.set(acc.id);
      setTimeout(() => this.pendingDelete.update((v) => (v === acc.id ? null : v)), 4000);
      return;
    }
    this.pendingDelete.set(null);
    await this.tauri.deleteAccount(acc.id);
  }

  private readonly titleSync = effect(() => {
    const count = this.needsYouCount();
    if (typeof document !== 'undefined') {
      document.title = count > 0 ? `(${count}) OpenAccounts` : 'OpenAccounts';
    }
  });

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
