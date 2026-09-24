import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { AgentStore } from '../core/agent.store';
import { LayoutService, RIGHT } from '../core/layout';
import { FileDiff } from '../core/model';
import { ConfirmService, SHORTCUTS, ToastService, UiStore, roveFocus } from '../core/ui';
import { Diff, diffStats } from './diff';
import { Icon, IconName } from './kit/icon';
import { ResizeHandle } from './kit/resize-handle';
import { Tooltip } from './kit/tooltip';

type Tab = 'changes' | 'evidence' | 'log';
type Command = 'verify' | 'changes' | 'report' | 'diagnose' | 'doctor';

@Component({
  selector: 'pa-inspector',
  imports: [Diff, Icon, Tooltip, ResizeHandle],
  template: `
    <aside class="inspector" aria-label="Inspector">
      <header class="inspector-head titlebar-row" data-tauri-drag-region="deep">
        <div class="tabs" role="tablist" aria-label="Inspector" (keydown)="tabKeys($event)">
          @for (tab of tabs; track tab.id) {
            <button
              class="tab"
              role="tab"
              [id]="'inspector-tab-' + tab.id"
              [attr.aria-selected]="active() === tab.id"
              aria-controls="inspector-panel"
              [attr.tabindex]="active() === tab.id ? 0 : -1"
              (click)="active.set(tab.id)"
            >
              {{ tab.label }}
              @if (tab.id === 'changes' && store.changes().length) {
                <span class="count" [attr.aria-label]="store.changes().length + ' changed files'">{{ store.changes().length }}</span>
              }
            </button>
          }
        </div>
        <button
          class="icon-btn icon-btn-sm"
          (click)="layout.toggleRight()"
          aria-label="Hide inspector"
          paTooltip="Hide inspector"
          [paTooltipKeys]="keys.toggleInspector"
        >
          <pa-icon name="x" [size]="16" />
        </button>
      </header>

      <div class="inspector-body" role="tabpanel" id="inspector-panel" [attr.aria-labelledby]="'inspector-tab-' + active()">
        @switch (active()) {
          @case ('changes') {
            @if (store.changes().length) {
              <div class="inspector-toolbar">
                <span class="num">{{ store.changes().length }} file{{ store.changes().length === 1 ? '' : 's' }} changed</span>
                <span class="num text-success">+{{ totals().added }}</span>
                <span class="num text-danger">−{{ totals().removed }}</span>
                <span class="spacer"></span>
                <button class="btn btn-sm btn-ghost" (click)="revertAll()" [disabled]="store.turnActive()">
                  <pa-icon name="undo" [size]="14" /> Revert all
                </button>
              </div>
              <div class="inspector-pad">
                @for (change of store.changes(); track change.path; let first = $first) {
                  <details class="change" [open]="first">
                    <summary>
                      <pa-icon class="chevron" name="chevron-right" [size]="14" />
                      <span class="change-path truncate" [attr.title]="change.path">{{ change.path }}</span>
                      <span class="delta">
                        <span class="add">+{{ stats(change).added }}</span>
                        <span class="del">−{{ stats(change).removed }}</span>
                      </span>
                      <button
                        class="icon-btn icon-btn-sm"
                        (click)="$event.preventDefault(); $event.stopPropagation(); revert(change)"
                        [disabled]="store.turnActive()"
                        [attr.aria-label]="'Revert ' + change.path"
                        paTooltip="Revert this file"
                      >
                        <pa-icon name="undo" [size]="14" />
                      </button>
                    </summary>
                    <pa-diff [diff]="change" />
                  </details>
                }
              </div>
            } @else {
              <div class="empty-state">
                <span class="empty-state-icon"><pa-icon name="git-compare" /></span>
                <p class="empty-state-title">No changes yet</p>
                <p class="empty-state-text">Files PWR edits in this conversation appear here, with their diffs.</p>
              </div>
            }
          }
          @case ('evidence') {
            <div class="inspector-pad">
              <div class="evidence-actions">
                @for (command of commands; track command.id) {
                  <button
                    [class]="command.id === 'verify' ? 'btn btn-primary' : 'btn'"
                    (click)="store.runCommand(command.id)"
                    [disabled]="!!store.commandRunning()"
                    [attr.aria-busy]="store.commandRunning() === command.id"
                    [paTooltip]="command.help"
                  >
                    <pa-icon [name]="command.icon" [size]="16" />
                    {{ command.label }}
                  </button>
                }
              </div>
              @if (store.commandOutput(); as output) {
                <section class="output-block" [attr.aria-busy]="!!store.commandRunning()">
                  <header class="output-head">
                    {{ output.name }}
                    @if (store.commandRunning()) {
                      <span class="spinner spinner-sm" aria-hidden="true"></span>
                    }
                    <span class="spacer"></span>
                    <button class="icon-btn icon-btn-sm" (click)="copy(output.text)" aria-label="Copy output" paTooltip="Copy">
                      <pa-icon name="copy" [size]="14" />
                    </button>
                  </header>
                  <pre class="output">{{ output.text }}</pre>
                </section>
              } @else {
                <p class="fine">Run the repository's checks, list the changes, or read the session's report.</p>
              }
            </div>
          }
          @case ('log') {
            <section class="output-block log-block">
              <header class="output-head">
                <span class="truncate mono" [attr.title]="store.corePath()">{{ store.corePath() || 'Core log' }}</span>
                <span class="spacer"></span>
                <button
                  class="icon-btn icon-btn-sm"
                  (click)="copy(store.logs().join('\\n'))"
                  [disabled]="!store.logs().length"
                  aria-label="Copy log"
                  paTooltip="Copy log"
                >
                  <pa-icon name="copy" [size]="14" />
                </button>
              </header>
              <pre class="output">{{ store.logs().join('\\n') || 'The core has written nothing to its log.' }}</pre>
            </section>
          }
        }
      </div>
    </aside>
    @if (layout.right() === 'docked') {
      <pa-resize-handle
        edge="left"
        label="Resize inspector"
        [width]="layout.rightWidth()"
        [min]="bounds.min"
        [max]="bounds.max"
        [initial]="bounds.initial"
        (resize)="layout.setRightWidth($event)"
      />
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Inspector {
  protected readonly store = inject(AgentStore);
  protected readonly layout = inject(LayoutService);
  private readonly confirm = inject(ConfirmService);
  private readonly toast = inject(ToastService);
  protected readonly keys = SHORTCUTS;
  protected readonly bounds = RIGHT;
  protected readonly active = inject(UiStore).inspectorTab;
  protected readonly tabs: { id: Tab; label: string }[] = [
    { id: 'changes', label: 'Changes' },
    { id: 'evidence', label: 'Evidence' },
    { id: 'log', label: 'Core log' },
  ];
  protected readonly commands: { id: Command; label: string; icon: IconName; help: string }[] = [
    { id: 'verify', label: 'Verify', icon: 'shield-check', help: "Run the workspace's own checks" },
    { id: 'changes', label: 'Changes', icon: 'git-compare', help: 'List the files this session changed' },
    { id: 'report', label: 'Report', icon: 'file-text', help: 'Read what happened in this session' },
    { id: 'diagnose', label: 'Diagnose', icon: 'activity', help: 'What happened, including failed generations' },
    { id: 'doctor', label: 'Doctor', icon: 'stethoscope', help: 'Check this machine and the engine' },
  ];

  protected readonly totals = computed(() =>
    this.store.changes().reduce(
      (sum, change) => {
        const stats = diffStats(change.oldText, change.newText);
        return { added: sum.added + stats.added, removed: sum.removed + stats.removed };
      },
      { added: 0, removed: 0 },
    ),
  );

  protected stats(change: { oldText: string; newText: string }) {
    return diffStats(change.oldText, change.newText);
  }

  protected tabKeys(event: KeyboardEvent): void {
    if (roveFocus(event, event.currentTarget as HTMLElement, '[role=tab]', 'horizontal')) {
      (document.activeElement as HTMLElement | null)?.click();
    }
  }

  protected async copy(text: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      this.toast.show('Copied to clipboard', 'success', 1800);
    } catch {
      this.toast.show('Could not copy to the clipboard', 'danger');
    }
  }

  protected async revert(change: FileDiff): Promise<void> {
    const reverted = await this.confirm.ask({
      title: 'Revert this file?',
      message: 'It is restored to how it was before this conversation changed it.',
      subject: change.path,
      confirmLabel: 'Revert file',
      tone: 'danger',
      action: () => this.store.revertChange(change),
    });
    if (reverted) this.toast.show(`Reverted ${change.path.split('/').pop()}`, 'success');
  }

  protected async revertAll(): Promise<void> {
    const count = this.store.changes().length;
    const reverted = await this.confirm.ask({
      title: `Revert all ${count} changed file${count === 1 ? '' : 's'}?`,
      message: 'Each file is restored to its contents before this conversation changed it.',
      confirmLabel: 'Revert all',
      tone: 'danger',
      action: () => this.store.revertAllChanges(),
    });
    if (reverted) this.toast.show('All changes reverted', 'success');
  }
}
