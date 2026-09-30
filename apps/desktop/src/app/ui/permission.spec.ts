import { TestBed } from '@angular/core/testing';
import { AgentStore } from '../core/agent.store';
import { PERMISSION_MODES } from '../core/model';
import { Permission } from './permission';

describe('permission boundaries', () => {
  for (const [approval, warning] of [
    ['network_access', 'all destinations'],
    ['local_service', 'not restricted to one port'],
    ['container_engine', 'outside the sandbox'],
    ['outside_sandbox', 'your full rights'],
    ['acceptance_change', 'named acceptance artifact'],
  ]) {
    it(`states the actual scope of ${approval}`, () => {
      TestBed.configureTestingModule({});
      TestBed.inject(AgentStore).permission.set({ id: 1, title: 'Approve this action', approval, options: [{ optionId: 'reject_once', name: 'Refuse', kind: 'reject_once' }] });
      const fixture = TestBed.createComponent(Permission);
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain(warning);
    });
  }
  it('states that Full access has no sandbox', () => {
    expect(PERMISSION_MODES.find(item => item.mode === 'full')?.summary).toContain('No sandbox');
    expect(PERMISSION_MODES.find(item => item.mode === 'full')?.summary).toContain('your full rights');
  });
});
