import {
  AfterViewInit,
  ChangeDetectionStrategy,
  Component,
  ElementRef,
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
import { bridge } from '../../core/bridge';
import { bytes, percent } from '../../core/format';
import { DownloadView, FileDiff } from '../../core/model';
import { TerminalService } from '../../core/terminal';
import { ConfirmService, ToastService } from '../../core/ui';
import { WorkbenchStore } from '../../core/workbench';
import { Diff, diffStats } from '../diff';
import { Icon, IconName } from '../kit/icon';
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
  imports: [Diff, Icon, Tooltip],
  template: `
    @if (store.changes().length) {
      <div class="card-toolbar">
        <span class="num">{{ store.changes().length }} file{{ store.changes().length === 1 ? '' : 's' }}</span>
        <span class="num text-success">+{{ totals().added }}</span>
        <span class="num text-danger">−{{ totals().removed }}</span>
        <span class="spacer"></span>
        <button class="btn btn-sm btn-ghost" (click)="revertAll()" [disabled]="store.turnActive()">
          <pa-icon name="undo" [size]="14" /> Revert all
        </button>
      </div>
      <div class="card-scroll card-pad">
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
                (click)="$event.preventDefault(); $event.stopPropagation(); work.openFile(change.path)"
                [attr.aria-label]="'Open ' + change.path"
                paTooltip="Open in Files"
              >
                <pa-icon name="file" [size]="14" />
              </button>
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
      <p class="card-empty">Files PWR edits in this conversation appear here, with their diffs.</p>
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
  imports: [Icon, Tooltip],
  template: `
    <div class="card-pad card-scroll">
      <div class="plan-actions">
        @for (command of commands; track command.id) {
          <button
            [class]="command.id === 'verify' ? 'btn btn-sm btn-primary' : 'btn btn-sm'"
            (click)="store.runCommand(command.id)"
            [disabled]="!!store.commandRunning()"
            [attr.aria-busy]="store.commandRunning() === command.id"
            [paTooltip]="command.help"
          >
            <pa-icon [name]="command.icon" [size]="14" /> {{ command.label }}
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
        <p class="card-empty">Run the workspace's checks, list the changes, or read the session's report.</p>
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
  imports: [Icon, Tooltip],
  template: `
    <div class="card-pad card-scroll">
      <ul class="activity-list">
        @if (activity.summarising(); as job) {
          <li class="activity-item">
            @if (job.state === 'finished') {
              <pa-icon name="check-circle" [size]="16" />
            } @else {
              <span class="spinner spinner-sm" aria-hidden="true"></span>
            }
            <div class="activity-text">
              <span>Wiki summaries</span>
              <span class="t-meta">
                @switch (job.state) {
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
          <button class="icon-btn icon-btn-sm" (click)="copy(store.logs().join('\\n'))" [disabled]="!store.logs().length" aria-label="Copy log" paTooltip="Copy log">
            <pa-icon name="copy" [size]="14" />
          </button>
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

interface FileEntry {
  name: string;
  path: string;
  dir: boolean;
  bytes: number;
}

/** Files: the workspace, a folder at a time, and one file read-only. */
@Component({
  selector: 'pa-files-card',
  imports: [Icon, Tooltip],
  template: `
    <div class="card-toolbar files-path">
      <button class="icon-btn icon-btn-sm" (click)="up()" [disabled]="!folder() && !file()" aria-label="Up" paTooltip="Up">
        <pa-icon name="arrow-up" [size]="14" />
      </button>
      <span class="truncate mono">{{ file()?.path || folder() || name() }}</span>
      <span class="spacer"></span>
      @if (file(); as open) {
        <button class="icon-btn icon-btn-sm" (click)="copyPath(open.path)" aria-label="Copy path" paTooltip="Copy path">
          <pa-icon name="copy" [size]="14" />
        </button>
      }
      <button class="icon-btn icon-btn-sm" (click)="reload()" aria-label="Refresh" paTooltip="Refresh">
        <pa-icon name="refresh" [size]="14" />
      </button>
    </div>
    @if (error()) {
      <p class="banner banner-danger card-banner" role="alert"><pa-icon name="alert" [size]="16" />{{ error() }}</p>
    }
    @if (file(); as open) {
      @if (open.binary) {
        <p class="card-empty">A binary file ({{ size(open.bytes) }}); nothing to show as text.</p>
      } @else {
        <div class="file-view card-scroll" role="document" [attr.aria-label]="open.path" animate.enter="anim-fade-in">
          @for (line of lines(); track $index) {
            <div class="file-line"><span class="file-number" aria-hidden="true">{{ $index + 1 }}</span><span class="file-text">{{ line }}</span></div>
          }
        </div>
        @if (open.truncated) {
          <p class="fine card-pad">Shown up to 1 MB of {{ size(open.bytes) }}.</p>
        }
      }
    } @else {
      <ul class="file-list card-scroll" animate.enter="anim-fade-in">
        @for (entry of entries(); track entry.path) {
          <li>
            <button class="file-entry" (click)="enter(entry)">
              <pa-icon [name]="entry.dir ? 'folder' : 'file'" [size]="14" />
              <span class="truncate">{{ entry.name }}</span>
              @if (!entry.dir) {
                <span class="t-meta num">{{ size(entry.bytes) }}</span>
              }
            </button>
          </li>
        } @empty {
          <li class="card-empty">{{ loading() ? 'Reading…' : 'This folder is empty.' }}</li>
        }
      </ul>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class FilesCard {
  private readonly agent = inject(AgentStore);
  private readonly work = inject(WorkbenchStore);
  private readonly toast = inject(ToastService);
  protected readonly folder = signal('');
  protected readonly entries = signal<FileEntry[]>([]);
  protected readonly file = signal<{ path: string; text?: string; bytes: number; binary: boolean; truncated?: boolean } | null>(null);
  protected readonly loading = signal(false);
  protected readonly error = signal('');
  protected readonly lines = computed(() => (this.file()?.text ?? '').split('\n'));
  protected readonly name = computed(() => this.agent.workspace().split('/').pop() || 'Workspace');
  protected readonly size = bytes;

  constructor() {
    // Another card asked for a file: open it, with its folder behind it.
    effect(() => {
      const requested = this.work.fileRequest();
      if (!requested) return;
      untracked(() => {
        this.work.fileRequest.set(null);
        this.folder.set(requested.split('/').slice(0, -1).join('/'));
        void this.open(requested);
      });
    });
    // A different workspace: back to its root.
    effect(() => {
      this.agent.workspace();
      untracked(() => {
        this.file.set(null);
        this.folder.set('');
        void this.list();
      });
    });
  }

  protected enter(entry: FileEntry): void {
    if (entry.dir) {
      this.folder.set(entry.path);
      void this.list();
    } else {
      void this.open(entry.path);
    }
  }

  protected up(): void {
    if (this.file()) {
      this.file.set(null);
      void this.list();
      return;
    }
    this.folder.set(this.folder().split('/').slice(0, -1).join('/'));
    void this.list();
  }

  protected reload(): void {
    const open = this.file();
    if (open) void this.open(open.path);
    else void this.list();
  }

  protected copyPath(path: string): Promise<void> {
    return copyText(this.toast, path);
  }

  private async list(): Promise<void> {
    if (!this.agent.workspace()) return;
    this.loading.set(true);
    this.error.set('');
    try {
      const reply = await this.agent.call('_pwr/files', { cwd: this.agent.workspace(), path: this.folder() });
      this.entries.set(reply.entries ?? []);
    } catch (error) {
      this.error.set(message(error));
      this.entries.set([]);
    } finally {
      this.loading.set(false);
    }
  }

  private async open(path: string): Promise<void> {
    this.error.set('');
    try {
      this.file.set(await this.agent.call('_pwr/file', { cwd: this.agent.workspace(), path }));
    } catch (error) {
      this.error.set(message(error));
    }
  }
}

/** Terminal: the person's shell in the workspace. */
@Component({
  selector: 'pa-terminal-card',
  imports: [Icon],
  template: `
    @if (terminal.state() === 'unavailable') {
      <p class="card-empty">The terminal runs in the PWR app, not in a browser preview.</p>
    } @else {
      @if (terminal.error()) {
        <p class="banner banner-danger card-banner" role="alert"><pa-icon name="alert" [size]="16" />{{ terminal.error() }}</p>
      }
      @if (terminal.state() === 'exited') {
        <div class="card-toolbar">
          <span class="t-meta">The shell has ended.</span>
          <span class="spacer"></span>
          <button class="btn btn-sm" (click)="terminal.restart()"><pa-icon name="refresh" [size]="14" /> New shell</button>
        </div>
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

/** Browser: a preview of the app the workspace serves on this machine. */
@Component({
  selector: 'pa-browser-card',
  imports: [Icon, Tooltip],
  template: `
    <form class="card-toolbar browser-bar" (submit)="$event.preventDefault(); go(address.value)">
      <button type="button" class="icon-btn icon-btn-sm" (click)="reload()" [disabled]="!url()" aria-label="Reload" paTooltip="Reload">
        <pa-icon name="refresh" [size]="14" />
      </button>
      <input #address class="input input-sm browser-address" [value]="url()" placeholder="localhost:4200" aria-label="Address" spellcheck="false" />
      <button type="button" class="icon-btn icon-btn-sm" (click)="external()" [disabled]="!url()" aria-label="Open in your browser" paTooltip="Open in your browser">
        <pa-icon name="external-link" [size]="14" />
      </button>
    </form>
    @if (error()) {
      <p class="banner banner-danger card-banner" role="alert"><pa-icon name="alert" [size]="16" />{{ error() }}</p>
    }
    @if (frame(); as src) {
      <iframe class="browser-frame" [src]="src" title="Preview" sandbox="allow-scripts allow-same-origin allow-forms allow-modals allow-popups allow-downloads"></iframe>
    } @else {
      <div class="card-pad">
        <p class="card-empty">Preview a page this machine serves: a development server, or one PWR started.</p>
        @if (suggestions().length) {
          <ul class="browser-suggestions">
            @for (address of suggestions(); track address) {
              <li><button class="btn btn-sm" (click)="go(address)"><pa-icon name="globe" [size]="14" /> {{ address }}</button></li>
            }
          </ul>
        }
      </div>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class BrowserCard {
  private readonly agent = inject(AgentStore);
  private readonly sanitizer = inject(DomSanitizer);
  protected readonly url = signal('');
  protected readonly frame = signal<SafeResourceUrl | null>(null);
  protected readonly error = signal('');
  private nonce = 0;

  /** Addresses on this machine the conversation mentioned, newest first. */
  protected readonly suggestions = computed(() => {
    const found: string[] = [];
    const pattern = /https?:\/\/(?:localhost|127\.0\.0\.1|\[::1\]):\d{2,5}[^\s'"`)<>]*/g;
    for (const entry of [...this.agent.timeline()].reverse()) {
      for (const match of entry.text.match(pattern) ?? []) {
        const address = match.replace(/[.,;:]+$/, '');
        if (!found.includes(address)) found.push(address);
      }
      if (found.length >= 5) break;
    }
    return found;
  });

  protected go(raw: string): void {
    const address = normalize(raw);
    if (!address) {
      this.error.set('Only pages on this machine: localhost or 127.0.0.1, with a port.');
      return;
    }
    this.error.set('');
    this.url.set(address);
    this.show(address);
  }

  protected reload(): void {
    if (!this.url()) return;
    this.nonce += 1;
    const separator = this.url().includes('?') ? '&' : '?';
    this.show(`${this.url()}${separator}__pwr=${this.nonce}`);
  }

  protected external(): void {
    if (this.url()) void bridge.openExternal(this.url());
  }

  private show(address: string): void {
    // Only an address `normalize` accepted reaches here: this machine, http.
    this.frame.set(this.sanitizer.bypassSecurityTrustResourceUrl(address));
  }
}

/** A local address, or null: the preview shows only this machine's pages. */
export function normalize(raw: string): string | null {
  let text = raw.trim();
  if (!text) return null;
  if (!/^https?:\/\//i.test(text)) text = `http://${text}`;
  try {
    const url = new URL(text);
    const local = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
    return local && (url.protocol === 'http:' || url.protocol === 'https:') ? url.toString() : null;
  } catch {
    return null;
  }
}

function message(error: unknown): string {
  return String(error).replace(/^Error: /, '');
}

