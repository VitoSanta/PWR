import { MatButtonModule } from '@angular/material/button';
import {
  AfterViewInit,
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  Injectable,
  OnDestroy,
  computed,
  effect,
  inject,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { DomSanitizer, SafeResourceUrl } from '@angular/platform-browser';
import { ActivityStore } from '../../core/activity';
import { AgentStore } from '../../core/agent.store';
import { bridge, inTauri } from '../../core/bridge';
import { bytes, percent } from '../../core/format';
import { DownloadView, FileDiff } from '../../core/model';
import { TerminalService } from '../../core/terminal';
import { ConfirmService, ToastService } from '../../core/ui';
import { WorkbenchStore } from '../../core/workbench';
import { Diff, diffStats } from '../diff';
import { Icon, IconName } from '../kit/icon';
import { Action, EmptyState, ToolBar, ToolTab } from '../kit/tool';
import { Tooltip } from '../kit/tooltip';

async function copyText(toast: ToastService, text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast.show('Copied', 'success', 1800);
  } catch {
    toast.show('Could not copy to the clipboard', 'danger');
  }
}

/** Review: what PWR changed in this conversation, file by file. */
@Component({
  selector: 'pa-review-card',
  imports: [MatButtonModule, Diff, Icon, Action, ToolBar, EmptyState],
  template: `
    @if (store.changes().length) {
      <pa-tool-bar>
        <span class="num">{{ store.changes().length }} file{{ store.changes().length === 1 ? '' : 's' }}</span>
        <span class="num text-success">+{{ totals().added }}</span>
        <span class="num text-danger">−{{ totals().removed }}</span>
        <span class="spacer"></span>
        <button matButton class="btn btn-sm btn-danger-quiet" (click)="revertAll()" [disabled]="store.turnActive()">
          <pa-icon name="undo" [size]="16" /> Revert all
        </button>
      </pa-tool-bar>
      <div class="card-scroll card-pad review-list">
        @for (change of store.changes(); track change.path; let first = $first) {
          <details class="change" [open]="first">
            <summary>
              <pa-icon class="chevron" name="chevron-right" [size]="14" />
              <span class="change-path truncate" [attr.title]="change.path">{{ change.path }}</span>
              <span class="delta">
                <span class="add">+{{ stats(change).added }}</span>
                <span class="del">−{{ stats(change).removed }}</span>
              </span>
              <pa-action
                icon="file"
                (click)="$event.preventDefault(); $event.stopPropagation(); work.openFile(change.path)"
                [label]="'Open ' + change.path + ' in Editor'"
              />
              <pa-action
                icon="undo"
                (click)="$event.preventDefault(); $event.stopPropagation(); revert(change)"
                [disabled]="store.turnActive()"
                [label]="'Revert ' + change.path"
              />
            </summary>
            <pa-diff [diff]="change" />
          </details>
        }
      </div>
    } @else {
      <pa-empty-state icon="git-compare" title="No changes yet">Files PWR edits in this conversation appear here, with their diffs.</pa-empty-state>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ReviewCard {
  protected readonly store = inject(AgentStore);
  protected readonly work = inject(WorkbenchStore);
  private readonly confirm = inject(ConfirmService);
  private readonly toast = inject(ToastService);

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

type Command = 'verify' | 'changes' | 'report' | 'diagnose' | 'doctor';

/** Plan & checks: the workspace's checks and the session's reports. */
@Component({
  selector: 'pa-plan-card',
  imports: [MatButtonModule, Icon, Tooltip, Action, EmptyState],
  template: `
    <div class="card-pad card-scroll">
      <div class="plan-actions">
        @for (command of commands; track command.id) {
          @if (command.id === 'verify') {
            <button matButton="filled" class="btn btn-primary" (click)="store.runCommand(command.id)" [disabled]="!!store.commandRunning()" [attr.aria-busy]="store.commandRunning() === command.id" [paTooltip]="command.help">
              <pa-icon [name]="command.icon" [size]="16" /> {{ command.label }}
            </button>
          } @else {
            <button matButton="tonal" class="btn" (click)="store.runCommand(command.id)" [disabled]="!!store.commandRunning()" [attr.aria-busy]="store.commandRunning() === command.id" [paTooltip]="command.help">
              <pa-icon [name]="command.icon" [size]="16" /> {{ command.label }}
            </button>
          }
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
            <pa-action icon="copy" label="Copy output" (click)="copy(output.text)" />
          </header>
          <pre class="output">{{ output.text }}</pre>
        </section>
      } @else {
        <pa-empty-state icon="shield-check" title="Nothing run yet">Run the workspace's checks, list the changes, or read the session's report.</pa-empty-state>
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PlanCard {
  protected readonly store = inject(AgentStore);
  private readonly toast = inject(ToastService);
  protected readonly commands: { id: Command; label: string; icon: IconName; help: string }[] = [
    { id: 'verify', label: 'Verify', icon: 'shield-check', help: "Run the workspace's own checks" },
    { id: 'changes', label: 'Changes', icon: 'git-compare', help: 'List the files this session changed' },
    { id: 'report', label: 'Report', icon: 'file-text', help: 'Read what happened in this session' },
    { id: 'diagnose', label: 'Diagnose', icon: 'activity', help: 'What happened, including failed generations' },
    { id: 'doctor', label: 'Doctor', icon: 'stethoscope', help: 'Check this machine and the engine' },
  ];

  protected copy(text: string): Promise<void> {
    return copyText(this.toast, text);
  }
}

/** Activity: what runs in the background, and the core's log. */
@Component({
  selector: 'pa-activity-card',
  imports: [MatButtonModule, Icon, Action],
  template: `
    <div class="card-pad card-scroll">
      <ul class="activity-list">
        @if (activity.summarising(); as job) {
          <li class="activity-item">
            @if (job.state === 'failed') {
              <pa-icon name="alert" [size]="16" />
            } @else if (job.state === 'finished') {
              <pa-icon name="check-circle" [size]="16" />
            } @else {
              <span class="spinner spinner-sm" aria-hidden="true"></span>
            }
            <div class="activity-text">
              <span>Wiki summaries</span>
              <span class="t-meta">
                @switch (job.state) {
                  @case ('failed') { {{ job.error || 'Summary generation failed' }} }
                  @case ('finished') { {{ job.written }} written }
                  @case ('writing') { Writing {{ label(job.module) }} · {{ job.written }} done }
                  @default { Starting }
                }
              </span>
            </div>
          </li>
        }
        @for (download of activity.downloads(); track download.downloadId) {
          <li class="activity-item">
            <span class="spinner spinner-sm" aria-hidden="true"></span>
            <div class="activity-text">
              <span class="truncate">{{ download.modelRef || download.downloadId }}</span>
              <div class="progress" role="progressbar" [attr.aria-valuenow]="progress(download)" aria-valuemin="0" aria-valuemax="100">
                <span [style.width.%]="progress(download)"></span>
              </div>
            </div>
          </li>
        }
        @if (!activity.summarising() && !activity.downloads().length) {
          <li class="card-empty">Nothing is running in the background.</li>
        }
      </ul>
      <section class="output-block log-block">
        <header class="output-head">
          <span class="truncate mono" [attr.title]="store.corePath()">Core log</span>
          <span class="spacer"></span>
          <pa-action icon="copy" label="Copy log" [disabled]="!store.logs().length" (click)="copy(store.logs().join('\\n'))" />
        </header>
        <pre class="output">{{ store.logs().join('\\n') || 'The core has written nothing to its log.' }}</pre>
      </section>
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ActivityCard {
  protected readonly store = inject(AgentStore);
  protected readonly activity = inject(ActivityStore);
  private readonly toast = inject(ToastService);

  protected label(module?: string): string {
    return (module ?? '').replace(/^dir:/, '') || 'the project';
  }

  protected progress(download: DownloadView): number {
    const state = download.state;
    if ('total' in state && 'bytes' in state) return percent(state.bytes, state.total);
    return 'bytes' in state && download.totalBytes ? percent(state.bytes, download.totalBytes) : 0;
  }

  protected copy(text: string): Promise<void> {
    return copyText(this.toast, text);
  }
}

export { FilesCard } from './editor';

/** Terminal: the person's shell in the workspace. */
@Component({
  selector: 'pa-terminal-card',
  imports: [MatButtonModule, Icon, Action, ToolBar, ToolTab, EmptyState],
  template: `
    <div class="tool-tabs" role="tablist" aria-label="Shells">
      @for (session of terminal.sessions(); track session.key) {
        <pa-tool-tab
          animate.leave="tool-out"
          icon="terminal"
          [label]="(session.modelOwned ? 'PWR process' : 'Terminal') + ' ' + session.key"
          [note]="session.state() === 'exited' ? 'ended' : ''"
          [active]="terminal.activeId() === session.key"
          (keydown)="tabKey($event, session.key)"
          (selected)="terminal.select(session.key)"
          (closed)="terminal.closeSession(session.key)"
        />
      }
      <pa-action icon="plus" label="New terminal" (click)="terminal.newSession()" />
    </div>
    @if (terminal.active(); as active) {
      <pa-tool-bar>
        <span class="tool-status" [class.is-model]="active.controller() === 'model'">{{ active.modelOwned ? (active.launch.sandboxed ? 'Sandboxed process' : 'Process with full access') : 'Your shell' }} · {{ active.controller() === 'model' ? 'PWR controls input' : 'You control input' }}</span>
        <span class="spacer"></span>
        @if (active.modelOwned) {
          @if (active.controller() === 'model') { <button matButton class="btn btn-sm" (click)="active.takeControl()">Take control</button> }
          @else { <button matButton="tonal" class="btn btn-sm" (click)="active.giveControl()"><pa-icon name="sparkles" [size]="16" />Give control to PWR</button> }
        }
        <pa-action icon="stop" tone="danger" label="Stop the running command" [disabled]="active.state() !== 'running'" (click)="active.interrupt()" />
      </pa-tool-bar>
    }
    @if (terminal.state() === 'unavailable') {
      <pa-empty-state icon="terminal" title="Desktop app only">The terminal runs in the PWR app, not in a browser preview.</pa-empty-state>
    } @else {
      @if (terminal.error()) {
        <p class="banner banner-danger tool-banner" role="alert"><pa-icon name="alert" [size]="16" />{{ terminal.error() }}</p>
      }
      @if (terminal.state() === 'exited') {
        <pa-tool-bar>
          <span class="tool-status">The shell has ended.</span>
          <span class="spacer"></span>
          <button matButton="tonal" class="btn btn-sm" (click)="terminal.restart()"><pa-icon name="refresh" [size]="16" /> New shell</button>
        </pa-tool-bar>
      }
      <div #host class="terminal-host" (click)="terminal.focus()"></div>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TerminalCard implements AfterViewInit, OnDestroy {
  protected readonly terminal = inject(TerminalService);
  private readonly host = viewChild<ElementRef<HTMLElement>>('host');
  private observer?: ResizeObserver;

  protected tabKey(event: KeyboardEvent, key: number): void {
    const sessions = this.terminal.sessions();
    const index = sessions.findIndex(s => s.key === key);
    let next = index;
    if (event.key === 'ArrowRight') next = (index + 1) % sessions.length;
    else if (event.key === 'ArrowLeft') next = (index + sessions.length - 1) % sessions.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = sessions.length - 1;
    else return;
    event.preventDefault();
    const list = (event.currentTarget as HTMLElement).closest('[role=tablist]');
    void this.terminal.select(sessions[next].key).then(() => {
      list?.querySelectorAll<HTMLElement>('[role=tab]')[next]?.focus();
    });
  }

  ngAfterViewInit(): void {
    const host = this.host()?.nativeElement;
    if (!host) {
      // Outside the app: attach only to learn the terminal is unavailable.
      void this.terminal.attach(document.createElement('div'));
      return;
    }
    void this.terminal.attach(host);
    this.observer = new ResizeObserver(() => this.terminal.resize());
    this.observer.observe(host);
  }

  ngOnDestroy(): void {
    this.observer?.disconnect();
    this.terminal.detach();
  }
}

export { BrowserCard } from './browser';


function message(error: unknown): string {
  return String(error).replace(/^Error: /, '');
}

