import { ChangeDetectionStrategy, Component, input } from '@angular/core';

/**
 * The PWR mark: a P drawn in two soft strokes, a stem and a ring, in
 * PWR's blue. While PWR works the ring turns -- its light sweeps round the
 * bowl -- and the mark breathes; at rest it is still. One mark everywhere,
 * from the welcome to each answer.
 */
@Component({
  selector: 'pa-brand-mark',
  host: { 'aria-hidden': 'true', '[class.is-active]': 'active()' },
  template: `
    <svg viewBox="0 0 32 32" aria-hidden="true" focusable="false">
      <defs>
        <linearGradient id="pa-mark-ring" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stop-color="var(--mark-a)" />
          <stop offset="0.55" stop-color="var(--mark-b)" />
          <stop offset="1" stop-color="var(--mark-c)" />
        </linearGradient>
      </defs>
      <rect class="stem" x="6" y="5" width="6" height="23" rx="3" />
      <g class="ring">
        <circle cx="17.5" cy="13" r="7.5" fill="none" stroke="url(#pa-mark-ring)" stroke-width="6" />
      </g>
    </svg>
  `,
  styles: [`
    :host {
      --mark-a: light-dark(#1a73e8, #8ab4f8);
      --mark-b: light-dark(#4c8df6, #a8c7fa);
      --mark-c: light-dark(#a8c7fa, #d3e3fd);
      display: block;
    }
    svg { display: block; width: 100%; height: 100%; overflow: visible; }
    .stem { fill: var(--mark-a); }
    .ring {
      transform-box: view-box;
      transform-origin: 17.5px 13px;
    }
    :host(.is-active) .ring {
      animation: pa-mark-turn 1.6s cubic-bezier(0.4, 0, 0.2, 1) infinite;
    }
    :host(.is-active) svg {
      animation: pa-mark-breathe 1.6s ease-in-out infinite;
    }
    @keyframes pa-mark-turn {
      to { transform: rotate(360deg); }
    }
    @keyframes pa-mark-breathe {
      50% { transform: scale(0.9); }
    }
    @media (prefers-reduced-motion: reduce) {
      :host(.is-active) .ring,
      :host(.is-active) svg { animation: none; }
    }
  `],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class BrandMark {
  /** PWR is working: the mark moves. */
  readonly active = input(false);
}
