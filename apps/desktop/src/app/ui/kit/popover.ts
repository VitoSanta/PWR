import {
  AfterViewInit,
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  HostListener,
  OnDestroy,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { DialogStack } from '../../core/ui';
import { focusables } from './dialog';

/**
 * A floating panel anchored to the control that opened it: fixed-position
 * so no scrolling container clips it, kept inside the window, closed by a
 * click elsewhere or Escape, and focus returns to the anchor.
 *
 * Usage: `@if (open) { <pa-popover [anchor]="button" animate.leave="anim-pop-out" (closed)="open = false"> }`
 */
@Component({
  selector: 'pa-popover',
  template: `<ng-content />`,
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    class: 'popover surface-floating anim-pop-in',
    '[attr.role]': 'panelRole()',
    '[attr.aria-label]': 'ariaLabel()',
    tabindex: '-1',
    '[style.top.px]': 'top()',
    '[style.bottom.px]': 'bottom()',
    '[style.left.px]': 'left()',
    '[style.right.px]': 'right()',
    '[style.max-height.px]': 'maxHeight()',
    '[style.width]': 'width()',
    '[style.transform-origin]': "side() === 'bottom' ? 'top' : 'bottom'",
  },
})
export class Popover implements AfterViewInit, OnDestroy {
  readonly anchor = input.required<HTMLElement>();
  readonly side = input<'bottom' | 'top'>('bottom');
  readonly anchorAlign = input<'start' | 'end'>('end');
  readonly width = input<string | null>(null);
  readonly panelRole = input<string>('dialog');
  readonly ariaLabel = input<string | null>(null);
  /** Move focus into the panel on open (menus); info panels take focus on the panel itself. */
  readonly focusFirst = input(false);
  readonly closed = output<void>();

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);
  private readonly dialogs = inject(DialogStack);
  protected readonly top = signal<number | null>(null);
  protected readonly bottom = signal<number | null>(null);
  protected readonly left = signal<number | null>(null);
  protected readonly right = signal<number | null>(null);
  protected readonly maxHeight = signal<number | null>(null);

  ngAfterViewInit(): void {
    this.place();
    queueMicrotask(() => {
      const element = this.host.nativeElement;
      const target = this.focusFirst() ? (focusables(element)[0] ?? element) : element;
      target.focus({ preventScroll: true });
    });
  }

  ngOnDestroy(): void {
    const active = document.activeElement;
    if (!active || active === document.body || this.host.nativeElement.contains(active)) {
      this.anchor().focus({ preventScroll: true });
    }
  }

  @HostListener('window:resize')
  protected place(): void {
    const rect = this.anchor().getBoundingClientRect();
    const gap = 6;
    const margin = 8;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    if (this.side() === 'bottom') {
      this.top.set(rect.bottom + gap);
      this.maxHeight.set(vh - rect.bottom - gap - margin);
    } else {
      this.bottom.set(vh - rect.top + gap);
      this.maxHeight.set(rect.top - gap - margin);
    }
    if (this.anchorAlign() === 'end') this.right.set(Math.max(margin, vw - rect.right));
    else this.left.set(Math.max(margin, rect.left));
  }

  @HostListener('document:pointerdown', ['$event'])
  protected outside(event: PointerEvent): void {
    const target = event.target as Node;
    if (this.host.nativeElement.contains(target) || this.anchor().contains(target)) return;
    // A select's list inside this popover is rendered in place, so this also
    // leaves clicks on it alone.
    this.closed.emit();
  }

  @HostListener('document:keydown', ['$event'])
  protected key(event: KeyboardEvent): void {
    if (event.key !== 'Escape' || event.defaultPrevented || this.dialogs.open) return;
    event.preventDefault();
    this.anchor().focus({ preventScroll: true });
    this.closed.emit();
  }
}
