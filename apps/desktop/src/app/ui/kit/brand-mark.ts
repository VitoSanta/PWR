import { ChangeDetectionStrategy, Component } from '@angular/core';

/** The PWR mark keeps its blue accent while the glyph follows the selected theme. */
@Component({
  selector: 'pa-brand-mark',
  host: { 'aria-hidden': 'true' },
  template: `
    <svg viewBox="275 275 705 705" aria-hidden="true" focusable="false">
      <path fill="currentColor" d="M 502 298 H 781 C 881 298 957 376 957 482 C 957 586 890 650 779 672 C 756 677 742 671 742 652 V 598 C 742 582 751 574 768 570 C 796 564 811 544 811 509 V 487 C 811 454 788 432 756 432 H 514 C 476 432 445 463 445 502 V 922 C 445 945 429 957 407 957 H 333 C 311 957 298 941 298 920 V 501 C 298 387 388 298 502 298 Z" />
      <path fill="currentColor" d="M 535 710 H 627 C 645 710 657 724 657 742 V 922 C 657 944 642 957 623 957 H 539 C 518 957 504 942 504 921 V 742 C 504 724 517 710 535 710 Z" />
      <rect class="accent" x="505" y="495" width="205" height="191" rx="35" />
    </svg>
  `,
  styles: [`
    :host { display: block; color: var(--text-primary); }
    svg { display: block; width: 100%; height: 100%; }
    .accent { fill: #087cf7; }
  `],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class BrandMark {}
