import { describe, expect, it } from 'vitest';
import { clockTime, conversationItems } from './conversation';

describe('clockTime', () => {
  it('reads an instant as the local time of day, on 24 hours and to the second', () => {
    expect(clockTime(new Date(2026, 9, 7, 14, 7, 5).getTime())).toBe('14:07:05');
    expect(clockTime(new Date(2026, 9, 7, 0, 0, 0).getTime())).toBe('00:00:00');
    expect(clockTime(new Date(2026, 9, 7, 23, 59, 59).getTime())).toBe('23:59:59');
  });
});


describe('reopened conversation timing', () => {
  it('measures the turn from the request when all historical replies share its end time', () => {
    const start = new Date(2026,9,15,20,49,26).getTime();
    const end = new Date(2026,9,15,21,3,57).getTime();
    const entry = (key: string, kind: string, at: number) => ({key, kind, at, text: '', title: '', status: 'done'} as any);
    const rows = conversationItems([entry('u','user',start),entry('r','reasoning',end),entry('a','reply',end)],false);
    const turn = rows.find(r => r.type === 'turn');
    expect(turn?.type).toBe('turn');
    if (turn?.type === 'turn') expect(turn.endedAt - turn.startedAt).toBe(871000);
  });
  it('keeps the next request separate from the preceding turn', () => {
    const entry = (key: string, kind: string, at: number) => ({key, kind, at, text: '', title: '', status: 'done'} as any);
    const turns = conversationItems([entry('u','user',1000),entry('r','reply',5000),entry('v','user',9000),entry('s','reply',12000)],false).filter(r=>r.type==='turn');
    expect(turns.map(t=>t.type==='turn' && t.endedAt-t.startedAt)).toEqual([4000,3000]);
  });
});
