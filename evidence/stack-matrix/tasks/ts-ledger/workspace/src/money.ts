// Money in integer cents.

/** Rounds a (possibly fractional) cent amount half away from zero. */
export function roundCents(value: number): number {
  return Math.round(value);
}

/** Cents from a decimal string such as "12.34" or "-0.5". */
export function parseMoney(text: string): number {
  const match = /^(-)?(\d+)(?:\.(\d{1,2}))?$/.exec(text.trim());
  if (!match) throw new Error(`not an amount: ${text}`);
  const [, sign, whole, fraction = ''] = match;
  const cents = Number(whole) * 100 + Number(fraction.padEnd(2, '0'));
  return sign ? -cents : cents;
}

/** "1234.50" for 123450 cents; always two decimals. */
export function formatMoney(cents: number): string {
  const sign = cents < 0 ? '-' : '';
  const abs = Math.abs(cents);
  return `${sign}${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, '0')}`;
}

/** Applies a percentage (e.g. 22 for 22%) to cents, rounding once. */
export function percentOf(cents: number, percent: number): number {
  return roundCents((cents * percent) / 100);
}
