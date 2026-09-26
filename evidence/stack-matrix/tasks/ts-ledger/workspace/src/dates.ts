// Dates as ISO strings (YYYY-MM-DD), computed in UTC so no timezone shifts them.

function parse(iso: string): Date {
  return new Date(`${iso}T00:00:00Z`);
}

function format(date: Date): string {
  return date.toISOString().slice(0, 10);
}

function isWeekend(date: Date): boolean {
  const day = date.getUTCDay();
  return day === 0 || day === 6;
}

/** The date `days` business days after `issued`. The issue date never counts. */
export function dueDate(issued: string, days: number): string {
  const date = parse(issued);
  let left = days;
  while (left > 0) {
    if (!isWeekend(date)) left--;
    date.setUTCDate(date.getUTCDate() + 1);
  }
  while (isWeekend(date)) date.setUTCDate(date.getUTCDate() + 1);
  return format(date);
}
