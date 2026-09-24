import { Directive, ElementRef, OnDestroy, inject, input } from '@angular/core';
import { shortcut } from '../../core/ui';

/** One tooltip element for the whole app, reused by every host. */
class TooltipLayer {
  private element: HTMLDivElement | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private owner: HTMLElement | null = null;
  private lastHidden = 0;

  schedule(owner: HTMLElement, text: string, keys: string | null, immediate: boolean): void {
    this.cancel();
    // Moving between neighbouring controls shows the next tip at once.
    const warm = Date.now() - this.lastHidden < 400;
    this.timer = setTimeout(() => this.show(owner, text, keys), immediate || warm ? 0 : 450);
  }

  hide(owner?: HTMLElement): void {
    if (owner && owner !== this.owner && !this.timer) return;
    this.cancel();
    if (this.element) {
      this.element.remove();
      this.element = null;
      this.lastHidden = Date.now();
    }
    this.owner?.removeAttribute('aria-describedby');
    this.owner = null;
  }

  private cancel(): void {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }

  private show(owner: HTMLElement, text: string, keys: string | null): void {
    this.timer = null;
    if (!owner.isConnected) return;
    this.hide();
    const element = document.createElement('div');
    element.className = 'tooltip';
    element.id = 'pa-tooltip';
    element.setAttribute('role', 'tooltip');
    const label = document.createElement('span');
    label.textContent = text;
    element.append(label);
    if (keys) {
      const kbd = document.createElement('span');
      kbd.className = 'kbd';
      kbd.textContent = shortcut(keys);
      element.append(kbd);
    }
    document.body.append(element);
    this.element = element;
    this.owner = owner;
    if (owner.getAttribute('aria-label') !== text)
      owner.setAttribute('aria-describedby', 'pa-tooltip');

    const rect = owner.getBoundingClientRect();
    const tip = element.getBoundingClientRect();
    const margin = 8;
    let top = rect.bottom + 6;
    if (top + tip.height > window.innerHeight - margin) top = rect.top - tip.height - 6;
    let left = rect.left + rect.width / 2 - tip.width / 2;
    left = Math.max(margin, Math.min(left, window.innerWidth - tip.width - margin));
    element.style.top = `${Math.round(top)}px`;
    element.style.left = `${Math.round(left)}px`;
  }
}

const layer = new TooltipLayer();

/**
 * `paTooltip="Toggle sidebar" paTooltipKeys="Mod+B"`: a short label shown on
 * hover (after a delay) and on keyboard focus. Icon-only controls still
 * carry their own aria-label; the tooltip is for sighted users.
 */
@Directive({
  selector: '[paTooltip]',
  host: {
    '(pointerenter)': 'enter()',
    '(pointerleave)': 'leave()',
    '(focusin)': 'focus()',
    '(focusout)': 'leave()',
    '(pointerdown)': 'leave()',
    '(keydown.escape)': 'leave()',
  },
})
export class Tooltip implements OnDestroy {
  readonly paTooltip = input<string | null | undefined>('');
  readonly paTooltipKeys = input<string | null>(null);
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);

  protected enter(): void {
    const text = this.paTooltip();
    if (text) layer.schedule(this.host.nativeElement, text, this.paTooltipKeys(), false);
  }

  protected focus(): void {
    const text = this.paTooltip();
    if (text && this.host.nativeElement.matches(':focus-visible')) {
      layer.schedule(this.host.nativeElement, text, this.paTooltipKeys(), true);
    }
  }

  protected leave(): void {
    layer.hide(this.host.nativeElement);
  }

  ngOnDestroy(): void {
    layer.hide(this.host.nativeElement);
  }
}
