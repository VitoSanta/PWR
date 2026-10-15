import { ChangeDetectionStrategy, Component, input, model } from '@angular/core';
import { MatSelectModule } from '@angular/material/select';

export interface SelectOption<T = unknown> {
  value: T;
  label: string;
  /** A short second line, or trailing text, shown in the list only. */
  hint?: string;
  disabled?: boolean;
}

/**
 * Material's select, outside a form field: the WAI-ARIA select-only
 * combobox, with the keyboard (arrows, Home/End, Enter/Space, Escape kept
 * to itself, type-ahead) and the list placed by the CDK overlay. PWR's API
 * stays: options, a two-way value, a label, a small size.
 */
@Component({
  selector: 'pa-select',
  imports: [MatSelectModule],
  template: `
    <mat-select
      class="select-trigger"
      [class.select-sm]="size() === 'sm'"
      panelClass="pa-select-panel"
      [value]="value()"
      (selectionChange)="value.set($event.value)"
      [placeholder]="placeholder()"
      [disabled]="disabled()"
      [aria-label]="ariaLabel() ?? ''"
      [aria-labelledby]="labelledBy() ?? ''"
      panelWidth=""
    >
      @for (option of options(); track $index) {
        <mat-option [value]="option.value" [disabled]="option.disabled ?? false">
          <span class="truncate">{{ option.label }}</span>
          @if (option.hint) {
            <span class="menu-hint">{{ option.hint }}</span>
          }
        </mat-option>
      }
    </mat-select>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'select' },
})
export class Select {
  readonly options = input.required<readonly SelectOption[]>();
  readonly value = model<unknown>();
  readonly ariaLabel = input<string | null>(null);
  readonly labelledBy = input<string | null>(null);
  readonly placeholder = input('Select…');
  readonly disabled = input(false);
  readonly size = input<'sm' | 'md'>('md');
}
