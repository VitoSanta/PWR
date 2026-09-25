import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  HostListener,
  OnDestroy,
  OnInit,
  effect,
  inject,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { UnlistenFn } from '@tauri-apps/api/event';
import { AgentStore } from '../core/agent.store';
import { inTauri } from '../core/bridge';
import { roveFocus } from '../core/ui';
import { fileKind } from './conversation';
import { Icon } from './kit/icon';
import { Popover } from './kit/popover';
import { Tooltip } from './kit/tooltip';

const MIN_HEIGHT = 40;
const MAX_HEIGHT = 260;

@Component({
  selector: 'pa-composer',
  imports: [Icon, Tooltip, Popover],
  template: `
    <form
      class="composer"
      [class.busy]="store.turnActive()"
      [class.dropping]="dropping()"
      (submit)="$event.preventDefault(); submit()"
      aria-label="Message"
    >
      @if (dropping()) {
        <div class="drop-hint" aria-hidden="true"><pa-icon name="paperclip" [size]="16" /> Drop files or folders to attach</div>
      }
      @if (store.queue().length) {
        <ol class="queue" aria-label="Queued messages">
          @for (queued of store.queue(); track $index; let index = $index) {
            <li class="queued" animate.enter="anim-pop-in" animate.leave="anim-pop-out">
              <span class="queued-badge num">{{ index + 1 }}</span>
              @if (editingQueued() === index) {
                <input
                  class="input input-sm queued-edit"
                  [value]="queued"
                  (keydown.enter)="$event.preventDefault(); commitQueued(index, $any($event.target).value)"
                  (keydown.escape)="editingQueued.set(null)"
                  (blur)="commitQueued(index, $any($event.target).value)"
                  aria-label="Edit queued message"
                />
              } @else {
                <button type="button" class="queued-text" (click)="editingQueued.set(index)" paTooltip="Edit">{{ queued }}</button>
              }
              @if (store.queue().length > 1) {
                <button type="button" class="icon-btn icon-btn-sm" (click)="store.moveQueued(index, -1)" [disabled]="index === 0" aria-label="Move up" paTooltip="Send earlier">
                  <pa-icon name="chevron-up" [size]="14" />
                </button>
                <button type="button" class="icon-btn icon-btn-sm" (click)="store.moveQueued(index, 1)" [disabled]="index === store.queue().length - 1" aria-label="Move down" paTooltip="Send later">
                  <pa-icon name="chevron-down" [size]="14" />
                </button>
              }
              @if (store.turnActive()) {
                <button
                  type="button"
                  class="btn btn-sm btn-ghost"
                  (click)="store.steerNow(index)"
                  paTooltip="Deliver now, between the model's next two actions"
                >
                  <pa-icon name="corner-down-right" [size]="14" /> Send now
                </button>
              }
              <button type="button" class="icon-btn icon-btn-sm" (click)="store.unqueue(index)" aria-label="Remove queued message" paTooltip="Remove">
                <pa-icon name="x" [size]="14" />
              </button>
            </li>
          }
        </ol>
      }
      @if (hasImage() && !store.modelSees()) {
        <div class="composer-note" role="alert">
          <pa-icon name="alert" [size]="14" />
          This model cannot see images, so a message with one is refused. Choose a model marked
          “sees images”, or describe the image in words.
        </div>
      }
      @if (store.attachments().length) {
        <ul class="attachments" aria-label="Attachments">
          @for (path of store.attachments(); track path) {
            <li class="attachment" [class]="'attachment tone-' + kind(path).tone" [attr.title]="path" animate.enter="anim-pop-in">
              <span class="attachment-icon"><pa-icon [name]="kind(path).icon" [size]="16" /></span>
              <span class="attachment-text">
                <strong>{{ name(path) }}</strong>
                <small>{{ kind(path).label }}{{ kind(path).tone === 'folder' ? ' · read-only reference' : '' }}</small>
              </span>
              <button type="button" class="icon-btn icon-btn-sm" (click)="store.detach(path)" [attr.aria-label]="'Remove ' + name(path)">
                <pa-icon name="x" [size]="14" />
              </button>
            </li>
          }
        </ul>
      }
      <textarea
        #box
        class="composer-input"
        [value]="draft()"
        (input)="onInput(box)"
        (keydown.enter)="onEnter($event)"
        [placeholder]="placeholder()"
        aria-label="Message"
        rows="1"
      ></textarea>
      <div class="composer-bar">
        <button
          #attachButton
          type="button"
          class="icon-btn"
          (click)="attachmentMenu.update((open) => !open)"
          [attr.aria-expanded]="attachmentMenu()"
          aria-haspopup="menu"
          aria-label="Attach"
          paTooltip="Attach images, files or a folder"
        >
          <pa-icon name="plus" />
        </button>
        @if (attachmentMenu()) {
          <pa-popover
            [anchor]="attachButton"
            side="top"
            anchorAlign="start"
            panelRole="menu"
            ariaLabel="Attach"
            [focusFirst]="true"
            (closed)="attachmentMenu.set(false)"
            (keydown)="menuKeys($event)"
            class="menu"
            animate.leave="anim-pop-out"
          >
            <button type="button" class="menu-item" role="menuitem" (click)="attach('images')">
              <pa-icon name="image" [size]="16" /> Images
            </button>
            <button type="button" class="menu-item" role="menuitem" (click)="attach('files')">
              <pa-icon name="file" [size]="16" /> Files
            </button>
            <button type="button" class="menu-item" role="menuitem" (click)="attach('folder')">
              <pa-icon name="folder" [size]="16" /> Folder <span class="menu-hint">read-only</span>
            </button>
          </pa-popover>
        }
        @if (!store.chatMode()) {
          <button
            type="button"
            class="toggle-chip"
            [attr.aria-pressed]="store.goalMode()"
            (click)="store.goalMode.set(!store.goalMode())"
            paTooltip="Keep working across check-ins until the goal is verified"
          >
            <span class="switch" aria-hidden="true"></span> Goal
          </button>
          <!-- One label whatever the state, as for Goal: the switch says whether
               approval is automatic. A switch that read "Ask" with its knob off
               was read as "asking is off" (2026-09-23). -->
          <button
            type="button"
            class="toggle-chip tone-warning"
            [attr.aria-pressed]="store.permissionMode() === 'auto'"
            (click)="store.setPermissionMode(store.permissionMode() === 'auto' ? 'ask' : 'auto')"
            [paTooltip]="store.permissionMode() === 'auto'
              ? 'On: every permission granted, nothing is asked. The sandbox still confines writes to the workspace.'
              : 'Off: PWR asks before changing dependencies, reaching the network, installing toolchains, rewriting history or publishing.'"
          >
            <span class="switch" aria-hidden="true"></span> Auto-approve
          </button>
          @if (!store.sandboxed()) {
            <span
              class="composer-warning"
              paTooltip="This platform has no sandbox adapter: commands the model runs are not confined to the workspace."
              tabindex="0"
            >
              <pa-icon name="alert" [size]="14" /> Not sandboxed
            </span>
          }
        } @else {
          <span class="badge badge-outline" paTooltip="Chat mode: the model reads what you attach and cannot edit files or run commands.">
            <pa-icon name="lock" [size]="12" /> Read-only chat
          </span>
        }
        <span class="spacer"></span>
        <span class="composer-hint" aria-hidden="true"><span class="kbd">↵</span> send <span class="kbd">⇧↵</span> new line</span>
        @if (store.turnActive()) {
          <button type="button" class="icon-btn icon-btn-outline composer-stop" (click)="store.cancel()" aria-label="Stop" paTooltip="Stop this turn">
            <pa-icon name="stop" [size]="16" />
          </button>
          <button type="submit" class="composer-send" [disabled]="!draft().trim()" aria-label="Queue message" paTooltip="Queue for when this turn ends">
            <pa-icon name="list-plus" [size]="16" />
          </button>
        } @else {
          <button
            type="submit"
            class="composer-send"
            [disabled]="!draft().trim() || !store.model()"
            aria-label="Send"
            [paTooltip]="store.model() ? 'Send' : 'Choose a model first'"
          >
            <pa-icon name="arrow-up" [size]="16" [stroke]="2" />
          </button>
        }
      </div>
    </form>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Composer implements OnInit, OnDestroy {
  protected readonly store = inject(AgentStore);
  protected readonly attachmentMenu = signal(false);

  protected attach(kind: 'images' | 'files' | 'folder'): void {
    this.attachmentMenu.set(false);
    if (kind === 'images') void this.store.attachImages();
    else if (kind === 'files') void this.store.attachFiles();
    else void this.store.attachFolder();
  }

  protected menuKeys(event: KeyboardEvent): void {
    roveFocus(event, event.currentTarget as HTMLElement, '[role=menuitem]');
  }

  protected placeholder(): string {
    if (this.store.turnActive()) return 'Write the next message — it is queued until this turn ends…';
    if (this.store.chatMode()) return 'Ask anything — attach files, folders or images for it to read…';
    return 'Ask PWR to build, fix or explain something…';
  }
  protected readonly draft = signal('');
  protected readonly editingQueued = signal<number | null>(null);

  protected commitQueued(index: number, text: string): void {
    if (this.editingQueued() !== index) return;
    this.editingQueued.set(null);
    this.store.editQueued(index, text);
  }
  protected readonly dropping = signal(false);
  private readonly box = viewChild.required<ElementRef<HTMLTextAreaElement>>('box');
  private unlisten?: UnlistenFn;

  constructor() {
    // A message being edited arrives here to be changed and sent again.
    effect(() => {
      const text = this.store.composerDraft();
      if (text === null) return;
      untracked(() => {
        this.store.composerDraft.set(null);
        this.draft.set(text);
        const box = this.box().nativeElement;
        box.value = text;
        this.grow(box);
        box.focus();
      });
    });
  }

  async ngOnInit(): Promise<void> {
    queueMicrotask(() => this.grow(this.box().nativeElement));
    if (!inTauri()) return;
    // Files dragged onto the window arrive as paths from the webview.
    const { getCurrentWebview } = await import('@tauri-apps/api/webview');
    this.unlisten = await getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === 'enter' || payload.type === 'over') this.dropping.set(true);
      else if (payload.type === 'leave') this.dropping.set(false);
      else if (payload.type === 'drop') {
        this.dropping.set(false);
        this.store.attachments.update((current) => [...new Set([...current, ...payload.paths])]);
      }
    });
  }

  ngOnDestroy(): void {
    this.unlisten?.();
  }

  @HostListener('window:resize')
  protected refit(): void {
    this.grow(this.box().nativeElement);
  }

  protected onInput(box: HTMLTextAreaElement): void {
    this.draft.set(box.value);
    this.grow(box);
  }

  /** Enter sends, Shift+Enter breaks the line, as in Claude and ChatGPT. */
  protected onEnter(event: Event): void {
    const key = event as KeyboardEvent;
    if (key.shiftKey || key.isComposing) return;
    key.preventDefault();
    this.submit();
  }

  protected submit(): void {
    const text = this.draft();
    if (!text.trim() || !this.store.model()) return;
    this.draft.set('');
    const box = this.box().nativeElement;
    box.value = '';
    this.grow(box);
    void this.store.send(text);
  }

  /**
   * Fits the box to its text between one full line and 260px, and scrolls
   * only past that: at its smallest it had been shorter than one line with
   * its padding, and showed a scrollbar before anything was typed.
   */
  private grow(box: HTMLTextAreaElement): void {
    box.style.height = 'auto';
    // In a short window the box stops growing sooner, so the conversation
    // above it always keeps room.
    const max = Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, Math.round(window.innerHeight * 0.3)));
    const wanted = Math.max(box.scrollHeight, MIN_HEIGHT);
    box.style.height = `${Math.min(wanted, max)}px`;
    box.style.overflowY = wanted > max ? 'auto' : 'hidden';
  }

  /** Whether an attachment is an image, which only a vision model could use. */
  protected hasImage(): boolean {
    return this.store.attachments().some((path) => fileKind(path).tone === 'image');
  }

  protected name(path: string): string {
    return path.split('/').filter(Boolean).pop() ?? path;
  }

  protected kind(path: string) {
    return fileKind(path);
  }
}
