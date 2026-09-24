// How numbers the core reports are written in the interface. Presentation
// only: every value here was computed by the core.

/** Binary gigabytes, as the OS reports memory and disk: "12.4 GB". */
export function bytes(value: number | null | undefined): string {
  if (value === null || value === undefined || !isFinite(value)) return 'unknown';
  const gib = value / 1024 ** 3;
  if (gib >= 10) return `${Math.round(gib)} GB`;
  if (gib >= 1) return `${gib.toFixed(1)} GB`;
  const mib = value / 1024 ** 2;
  return mib >= 1 ? `${Math.round(mib)} MB` : `${Math.max(0, Math.round(value / 1024))} KB`;
}

/** Tokens, compact: 54k, 1.2M. */
export function tokens(value: number | null | undefined): string {
  if (value === null || value === undefined || !isFinite(value)) return '—';
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1000) return `${Math.round(value / 1000)}k`;
  return String(Math.round(value));
}

/** A share of a whole, as a whole percent clamped to 0–100; 0 for no whole. */
export function percent(part: number, whole: number): number {
  if (!whole || whole <= 0 || !isFinite(part)) return 0;
  return Math.max(0, Math.min(100, Math.round((part / whole) * 100)));
}

/** Parameters: 4.0B, 350M. */
export function parameters(value: number | null | undefined): string {
  if (!value) return 'unknown';
  if (value >= 1e9) return `${(value / 1e9).toFixed(value >= 1e10 ? 0 : 1)}B`;
  return `${Math.round(value / 1e6)}M`;
}

export type FitLevel =
  'recommended' | 'should_fit' | 'tight_fit' | 'not_recommended' | 'incompatible' | 'unknown';

/** The colour a fit level is drawn in. */
export function fitTone(level: FitLevel | string): 'ok' | 'fine' | 'warn' | 'bad' | 'muted' {
  switch (level) {
    case 'recommended':
      return 'ok';
    case 'should_fit':
      return 'fine';
    case 'tight_fit':
      return 'warn';
    case 'not_recommended':
    case 'incompatible':
      return 'bad';
    default:
      return 'muted';
  }
}

/** How full a window is, for the indicator's colour. */
export function fullness(percentUsed: number, thresholdPercent = 75): 'low' | 'mid' | 'high' {
  if (percentUsed >= thresholdPercent) return 'high';
  if (percentUsed >= Math.max(40, thresholdPercent - 20)) return 'mid';
  return 'low';
}

export function when(value: string | null | undefined): string {
  if (!value) return '';
  const date = new Date(value);
  if (isNaN(date.getTime())) return '';
  const seconds = Math.round((Date.now() - date.getTime()) / 1000);
  if (seconds < 60) return 'just now';
  if (seconds < 3600) return `${Math.round(seconds / 60)} min ago`;
  return date.toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' });
}
