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
    '[style.transform-origin]': "placed() === 'bottom' ? 'top' : placed() === 'top' ? 'bottom' : 'left'",
    // It opens from the anchor's side: down, up, or beside it.
    '[style.--pop-y]': "placed() === 'bottom' ? '-4px' : placed() === 'top' ? '4px' : '0px'",
  },
})
export class Popover implements AfterViewInit, OnDestroy {
  readonly anchor = input.required<HTMLElement>();
  readonly side = input<'bottom' | 'top' | 'right'>('bottom');
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
  /** The side it opened on: the one asked for, unless only the other has room. */
  protected readonly placed = signal<'bottom' | 'top' | 'right'>('bottom');

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
    // Where the window's edges are for this panel: a glass or transformed
    // ancestor makes itself the frame fixed positions are measured from.
    const frame = containingFrame(this.host.nativeElement, vw, vh);
    // Its laid-out size: the opening animation scales what the screen shows.
    const host = this.host.nativeElement;
    const panel = { width: host.offsetWidth, height: host.offsetHeight };
    if (this.side() === 'right') {
      this.placed.set('right');
      this.top.set(Math.max(margin, Math.min(rect.top, vh - panel.height - margin)) - frame.top);
      this.left.set(Math.max(margin, Math.min(rect.right + gap, vw - panel.width - margin)) - frame.left);
      this.maxHeight.set(vh - 2 * margin);
      return;
    }
    // Below or above as asked, unless the panel only fits on the other side.
    const below = vh - rect.bottom - gap - margin;
    const above = rect.top - gap - margin;
    const side =
      this.side() === 'bottom'
        ? below < panel.height && above > below
          ? 'top'
          : 'bottom'
        : above < panel.height && below > above
          ? 'bottom'
          : 'top';
    this.placed.set(side);
    if (side === 'bottom') {
      this.top.set(rect.bottom + gap - frame.top);
      this.bottom.set(null);
      this.maxHeight.set(below);
    } else {
      this.bottom.set(vh - rect.top + gap - frame.bottom);
      this.top.set(null);
      this.maxHeight.set(above);
    }
    // Along the anchor, but never past the window's edge.
    const widest = Math.max(margin, vw - panel.width - margin);
    if (this.anchorAlign() === 'end') {
      this.right.set(Math.min(Math.max(margin, vw - rect.right), widest) - frame.right);
      this.left.set(null);
    } else {
      this.left.set(Math.min(Math.max(margin, rect.left), widest) - frame.left);
      this.right.set(null);
    }
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
    if (event.key !== 'Escape' || event.defaultPrevented) return;
    // Behind an open dialog, Escape is the dialog's; inside one, it closes
    // this first -- it closed the whole Model Manager from its filters.
    if (this.dialogs.open && !this.host.nativeElement.closest('pa-dialog')) return;
    event.preventDefault();
    this.anchor().focus({ preventScroll: true });
    this.closed.emit();
  }
}

/**
 * How far the frame a fixed-position element is placed in sits from each
 * edge of the window: zero, unless an ancestor has a backdrop filter, a
 * filter, a transform or the like, which makes its own box that frame.
 */
function containingFrame(element: HTMLElement, vw: number, vh: number) {
  for (let node = element.parentElement; node && node !== document.documentElement; node = node.parentElement) {
    const style = getComputedStyle(node);
    const backdrop = style.backdropFilter || (style as CSSStyleDeclaration & { webkitBackdropFilter?: string }).webkitBackdropFilter;
    const frames =
      style.transform !== 'none' ||
      style.filter !== 'none' ||
      (!!backdrop && backdrop !== 'none') ||
      style.perspective !== 'none' ||
      /paint|layout|strict|content/.test(style.contain) ||
      /transform|filter|perspective/.test(style.willChange);
    if (!frames) continue;
    const box = node.getBoundingClientRect();
    const left = box.left + node.clientLeft;
    const top = box.top + node.clientTop;
    return { left, top, right: vw - (left + node.clientWidth), bottom: vh - (top + node.clientHeight) };
  }
  return { left: 0, top: 0, right: 0, bottom: 0 };
}
