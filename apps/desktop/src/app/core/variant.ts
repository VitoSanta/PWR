import { Injectable, effect, signal } from '@angular/core';

/**
 * The app's interchangeable shells. Every variant drives the same stores,
 * conversation, composer, workbench cards and dialogs; what changes is where
 * they are, how the run is shown, and the tokens they are drawn with.
 */
export type Variant = 'studio' | 'instrument' | 'paper' | 'islands' | 'mission' | 'focus'
  | 'relay' | 'workshop' | 'chronicle' | 'pulse' | 'map' | 'deck';

export interface VariantInfo {
  id: Variant;
  label: string;
  /** One line: what the layout is for. */
  description: string;
  /** Whether conversations live in a sidebar; the others reach them from a switcher. */
  sidebar: boolean;
}

export const VARIANTS: VariantInfo[] = [
  {
    id: 'studio',
    label: 'Studio',
    description: 'Sidebar, conversation and workbench side by side',
    sidebar: true,
  },
  {
    id: 'instrument',
    label: 'Instrument',
    description: 'Monochrome, a rail of tools, the run on a strip and a status bar',
    sidebar: false,
  },
  {
    id: 'paper',
    label: 'Paper',
    description: 'A document: the run in the margin, tools in a drawer',
    sidebar: false,
  },
  {
    id: 'islands',
    label: 'Islands',
    description: 'Floating panels, the model and context in the composer',
    sidebar: true,
  },
  {
    id: 'mission',
    label: 'Mission',
    description: 'The run as a board of phases beside the conversation',
    sidebar: false,
  },
  {
    id: 'focus',
    label: 'Focus',
    description: 'No chrome: the page, with the rest floating on glass',
    sidebar: false,
  },
  { id: 'relay', label: 'Relay', description: 'A vertical run timeline hands off to the conversation and tools', sidebar: false },
  { id: 'workshop', label: 'Workshop', description: 'Files, diffs and tools lead; conversation stays at the side', sidebar: false },
  { id: 'chronicle', label: 'Chronicle', description: 'The conversation becomes a wide operational record', sidebar: false },
  { id: 'pulse', label: 'Pulse', description: 'The active phase decides which surface gets the most space', sidebar: false },
  { id: 'map', label: 'Map', description: 'A spatial task board with conversation and tools around it', sidebar: false },
  { id: 'deck', label: 'Deck', description: 'One focused surface at a time: task, dialogue or tools', sidebar: false },
];

/** Where the choice is kept; `index.html` reads the same key before first paint. */
export const VARIANT_KEY = 'pwr:variant';

export function readVariant(storage: Pick<Storage, 'getItem'> | undefined, search = ''): Variant {
  // `?variant=paper` wins, so a variant can be opened in a plain browser.
  const asked = new URLSearchParams(search).get('variant');
  if (isVariant(asked)) return asked;
  try {
    const saved = storage?.getItem(VARIANT_KEY);
    return isVariant(saved) ? saved : 'studio';
  } catch {
    return 'studio';
  }
}

function isVariant(value: string | null | undefined): value is Variant {
  return VARIANTS.some((variant) => variant.id === value);
}

export function variantInfo(id: Variant): VariantInfo {
  return VARIANTS.find((variant) => variant.id === id) ?? VARIANTS[0];
}

/** The chosen shell, applied to `<html data-variant>` and remembered. */
@Injectable({ providedIn: 'root' })
export class VariantService {
  private readonly storage = typeof localStorage !== 'undefined' ? localStorage : undefined;

  readonly variant = signal<Variant>(
    readVariant(this.storage, typeof location !== 'undefined' ? location.search : ''),
  );

  constructor() {
    effect(() => {
      const variant = this.variant();
      if (typeof document !== 'undefined') document.documentElement.dataset['variant'] = variant;
      try {
        if (variant === 'studio') this.storage?.removeItem(VARIANT_KEY);
        else this.storage?.setItem(VARIANT_KEY, variant);
      } catch {
        /* storage unavailable: the choice lasts for this run */
      }
    });
  }

  set(variant: Variant): void {
    this.variant.set(variant);
  }

  info(): VariantInfo {
    return variantInfo(this.variant());
  }
}
