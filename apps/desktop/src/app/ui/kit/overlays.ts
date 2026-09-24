import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { ConfirmService, ToastService } from '../../core/ui';
import { Dialog } from './dialog';
import { Icon } from './icon';

/** The toasts, bottom right. Announced politely to screen readers. */
@Component({
  selector: 'pa-toasts',
  imports: [Icon],
  template: `
    <div class="toasts" role="status" aria-live="polite">
      @for (toast of toasts.toasts(); track toast.id) {
        <div class="toast" [class]="'toast tone-' + toast.tone" animate.leave="is-leaving">
          <pa-icon
            [name]="
              toast.tone === 'danger'
                ? 'x-circle'
                : toast.tone === 'success'
                  ? 'check-circle'
                  : 'info'
            "
            [size]="16"
          />
          <span class="toast-text">{{ toast.text }}</span>
          <button
            class="icon-btn icon-btn-sm"
            (click)="toasts.dismiss(toast.id)"
            aria-label="Dismiss"
          >
            <pa-icon name="x" [size]="14" />
          </button>
        </div>
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Toasts {
  protected readonly toasts = inject(ToastService);
}

/** Renders ConfirmService requests with the shared dialog. */
@Component({
  selector: 'pa-confirm-host',
  imports: [Dialog, Icon],
  template: `
    @if (confirm.request(); as request) {
      <pa-dialog
        dialogRole="alertdialog"
        labelledBy="confirm-title"
        describedBy="confirm-message"
        [dismissible]="!busy()"
        (closed)="cancel()"
        animate.leave="is-leaving"
      >
        <div class="dialog-header">
          <span
            class="dialog-icon"
            [class]="'dialog-icon tone-' + (request.tone === 'primary' ? 'accent' : 'danger')"
          >
            <pa-icon [name]="request.tone === 'primary' ? 'info' : 'alert'" [size]="18" />
          </span>
          <div class="dialog-header-text">
            <h2 class="dialog-title" id="confirm-title">{{ request.title }}</h2>
            @if (request.message) {
              <p class="dialog-description" id="confirm-message">{{ request.message }}</p>
            }
          </div>
        </div>
        @if (request.subject || error()) {
          <div class="dialog-body">
            @if (request.subject) {
              <p class="dialog-subject" [class.is-text]="request.subjectIsText">
                {{ request.subject }}
              </p>
            }
            @if (error()) {
              <p class="banner banner-danger" role="alert">
                <pa-icon name="alert" [size]="16" />{{ error() }}
              </p>
            }
          </div>
        }
        <div class="dialog-footer">
          <button class="btn" (click)="cancel()" [disabled]="busy()" data-autofocus>Cancel</button>
          <button
            [class]="request.tone === 'primary' ? 'btn btn-primary' : 'btn btn-danger'"
            (click)="accept()"
            [disabled]="busy()"
            [attr.aria-busy]="busy()"
          >
            {{ request.confirmLabel }}
          </button>
        </div>
      </pa-dialog>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ConfirmHost {
  protected readonly confirm = inject(ConfirmService);
  protected readonly busy = signal(false);
  protected readonly error = signal('');

  protected cancel(): void {
    if (this.busy()) return;
    this.error.set('');
    this.confirm.settle(false);
  }

  protected async accept(): Promise<void> {
    const request = this.confirm.request();
    if (!request || this.busy()) return;
    if (!request.action) {
      this.confirm.settle(true);
      return;
    }
    this.busy.set(true);
    this.error.set('');
    try {
      await request.action();
      this.confirm.settle(true);
    } catch (error) {
      this.error.set(String(error instanceof Error ? error.message : error));
    } finally {
      this.busy.set(false);
    }
  }
}
