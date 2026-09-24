import { Injectable, computed, effect, signal, untracked } from '@angular/core';

export const LEFT = { min: 240, max: 360, initial: 288 } as const;
export const RIGHT = { min: 280, max: 420, initial: 340 } as const;
/** The conversation never gets narrower than this while a side panel is docked. */
export const MAIN_MIN = 560;
/** The collapsed left navigation. */
export const RAIL = 52;

const KEY = 'pwr:layout';

export type LeftMode = 'docked' | 'rail' | 'overlay';
export type RightMode = 'docked' | 'hidden' | 'overlay';

interface Saved {
  leftOpen: boolean;
  rightOpen: boolean;
  leftWidth: number;
  rightWidth: number;
}

const clamp = (value: number, min: number, max: number) => Math.max(min, Math.min(max, value));

/** Where each side panel goes for a window width: the pure part of the layout. */
export function arrange(
  viewport: number,
  state: Saved & { leftPeek: boolean; rightPeek: boolean },
): { left: LeftMode; right: RightMode; leftDockable: boolean; rightDockable: boolean } {
  const leftDockable = viewport - state.leftWidth >= MAIN_MIN;
  const left: LeftMode =
    state.leftOpen && leftDockable ? 'docked' : state.leftPeek ? 'overlay' : 'rail';
  const leftTaken = left === 'docked' ? state.leftWidth : RAIL;
  // The inspector gives way first: it docks only if the conversation keeps
  // its minimum beside whatever the left side takes.
  const rightDockable = viewport - leftTaken - state.rightWidth >= MAIN_MIN;
  const right: RightMode =
    state.rightOpen && rightDockable ? 'docked' : state.rightPeek ? 'overlay' : 'hidden';
  return { left, right, leftDockable, rightDockable };
}

/**
 * The desktop shell's three columns. Each side panel is docked when the
 * window has room for it, otherwise it collapses (the inspector first) and
 * can be opened over the conversation instead. Open/closed and the widths a
 * person drags to are remembered; an overlay is not.
 */
@Injectable({ providedIn: 'root' })
export class LayoutService {
  private readonly saved = this.read();
  readonly viewport = signal(typeof window !== 'undefined' ? window.innerWidth : 1440);
  readonly leftOpen = signal(this.saved.leftOpen);
  readonly rightOpen = signal(this.saved.rightOpen);
  readonly leftWidth = signal(this.saved.leftWidth);
  readonly rightWidth = signal(this.saved.rightWidth);
  readonly leftPeek = signal(false);
  readonly rightPeek = signal(false);
  /** A resize handle is being dragged: width transitions are paused. */
  readonly resizing = signal(false);

  private readonly arrangement = computed(() =>
    arrange(this.viewport(), {
      leftOpen: this.leftOpen(),
      rightOpen: this.rightOpen(),
      leftWidth: this.leftWidth(),
      rightWidth: this.rightWidth(),
      leftPeek: this.leftPeek(),
      rightPeek: this.rightPeek(),
    }),
  );
  readonly left = computed(() => this.arrangement().left);
  readonly right = computed(() => this.arrangement().right);

  constructor() {
    if (typeof window !== 'undefined') {
      window.addEventListener('resize', () => this.viewport.set(window.innerWidth), {
        passive: true,
      });
    }

    // An overlay is only a stand-in: once the panel can dock again, it does.
    effect(() => {
      const { leftDockable, rightDockable } = this.arrangement();
      untracked(() => {
        if (leftDockable && this.leftPeek()) {
          this.leftPeek.set(false);
          this.leftOpen.set(true);
        }
        if (rightDockable && this.rightPeek()) {
          this.rightPeek.set(false);
          this.rightOpen.set(true);
        }
      });
    });

    effect(() => {
      const state: Saved = {
        leftOpen: this.leftOpen(),
        rightOpen: this.rightOpen(),
        leftWidth: this.leftWidth(),
        rightWidth: this.rightWidth(),
      };
      if (this.resizing()) return;
      try {
        localStorage.setItem(KEY, JSON.stringify(state));
      } catch {
        /* storage unavailable */
      }
    });
  }

  toggleLeft(): void {
    const { left, leftDockable } = this.arrangement();
    if (left === 'docked') this.leftOpen.set(false);
    else if (leftDockable) {
      this.leftOpen.set(true);
      this.leftPeek.set(false);
    } else this.leftPeek.update((open) => !open);
  }

  toggleRight(): void {
    const { right, rightDockable } = this.arrangement();
    if (right === 'docked') this.rightOpen.set(false);
    else if (rightDockable) {
      this.rightOpen.set(true);
      this.rightPeek.set(false);
    } else this.rightPeek.update((open) => !open);
  }

  /** Closes whichever panel floats over the conversation; true if one did. */
  closeOverlays(): boolean {
    const any = this.leftPeek() || this.rightPeek();
    this.leftPeek.set(false);
    this.rightPeek.set(false);
    return any;
  }

  setLeftWidth(width: number): void {
    const right = this.right() === 'docked' ? this.rightWidth() : 0;
    const room = this.viewport() - right - MAIN_MIN;
    this.leftWidth.set(
      Math.round(clamp(width, LEFT.min, Math.max(LEFT.min, Math.min(LEFT.max, room)))),
    );
  }

  setRightWidth(width: number): void {
    const left = this.left() === 'docked' ? this.leftWidth() : RAIL;
    const room = this.viewport() - left - MAIN_MIN;
    this.rightWidth.set(
      Math.round(clamp(width, RIGHT.min, Math.max(RIGHT.min, Math.min(RIGHT.max, room)))),
    );
  }

  private read(): Saved {
    const fallback: Saved = {
      leftOpen: true,
      rightOpen: true,
      leftWidth: LEFT.initial,
      rightWidth: RIGHT.initial,
    };
    try {
      const saved = JSON.parse(localStorage.getItem(KEY) ?? 'null') as Partial<Saved> | null;
      if (!saved) {
        // Widths saved by the earlier layout.
        const left = Number(localStorage.getItem('pwr:left-width'));
        const right = Number(localStorage.getItem('pwr:right-width'));
        return {
          ...fallback,
          leftWidth: left ? clamp(left, LEFT.min, LEFT.max) : fallback.leftWidth,
          rightWidth: right ? clamp(right, RIGHT.min, RIGHT.max) : fallback.rightWidth,
        };
      }
      return {
        leftOpen: saved.leftOpen ?? true,
        rightOpen: saved.rightOpen ?? true,
        leftWidth: clamp(Number(saved.leftWidth) || LEFT.initial, LEFT.min, LEFT.max),
        rightWidth: clamp(Number(saved.rightWidth) || RIGHT.initial, RIGHT.min, RIGHT.max),
      };
    } catch {
      return fallback;
    }
  }
}
