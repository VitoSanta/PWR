// The colour themes Settings offers. The app's own colours are Google's
// through Material 3 (styles/material.scss, styles/tokens.css); beside them,
// Material's high-contrast overrides of the same palettes, light and dark.

export type Scheme = 'light' | 'dark';

export interface Swatch {
  sidebar: string;
  app: string;
  surface: string;
  accent: string;
  text: string;
  secondary: string;
}

export interface PaletteInfo {
  id: string;
  label: string;
  scheme: Scheme;
  description: string;
  /** What Settings draws for it, before it is applied. */
  swatch: Swatch;
}

export const PALETTES: PaletteInfo[] = [
  { id: 'contrast-dark', label: 'High contrast', scheme: 'dark', description: 'Material high contrast, for legibility first', swatch: { sidebar: '#000000', app: '#000000', surface: '#111318', accent: '#ecefff', text: '#ffffff', secondary: '#a6c3ff' } },
  { id: 'contrast-light', label: 'High contrast', scheme: 'light', description: 'Material high contrast, for legibility first', swatch: { sidebar: '#ffffff', app: '#ffffff', surface: '#ffffff', accent: '#002a60', text: '#000000', secondary: '#004697' } },
];

/** Google's colours, drawn for the default palette choice. */
export const LAYOUT_SWATCHES: Record<string, Record<Scheme, Swatch>> = {
  focus: {
    dark: { sidebar: '#1b1b1f', app: '#121316', surface: '#1f1f23', accent: '#adc7ff', text: '#e3e2e6', secondary: '#81c995' },
    light: { sidebar: '#f3f3f6', app: '#f9f9fc', surface: '#ffffff', accent: '#005bc0', text: '#1a1b1f', secondary: '#188038' },
  },
};
