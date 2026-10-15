import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  afterNextRender,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { AgentStore } from '../../core/agent.store';
import { UiStore, shortcut } from '../../core/ui';
import { CardId, WorkbenchStore } from '../../core/workbench';
import { Icon, IconName } from '../kit/icon';

/** Shorter names where the card's own would crowd the wheel. */
const SHORT: Partial<Record<CardId, string>> = { browser: 'Preview', plan: 'Checks' };

const OUTER = 156;
const RING = 112;
const INNER = 62;

interface Spoke {
  key: string;
  label: string;
  description: string;
  icon: IconName;
  keys: string | null;
  lit: boolean;
  choose: () => void;
}

/**
 * The tools as a wheel, where a right click (a two-finger click) landed:
 * every tool on one ring, the one under the pointer's direction lit, its
 * name and what it does in the hub. Point and click; the arrow keys turn
 * it, Enter picks, Escape or a click outside closes it.
 */
@Component({
  selector: 'pa-tool-wheel',
  imports: [Icon],
  host: {
    class: 'tool-wheel-layer',
    '(pointermove)': 'track($event)',
    '(click)': 'clickAt($event)',
    '(keydown)': 'keys($event)',
  },
  template: `
    <div
      class="tool-wheel"
      role="menu"
      aria-label="Tools"
      tabindex="-1"
      [style.left.px]="center().x"
      [style.top.px]="center().y"
      [style.--sector.deg]="sector()"
      [style.--from.deg]="from()"
      [class.has-active]="active() !== null"
    >
      <div class="tool-wheel-disc" aria-hidden="true"></div>
      @for (spoke of spokes(); track spoke.key; let i = $index) {
        <button
          type="button"
          class="tool-wheel-item"
          role="menuitem"
          [class.is-active]="active() === i"
          [class.is-lit]="spoke.lit"
          [attr.aria-label]="spoke.label"
          [style.--x.px]="pos(i).x"
          [style.--y.px]="pos(i).y"
          [style.--i]="i"
          (pointerenter)="active.set(i)"
          (focus)="active.set(i)"
        >
          <pa-icon [name]="spoke.icon" [size]="22" />
        </button>
      }
      <div class="tool-wheel-hub" aria-live="polite">
        @if (current(); as spoke) {
          <strong>{{ spoke.label }}</strong>
          <small>{{ spoke.description }}</small>
          @if (spoke.keys) {
            <span class="kbd">{{ spoke.keys }}</span>
          }
        } @else {
          <strong>Tools</strong>
          <small>Point at one</small>
        }
      </div>
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ToolWheel {
  private readonly work = inject(WorkbenchStore);
  private readonly store = inject(AgentStore);
  private readonly ui = inject(UiStore);
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);

  /** Where the right click landed, in the window. */
  readonly at = input.required<{ x: number; y: number }>();
  readonly close = output<void>();
  /** The run's controls were asked for. */
  readonly run = output<void>();

  protected readonly active = signal<number | null>(null);

  protected readonly spokes = computed<Spoke[]>(() => {
    const cards: Spoke[] = this.work.available().map((card) => ({
      key: card.id,
      label: SHORT[card.id] ?? card.label,
      description: card.description,
      icon: card.icon as IconName,
      keys: card.keys ? shortcut(card.keys) : null,
      lit: this.work.panelVisible() && this.work.isOpen(card.id),
      choose: () => this.work.toggle(card.id),
    }));
    const extra: Spoke[] = [];
    if (!this.store.chatMode()) {
      extra.push({
        key: 'run',
        label: 'Run',
        description: 'Goal mode and approvals',
        icon: 'zap',
        keys: null,
        lit: this.store.goalMode() || this.store.permissionMode() === 'auto',
        choose: () => this.run.emit(),
      });
    }
    extra.push(
      {
        key: 'commands',
        label: 'Commands',
        description: 'Everything PWR can do, by name',
        icon: 'command',
        keys: null,
        lit: false,
        choose: () => this.ui.paletteOpen.set(true),
      },
      {
        key: 'settings',
        label: 'Settings',
        description: 'Models, appearance and permissions',
        icon: 'settings',
        keys: null,
        lit: false,
        choose: () => this.ui.settingsOpen.set(true),
      },
    );
    return [...cards, ...extra];
  });

  protected readonly sector = computed(() => 360 / this.spokes().length);
  /** Where the lit sector starts, clockwise from the top. */
  protected readonly from = computed(() => {
    const i = this.active();
    return i === null ? 0 : i * this.sector() - this.sector() / 2;
  });
  protected readonly current = computed(() => {
    const i = this.active();
    return i === null ? null : this.spokes()[i];
  });

  /** The wheel stays whole inside the window. */
  protected readonly center = computed(() => {
    const { x, y } = this.at();
    const m = OUTER + 12;
    return {
      x: Math.min(Math.max(x, m), window.innerWidth - m),
      y: Math.min(Math.max(y, m), window.innerHeight - m),
    };
  });

  constructor() {
    afterNextRender(() => this.host.nativeElement.querySelector<HTMLElement>('.tool-wheel')?.focus());
  }

  protected pos(i: number): { x: number; y: number } {
    const angle = ((i * this.sector() - 90) * Math.PI) / 180;
    return { x: Math.cos(angle) * RING, y: Math.sin(angle) * RING };
  }

  /** The direction from the hub picks the spoke, as a game's wheel does. */
  protected track(event: PointerEvent): void {
    const { x, y } = this.center();
    const dx = event.clientX - x;
    const dy = event.clientY - y;
    const distance = Math.hypot(dx, dy);
    if (distance < INNER * 0.6) {
      this.active.set(null);
      return;
    }
    if (distance > OUTER + 80) return;
    const angle = (Math.atan2(dy, dx) * 180) / Math.PI + 90 + this.sector() / 2;
    this.active.set(Math.floor((((angle % 360) + 360) % 360) / this.sector()));
  }

  protected clickAt(event: MouseEvent): void {
    const { x, y } = this.center();
    const distance = Math.hypot(event.clientX - x, event.clientY - y);
    if (distance > OUTER + 80 || this.active() === null) {
      this.close.emit();
      return;
    }
    this.pick(this.active()!);
  }

  protected keys(event: KeyboardEvent): void {
    const n = this.spokes().length;
    const i = this.active();
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      this.close.emit();
    } else if (event.key === 'ArrowRight' || event.key === 'ArrowDown' || event.key === 'Tab' && !event.shiftKey) {
      event.preventDefault();
      this.active.set(i === null ? 0 : (i + 1) % n);
    } else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp' || event.key === 'Tab') {
      event.preventDefault();
      this.active.set(i === null ? n - 1 : (i - 1 + n) % n);
    } else if ((event.key === 'Enter' || event.key === ' ') && i !== null) {
      event.preventDefault();
      this.pick(i);
    }
  }

  private pick(i: number): void {
    const spoke = this.spokes()[i];
    if (spoke.key !== 'run') this.close.emit();
    spoke.choose();
  }
}
