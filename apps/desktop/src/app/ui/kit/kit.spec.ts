import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Dialog } from './dialog';
import { Popover } from './popover';
import { Select, SelectOption } from './select';

@Component({
  imports: [Select],
  template: `<pa-select ariaLabel="Effort" [options]="options" [(value)]="value" />`,
})
class SelectHost {
  readonly options: SelectOption<string>[] = [
    { value: 'low', label: 'Low' },
    { value: 'medium', label: 'Medium' },
    { value: 'high', label: 'High' },
  ];
  readonly value = signal<unknown>('medium');
}

@Component({
  imports: [Select],
  template: `
    <pa-select ariaLabel="Size" [options]="options" />
    <pa-select ariaLabel="Context" [options]="options" />
  `,
})
class TwoSelects {
  readonly options: SelectOption<string>[] = [{ value: 'a', label: 'A' }];
}

@Component({
  imports: [Dialog],
  template: `
    <button id="opener">Open</button>
    @if (open()) {
      <pa-dialog labelledBy="t" (closed)="open.set(false)">
        <h2 id="t">Title</h2>
        <button id="first">First</button>
        <button id="last">Last</button>
      </pa-dialog>
    }
  `,
})
class DialogHost {
  readonly open = signal(false);
}

@Component({
  imports: [Dialog, Popover],
  template: `
    <pa-dialog labelledBy="t" (closed)="open.set(false)">
      <h2 id="t">Title</h2>
      <button #anchor id="anchor">Filters</button>
      @if (popover()) {
        <pa-popover [anchor]="anchor" ariaLabel="Filter" (closed)="popover.set(false)"><button>Inside</button></pa-popover>
      }
    </pa-dialog>
  `,
})
class PopoverInDialog {
  readonly open = signal(true);
  readonly popover = signal(true);
}

const KEY_CODES: Record<string, number> = { Enter: 13, Escape: 27, Tab: 9, ArrowDown: 40, ArrowUp: 38, ' ': 32 };

/** A key press, with the legacy keyCode Material's keyboard handling reads. */
const press = (target: Element, key: string) => {
  const keyCode = KEY_CODES[key] ?? key.toUpperCase().charCodeAt(0);
  const event = new KeyboardEvent('keydown', { key, keyCode, bubbles: true, cancelable: true } as KeyboardEventInit);
  target.dispatchEvent(event);
  return event;
};

const pause = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const settle = async (fixture: { whenStable(): Promise<unknown>; detectChanges(): void }) => {
  fixture.detectChanges();
  await fixture.whenStable();
  await new Promise((resolve) => setTimeout(resolve));
  fixture.detectChanges();
};

describe('Select', () => {
  // Material's list lives in the CDK overlay, outside the fixture.
  const optionsShown = () => document.querySelectorAll<HTMLElement>('mat-option');

  it('is a labelled combobox that opens, moves and chooses with the keyboard', async () => {
    const fixture = TestBed.createComponent(SelectHost);
    await settle(fixture);
    const trigger: HTMLElement = fixture.nativeElement.querySelector('[role=combobox]');
    expect(trigger.getAttribute('aria-label')).toBe('Effort');
    expect(trigger.textContent).toContain('Medium');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');

    press(trigger, 'Enter');
    await settle(fixture);
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
    const options = optionsShown();
    expect(options.length).toBe(3);
    expect(trigger.getAttribute('aria-activedescendant')).toBe(options[1].id);
    expect(options[1].getAttribute('aria-selected')).toBe('true');

    press(trigger, 'ArrowDown');
    await settle(fixture);
    expect(trigger.getAttribute('aria-activedescendant')).toBe(options[2].id);

    press(trigger, 'Enter');
    await settle(fixture);
    expect(fixture.componentInstance.value()).toBe('high');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    fixture.destroy();
  });

  it('keeps Escape to itself while open, so the dialog around it stays', async () => {
    const fixture = TestBed.createComponent(SelectHost);
    await settle(fixture);
    const trigger: HTMLElement = fixture.nativeElement.querySelector('[role=combobox]');
    press(trigger, 'Enter');
    await settle(fixture);
    const escape = press(trigger, 'Escape');
    await settle(fixture);
    expect(escape.defaultPrevented).toBe(true);
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(fixture.componentInstance.value()).toBe('medium');
    fixture.destroy();
  });

  it('labels each of several selects on its own', async () => {
    const fixture = TestBed.createComponent(TwoSelects);
    await settle(fixture);
    const triggers = [...fixture.nativeElement.querySelectorAll('[role=combobox]')] as HTMLElement[];
    expect(triggers.map((trigger) => trigger.getAttribute('aria-label'))).toEqual(['Size', 'Context']);
    fixture.destroy();
  });

  it('selects by typing the start of an option', async () => {
    const fixture = TestBed.createComponent(SelectHost);
    await settle(fixture);
    const trigger: HTMLElement = fixture.nativeElement.querySelector('[role=combobox]');
    press(trigger, 'l');
    // Material's type-ahead waits for the typing to pause.
    await pause(300);
    await settle(fixture);
    expect(fixture.componentInstance.value()).toBe('low');
    fixture.destroy();
  });
});

describe('Dialog', () => {
  it('is modal and labelled, keeps Tab inside, closes on Escape and gives focus back', async () => {
    const fixture = TestBed.createComponent(DialogHost);
    document.body.append(fixture.nativeElement);
    await settle(fixture);
    const opener: HTMLButtonElement = fixture.nativeElement.querySelector('#opener');
    opener.focus();
    fixture.componentInstance.open.set(true);
    await settle(fixture);

    const panel: HTMLElement = fixture.nativeElement.querySelector('.dialog');
    expect(panel.getAttribute('role')).toBe('dialog');
    expect(panel.getAttribute('aria-modal')).toBe('true');
    expect(panel.getAttribute('aria-labelledby')).toBe('t');

    const first: HTMLButtonElement = fixture.nativeElement.querySelector('#first');
    const last: HTMLButtonElement = fixture.nativeElement.querySelector('#last');
    last.focus();
    const tab = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true });
    last.dispatchEvent(tab);
    expect(tab.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(first);

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }));
    await settle(fixture);
    expect(fixture.componentInstance.open()).toBe(false);
    expect(document.activeElement).toBe(opener);
    fixture.nativeElement.remove();
  });

  it('lets Escape close a popover inside it first, and only then the dialog', async () => {
    const fixture = TestBed.createComponent(PopoverInDialog);
    document.body.append(fixture.nativeElement);
    await settle(fixture);
    const escape = () => {
      const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
      document.dispatchEvent(event);
      window.dispatchEvent(event);
    };
    escape();
    await settle(fixture);
    expect(fixture.componentInstance.popover()).toBe(false);
    expect(fixture.componentInstance.open()).toBe(true);
    escape();
    await settle(fixture);
    expect(fixture.componentInstance.open()).toBe(false);
    fixture.nativeElement.remove();
  });
});
