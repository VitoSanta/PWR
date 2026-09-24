import { ChangeDetectionStrategy, Component, EventEmitter, HostListener, inject, Input, Output, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';

@Component({
  selector: 'pa-sidebar',
  template: `
    <aside class="sidebar">
      <div class="brand">
        <img class="logo" src="/pwr-mark.png" alt="" />
        <div class="brand-copy">
          <strong>PWR</strong>
          <small>local engineering agent</small>
        </div>
        <button class="sidecar-toggle" (click)="toggleCollapsed()" [attr.aria-expanded]="!collapsed()" [attr.aria-label]="collapsed() ? 'Apri barra laterale sinistra' : 'Chiudi barra laterale sinistra'" title="Mostra o nascondi la barra laterale">
          <span class="sidecar-icon sidecar-icon-left" aria-hidden="true"></span>
        </button>
      </div>

      <div class="mode-control">
        <nav class="mode-toggle" aria-label="Conversation mode" [attr.aria-busy]="store.switchingMode() || store.switchingWorkspace()">
          <button [class.active-mode]="!store.chatMode()" (click)="store.chatMode() ? store.leaveChat() : null" [disabled]="store.turnActive() || !store.chatMode() || store.switchingMode() || store.switchingWorkspace()">Agente</button>
          <button [class.active-mode]="store.chatMode()" (click)="store.chatMode() ? null : store.openChat()" [disabled]="store.turnActive() || store.chatMode() || !store.chatHome() || store.switchingMode() || store.switchingWorkspace()">Solo Chat</button>
        </nav>
        @if (store.switchingMode() || store.switchingWorkspace()) {
          <span class="mode-loading" role="status"><span class="spinner" aria-hidden="true"></span>Apertura workspace…</span>
        }
      </div>

      <button class="new" (click)="store.newConversation()" [disabled]="store.turnActive()">＋ New conversation</button>

      <section class="panel">
        <h3>{{ store.chatMode() ? 'Global chats' : 'Workspace' }}</h3>
        @if (store.chatMode()) {
          <div class="chat-mode" [title]="store.chatHome() + ' · The model reads only attachments and cannot edit or run anything.'">
            <span class="folder">💬</span>
            <span class="path">Saved globally · no project workspace</span>
          </div>
        } @else {
          <button class="workspace" (click)="store.chooseWorkspace()" [title]="store.workspace()" [disabled]="store.switchingMode() || store.switchingWorkspace()">
            <span class="folder">🗂</span>
            <span class="path">{{ short(store.workspace()) }}</span>
          </button>
        }
      </section>

      <section class="panel sessions">
        <h3>Conversations</h3>
        @for (session of store.sessions(); track session.sessionId) {
          <div class="session-row" [class.current]="session.sessionId === store.sessionId()">
            <button class="session" (click)="store.resume(session.sessionId)" [disabled]="store.turnActive()">
              <span>{{ session.title || 'Untitled' }}</span>
              <small>{{ when(session.updatedAt) }}</small>
            </button>
            <button class="session-delete" (click)="askDelete(session.sessionId, session.title)" [disabled]="store.turnActive()" [attr.aria-label]="'Elimina chat ' + (session.title || 'senza titolo')" title="Elimina chat">
              <svg viewBox="0 0 20 20" fill="none" aria-hidden="true"><path d="M4.5 4.5 15.5 15.5M15.5 4.5 4.5 15.5" /></svg>
            </button>
          </div>
        } @empty {
          <p class="muted">No conversations yet.</p>
        }
        @if (deleteError()) { <p class="warn">{{ deleteError() }}</p> }
      </section>

    </aside>
    <div class="resize-handle resize-handle-left" role="separator" aria-label="Ridimensiona barra laterale sinistra" aria-orientation="vertical" [attr.aria-valuenow]="width" aria-valuemin="230" aria-valuemax="500" tabindex="0" (pointerdown)="startResize($event)" (pointermove)="moveResize($event)" (pointerup)="endResize($event)" (pointercancel)="endResize($event)" (keydown)="resizeByKey($event)"></div>
    @if (pendingDelete(); as session) {
      <div class="scrim">
        <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="delete-chat-title" aria-describedby="delete-chat-description">
          <span class="shield delete-shield" aria-hidden="true">×</span>
          <h2 id="delete-chat-title">Eliminare questa chat?</h2>
          <p class="asked">{{ session.title || 'Chat senza titolo' }}</p>
          <p id="delete-chat-description" class="muted">La chat scomparirà dalla cronologia e non potrà essere riaperta. I file del workspace restano invariati; gli eventi rimangono nel registro di audit.</p>
          @if (deleteError()) { <p class="warn">{{ deleteError() }}</p> }
          <div class="choices">
            <button class="ghost" (click)="cancelDelete()" [disabled]="deleting()">Annulla</button>
            <button class="danger" (click)="confirmDelete()" [disabled]="deleting() || store.turnActive()">{{ deleting() ? 'Eliminazione…' : 'Elimina chat' }}</button>
          </div>
        </div>
      </div>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '[class.collapsed]': 'collapsed()' },
})
export class Sidebar {
  protected readonly store = inject(AgentStore);
  protected readonly collapsed = signal(typeof window !== 'undefined' && window.innerWidth <= 760);
  protected readonly deleteError = signal('');
  protected readonly pendingDelete = signal<{ sessionId: string; title: string } | null>(null);
  protected readonly deleting = signal(false);
  @Input() width = 272;
  @Output() widthChange = new EventEmitter<number>();
  private drag: { pointerId: number; x: number; width: number } | null = null;

  protected toggleCollapsed(): void { this.collapsed.update((value) => !value); }

  protected askDelete(sessionId: string, title: string): void {
    this.deleteError.set('');
    this.pendingDelete.set({ sessionId, title });
  }

  protected cancelDelete(): void {
    if (!this.deleting()) this.pendingDelete.set(null);
  }

  @HostListener('document:keydown.escape')
  protected escapeDelete(): void { this.cancelDelete(); }

  protected async confirmDelete(): Promise<void> {
    const session = this.pendingDelete();
    if (!session || this.deleting() || this.store.turnActive()) return;
    this.deleting.set(true);
    try {
      await this.store.deleteConversation(session.sessionId);
      this.deleteError.set('');
      this.pendingDelete.set(null);
    } catch (error) {
      this.deleteError.set(String(error));
    } finally {
      this.deleting.set(false);
    }
  }

  protected startResize(event: PointerEvent): void {
    this.drag = { pointerId: event.pointerId, x: event.clientX, width: this.width };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    event.preventDefault();
  }

  protected moveResize(event: PointerEvent): void {
    if (!this.drag || this.drag.pointerId !== event.pointerId) return;
    this.widthChange.emit(this.clampWidth(this.drag.width + event.clientX - this.drag.x));
  }

  protected endResize(event: PointerEvent): void {
    if (this.drag?.pointerId !== event.pointerId) return;
    this.drag = null;
    (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
  }

  protected resizeByKey(event: KeyboardEvent): void {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    this.widthChange.emit(this.clampWidth(this.width + (event.key === 'ArrowRight' ? 16 : -16)));
    event.preventDefault();
  }

  private clampWidth(width: number): number {
    const right = document.querySelector('pa-inspector')?.getBoundingClientRect().width ?? 48;
    return Math.max(230, Math.min(500, window.innerWidth - right - 600, width));
  }

  protected short(path: string): string {
    const parts = path.split('/').filter(Boolean);
    return parts.length > 2 ? '…/' + parts.slice(-2).join('/') : path || 'Choose a folder';
  }

  protected when(value: string): string {
    const date = new Date(value);
    return isNaN(date.getTime()) ? '' : date.toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' });
  }
}
