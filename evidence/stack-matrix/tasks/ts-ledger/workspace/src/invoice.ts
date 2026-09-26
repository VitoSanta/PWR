import { percentOf } from './money.ts';

export interface Line {
  description: string;
  quantity: number;
  unitCents: number;
  /** Percentage discount on this line, 0-100. */
  discountPercent?: number;
  /** VAT rate in percent, e.g. 22. */
  vatPercent: number;
}

export interface Invoice {
  lines: Line[];
  /** Percentage discount on the whole order, 0-100. */
  orderDiscountPercent?: number;
}

export interface Totals {
  net: number;
  discount: number;
  vat: number;
  gross: number;
  vatByRate: Record<string, number>;
}

export function lineNet(line: Line): number {
  return line.quantity * line.unitCents;
}

export function lineDiscounted(line: Line): number {
  const net = lineNet(line);
  return net - percentOf(net, line.discountPercent ?? 0);
}

export function totals(invoice: Invoice): Totals {
  const net = invoice.lines.reduce((sum, line) => sum + lineNet(line), 0);
  const afterLines = invoice.lines.reduce((sum, line) => sum + lineDiscounted(line), 0);
  const orderPercent = invoice.orderDiscountPercent ?? 0;

  // VAT per rate on each line's discounted amount.
  const baseByRate: Record<string, number> = {};
  for (const line of invoice.lines) {
    const key = String(line.vatPercent);
    baseByRate[key] = (baseByRate[key] ?? 0) + lineDiscounted(line);
  }
  const vatByRate: Record<string, number> = {};
  for (const [rate, base] of Object.entries(baseByRate)) {
    vatByRate[rate] = percentOf(base, Number(rate));
  }
  const vat = Object.values(vatByRate).reduce((a, b) => a + b, 0);

  const orderDiscount = percentOf(afterLines, orderPercent);
  const discount = net - afterLines + orderDiscount;
  return { net, discount, vat, gross: net - discount + vat, vatByRate };
}

/** Lines ordered by description, for printing. */
export function sortedLines(invoice: Invoice): Line[] {
  return invoice.lines.sort((a, b) => a.description.localeCompare(b.description));
}
