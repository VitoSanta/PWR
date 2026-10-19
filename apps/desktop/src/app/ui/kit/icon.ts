import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { MatIconModule } from '@angular/material/icon';

/*
 * PWR's one icon family: Google's Material Symbols Rounded, served from the
 * app (public/fonts, a subset holding only these glyphs; Apache 2.0). Each
 * PWR name maps to the symbol drawn for it; a new icon needs its symbol
 * added here and to the font's subset (`icon_names` in the Google Fonts
 * request that made it, fonts.css).
 */
export const ICONS = {
  plus: 'add',
  minus: 'remove',
  x: 'close',
  check: 'check',
  'chevron-down': 'expand_more',
  'chevron-up': 'expand_less',
  'chevron-right': 'chevron_right',
  'chevron-left': 'chevron_right',
  'arrow-up': 'arrow_upward',
  'arrow-right': 'arrow_forward',
  'corner-down-right': 'subdirectory_arrow_right',
  'list-plus': 'playlist_add',
  stop: 'stop',
  folder: 'folder',
  file: 'draft',
  'file-text': 'description',
  'file-code': 'code_blocks',
  image: 'image',
  paperclip: 'attach_file',
  message: 'chat_bubble',
  'square-pen': 'edit_square',
  'panel-left': 'left_panel_open',
  'panel-right': 'right_panel_open',
  grid: 'grid_view',
  columns: 'view_column',
  rows: 'table_rows',
  maximize: 'open_in_full',
  minimize: 'close_fullscreen',
  settings: 'settings',
  trash: 'delete',
  undo: 'undo',
  sparkles: 'auto_awesome',
  eye: 'visibility',
  pencil: 'edit',
  terminal: 'terminal',
  search: 'search',
  download: 'download',
  globe: 'language',
  move: 'open_with',
  'circle-dot': 'radio_button_checked',
  alert: 'warning',
  info: 'info',
  'check-circle': 'check_circle',
  'x-circle': 'cancel',
  shield: 'shield',
  'shield-check': 'verified_user',
  lock: 'lock',
  cpu: 'memory',
  memory: 'memory_alt',
  'hard-drive': 'hard_drive',
  monitor: 'desktop_windows',
  sun: 'light_mode',
  moon: 'dark_mode',
  refresh: 'refresh',
  'external-link': 'open_in_new',
  copy: 'content_copy',
  target: 'adjust',
  zap: 'bolt',
  command: 'keyboard_command_key',
  box: 'package_2',
  gauge: 'speed',
  'git-compare': 'compare_arrows',
  activity: 'monitoring',
  stethoscope: 'stethoscope',
  'scroll-text': 'receipt_long',
  heart: 'favorite',
  history: 'history',
  more: 'more_horiz',
  selector: 'unfold_more',
  template: 'dashboard_customize',
  host: 'dns',
} as const satisfies Record<string, string>;

export type IconName = keyof typeof ICONS;

/** Symbols drawn filled, as their outline glyphs once were. */
const FILLED: ReadonlySet<IconName> = new Set<IconName>(['stop', 'circle-dot', 'more']);

@Component({
  selector: 'pa-icon',
  imports: [MatIconModule],
  template: `<mat-icon fontSet="material-symbols-rounded" aria-hidden="true">{{ glyph() }}</mat-icon>`,
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    class: 'icon',
    '[class.icon-reversed]': "name() === 'chevron-left'",
    '[style.width.px]': 'size()',
    '[style.height.px]': 'size()',
    '[style.--icon-size.px]': 'size()',
    '[style.--icon-weight]': 'weight()',
    '[style.--icon-fill]': 'filled() ? 1 : 0',
  },
})
export class Icon {
  readonly name = input.required<IconName>();
  readonly size = input(18);
  /** The old stroke width, read as the symbol's weight. */
  readonly stroke = input(1.75);
  protected readonly glyph = computed(() => ICONS[this.name()]);
  protected readonly filled = computed(() => FILLED.has(this.name()));
  protected readonly weight = computed(() => {
    const stroke = this.stroke();
    return stroke <= 1.5 ? 300 : stroke >= 2.25 ? 500 : 400;
  });
}
