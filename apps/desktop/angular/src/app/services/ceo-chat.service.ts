import { computed, inject, Injectable, signal } from '@angular/core';
import { TauriService } from './tauri.service';

/** One message in the shared conversation (the same file the Telegram bot writes). */
export interface CeoMessage {
  id: string;
  role: 'user' | 'ceo';
  channel: 'desktop' | 'telegram' | string;
  text: string;
  ts: string;
  tools?: { name: string; params: Record<string, unknown>; ok: boolean; summary: string }[];
  status?: 'ok' | 'error' | 'needs_confirmation';
  confirmation?: { tool: string; params: Record<string, unknown> } | null;
}

export interface PendingAction {
  tool: string;
  params: Record<string, unknown>;
  requested_at: string;
  channel: string;
  message_id?: string;
}

interface CeoHistoryResponse {
  messages: CeoMessage[];
  pending: { action?: PendingAction };
  updated_at: string | null;
  store: string;
  available: boolean;
  error: string | null;
}

interface CeoEvent {
  type: 'token' | 'tool' | 'done' | 'error';
  text?: string;
  name?: string;
  status?: 'start' | 'ok' | 'error';
  summary?: string;
  message?: CeoMessage;
}

export interface LiveToolEvent {
  name: string;
  status: 'start' | 'ok' | 'error';
  summary?: string;
}

const HISTORY_POLL_MS = 8000;

@Injectable({ providedIn: 'root' })
export class CeoChatService {
  private tauri = inject(TauriService);

  messages = signal<CeoMessage[]>([]);
  pending = signal<PendingAction | null>(null);
  store = signal<string>('');
  available = signal<boolean>(true);
  loadError = signal<string | null>(null);
  loaded = signal<boolean>(false);

  /** The reply being streamed right now. */
  streaming = signal<boolean>(false);
  streamText = signal<string>('');
  liveTools = signal<LiveToolEvent[]>([]);
  sendError = signal<string | null>(null);

  telegramMessages = computed(() => this.messages().filter((m) => m.channel === 'telegram').length);

  private unlisten: (() => void) | null = null;
  private lastSeenUpdatedAt: string | null = null;

  constructor() {
    if (this.tauri.isTauriAvailable) {
      void this.init();
    } else {
      this.loaded.set(true);
      this.available.set(false);
      this.loadError.set('CEO chat is only available inside the desktop app.');
    }
  }

  private async init() {
    await this.loadHistory();
    // Telegram turns land in the same file; pick them up while idle.
    setInterval(() => {
      if (!this.streaming()) void this.loadHistory();
    }, HISTORY_POLL_MS);
  }

  async loadHistory() {
    try {
      const h = await this.tauri.invoke<CeoHistoryResponse>('ceo_history', { limit: 300 });
      this.messages.set(h.messages ?? []);
      this.pending.set(h.pending?.action ?? null);
      this.store.set(h.store);
      this.available.set(h.available);
      this.loadError.set(h.error);
      this.lastSeenUpdatedAt = h.updated_at;
    } catch (err) {
      this.loadError.set(String(err));
    } finally {
      this.loaded.set(true);
    }
  }

  /** Streaming needs the Tauri event channel; without it the turn still completes, just not token by token. */
  private async subscribe() {
    if (this.unlisten) return;
    try {
      const { listen } = await import('@tauri-apps/api/event');
      this.unlisten = await listen<CeoEvent>('ceo://event', (e) => this.onEvent(e.payload));
    } catch (err) {
      console.warn('CEO stream events unavailable; replies arrive whole', err);
    }
  }

  private onEvent(ev: CeoEvent) {
    switch (ev.type) {
      case 'token':
        this.streamText.update((t) => t + (ev.text ?? ''));
        break;
      case 'tool':
        this.liveTools.update((list) => {
          const idx = list.findIndex((t) => t.name === ev.name && t.status === 'start');
          const entry: LiveToolEvent = { name: ev.name ?? '?', status: ev.status ?? 'start', summary: ev.summary };
          if (ev.status !== 'start' && idx >= 0) {
            const copy = [...list];
            copy[idx] = entry;
            return copy;
          }
          return [...list, entry];
        });
        break;
      case 'error':
        this.sendError.set(ev.text ?? 'unknown error');
        break;
      case 'done':
        break;
    }
  }

  /** Sends Eli's message and streams V. Sterling's reply. */
  async send(text: string): Promise<void> {
    const trimmed = text.trim();
    if (!trimmed || this.streaming()) return;
    await this.subscribe();
    this.sendError.set(null);
    this.streaming.set(true);
    this.streamText.set('');
    this.liveTools.set([]);
    // Show the question at once; the service persists it and returns the real record.
    const optimistic: CeoMessage = {
      id: `local-${Date.now()}`,
      role: 'user',
      channel: 'desktop',
      text: trimmed,
      ts: new Date().toISOString(),
    };
    this.messages.update((m) => [...m, optimistic]);
    try {
      await this.tauri.invoke<CeoMessage>('ceo_send', { text: trimmed });
    } catch (err) {
      this.sendError.set(String(err));
    } finally {
      this.streaming.set(false);
      this.streamText.set('');
      this.liveTools.set([]);
      await this.loadHistory();
    }
  }

  async confirm(): Promise<void> {
    if (this.streaming()) return;
    await this.subscribe();
    this.sendError.set(null);
    this.streaming.set(true);
    this.streamText.set('');
    this.liveTools.set([]);
    try {
      await this.tauri.invoke<CeoMessage>('ceo_confirm');
    } catch (err) {
      this.sendError.set(String(err));
    } finally {
      this.streaming.set(false);
      this.streamText.set('');
      this.liveTools.set([]);
      await this.loadHistory();
    }
  }

  async cancel(): Promise<void> {
    try {
      await this.tauri.invoke('ceo_cancel');
    } catch (err) {
      this.sendError.set(String(err));
    }
    await this.loadHistory();
  }

  async clear(): Promise<void> {
    try {
      await this.tauri.invoke('ceo_clear');
    } catch (err) {
      this.sendError.set(String(err));
    }
    await this.loadHistory();
  }
}
