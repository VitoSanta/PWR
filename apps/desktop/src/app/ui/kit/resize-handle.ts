import { ChangeDetectionStrategy, Component, inject, input, output, signal } from '@angular/core';
import { LayoutService } from '../../core/layout';

/**
 * The edge of a side panel. Drag it, or focus it and use the arrow keys;
 * the panel's own side decides which direction widens it.
 */
@Component({
  selector: 'pa-resize-handle',
  template: '',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    class: 'resize-handle',
    role: 'separator',
    tabindex: '0',
    'aria-orientation': 'vertical',
    '[attr.aria-label]': 'label()',
    '[attr.aria-valuenow]': 'width()',
    '[attr.aria-valuemin]': 'min()',
    '[attr.aria-valuemax]': 'max()',
    '[class.is-dragging]': 'dragging()',
    '[style.right.px]': "edge() === 'right' ? -5 : null",
    '[style.left.px]': "edge() === 'left' ? -5 : null",
    '(pointerdown)': 'start($event)',
    '(pointermove)': 'move($event)',
    '(pointerup)': 'end($event)',
    '(pointercancel)': 'end($event)',
    '(lostpointercapture)': 'end($event)',
    '(keydown)': 'key($event)',
    '(dblclick)': 'reset()',
  },
})
export class ResizeHandle {
  /** Which edge of its panel the handle sits on. */
  readonly edge = input.required<'left' | 'right'>();
  readonly width = input.required<number>();
  readonly min = input.required<number>();
  readonly max = input.required<number>();
  readonly initial = input.required<number>();
  readonly label = input('Resize panel');
  readonly resize = output<number>();

  private readonly layout = inject(LayoutService);
  protected readonly dragging = signal(false);
  private origin: { pointerId: number; x: number; width: number } | null = null;

  private direction(): 1 | -1 {
    return this.edge() === 'right' ? 1 : -1;
  }

  protected start(event: PointerEvent): void {
    if (event.button !== 0) return;
    this.origin = { pointerId: event.pointerId, x: event.clientX, width: this.width() };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    this.dragging.set(true);
    this.layout.resizing.set(true);
    event.preventDefault();
  }

  protected move(event: PointerEvent): void {
    if (this.origin?.pointerId !== event.pointerId) return;
    this.resize.emit(this.origin.width + (event.clientX - this.origin.x) * this.direction());
  }

  protected end(event: PointerEvent): void {
    if (this.origin?.pointerId !== event.pointerId) return;
    this.origin = null;
    this.dragging.set(false);
    this.layout.resizing.set(false);
  }

  protected key(event: KeyboardEvent): void {
    const step = event.shiftKey ? 48 : 16;
    if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      const grow = (event.key === 'ArrowRight' ? 1 : -1) * this.direction();
      this.resize.emit(this.width() + grow * step);
    } else if (event.key === 'Home') this.resize.emit(this.min());
    else if (event.key === 'End') this.resize.emit(this.max());
    else return;
    event.preventDefault();
  }

  protected reset(): void {
    this.resize.emit(this.initial());
  }
}
