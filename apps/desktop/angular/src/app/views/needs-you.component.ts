import { Component, inject } from '@angular/core';
import { TauriService } from '../services/tauri.service';
import { CeoChatService } from '../services/ceo-chat.service';
import { LocalWhenPipe, TimeAgoPipe } from '../shared/time-ago.pipe';

/** Human gates: tasks the agents cannot finish on their own. */
@Component({
  selector: 'app-needs-you',
  standalone: true,
  imports: [TimeAgoPipe, LocalWhenPipe],
  template: `
    <header class="view-header">
      <div>
        <h1>Needs You</h1>
        <p class="view-sub">Human gates raised by the agents. Resolving one lets the blocked job continue on its next run.</p>
      </div>
    </header>

    @if (ceo.pending(); as p) {
      <section class="panel">
        <div class="panel-header"><h2>CEO action awaiting confirmation</h2></div>
        <div class="pending-box">
          <span><strong>{{ p.tool }}</strong> requested via {{ p.channel }} {{ p.requested_at | timeAgo }}</span>
          <div class="row-actions">
            <button class="btn btn-primary btn-sm" (click)="ceo.confirm()" [disabled]="ceo.streaming()">Confirm</button>
            <button class="btn btn-ghost btn-sm" (click)="ceo.cancel()" [disabled]="ceo.streaming()">Cancel</button>
          </div>
        </div>
      </section>
    }

    <section class="panel">
      <div class="panel-header">
        <h2>Open tasks</h2>
        <span class="badge" [class.badge-warn]="tauri.humanTasks().length">{{ tauri.humanTasks().length }}</span>
      </div>
      @for (t of tauri.humanTasks(); track t.id) {
        <article class="task-card">
          <div class="task-head">
            <span class="chip chip-muted">{{ t.challenge_type }}</span>
            <span class="task-title">{{ t.title }}</span>
            <span class="muted small">{{ t.created_at | localWhen }}</span>
          </div>
          <p class="task-instructions">{{ t.instructions }}</p>
          @if (t.automation_id) { <div class="muted small mono">{{ t.automation_id }}</div> }
          <div class="row-actions">
            @if (t.page_url) {
              <button class="btn btn-ghost btn-sm" (click)="tauri.openExternal(t.page_url!)">Open page</button>
            }
            <button class="btn btn-primary btn-sm" (click)="tauri.resolveTask(t.id)">Mark resolved</button>
          </div>
        </article>
      } @empty {
        <div class="empty">No human task is open. CAPTCHAs, sign-ins, approvals and Nova's launch gates appear here.</div>
      }
    </section>
  `,
})
export class NeedsYouComponent {
  tauri = inject(TauriService);
  ceo = inject(CeoChatService);
}
