import { formatMoney } from './money.ts';
import type { Invoice } from './invoice.ts';
import { lineDiscounted } from './invoice.ts';

function field(value: string): string {
  return /[",]/.test(value) ? `"${value.replace(/"/g, '""')}"` : value;
}

/** One row per line: description, quantity, unit, discounted net, VAT rate. */
export function toCsv(invoice: Invoice): string {
  const rows = [['description', 'quantity', 'unit', 'net', 'vat']];
  for (const line of invoice.lines) {
    rows.push([
      line.description,
      String(line.quantity),
      formatMoney(line.unitCents),
      formatMoney(lineDiscounted(line)),
      `${line.vatPercent}%`,
    ]);
  }
  return rows.map((row) => row.map(field).join(',')).join('\n') + '\n';
}
