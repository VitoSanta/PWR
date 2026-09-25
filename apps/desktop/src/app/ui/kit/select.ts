import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  HostListener,
  computed,
  inject,
  input,
  model,
  signal,
  viewChild,
} from '@angular/core';
import { Icon } from './icon';

export interface SelectOption<T = unknown> {
  value: T;
  label: string;
  /** A short second line, or trailing text, shown in the list only. */
  hint?: string;
  disabled?: boolean;
}

let nextId = 0;

/**
 * A themed replacement for the native <select>: the WAI-ARIA "select-only
 * combobox". Focus stays on the button; the list is referenced with
 * aria-activedescendant. Arrow keys, Home/End, Enter/Space, Escape, Tab and
 * type-ahead behave as in a native select.
 */
@Component({
  selector: 'pa-select',
  imports: [Icon],
  template: `
    <button
      #trigger
      type="button"
      class="select-trigger"
      [class.select-sm]="size() === 'sm'"
      role="combobox"
      aria-haspopup="listbox"
      [attr.aria-expanded]="open()"
      [attr.aria-controls]="listId"
      [attr.aria-activedescendant]="open() && active() >= 0 ? optionId(active()) : null"
      [attr.aria-label]="ariaLabel()"
      [attr.aria-labelledby]="labelledBy()"
      [disabled]="disabled()"
      (click)="toggle()"
      (keydown)="onKey($event)"
      (blur)="close()"
    >
      <span class="select-value truncate">{{ selected()?.label ?? placeholder() }}</span>
      <pa-icon name="chevron-down" [size]="16" />
    </button>
    @if (open()) {
      <div
        class="select-list menu surface-floating"
        role="listbox"
        [id]="listId"
        [attr.aria-label]="ariaLabel()"
        [style.top.px]="position().top"
        [style.bottom.px]="position().bottom"
        [style.left.px]="position().left"
        [style.min-width.px]="position().width"
        [style.max-height.px]="position().maxHeight"
        animate.leave="anim-fade-out"
      >
        @for (option of options(); track $index; let index = $index) {
          <div
            class="menu-item"
            role="option"
            [id]="optionId(index)"
            [class.is-active]="index === active()"
            [attr.aria-selected]="option.value === value()"
            [attr.aria-disabled]="option.disabled || null"
            (pointerdown)="$event.preventDefault()"
            (click)="choose(index)"
            (pointermove)="active.set(index)"
          >
            <span class="truncate">{{ option.label }}</span>
            @if (option.hint) {
              <span class="menu-hint">{{ option.hint }}</span>
            }
            <pa-icon class="menu-check" name="check" [size]="16" />
          </div>
        }
      </div>
    }
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

  protected readonly listId = `pa-select-${nextId++}`;
  protected readonly open = signal(false);
  protected readonly active = signal(-1);
  protected readonly position = signal({
    top: null as number | null,
    bottom: null as number | null,
    left: 0,
    width: 0,
    maxHeight: 280,
  });
  protected readonly selected = computed(() =>
    this.options().find((option) => option.value === this.value()),
  );
  private readonly trigger = viewChild.required<ElementRef<HTMLButtonElement>>('trigger');
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);
  private typed = '';
  private typedAt = 0;

  protected optionId(index: number): string {
    return `${this.listId}-${index}`;
  }

  protected toggle(): void {
    if (this.open()) this.close();
    else this.show();
  }

  private show(): void {
    if (this.disabled()) return;
    const rect = this.trigger().nativeElement.getBoundingClientRect();
    const below = window.innerHeight - rect.bottom - 12;
    const above = rect.top - 12;
    const wanted = Math.min(280, this.options().length * 33 + 10);
    const up = below < wanted && above > below;
    this.position.set({
      top: up ? null : rect.bottom + 4,
      bottom: up ? window.innerHeight - rect.top + 4 : null,
      left: Math.min(rect.left, window.innerWidth - Math.max(rect.width, 180) - 8),
      width: rect.width,
      maxHeight: Math.max(120, Math.min(280, up ? above : below)),
    });
    const current = this.options().findIndex((option) => option.value === this.value());
    this.active.set(current >= 0 ? current : this.firstEnabled(0, 1));
    this.open.set(true);
    // WebKit does not focus a button that is clicked, and the arrow keys and
    // Escape are the trigger's.
    this.trigger().nativeElement.focus();
    queueMicrotask(() => this.revealActive());
  }

  close(): void {
    this.open.set(false);
  }

  protected choose(index: number): void {
    const option = this.options()[index];
    if (!option || option.disabled) return;
    this.value.set(option.value);
    this.close();
    this.trigger().nativeElement.focus();
  }

  protected onKey(event: KeyboardEvent): void {
    const open = this.open();
    switch (event.key) {
      case 'ArrowDown':
      case 'ArrowUp': {
        event.preventDefault();
        if (!open) return this.show();
        const step = event.key === 'ArrowDown' ? 1 : -1;
        this.move(this.firstEnabled(this.active() + step, step));
        return;
      }
      case 'Home':
      case 'End':
        if (!open) return;
        event.preventDefault();
        this.move(
          event.key === 'Home'
            ? this.firstEnabled(0, 1)
            : this.firstEnabled(this.options().length - 1, -1),
        );
        return;
      case 'Enter':
      case ' ':
        event.preventDefault();
        if (open) this.choose(this.active());
        else this.show();
        return;
      case 'Escape':
        if (!open) return;
        // Handled here: the dialog or popover around this stays open.
        event.preventDefault();
        event.stopPropagation();
        this.close();
        return;
      case 'Tab':
        if (open) this.close();
        return;
      default:
        if (event.key.length === 1 && !event.metaKey && !event.ctrlKey) this.typeAhead(event.key);
    }
  }

  private typeAhead(key: string): void {
    const now = Date.now();
    this.typed = now - this.typedAt > 700 ? key.toLowerCase() : this.typed + key.toLowerCase();
    this.typedAt = now;
    const index = this.options().findIndex(
      (option) => !option.disabled && option.label.toLowerCase().startsWith(this.typed),
    );
    if (index < 0) return;
    if (this.open()) this.move(index);
    else this.value.set(this.options()[index].value);
  }

  private firstEnabled(from: number, step: 1 | -1): number {
    const options = this.options();
    for (let i = 0, index = from; i < options.length; i++, index += step) {
      const wrapped = (index + options.length) % options.length;
      if (!options[wrapped].disabled) return wrapped;
    }
    return -1;
  }

  private move(index: number): void {
    this.active.set(index);
    this.revealActive();
  }

  private revealActive(): void {
    document.getElementById(this.optionId(this.active()))?.scrollIntoView?.({ block: 'nearest' });
  }

  @HostListener('window:resize')
  @HostListener('window:blur')
  protected dismiss(): void {
    this.close();
  }

  /**
   * A press anywhere else closes the list. Blur alone did not: WebKit -- the
   * app's engine on macOS -- does not move focus to a button that is clicked,
   * so opening another select left this one open, and several lists showed
   * at once.
   */
  @HostListener('document:pointerdown', ['$event'])
  protected pressed(event: PointerEvent): void {
    if (this.open() && !this.host.nativeElement.contains(event.target as Node)) this.close();
  }

  @HostListener('document:scroll', ['$event'])
  protected scrolled(event: Event): void {
    // The list is fixed-position: it would drift from its button.
    if (this.open() && !this.host.nativeElement.contains(event.target as Node)) this.close();
  }
}
