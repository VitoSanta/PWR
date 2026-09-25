import {
  AfterViewInit,
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  OnDestroy,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { ActivityStore } from '../../core/activity';
import { ThemeService } from '../../core/theme';
import { AgentStore } from '../../core/agent.store';
import { CORE_TOO_OLD, PersonalStore } from '../../core/personal.store';
import { WorkbenchStore } from '../../core/workbench';
import { Icon } from '../kit/icon';
import { Select, SelectOption } from '../kit/select';
import { Tooltip } from '../kit/tooltip';
import { Markdown } from '../markdown';

export type NodeKind = 'project' | 'module' | 'file' | 'symbol' | 'package' | 'work' | 'decision';
export type EdgeKind = 'contains' | 'defines' | 'imports' | 'tests' | 'changed_in' | 'about';
export type Certainty = 'fact' | 'resolved' | 'named' | 'guess';

export interface GraphNode {
  id: string;
  kind: NodeKind;
  label: string;
  summary?: string | null;
  stale?: boolean | null;
  builtin?: boolean | null;
}

export interface GraphEdge {
  from: string;
  to: string;
  kind: EdgeKind;
  certainty: Certainty;
}

interface WikiView {
  graph: { nodes: GraphNode[]; edges: GraphEdge[] };
  overview: string;
  outline: string;
  nodes: number;
  edges: number;
  modules: { id: string; label: string; summary?: string; stale?: boolean; model?: string }[];
  work: { when: string; request: string; files: string[] }[];
}

/** How each kind of node is drawn: a design token, and a size. */
const KIND: Record<NodeKind, { token: string; label: string; size: number }> = {
  project: { token: '--accent-solid', label: 'Project', size: 10 },
  module: { token: '--data-1', label: 'Folder', size: 4 },
  file: { token: '--data-3', label: 'File', size: 2 },
  symbol: { token: '--data-6', label: 'Symbol', size: 0.6 },
  package: { token: '--data-4', label: 'Package', size: 1.6 },
  work: { token: '--data-5', label: 'Work', size: 2.4 },
  decision: { token: '--data-7', label: 'Decision', size: 2.4 },
};

const VERB: Record<EdgeKind, [string, string]> = {
  contains: ['contains', 'is in'],
  defines: ['defines', 'is defined in'],
  imports: ['imports', 'is imported by'],
  tests: ['tests', 'is tested by'],
  changed_in: ['was changed in', 'changed'],
  about: ['is about', 'is named by'],
};

const HOW: Record<Certainty, string> = {
  fact: '',
  resolved: 'resolved by path',
  named: 'by name',
  guess: 'a guess from naming',
};

/**
 * The project in three dimensions: every node a sphere sized by how connected
 * it is, every link a thin line. Drag to turn, scroll to zoom, click a node to
 * centre it. The rendering library loads only when this is first shown.
 */
@Component({
  selector: 'pa-graph3d',
  template: `<div #canvas class="graph3d-canvas"></div>`,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Graph3d implements AfterViewInit, OnDestroy {
  readonly nodes = input.required<GraphNode[]>();
  readonly edges = input.required<GraphEdge[]>();
  readonly selected = input<string | null>(null);
  readonly picked = output<string | null>();
  /** The canvas is on the page: drawing may start. */
  private readonly ready = signal(false);

  private readonly canvas = viewChild.required<ElementRef<HTMLElement>>('canvas');
  private graph: any;
  private sprite: any;
  private observer?: ResizeObserver;
  private degree = new Map<string, number>();
  private neighbours = new Map<string, Set<string>>();

  constructor() {
    const theme = inject(ThemeService);
    effect(() => {
      const nodes = this.nodes();
      const edges = this.edges();
      // Redrawn with the theme: its colours are read from the page's tokens,
      // after the theme has been applied to it.
      theme.theme();
      if (!this.ready()) return;
      untracked(() => requestAnimationFrame(() => void this.draw(nodes, edges)));
    });
    effect(() => {
      const selected = this.selected();
      untracked(() => this.focus(selected));
    });
  }

  ngAfterViewInit(): void {
    this.ready.set(true);
  }

  ngOnDestroy(): void {
    this.observer?.disconnect();
    this.graph?.pauseAnimation?.();
    this.graph?._destructor?.();
  }

  private async draw(nodes: GraphNode[], edges: GraphEdge[]): Promise<void> {
    const element = this.canvas().nativeElement;
    if (!this.graph) {
      const [{ default: ForceGraph3D }, { default: SpriteText }] = await Promise.all([
        import('3d-force-graph'),
        import('three-spritetext'),
      ]);
      this.sprite = SpriteText;
      this.graph = new (ForceGraph3D as any)(element, { controlType: 'orbit' })
        .showNavInfo(false)
        .nodeId('id')
        .linkSource('source')
        .linkTarget('target')
        .nodeOpacity(0.95)
        .nodeResolution(16)
        .linkWidth(0)
        .enableNodeDrag(false)
        .onNodeClick((node: any) => this.picked.emit(node.id))
        .onBackgroundClick(() => this.picked.emit(null))
        .onNodeHover((node: any) => (element.style.cursor = node ? 'pointer' : ''));
      this.observer = new ResizeObserver(() => {
        this.graph.width(element.clientWidth).height(element.clientHeight);
      });
      this.observer.observe(element);
      this.graph.width(element.clientWidth).height(element.clientHeight);
    }
    const style = getComputedStyle(document.documentElement);
    const token = (name: string) => style.getPropertyValue(name).trim() || '#888';
    const background = token('--surface-primary');
    const muted = token('--text-muted');
    const ids = new Set(nodes.map((node) => node.id));
    this.degree = new Map();
    this.neighbours = new Map();
    const links = edges
      .filter((edge) => ids.has(edge.from) && ids.has(edge.to))
      .map((edge) => {
        for (const [a, b] of [
          [edge.from, edge.to],
          [edge.to, edge.from],
        ]) {
          this.degree.set(a, (this.degree.get(a) ?? 0) + 1);
          if (!this.neighbours.has(a)) this.neighbours.set(a, new Set());
          this.neighbours.get(a)!.add(b);
        }
        return { source: edge.from, target: edge.to, certainty: edge.certainty };
      });
    // Labels on everything in a small graph; in a large one, on the nodes
    // that hold it together, and on whatever is selected.
    const labelled = nodes.length <= 150 ? 0 : 4;
    const data = {
      nodes: nodes.map((node) => ({
        id: node.id,
        kind: node.kind,
        label: node.label.replace(/\/$/, '').split('/').pop() || node.label,
        builtin: !!node.builtin,
        color: node.builtin ? muted : token(KIND[node.kind].token),
        val: KIND[node.kind].size + Math.min(12, (this.degree.get(node.id) ?? 0) * 0.6),
      })),
      links,
    };
    this.graph
      .backgroundColor(background)
      .nodeVal('val')
      .nodeColor((node: any) => this.colour(node))
      .linkColor((link: any) => alpha(muted, link.certainty === 'guess' || link.certainty === 'named' ? 0.3 : 0.6))
      .linkOpacity(0.45)
      .nodeThreeObjectExtend(true)
      .nodeThreeObject((node: any) => {
        if (labelled && (this.degree.get(node.id) ?? 0) < labelled && node.kind !== 'project') return null;
        const text = new this.sprite(node.label);
        text.color = token('--text-secondary');
        text.textHeight = node.kind === 'project' ? 7 : node.kind === 'module' ? 5 : 4;
        text.fontFace = style.getPropertyValue('--font').trim() || 'sans-serif';
        text.position.y = -(Math.cbrt(node.val) * 4 + 4);
        return text;
      })
      .graphData(data);
    this.focus(this.selected());
  }

  private colour(node: any): string {
    const selected = this.selected();
    if (!selected) return node.color;
    if (node.id === selected || this.neighbours.get(selected)?.has(node.id)) return node.color;
    return alpha(node.color, 0.18);
  }

  private focus(id: string | null): void {
    if (!this.graph) return;
    this.graph.nodeColor((node: any) => this.colour(node));
    if (!id) return;
    const node = this.graph.graphData().nodes.find((item: any) => item.id === id);
    if (!node || node.x === undefined) return;
    const distance = 90;
    const ratio = 1 + distance / Math.hypot(node.x || 1, node.y || 1, node.z || 1);
    this.graph.cameraPosition({ x: node.x * ratio, y: node.y * ratio, z: node.z * ratio }, node, 900);
  }
}

type View = 'graph' | 'modules' | 'work' | 'overview';

/** Knowledge: the project as a graph, its modules, the work done, its overview. */
@Component({
  selector: 'pa-knowledge-card',
  imports: [Graph3d, Icon, Tooltip, Markdown, Select],
  template: `
    @if (agent.chatMode() && projectOptions().length) {
      <div class="card-toolbar">
        <pa-select
          size="sm"
          ariaLabel="Project"
          [options]="projectOptions()"
          [value]="source()"
          (valueChange)="choose($any($event))"
        />
        <span class="t-meta truncate">Read only</span>
      </div>
    }
    <div class="card-toolbar">
      <div class="segmented" role="radiogroup" aria-label="Knowledge view">
        @for (item of views; track item.id) {
          <button role="radio" [attr.aria-checked]="view() === item.id" (click)="view.set(item.id)">{{ item.label }}</button>
        }
      </div>
      <span class="spacer"></span>
      <button class="icon-btn icon-btn-sm" (click)="load()" [disabled]="loading()" aria-label="Refresh" paTooltip="Refresh">
        <pa-icon name="refresh" [size]="14" />
      </button>
    </div>
    @if (error()) {
      <p class="banner banner-danger card-banner" role="alert"><pa-icon name="alert" [size]="16" />{{ error() }}</p>
    }
    @if (!source()) {
      <p class="card-empty card-pad">
        {{ agent.chatMode()
          ? 'No projects yet. A workspace appears here after PWR’s first reply in it, and any conversation can then look at what PWR knows about it.'
          : 'Open a workspace to see what PWR knows about it.' }}
      </p>
    } @else if (wiki(); as data) {
      @switch (view()) {
        @case ('graph') {
          <div class="knowledge-view" animate.enter="anim-fade-in">
          <div class="graph-controls">
            <form class="graph-search" (submit)="$event.preventDefault(); find(search.value)">
              <input #search class="input input-sm" placeholder="Find a file, folder or package" aria-label="Find in the graph" />
            </form>
            <label class="graph-toggle"><input type="checkbox" [checked]="symbols()" (change)="toggleSymbols()" /> Symbols</label>
            <label class="graph-toggle"><input type="checkbox" [checked]="builtins()" (change)="builtins.set(!builtins())" /> Standard library</label>
          </div>
          <div class="graph-stage">
            <pa-graph3d [nodes]="shownNodes()" [edges]="data.graph.edges" [selected]="selected()" (picked)="selected.set($event)" />
            <ul class="graph-legend" aria-label="Legend">
              @for (kind of legend(); track kind.kind) {
                <li><span class="graph-dot" [style.background]="'var(' + kind.token + ')'"></span>{{ kind.label }}</li>
              }
            </ul>
            @if (detail(); as node) {
              <section class="graph-detail" aria-label="Selected node" animate.enter="anim-rise-in" animate.leave="anim-fade-out">
                <header>
                  <span class="badge">{{ kindLabel(node.kind) }}</span>
                  <strong class="truncate" [attr.title]="node.label">{{ node.label }}</strong>
                  <span class="spacer"></span>
                  @if (node.kind === 'file' && !agent.chatMode()) {
                    <button class="icon-btn icon-btn-sm" (click)="work.openFile(node.label)" aria-label="Open in Files" paTooltip="Open in Files"><pa-icon name="file" [size]="14" /></button>
                  }
                  <button class="icon-btn icon-btn-sm" (click)="selected.set(null)" aria-label="Close"><pa-icon name="x" [size]="14" /></button>
                </header>
                @if (node.summary) {
                  <p class="graph-summary">{{ node.summary }} <span class="t-meta">· {{ node.stale ? 'written before its files changed' : 'written by the model, unverified' }}</span></p>
                }
                @for (group of links(); track group.label) {
                  <div class="graph-links">
                    <span class="t-meta">{{ group.label }}</span>
                    <div class="graph-chips">
                      @for (other of group.nodes; track other.id) {
                        <button class="chip" (click)="selected.set(other.id)">{{ other.label }}</button>
                      }
                    </div>
                  </div>
                }
              </section>
            }
          </div>
          </div>
        }
        @case ('modules') {
          <div class="knowledge-view" animate.enter="anim-fade-in">
          <div class="card-toolbar">
            <span class="t-meta">{{ summaryStatus() }}</span>
            <span class="spacer"></span>
            @if (!agent.chatMode()) {
              <button class="btn btn-sm" (click)="summarise()" [disabled]="writing()">
                <pa-icon name="sparkles" [size]="14" /> Write summaries
              </button>
            }
          </div>
          <div class="card-scroll card-pad knowledge-list">
            @for (module of data.modules; track module.id) {
              <article class="knowledge-item">
                <button class="knowledge-name mono" (click)="showInGraph(module.id)">{{ module.label }}</button>
                @if (module.summary) {
                  <p>{{ module.summary }}</p>
                  <span class="t-meta">{{ module.stale ? 'Written before its files changed' : 'Written by ' + (module.model || 'the model') + ', unverified' }}</span>
                } @else {
                  <p class="t-meta">No summary yet.</p>
                }
              </article>
            } @empty {
              <p class="card-empty">No source folders found.</p>
            }
          </div>
          </div>
        }
        @case ('work') {
          <ol class="card-scroll card-pad work-list" animate.enter="anim-fade-in">
            @for (entry of data.work; track $index) {
              <li class="work-item">
                <span class="t-meta num">{{ entry.when }}</span>
                <span>{{ entry.request }}</span>
                @if (entry.files.length) {
                  <div class="graph-chips">
                    @for (file of entry.files; track file) {
                      @if (agent.chatMode()) {
                        <span class="chip mono">{{ file }}</span>
                      } @else {
                        <button class="chip mono" (click)="work.openFile(file)">{{ file }}</button>
                      }
                    }
                  </div>
                }
              </li>
            } @empty {
              <li class="card-empty">Nothing recorded yet: each turn that changes or finishes something is logged here.</li>
            }
          </ol>
        }
        @case ('overview') {
          <div class="card-scroll card-pad" animate.enter="anim-fade-in">
            <pa-markdown [text]="data.overview" [copyable]="false" />
          </div>
        }
      }
    } @else if (loading()) {
      <p class="card-empty card-pad"><span class="spinner spinner-sm" aria-hidden="true"></span> Building the knowledge graph…</p>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class KnowledgeCard {
  protected readonly agent = inject(AgentStore);
  protected readonly work = inject(WorkbenchStore);
  private readonly personal = inject(PersonalStore);
  /** Whose wiki `wiki` holds, or is being loaded for. */
  private shown: string | null = null;
  /** In chat mode, the project chosen to look at; `null` is the most recent. */
  private readonly chosen = signal<string | null>(null);

  protected readonly projectOptions = computed<SelectOption[]>(() =>
    this.personal.projects().map((project) => ({ value: project.path, label: project.name, hint: project.path })),
  );

  /**
   * Whose knowledge the card shows: the workspace; in chat mode, which has
   * none, a project PWR has worked in, read only.
   */
  protected readonly source = computed(() => {
    if (!this.agent.chatMode()) return this.agent.workspace() || null;
    const known = this.personal.projects().map((project) => project.path);
    const chosen = this.chosen();
    return chosen && known.includes(chosen) ? chosen : (known[0] ?? null);
  });
  private readonly activity = inject(ActivityStore);
  protected readonly wiki = signal<WikiView | null>(null);
  protected readonly loading = signal(false);
  protected readonly error = signal('');
  protected readonly view = signal<View>('graph');
  protected readonly selected = signal<string | null>(null);
  protected readonly symbols = signal(false);
  protected readonly builtins = signal(false);
  protected readonly views: { id: View; label: string }[] = [
    { id: 'graph', label: 'Graph' },
    { id: 'modules', label: 'Modules' },
    { id: 'work', label: 'Work' },
    { id: 'overview', label: 'Overview' },
  ];

  protected readonly shownNodes = computed(() => {
    const nodes = this.wiki()?.graph.nodes ?? [];
    return this.builtins() ? nodes : nodes.filter((node) => !node.builtin);
  });

  protected readonly legend = computed(() => {
    const present = new Set(this.shownNodes().map((node) => node.kind));
    return (Object.keys(KIND) as NodeKind[])
      .filter((kind) => present.has(kind))
      .map((kind) => ({ kind, ...KIND[kind] }));
  });

  protected readonly detail = computed(() => {
    const id = this.selected();
    return id ? (this.wiki()?.graph.nodes.find((node) => node.id === id) ?? null) : null;
  });

  /** The selected node's links, grouped by what they say. */
  protected readonly links = computed(() => {
    const id = this.selected();
    const data = this.wiki();
    if (!id || !data) return [];
    const byId = new Map(data.graph.nodes.map((node) => [node.id, node]));
    const groups = new Map<string, GraphNode[]>();
    for (const edge of data.graph.edges) {
      const outgoing = edge.from === id;
      if (!outgoing && edge.to !== id) continue;
      const other = byId.get(outgoing ? edge.to : edge.from);
      if (!other) continue;
      const how = HOW[edge.certainty];
      const label = VERB[edge.kind][outgoing ? 0 : 1] + (how ? ` (${how})` : '');
      groups.set(label, [...(groups.get(label) ?? []), other]);
    }
    return [...groups].map(([label, nodes]) => ({ label, nodes: nodes.slice(0, 40) }));
  });

  protected readonly writing = computed(() => {
    const job = this.activity.summarising();
    return !!job && job.state !== 'finished' && job.cwd === this.agent.workspace();
  });

  protected readonly summaryStatus = computed(() => {
    const job = this.activity.summarising();
    if (this.writing() && job) return `Writing ${(job.module ?? '').replace(/^dir:/, '') || 'summaries'}…`;
    const modules = this.wiki()?.modules ?? [];
    const written = modules.filter((module) => module.summary).length;
    return `${written} of ${modules.length} summarised, by the model, unverified`;
  });

  constructor() {
    const stop = this.agent.on('_pwr/wiki_updated', (params) => {
      if (params.cwd === this.source()) void this.load();
    });
    inject(DestroyRef).onDestroy(stop);
    if (this.agent.chatMode()) void this.personal.load();
    // A new workspace or project, or a turn that just ended: the wiki was
    // rebuilt.
    effect(() => {
      const source = this.source();
      const turn = this.agent.turnActive();
      untracked(() => {
        // Another project's graph is not left showing while this one loads.
        if (source !== this.shown) {
          this.shown = source;
          this.wiki.set(null);
          this.selected.set(null);
        }
        if (!turn) void this.load();
      });
    });
  }

  protected kindLabel(kind: NodeKind): string {
    return KIND[kind].label;
  }

  protected find(query: string): void {
    const wanted = query.trim().toLowerCase();
    if (!wanted) return;
    const nodes = this.shownNodes();
    const hit =
      nodes.find((node) => node.label.toLowerCase() === wanted) ??
      nodes.find((node) => node.label.toLowerCase().split('/').pop() === wanted) ??
      nodes.find((node) => node.label.toLowerCase().includes(wanted));
    this.selected.set(hit?.id ?? null);
    if (!hit) this.error.set(`Nothing in the graph matches “${query.trim()}”.`);
    else this.error.set('');
  }

  protected showInGraph(id: string): void {
    this.view.set('graph');
    this.selected.set(id);
  }

  protected toggleSymbols(): void {
    this.symbols.set(!this.symbols());
    void this.load();
  }

  protected async summarise(): Promise<void> {
    try {
      await this.activity.summariseNow();
    } catch (error) {
      this.error.set(describe(error));
    }
  }

  protected choose(path: string): void {
    this.chosen.set(path);
  }

  async load(): Promise<void> {
    const cwd = this.source();
    if (!cwd) {
      this.wiki.set(null);
      return;
    }
    const readOnly = this.agent.chatMode();
    this.loading.set(true);
    try {
      const wiki = await this.agent.call('_pwr/wiki', { cwd, includeSymbols: this.symbols(), readOnly });
      // A reply for a project since left behind is not shown.
      if (cwd !== this.source()) return;
      this.wiki.set(wiki);
      this.error.set('');
    } catch (error) {
      if (cwd !== this.source()) return;
      this.wiki.set(null);
      this.error.set(describe(error));
    } finally {
      this.loading.set(false);
    }
  }
}

/** A colour at an opacity: hex tokens get an alpha, anything else is kept. */
function alpha(color: string, opacity: number): string {
  const hex = color.trim().replace('#', '');
  if (!/^[0-9a-f]{3}([0-9a-f]{3})?$/i.test(hex)) return color;
  const full = hex.length === 3 ? [...hex].map((c) => c + c).join('') : hex;
  const [r, g, b] = [0, 2, 4].map((at) => parseInt(full.slice(at, at + 2), 16));
  return `rgba(${r}, ${g}, ${b}, ${opacity})`;
}

function describe(error: unknown): string {
  const text = String(error).replace(/^Error: /, '');
  return /method not found/i.test(text) ? CORE_TOO_OLD : text;
}
