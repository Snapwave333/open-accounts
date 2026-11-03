import { AfterViewChecked, Component, ElementRef, inject, signal, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { CeoChatService, CeoMessage } from '../services/ceo-chat.service';
import { TauriService } from '../services/tauri.service';
import { LocalWhenPipe } from '../shared/time-ago.pipe';

/** Direct, persistent, streamed conversation with V. Sterling (shared with Telegram). */
@Component({
  selector: 'app-ceo-chat',
  standalone: true,
  imports: [FormsModule, LocalWhenPipe],
  templateUrl: './ceo-chat.component.html',
})
export class CeoChatComponent implements AfterViewChecked {
  chat = inject(CeoChatService);
  tauri = inject(TauriService);

  draft = signal<string>(this.tauri.uiState().chatDraft ?? '');
  scroller = viewChild<ElementRef<HTMLDivElement>>('scroller');
  private lastCount = -1;
  private lastStreamLen = -1;

  quickPrompts = [
    'What did the agents do in the last cycle?',
    'Where are the campaigns blocked right now?',
    'What did the OpenAccounts agents post and reply to today?',
    'Which comment replies are waiting?',
  ];

  ngAfterViewChecked() {
    const count = this.chat.messages().length;
    const streamLen = this.chat.streamText().length;
    if (count !== this.lastCount || streamLen !== this.lastStreamLen) {
      this.lastCount = count;
      this.lastStreamLen = streamLen;
      const el = this.scroller()?.nativeElement;
      if (el) el.scrollTop = el.scrollHeight;
    }
  }

  onDraft(v: string) {
    this.draft.set(v);
    this.tauri.updateUiState({ chatDraft: v });
  }

  onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void this.send();
    }
  }

  async send(text?: string) {
    const t = (text ?? this.draft()).trim();
    if (!t) return;
    this.draft.set('');
    this.tauri.updateUiState({ chatDraft: '' });
    await this.chat.send(t);
  }

  dayLabel(m: CeoMessage, prev: CeoMessage | undefined): string | null {
    const d = new Date(m.ts).toDateString();
    if (prev && new Date(prev.ts).toDateString() === d) return null;
    return d === new Date().toDateString() ? 'Today' : new Date(m.ts).toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' });
  }

  fmtParams(p: Record<string, unknown>): string {
    const entries = Object.entries(p ?? {});
    return entries.length ? entries.map(([k, v]) => `${k}=${String(v)}`).join(', ') : 'no parameters';
  }
}
