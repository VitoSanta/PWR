import { ChangeDetectionStrategy, Component, EventEmitter, inject, Input, Output, signal } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { Diff, diffStats } from './diff';

@Component({
  selector: 'pa-inspector',
  imports: [Diff],
  template: `
    <aside class="inspector">
      <div class="inspector-heading">
        <button class="sidecar-toggle" (click)="toggleCollapsed()" [attr.aria-expanded]="!collapsed()" [attr.aria-label]="collapsed() ? 'Apri barra laterale destra' : 'Chiudi barra laterale destra'" title="Mostra o nascondi la barra laterale">
          <span class="sidecar-icon sidecar-icon-right" aria-hidden="true"></span>
        </button>
        <strong>Changes</strong>
      </div>
      <nav class="tabs">
        @for (tab of tabs; track tab.id) {
          <button [class.active]="active() === tab.id" (click)="active.set(tab.id)">
            {{ tab.label }}
            @if (tab.id === 'changes' && store.changes().length) { <span class="badge">{{ store.changes().length }}</span> }
          </button>
        }
      </nav>

      @switch (active()) {
        @case ('changes') {
          @if (store.changes().length) {
            <div class="revert-all-row"><button class="ghost" (click)="revertAll()" [disabled]="store.turnActive()">Revert all</button></div>
          }
          <div class="pane">
            @for (change of store.changes(); track change.path; let first = $first) {
              <details class="change" [open]="first">
                <summary>
                  <span class="change-path">{{ change.path }}</span>
                  <button class="change-revert" (click)="$event.preventDefault(); $event.stopPropagation(); revert(change)" [disabled]="store.turnActive()" [attr.aria-label]="'Revert ' + change.path">Revert</button>
                  <span class="delta">
                    <span class="add">+{{ stats(change).added }}</span>
                    <span class="del">−{{ stats(change).removed }}</span>
                  </span>
                </summary>
                <pa-diff [diff]="change" />
              </details>
            } @empty {
              <p class="muted">No file changed in this conversation yet.</p>
            }
            @if (revertError()) { <p class="warn">{{ revertError() }}</p> }
          </div>
        }
        @case ('evidence') {
          <div class="pane">
            <div class="commands">
              @for (command of commands; track command) {
                <button (click)="store.runCommand(command)">{{ command }}</button>
              }
            </div>
            @if (store.commandOutput(); as output) {
              <h4>{{ output.name }}</h4>
              <pre class="output">{{ output.text }}</pre>
            } @else {
              <p class="muted">Run the repository's checks, list the changes, or read the session's report.</p>
            }
          </div>
        }
        @case ('log') {
          <div class="pane">
            <p class="muted small">{{ store.corePath() }}</p>
            <pre class="output log">{{ store.logs().join('\\n') || 'The core has written nothing to its log.' }}</pre>
          </div>
        }
      }
    </aside>
    <div class="resize-handle resize-handle-right" role="separator" aria-label="Ridimensiona barra laterale destra" aria-orientation="vertical" [attr.aria-valuenow]="width" aria-valuemin="230" aria-valuemax="500" tabindex="0" (pointerdown)="startResize($event)" (pointermove)="moveResize($event)" (pointerup)="endResize($event)" (pointercancel)="endResize($event)" (keydown)="resizeByKey($event)"></div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '[class.collapsed]': 'collapsed()' },
})
export class Inspector {
  protected readonly store = inject(AgentStore);
  protected readonly collapsed = signal(typeof window !== 'undefined' && window.innerWidth <= 1180);
  @Input() width = 360;
  @Output() widthChange = new EventEmitter<number>();
  private drag: { pointerId: number; x: number; width: number } | null = null;
  protected readonly revertError = signal('');
  protected readonly active = signal<'changes' | 'evidence' | 'log'>('changes');
  protected readonly tabs = [
    { id: 'changes' as const, label: 'Changes' },
    { id: 'evidence' as const, label: 'Evidence' },
    { id: 'log' as const, label: 'Core log' },
  ];
  protected readonly commands = ['verify', 'changes', 'report', 'diagnose', 'doctor'] as const;

  protected toggleCollapsed(): void { this.collapsed.update((value) => !value); }

  protected startResize(event: PointerEvent): void {
    this.drag = { pointerId: event.pointerId, x: event.clientX, width: this.width };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    event.preventDefault();
  }

  protected moveResize(event: PointerEvent): void {
    if (!this.drag || this.drag.pointerId !== event.pointerId) return;
    this.widthChange.emit(this.clampWidth(this.drag.width - (event.clientX - this.drag.x)));
  }

  protected endResize(event: PointerEvent): void {
    if (this.drag?.pointerId !== event.pointerId) return;
    this.drag = null;
    (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
  }

  protected resizeByKey(event: KeyboardEvent): void {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    this.widthChange.emit(this.clampWidth(this.width + (event.key === 'ArrowLeft' ? 16 : -16)));
    event.preventDefault();
  }

  private clampWidth(width: number): number {
    const left = document.querySelector('pa-sidebar')?.getBoundingClientRect().width ?? 48;
    return Math.max(230, Math.min(500, window.innerWidth - left - 600, width));
  }

  protected stats(change: { oldText: string; newText: string }) {
    return diffStats(change.oldText, change.newText);
  }

  protected async revert(change: import('../core/model').FileDiff): Promise<void> {
    if (!confirm(`Restore ${change.path} to how it was before this conversation changed it?`)) return;
    try { await this.store.revertChange(change); this.revertError.set(''); }
    catch (error) { this.revertError.set(String(error)); }
  }

  protected async revertAll(): Promise<void> {
    if (!confirm(`Restore all ${this.store.changes().length} changed files to their original contents?`)) return;
    try { await this.store.revertAllChanges(); this.revertError.set(''); }
    catch (error) { this.revertError.set(String(error)); }
  }
}
