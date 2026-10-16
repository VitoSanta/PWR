import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { Graph3d, graphColours } from './knowledge';

const drawing = vi.hoisted(() => ({ create: vi.fn(), data: vi.fn() }));
vi.mock('3d-force-graph', () => ({ default: class {
  constructor() {
    drawing.create();
    const graph: any = new Proxy({}, { get: (_, key) => {
      if (key === 'graphData') return (value?: unknown) => {
        if (!value) return { nodes: [] };
        drawing.data(value);
        return graph;
      };
      return () => graph;
    } });
    return graph;
  }
} }));
vi.mock('three-spritetext', () => ({ default: class {} }));

describe('Knowledge graph rendering lifetime', () => {
  beforeEach(() => {
    drawing.create.mockClear();
    drawing.data.mockClear();
    vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} });
  });
  afterEach(() => vi.unstubAllGlobals());

  const graph = () => {
    const fixture = TestBed.createComponent(Graph3d);
    fixture.componentRef.setInput('nodes', []);
    fixture.componentRef.setInput('edges', []);
    fixture.detectChanges();
    return fixture;
  };

  it('initializes once and renders only the latest data during lazy loading', async () => {
    const fixture = graph();
    const card = fixture.componentInstance as any;
    card.drawVersion = 2;
    const first = card.draw([], [], 1);
    const latest = card.draw([{ id: 'new', kind: 'file', label: 'new.ts' }], [], 2);
    await Promise.all([first, latest]);
    expect(drawing.create).toHaveBeenCalledOnce();
    expect(drawing.data).toHaveBeenCalledOnce();
    expect(drawing.data.mock.calls[0][0].nodes[0].id).toBe('new');
    fixture.destroy();
  });

  it('does not create a renderer after the card was closed while importing', async () => {
    const fixture = graph();
    const pending = (fixture.componentInstance as any).draw([], [], 0);
    fixture.destroy();
    await pending;
    expect(drawing.create).not.toHaveBeenCalled();
  });

  it('converts CSS colour functions to sRGB for the graph library', () => {
    vi.stubGlobal('getComputedStyle', () => ({ color: 'color(srgb 0.2 0.4 0.6)' }));
    const context = { clearRect: vi.fn(), fillRect: vi.fn(), fillStyle: '',
      getImageData: () => ({ data: [51, 102, 153, 255] }) };
    const getContext = vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(context as any);
    try {
      expect(graphColours()('--data-1')).toBe('rgba(51, 102, 153, 1)');
      expect(context.fillStyle).toBe('color(srgb 0.2 0.4 0.6)');
    } finally { getContext.mockRestore(); }
  });

  it('starts rendering after the canvas appears' , async () => {
    const fixture = graph();
    await vi.waitFor(() => expect(drawing.create).toHaveBeenCalledOnce());
    fixture.destroy();
  });
});
