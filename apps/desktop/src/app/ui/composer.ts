import { MatButtonModule } from '@angular/material/button';
import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  HostListener,
  OnDestroy,
  OnInit,
  DestroyRef,
  afterNextRender,
  afterRenderEffect,
  computed,
  effect,
  inject,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { UnlistenFn } from '@tauri-apps/api/event';
import { AgentStore } from '../core/agent.store';
import { inTauri } from '../core/bridge';
import { UiStore, roveFocus } from '../core/ui';

const reducedMotion = () =>
  typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
import { fileKind } from './conversation';
import { Icon } from './kit/icon';
import { BrandMark } from './kit/brand-mark';
import { Popover } from './kit/popover';
import { Tooltip } from './kit/tooltip';

const MIN_HEIGHT = 36;
const MAX_HEIGHT = 260;

@Component({
  selector: 'pa-composer',
  imports: [MatButtonModule, Icon, BrandMark, Tooltip, Popover],
  template: `
    <!-- What goes with the next message -- queued messages, attachments --
         sits just above the bar, outside it: attaching an image leaves the
         bar one slim line. -->
    @if (store.queue().length || store.attachments().length) {
      <div class="composer-context composer-tray" animate.enter="anim-pop-in" animate.leave="anim-pop-out">
        @if (store.queue().length) {
          <ol class="queue" aria-label="Queued messages">
            @for (queued of store.queue(); track $index; let index = $index) {
              <li class="queued" animate.enter="anim-pop-in" animate.leave="anim-pop-out">
                <span class="queued-badge num">{{ index + 1 }}</span>
                @if (editingQueued() === index) {
                  <input
                    class="input input-sm queued-edit"
                    [value]="queued"
                    (keydown.enter)="
                      $event.preventDefault(); commitQueued(index, $any($event.target).value)
                    "
                    (keydown.escape)="editingQueued.set(null)"
                    (blur)="commitQueued(index, $any($event.target).value)"
                    aria-label="Edit queued message"
                  />
                } @else {
                  <button
                    type="button"
                    class="queued-text"
                    (click)="editingQueued.set(index)"
                    paTooltip="Edit"
                  >
                    {{ queued }}
                  </button>
                }
                @if (store.queue().length > 1) {
                  <button matIconButton
                    type="button"
                    class="icon-btn icon-btn-sm"
                    (click)="store.moveQueued(index, -1)"
                    [disabled]="index === 0"
                    aria-label="Move up"
                    paTooltip="Send earlier"
                  >
                    <pa-icon name="chevron-up" [size]="14" />
                  </button>
                  <button matIconButton
                    type="button"
                    class="icon-btn icon-btn-sm"
                    (click)="store.moveQueued(index, 1)"
                    [disabled]="index === store.queue().length - 1"
                    aria-label="Move down"
                    paTooltip="Send later"
                  >
                    <pa-icon name="chevron-down" [size]="14" />
                  </button>
                }
                @if (store.turnActive()) {
                  <button matButton
                    type="button"
                    class="btn btn-sm btn-ghost"
                    (click)="store.steerNow(index)"
                    paTooltip="Deliver now, between the model's next two actions"
                  >
                    <pa-icon name="corner-down-right" [size]="14" /> Send now
                  </button>
                }
                <button matIconButton
                  type="button"
                  class="icon-btn icon-btn-sm"
                  (click)="store.unqueue(index)"
                  aria-label="Remove queued message"
                  paTooltip="Remove"
                >
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
              <li
                class="attachment"
                [class]="'attachment tone-' + kind(path).tone"
                [attr.title]="path"
                animate.enter="anim-pop-in"
                animate.leave="anim-pop-out"
              >
                <span class="attachment-icon"
                  ><pa-icon [name]="kind(path).icon" [size]="16"
                /></span>
                <span class="attachment-text">
                  <strong>{{ name(path) }}</strong>
                  <small
                    >{{ kind(path).label
                    }}{{ kind(path).tone === 'folder' ? ' · read-only reference' : '' }}</small
                  >
                </span>
                <button matIconButton
                  type="button"
                  class="icon-btn icon-btn-sm"
                  (click)="store.detach(path)"
                  [attr.aria-label]="'Remove ' + name(path)"
                >
                  <pa-icon name="x" [size]="14" />
                </button>
              </li>
            }
          </ul>
        }
      </div>
    }
    <div class="composer-dock" [class.is-pinned]="draft().length > 0 || store.turnActive() || store.attachments().length > 0 || store.queue().length > 0 || dropping() || !!ui.runControls()">
    <button
      type="button"
      class="composer-beacon"
      aria-label="Write a message to PWR"
      (focus)="reveal()"
      (click)="reveal()"
    ><pa-brand-mark /></button>
    <form
      #form
      class="composer"
      [class.busy]="store.turnActive()"
      [class.dropping]="dropping()"
      [class.is-expanded]="open()"
      (submit)="$event.preventDefault(); submit()"
      aria-label="Message"
    >
      @if (dropping()) {
        <div class="drop-hint" aria-hidden="true">
          <pa-icon name="paperclip" [size]="16" /> Drop files or folders to attach
        </div>
      }
      <div class="composer-attach">
        <button matIconButton
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
        <!-- The shell's tools and where you are, beside attach. -->
        <ng-content select="[paComposerStart]" />
      </div>
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
        @if (!store.chatMode() && !store.sandboxed()) {
          <button matIconButton
            type="button"
            class="icon-btn composer-warning"
            (click)="ui.runControls.set(ui.runControls() ? null : $any($event.currentTarget))"
            [attr.aria-expanded]="!!ui.runControls()"
            aria-haspopup="dialog"
            [attr.aria-label]="store.permissionMode() === 'full' ? 'Full access · manage permissions' : 'Not sandboxed · manage permissions'"
            [paTooltip]="store.permissionMode() === 'full'
              ? 'Full access: commands run with your rights. Manage permissions.'
              : 'Commands are not confined to the workspace. Manage permissions.'"
          >
            <pa-icon name="alert" [size]="16" />
          </button>
        }
        <span class="spacer"></span>
        <span class="composer-hint" aria-hidden="true"
          ><span class="kbd">↵</span> send <span class="kbd">⇧↵</span> new line</span
        >
        <!-- The shell's run status and model, before stop and send. -->
        <ng-content select="[paComposerEnd]" />
        @if (store.turnActive()) {
          <button matIconButton
            type="button"
            class="icon-btn composer-stop"
            (click)="store.cancel()"
            aria-label="Stop"
            paTooltip="Stop this turn"
          >
            <pa-icon name="stop" [size]="16" />
          </button>
          <button matIconButton
            type="submit"
            class="composer-send"
            [disabled]="!draft().trim()"
            aria-label="Queue message"
            paTooltip="Queue for when this turn ends"
          >
            <pa-icon name="list-plus" [size]="16" />
          </button>
        } @else {
          <button matIconButton
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
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Composer implements OnInit, OnDestroy {
  protected readonly store = inject(AgentStore);
  protected readonly ui = inject(UiStore);
  protected readonly attachmentMenu = signal(false);
  protected readonly expanded = signal(false);

  protected attach(kind: 'images' | 'files' | 'folder'): void {
    this.attachmentMenu.set(false);
    if (kind === 'images') void this.store.attachImages();
    else if (kind === 'files') void this.store.attachFiles();
    else void this.store.attachFolder();
  }

  protected menuKeys(event: KeyboardEvent): void {
    roveFocus(event, event.currentTarget as HTMLElement, '[role=menuitem]');
  }

  protected reveal(): void {
    this.box().nativeElement.focus();
  }

  protected placeholder(): string {
    return this.store.turnActive() ? 'Write the next message…' : 'Ask PWR…';
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
  private readonly form = viewChild.required<ElementRef<HTMLFormElement>>('form');
  /** One slim line, or the writing surface once the text wraps. */
  protected readonly open = computed(() => this.expanded());
  private wasOpen: boolean | null = null;
  private lastHeight = 0;
  /** The height animation under way, and the height it last showed. */
  private resizing?: Animation;
  private get shown(): number {
    return this.form().nativeElement.getBoundingClientRect().height;
  }
  private unlisten?: UnlistenFn;

  constructor() {
    // Opening and closing grow and shrink the bar instead of jumping: from
    // the height it had to the one it has now, Material's emphasized curve,
    // its parts fading in where the layout moved them.
    afterRenderEffect(() => {
      const open = this.open();
      const form = this.form().nativeElement;
      // The text's box first, in the layout it now has: measured in the
      // other one it kept that one's height (two lines where one fits).
      untracked(() => this.fit(this.box().nativeElement));
      const now = form.offsetHeight;
      if (this.wasOpen !== null && open !== this.wasOpen && this.lastHeight && this.lastHeight !== now
          && typeof form.animate === 'function' && !reducedMotion()) {
        // Turned back before it finished: from where it stands now.
        const from = this.resizing?.playState === 'running' ? this.shown : this.lastHeight;
        this.resizing?.cancel();
        form.style.overflow = 'hidden';
        const grow = form.animate([{ height: `${from}px` }, { height: `${now}px` }], {
          duration: 260,
          easing: 'cubic-bezier(0.2, 0, 0, 1)',
        });
        this.resizing = grow;
        grow.onfinish = grow.oncancel = () => {
          if (this.resizing === grow) form.style.overflow = '';
        };
        for (const part of Array.from(form.querySelectorAll<HTMLElement>('.composer-attach, .composer-bar'))) {
          part.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 200, delay: 80, easing: 'ease-out', fill: 'backwards' });
        }
      }
      this.wasOpen = open;
      this.lastHeight = now;
    });
    // The height to grow from: what the bar measures as it changes otherwise
    // (the text growing line by line).
    if (typeof ResizeObserver === 'function') {
      let boxWidth = -1;
      const watch = new ResizeObserver(() => {
        const form = this.form().nativeElement;
        const box = this.box().nativeElement;
        // Refold the text during the dock's width animation, including
        // when the window or a side panel changes the available room.
        if (box.clientWidth !== boxWidth) {
          boxWidth = box.clientWidth;
          this.fit(box);
        }
        if (!form.getAnimations?.().length) this.lastHeight = form.offsetHeight;
      });
      afterNextRender(() => {
        watch.observe(this.form().nativeElement);
        watch.observe(this.box().nativeElement);
      });
      inject(DestroyRef).onDestroy(() => watch.disconnect());
    }
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

  /** Enter sends, Shift+Enter breaks the line, as in common chat apps. */
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
    this.decide(box);
    this.fit(box);
  }

  /** The text's room on the slim line, measured while the bar is slim. */
  private narrow = 0;
  private ruler: CanvasRenderingContext2D | null | undefined;

  /**
   * One slim line, or the writing surface. The slim line keeps the text
   * that fits on it; past that the bar opens, and it closes again once the
   * text would fit on the slim line with room to spare -- by the text's
   * width, not the box's height, since the open bar gives the text more
   * room and one line there says nothing about the slim one. A line break
   * always opens it.
   */
  private decide(box: HTMLTextAreaElement): void {
    const text = box.value;
    if (!text) return this.expanded.set(false);
    if (text.includes('\n')) return this.expanded.set(true);
    if (!this.expanded()) {
      this.narrow = this.room(box);
      // A word that does not fit wraps before the width is used up.
      const wrapped = box.scrollHeight > MIN_HEIGHT + 8;
      if (wrapped || this.width(box) > this.narrow) this.expanded.set(true);
    } else if (this.narrow && this.width(box) < this.narrow - 24) {
      this.expanded.set(false);
    }
  }

  /** The box's height to its text, between one line and 30% of the window. */
  private fit(box: HTMLTextAreaElement): void {
    box.style.height = 'auto';
    // In a short window the box stops growing sooner, so the conversation
    // above it always keeps room.
    const max = Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, Math.round(window.innerHeight * 0.3)));
    const wanted = Math.max(box.scrollHeight, MIN_HEIGHT);
    box.style.height = `${Math.min(wanted, max)}px`;
    box.style.overflowY = wanted > max ? 'auto' : 'hidden';
  }

  private room(box: HTMLTextAreaElement): number {
    const style = getComputedStyle(box);
    return box.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
  }

  private width(box: HTMLTextAreaElement): number {
    if (this.ruler === undefined) this.ruler = document.createElement('canvas').getContext?.('2d') ?? null;
    if (!this.ruler) return 0;
    const style = getComputedStyle(box);
    this.ruler.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
    return this.ruler.measureText(box.value).width;
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
