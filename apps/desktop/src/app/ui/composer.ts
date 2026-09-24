import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  HostListener,
  OnDestroy,
  OnInit,
  inject,
  signal,
  viewChild,
} from '@angular/core';
import { UnlistenFn } from '@tauri-apps/api/event';
import { AgentStore } from '../core/agent.store';
import { inTauri } from '../core/bridge';
import { fileKind } from './conversation';

const MIN_HEIGHT = 40;
const MAX_HEIGHT = 260;

@Component({
  selector: 'pa-composer',
  template: `
    <form
      class="composer"
      [class.busy]="store.turnActive()"
      [class.dropping]="dropping()"
      (submit)="$event.preventDefault(); submit()"
    >
      @if (dropping()) {
        <div class="drop-hint">Drop files or folders to attach</div>
      }
      @if (store.queue().length) {
        <div class="queue">
          @for (queued of store.queue(); track $index; let index = $index) {
            <div class="queued">
              <span class="queued-badge">{{ index + 1 }}</span>
              <span class="queued-text">{{ queued }}</span>
              @if (store.turnActive()) {
                <button type="button" class="queued-now" (click)="store.steerNow(index)" title="Deliver now, between the model's next two actions">↳ now</button>
              }
              <button type="button" class="queued-remove" (click)="store.unqueue(index)" aria-label="Remove">×</button>
            </div>
          }
        </div>
      }
      @if (hasImage() && !store.modelSees()) {
        <div class="notice-line">
          This model cannot see images, so a message with one is refused. Choose a model marked
          "sees images", or describe the image in words.
        </div>
      }
      @if (store.attachments().length) {
        <div class="attachments">
          @for (path of store.attachments(); track path) {
            <div class="attachment" [class]="'attachment ' + kind(path).tone" [title]="path">
              <span class="attachment-icon">{{ kind(path).glyph }}</span>
              <span class="attachment-text">
                <strong>{{ name(path) }}</strong>
                <small>{{ kind(path).label }}{{ kind(path).tone === 'folder' ? ' · read-only reference' : '' }}</small>
              </span>
              <button type="button" (click)="store.detach(path)" aria-label="Remove">×</button>
            </div>
          }
        </div>
      }
      <textarea
        #box
        [value]="draft()"
        (input)="onInput(box)"
        (keydown.enter)="onEnter($event)"
        [placeholder]="store.turnActive() ? 'Write the next message — it is queued until this turn ends…' : store.chatMode() ? 'Ask anything — attach files, folders or images for it to read…' : 'Ask PWR to build, fix or explain something…'"
        rows="1"
      ></textarea>
      <div class="composer-bar">
        <div class="attach-wrap">
          <button type="button" class="icon-button" (click)="attachmentMenu.update((open) => !open)" [attr.aria-expanded]="attachmentMenu()" aria-label="Aggiungi allegato" title="Aggiungi immagini, file o cartelle">＋</button>
          @if (attachmentMenu()) {
            <div class="attach-menu" role="group" aria-label="Tipo di allegato">
              <button type="button" (click)="attach('images')">Immagini</button>
              <button type="button" (click)="attach('files')">File</button>
              <button type="button" (click)="attach('folder')">Cartella</button>
            </div>
          }
        </div>
        @if (!store.chatMode()) {
        <button
          type="button"
          class="toggle"
          [class.on]="store.goalMode()"
          (click)="store.goalMode.set(!store.goalMode())"
          title="Keep working across check-ins until the goal is verified"
        >
          <span class="knob"></span> Goal
        </button>
        <button
          type="button"
          class="toggle permissions"
          [class.on]="store.permissionMode() === 'auto'"
          [class.auto]="store.permissionMode() === 'auto'"
          (click)="store.setPermissionMode(store.permissionMode() === 'auto' ? 'ask' : 'auto')"
          [attr.aria-pressed]="store.permissionMode() === 'auto'"
          [title]="store.permissionMode() === 'auto'
            ? 'On: every permission granted, nothing is asked. The sandbox still confines writes to the workspace.'
            : 'Off: PWR asks before changing dependencies, reaching the network, installing toolchains, rewriting history or publishing.'"
        >
          <!-- One label whatever the state, as for Goal: the knob says whether
               approval is automatic. A switch that read "Ask" with its knob off
               was read as "asking is off" (2026-09-23). -->
          <span class="knob"></span> Auto-approve
        </button>
        } @else {
          <span class="chat-note" title="Chat mode: the model reads what you attach and cannot edit files or run commands.">Chat · read-only</span>
        }
        @if (!store.chatMode() && !store.sandboxed()) {
          <span class="unconfined" title="This platform has no sandbox adapter: commands the model runs are not confined to the workspace.">⚠ commands not sandboxed</span>
        }
        <span class="spacer"></span>
        <span class="hint">↵ send · ⇧↵ new line</span>
        @if (store.turnActive()) {
          <button type="button" class="round stop" (click)="store.cancel()" title="Stop">■</button>
          <button type="submit" class="round send" [disabled]="!draft().trim()" title="Queue for when this turn ends">⇥</button>
        } @else {
          <button type="submit" class="round send" [disabled]="!draft().trim() || !store.model()" title="Send">↑</button>
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

  @HostListener('document:pointerdown', ['$event'])
  protected closeAttachmentMenu(event: PointerEvent): void {
    if (!(event.target as Element).closest('.attach-wrap')) this.attachmentMenu.set(false);
  }

  @HostListener('document:keydown.escape')
  protected escapeAttachmentMenu(): void { this.attachmentMenu.set(false); }
  protected readonly draft = signal('');
  protected readonly dropping = signal(false);
  private readonly box = viewChild.required<ElementRef<HTMLTextAreaElement>>('box');
  private unlisten?: UnlistenFn;

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
    const wanted = Math.max(box.scrollHeight, MIN_HEIGHT);
    box.style.height = `${Math.min(wanted, MAX_HEIGHT)}px`;
    box.style.overflowY = wanted > MAX_HEIGHT ? 'auto' : 'hidden';
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
