import { AfterViewInit, ChangeDetectionStrategy, Component, ElementRef, NgZone, OnDestroy, inject, input, signal, viewChild } from '@angular/core';
import { bridge } from '../../core/bridge';

// One native child per tab, shown one at a time. Serialize replacement and disposal across cards.
let pending: Promise<unknown> = Promise.resolve();
function enqueue(action: () => Promise<unknown>): Promise<unknown> {
  const next = pending.then(action, action);
  pending = next.catch(() => {});
  return next;
}

/** A Browser tab's page: a native web view over this element, with no PWR capabilities. */
@Component({
  selector: 'pa-web-view',
  host: { class: 'browser-frame web-view' },
  template: `
    <div #surface class="web-view-surface"></div>
    @if (error()) { <p class="banner banner-danger card-banner" role="alert">{{ error() }}</p> }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class WebView implements AfterViewInit, OnDestroy {
  readonly address = input.required<string>();
  readonly tabId = input.required<string>();
  protected readonly error = signal('');
  private readonly surface = viewChild.required<ElementRef<HTMLElement>>('surface');
  private readonly zone = inject(NgZone);
  private animation = 0;
  private disposed = false;
  private signature = '';
  /** The card slides in for a moment; the native page, which no transition can carry, waits for it. */
  private settled = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  private settleTimer?: ReturnType<typeof setTimeout>;

  ngAfterViewInit(): void {
    if (!this.settled) this.settleTimer = setTimeout(() => (this.settled = true), 360);
    this.zone.runOutsideAngular(() => {
      const tick = () => {
        if (this.disposed) return;
        const element = this.surface().nativeElement;
        const rect = element.getBoundingClientRect();
        // Hide native content under menus/dialogs, out of view and on collapse.
        const visible = this.settled && webViewVisible(element, rect);
        // The page fills its card, so its bottom corners follow the card's.
        const card = element.closest('.wb-card');
        const radius = card ? Math.max(0, parseFloat(getComputedStyle(card).borderBottomLeftRadius) - 1) || 0 : 0;
        const options = { id: this.tabId(), address: this.address(), x: Math.max(0, rect.x), y: Math.max(0, rect.y),
          width: Math.max(1, rect.width), height: Math.max(1, rect.height), visible, radius };
        const signature = JSON.stringify(options);
        if (signature !== this.signature) {
          this.signature = signature;
          void enqueue(() => bridge.browserSync(options)).then(() => {
            if (!this.disposed && this.error()) this.zone.run(() => this.error.set(''));
          }, error => {
            if (!this.disposed) this.zone.run(() => this.error.set(String(error).replace(/^Error: /, '')));
          });
        }
        this.animation = requestAnimationFrame(tick);
      };
      this.animation = requestAnimationFrame(tick);
    });
  }
  ngOnDestroy(): void {
    this.disposed = true;
    clearTimeout(this.settleTimer);
    cancelAnimationFrame(this.animation);
    const id = this.tabId();
    void enqueue(() => bridge.browserSync({ id, x: 0, y: 0, width: 1, height: 1, visible: false })).catch(() => {});
  }
}

export function webViewVisible(element: HTMLElement, rect: DOMRect): boolean {
  if (!element.isConnected || document.visibilityState === 'hidden' || rect.width < 2 || rect.height < 2
      || rect.x < 0 || rect.y < 0 || rect.right > innerWidth + 1 || rect.bottom > innerHeight + 1) return false;
  return [[rect.x + 1, rect.y + 1], [rect.right - 1, rect.y + 1],
    [rect.x + rect.width / 2, rect.y + rect.height / 2], [rect.right - 1, rect.bottom - 1]]
    .every(([x,y]) => { const top = document.elementFromPoint(x,y); return top === element || !!top && element.contains(top); });
}
