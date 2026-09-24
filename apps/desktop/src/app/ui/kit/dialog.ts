import {
  AfterViewInit,
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  OnDestroy,
  inject,
  input,
  output,
  viewChild,
} from '@angular/core';
import { DialogStack } from '../../core/ui';

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function focusables(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter((element) =>
    typeof element.checkVisibility === 'function' ? element.checkVisibility() : true,
  );
}

/**
 * The one modal dialog: scrim, panel, focus kept inside, Escape and the
 * scrim close it when that is safe, and focus goes back where it was.
 *
 * Usage: `@if (open) { <pa-dialog animate.leave="is-leaving" ...> }` -- the
 * host carries the leave animation so it can play before removal.
 */
@Component({
  selector: 'pa-dialog',
  template: `
    <div class="dialog-scrim" (pointerdown)="scrimDown($event)" (click)="scrimClick($event)">
      <div
        #panel
        class="dialog"
        [class]="'dialog size-' + size()"
        [attr.role]="dialogRole()"
        aria-modal="true"
        [attr.aria-labelledby]="labelledBy()"
        [attr.aria-describedby]="describedBy()"
        [attr.aria-label]="ariaLabel()"
        tabindex="-1"
        (keydown)="trap($event)"
      >
        <ng-content />
      </div>
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Dialog implements AfterViewInit, OnDestroy {
  readonly size = input<'sm' | 'md' | 'lg'>('sm');
  readonly dialogRole = input<'dialog' | 'alertdialog'>('dialog');
  readonly labelledBy = input<string | null>(null);
  readonly describedBy = input<string | null>(null);
  readonly ariaLabel = input<string | null>(null);
  /** False while something runs that must not be interrupted. */
  readonly dismissible = input(true);
  readonly closed = output<void>();

  private readonly stack = inject(DialogStack);
  private readonly panel = viewChild.required<ElementRef<HTMLElement>>('panel');
  private readonly returnTo =
    typeof document !== 'undefined' ? (document.activeElement as HTMLElement | null) : null;
  private pressedOnScrim = false;
  private readonly entry = { close: () => this.dismiss() };

  ngAfterViewInit(): void {
    this.stack.push(this.entry);
    const panel = this.panel().nativeElement;
    queueMicrotask(() => {
      const preferred = panel.querySelector<HTMLElement>('[data-autofocus]');
      (preferred ?? focusables(panel)[0] ?? panel).focus({ preventScroll: true });
    });
  }

  ngOnDestroy(): void {
    this.stack.remove(this.entry);
    // Only when focus would otherwise be lost: not if something else took it.
    const active = document.activeElement;
    const lost = !active || active === document.body || this.panel().nativeElement.contains(active);
    if (lost && this.returnTo?.isConnected) this.returnTo.focus({ preventScroll: true });
  }

  protected dismiss(): void {
    if (this.dismissible()) this.closed.emit();
  }

  protected scrimDown(event: PointerEvent): void {
    this.pressedOnScrim = event.target === event.currentTarget;
  }

  protected scrimClick(event: MouseEvent): void {
    // A drag that starts inside the panel and ends on the scrim is not a click away.
    if (this.pressedOnScrim && event.target === event.currentTarget) this.dismiss();
    this.pressedOnScrim = false;
  }

  protected trap(event: KeyboardEvent): void {
    if (event.key !== 'Tab') return;
    const panel = this.panel().nativeElement;
    const items = focusables(panel);
    if (!items.length) {
      event.preventDefault();
      panel.focus();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    if (event.shiftKey && (document.activeElement === first || document.activeElement === panel)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
}
