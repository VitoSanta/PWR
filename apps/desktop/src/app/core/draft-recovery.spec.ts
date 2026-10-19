import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { DraftRecovery } from './draft-recovery';
import { bridge } from './bridge';

describe('Recovery record failures', () => {
  it('preserves an unreadable record while permitting clean quit or explicit discard', async () => {
    const store = TestBed.inject(DraftRecovery);
    Object.defineProperty(store, 'enabled', {value: true});
    vi.spyOn(bridge, 'draftsRead').mockRejectedValue(new Error('corrupt JSON'));
    const write = vi.spyOn(bridge, 'draftsWrite').mockResolvedValue(undefined);
    expect(await store.read('/project')).toBe(null);
    expect(store.error()).toContain('retained');
    await expect(store.write('/project', null)).resolves.toBeUndefined();
    expect(write).not.toHaveBeenCalled();
    await expect(store.write('/project', {version: 1, workspace: '/project', active: 'a', documents: []})).rejects.toThrow('needs attention');
    expect(write).not.toHaveBeenCalled();
  });
  it('serializes writes and continues after a failed write', async () => {
    const store = TestBed.inject(DraftRecovery); Object.defineProperty(store, 'enabled', {value: true});
    let reject!: (reason: unknown) => void;
    const write = vi.spyOn(bridge, 'draftsWrite').mockImplementationOnce(() => new Promise((_, no) => {reject = no;})).mockResolvedValue(undefined);
    const first = store.write('/a', null); const rejected = first.catch(() => {});
    const second = store.write('/b', null); await Promise.resolve();
    expect(write).toHaveBeenCalledTimes(1); reject(new Error('disk full')); await rejected; await second;
    expect(write).toHaveBeenLastCalledWith('/b', null); expect(store.error()).toContain('disk full');
  });
});
