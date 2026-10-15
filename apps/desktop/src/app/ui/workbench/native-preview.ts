import { AfterViewInit, ChangeDetectionStrategy, Component, ElementRef, NgZone, OnDestroy, inject, input, signal, viewChild } from '@angular/core';
import { bridge } from '../../core/bridge';

// One native child per window. Serialize replacement and disposal across cards.
let pending: Promise<unknown> = Promise.resolve();
function enqueue(action: () => Promise<unknown>): Promise<unknown> {
  const next = pending.then(action, action);
  pending = next.catch(() => {});
  return next;
}

/** The page is a top-level, local-only webview with no PWR capabilities. */
@Component({
  selector: 'pa-native-preview',
  host: { class: 'browser-frame native-preview' },
  template: `
    <div #surface class="native-preview-surface"></div>
    @if (error()) { <p class="banner banner-danger card-banner" role="alert">{{ error() }}</p> }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class NativePreview implements AfterViewInit, OnDestroy {
  readonly address = input.required<string>();
  protected readonly error = signal('');
  private readonly surface = viewChild.required<ElementRef<HTMLElement>>('surface');
  private readonly zone = inject(NgZone);
  private animation = 0;
  private disposed = false;
  private signature = '';

  ngAfterViewInit(): void {
    this.zone.runOutsideAngular(() => {
      const tick = () => {
        if (this.disposed) return;
        const element = this.surface().nativeElement;
        const rect = element.getBoundingClientRect();
        // Hide native content under menus/dialogs, out of view and on collapse.
        const visible = previewVisible(element, rect);
        const options = { address: this.address(), x: Math.max(0, rect.x), y: Math.max(0, rect.y),
          width: Math.max(1, rect.width), height: Math.max(1, rect.height), visible };
        const signature = JSON.stringify(options);
        if (signature !== this.signature) {
          this.signature = signature;
          void enqueue(() => bridge.previewSync(options)).then(() => {
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
    cancelAnimationFrame(this.animation);
    void enqueue(() => bridge.previewClose()).catch(() => {});
  }
}

export function previewVisible(element: HTMLElement, rect: DOMRect): boolean {
  if (!element.isConnected || document.visibilityState === 'hidden' || rect.width < 2 || rect.height < 2
      || rect.x < 0 || rect.y < 0 || rect.right > innerWidth + 1 || rect.bottom > innerHeight + 1) return false;
  return [[rect.x + 1, rect.y + 1], [rect.right - 1, rect.y + 1],
    [rect.x + rect.width / 2, rect.y + rect.height / 2], [rect.right - 1, rect.bottom - 1]]
    .every(([x,y]) => { const top = document.elementFromPoint(x,y); return top === element || !!top && element.contains(top); });
}
