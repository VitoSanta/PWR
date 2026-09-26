import { TestBed, ComponentFixture } from '@angular/core/testing';
import { TodoList } from './todo-list';

describe('TodoList', () => {
  let fixture: ComponentFixture<TodoList>;
  let root: HTMLElement;

  beforeEach(async () => {
    localStorage.clear();
    await TestBed.configureTestingModule({ imports: [TodoList] }).compileComponents();
    fixture = TestBed.createComponent(TodoList);
    root = fixture.nativeElement as HTMLElement;
    await fixture.whenStable();
  });

  const input = () => root.querySelector<HTMLInputElement>('input[aria-label="New todo"], input#new-todo, input[placeholder="New todo"]') ?? labelled('New todo');
  function labelled(text: string): HTMLInputElement {
    const label = [...root.querySelectorAll('label')].find((l) => l.textContent?.trim() === text);
    if (!label) throw new Error(`no label ${text}`);
    return (label.control ?? label.querySelector('input')) as HTMLInputElement;
  }
  const button = (name: string) =>
    [...root.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') ?? b.textContent?.trim()) === name) as HTMLButtonElement;
  const items = () => [...root.querySelectorAll('li')];
  const status = () => root.querySelector('[role="status"]')!.textContent!.trim();

  async function add(title: string) {
    const field = input();
    field.value = title;
    field.dispatchEvent(new Event('input'));
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    field.dispatchEvent(new KeyboardEvent('keyup', { key: 'Enter' }));
    await fixture.whenStable();
  }

  it('adds on Enter and clears the input', async () => {
    await add('milk');
    await add('bread');
    expect(items().map((li) => li.textContent?.includes('milk'))).toEqual([true, false]);
    expect(items()).toHaveLength(2);
    expect(input().value).toBe('');
    expect(status()).toBe('2 items left');
  });

  it('toggles through the checkbox and deletes through its button', async () => {
    await add('milk');
    await add('bread');
    const box = labelled('milk');
    box.click();
    await fixture.whenStable();
    expect(labelled('milk').checked).toBe(true);
    expect(status()).toBe('1 item left');
    button('Delete bread').click();
    await fixture.whenStable();
    expect(items()).toHaveLength(1);
  });

  it('filters with pressed buttons and clears done', async () => {
    await add('a');
    await add('b');
    expect(button('Clear done').disabled).toBe(true);
    labelled('a').click();
    await fixture.whenStable();
    button('Done').click();
    await fixture.whenStable();
    expect(items()).toHaveLength(1);
    expect(button('Done').getAttribute('aria-pressed')).toBe('true');
    expect(button('All').getAttribute('aria-pressed')).toBe('false');
    button('All').click();
    await fixture.whenStable();
    expect(button('Clear done').disabled).toBe(false);
    button('Clear done').click();
    await fixture.whenStable();
    expect(items()).toHaveLength(1);
    expect(status()).toBe('1 item left');
  });
});
