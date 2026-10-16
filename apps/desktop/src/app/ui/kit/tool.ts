import { ChangeDetectionStrategy, Component, input, model, output } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { FormsModule } from '@angular/forms';
import { Icon, IconName } from './icon';
import { Tooltip } from './tooltip';

/*
 * The parts every tool window is built from, so Editor, Browser, Terminal,
 * Plan and the rest read as one family: a bar, round icon actions, tabs, a
 * search field and an empty state. Styles are in styles/tools.css.
 */

/** A round, icon-only action with its name as tooltip and accessible label. */
@Component({
  selector: 'pa-action',
  imports: [MatButtonModule, Icon, Tooltip],
  template: `
    <button
      matIconButton
      class="icon-btn pa-action-btn"
      [class.icon-btn-sm]="size() === 'sm'"
      [attr.data-tone]="tone()"
      [type]="type()"
      [disabled]="disabled()"
      [attr.aria-label]="label()"
      [attr.aria-pressed]="pressed()"
      [paTooltip]="label()"
      [paTooltipKeys]="keys()"
    >
      <pa-icon [name]="icon()" [size]="size() === 'sm' ? 16 : 18" />
    </button>
  `,
  host: { class: 'pa-action' },
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Action {
  readonly icon = input.required<IconName>();
  readonly label = input.required<string>();
  /** Plain by default; tonal and filled carry emphasis; danger is for stopping things. */
  readonly tone = input<'plain' | 'tonal' | 'filled' | 'danger' | 'quiet-danger'>('plain');
  readonly size = input<'sm' | 'md'>('sm');
  readonly disabled = input(false);
  readonly pressed = input<boolean | null>(null);
  readonly keys = input<string | null>(null);
  readonly type = input<'button' | 'submit'>('button');
}

/** One row of controls: a title or status on the left, actions on the right. */
@Component({
  selector: 'pa-tool-bar',
  template: `<ng-content />`,
  host: { class: 'tool-bar' },
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ToolBar {}

/** A tab, with an optional icon, unsaved dot, note and close button. */
@Component({
  selector: 'pa-tool-tab',
  imports: [Icon],
  template: `
    <button
      class="tool-tab-label"
      role="tab"
      [attr.aria-selected]="active()"
      [attr.tabindex]="active() ? 0 : -1"
      [attr.title]="hint() || label()"
      (click)="selected.emit()"
    >
      @if (icon(); as name) {
        <pa-icon [name]="name" [size]="16" />
      }
      <span class="tool-tab-name">{{ label() }}</span>
      @if (dirty()) {
        <span class="tool-tab-dirty" role="img" aria-label="Unsaved changes"></span>
      }
      @if (note()) {
        <span class="tool-tab-note">{{ note() }}</span>
      }
    </button>
    @if (closable()) {
      <button type="button" class="tool-tab-close" [attr.aria-label]="'Close ' + label()" (click)="closed.emit()">
        <pa-icon name="x" [size]="14" />
      </button>
    }
  `,
  host: { class: 'tool-tab', '[class.active]': 'active()', '[class.is-closable]': 'closable()' },
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ToolTab {
  readonly label = input.required<string>();
  readonly icon = input<IconName | null>(null);
  readonly active = input(false);
  readonly dirty = input(false);
  readonly closable = input(true);
  readonly note = input('');
  readonly hint = input('');
  readonly selected = output<void>();
  readonly closed = output<void>();
}

/** Google's filled, full-round search field. */
@Component({
  selector: 'pa-search-field',
  imports: [FormsModule, Icon],
  template: `
    <pa-icon name="search" [size]="16" />
    <input
      type="search"
      [attr.aria-label]="label()"
      [placeholder]="placeholder()"
      [ngModel]="value()"
      (ngModelChange)="value.set($event)"
      (keydown.enter)="submitted.emit(value())"
    />
    @if (value()) {
      <button type="button" class="search-clear" aria-label="Clear" (click)="value.set(''); cleared.emit()">
        <pa-icon name="x" [size]="14" />
      </button>
    }
  `,
  host: { class: 'search-field' },
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SearchField {
  readonly value = model('');
  readonly label = input.required<string>();
  readonly placeholder = input('');
  readonly submitted = output<string>();
  readonly cleared = output<void>();
}

/** What a tool says when it has nothing to show yet. */
@Component({
  selector: 'pa-empty-state',
  imports: [Icon],
  template: `
    <span class="empty-icon"><pa-icon [name]="icon()" [size]="24" /></span>
    <strong class="empty-title">{{ title() }}</strong>
    <span class="empty-text"><ng-content /></span>
  `,
  host: { class: 'empty-state' },
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class EmptyState {
  readonly icon = input.required<IconName>();
  readonly title = input.required<string>();
}
