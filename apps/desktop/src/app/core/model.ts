// What the interface shows, derived from the protocol. Nothing here decides
// anything about the work: the core does.

/**
 * One stream of what a run did, in order. `retry`, `recovery`, `generation`,
 * `note` and `stop` come from `_pwr/turn_event` and the turn's reply; the
 * conversation's phases and their steps read this same list.
 */
export type EntryKind =
  | 'user'
  | 'thought'
  | 'reply'
  | 'tool'
  | 'notice'
  | 'retry'
  | 'recovery'
  | 'generation'
  | 'note'
  | 'stop';
export type EntryStatus = 'live' | 'pending' | 'running' | 'done' | 'failed' | 'sent' | 'info' | 'error';

export interface FileDiff {
  path: string;
  oldText: string;
  newText: string;
  /** The file did not exist when this conversation first changed it. */
  created?: boolean;
}

export interface Entry {
  key: string;
  kind: EntryKind;
  title: string;
  text: string;
  status: EntryStatus;
  /** For a tool call: what it acted on, and the edit it made. */
  toolKind?: string;
  diff?: FileDiff;
  attachments?: string[];
  /** Structured fields of a turn event: a retry's cause, a generation's counts. */
  data?: Record<string, any>;
  /** The protocol messages this entry was built from, for a diagnostic export. */
  raw?: unknown[];
  /** For the person's message: its number in this session, when it can be rewound to. */
  turn?: number;
  at: number;
}

export interface ContextWindow {
  tokens: number;
  rationale: string;
  ceiling: string;
  setting: number | null;
}

export type ProfileStatus =
  | 'verified'
  | 'locally_calibrated'
  | 'provisional'
  | 'limited'
  | 'incompatible';

export interface CalibrationCheck {
  name: string;
  /** `null`: not run. */
  passed: boolean | null;
  critical: boolean;
  detail: string;
}

export interface CapabilityLine {
  label: string;
  result: 'supported' | 'not_reliable' | 'detected' | 'not_detected' | 'provisional' | 'not_tested';
}

/** What the core knows about the selected model (`pwr_models::profile::Assessment`). */
export interface ModelCompatibility {
  status: ProfileStatus;
  confidence?: 'established' | 'preliminary' | 'reduced' | 'untested';
  summary?: string;
  reasons?: string[];
  features?: { chat: boolean; agent: boolean; note: string | null };
  checks?: CalibrationCheck[];
  capabilities?: CapabilityLine[];
  recalibrate?: boolean;
  /** The person chose conservative defaults for this untested model. */
  acknowledged?: boolean;
}

export type ReasoningEffort = 'low' | 'medium' | 'high';

/** How Reasoning Effort applies to the selected model; policy stays in the core. */
export interface ReasoningInfo {
  effort: ReasoningEffort;
  applies: boolean;
  control: 'budget' | 'level' | 'off_by_profile' | 'unsafe' | 'none' | 'observable_only' | 'unknown';
  budgets: Record<ReasoningEffort, number> | null;
  budgetSource: 'profile' | 'calibrated' | 'conservative' | null;
}

export interface SessionSummary {
  sessionId: string;
  title: string;
  updatedAt: string;
}

export interface PermissionRequest {
  id: number | string;
  title: string;
  approval: string;
  options: { optionId: string; name: string; kind: string }[];
}

export const CONTEXT_STEPS = [2048, 4096, 8192, 16384, 32768, 65536, 131072, 196608, 262144];

export function modelLabel(ref: string): string {
  const name = ref.split('/').pop() ?? ref;
  return name
    .replace(/-MLX|-4bit|-8bit|-MXFP4|-Q8|-GGUF/gi, '')
    .replace(/-/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/** What `_pwr/context` reports. Composition figures are estimates. */
export interface ContextInfo {
  window: number;
  used: number;
  /** `engine` when the engine counted it after the last reply, else `estimate`. */
  usedSource: 'engine' | 'estimate';
  estimatedTokens: number;
  estimateBasis: string;
  composition: {
    system: number;
    conversation: number;
    repository: number;
    toolResults: number;
    taskState: number;
    compactedMemory: number;
    /** Profile, memories and the project's instructions (absent from older cores). */
    personal?: number;
  };
  autoCompact: {
    enabled: boolean;
    thresholdPercent: number;
    thresholdTokens: number;
    bounds: [number, number];
    custom: boolean;
  };
  lastCompaction: { trigger: 'manual' | 'automatic'; at: string; tokensBefore: number; tokensAfter: number } | null;
  /** The last reply's tokens by phase; counts only, never the reasoning. */
  lastGeneration?: {
    reasoningTokens: number | null;
    answerTokens: number | null;
    generatedTokens: number | null;
    tokenAccounting: 'engine_tokenizer' | 'server_usage' | 'estimated' | null;
    budgetReached: boolean | null;
    effort: string | null;
    requestedTokens: number | null;
    effectiveTokens: number | null;
    clamped: boolean | null;
  } | null;
}

export interface HardwareInfo {
  host: {
    platform: 'macos' | 'windows' | 'linux' | 'unknown';
    osName: string | null;
    osVersion: string | null;
    architecture: string;
    cpu: string | null;
    appleChip: { name: string; generation: number | null; tier: string | null } | null;
    memory: { totalBytes: number | null; availableBytes: number | null; availableIsEstimate: boolean; unified: boolean };
    gpus: { name: string; vendor: string; vramBytes: number | null; vramSource: string; unifiedMemory: boolean }[];
    disk: { path: string; freeBytes: number; totalBytes: number } | null;
    unknown: string[];
  };
  backends: BackendStatus[];
}

export interface BackendStatus {
  id: 'mlx' | 'llama';
  label: string;
  format: 'mlx' | 'gguf';
  active: boolean;
  available: boolean;
  detail: string;
  modelsRoot: string;
}

export interface FitEstimate {
  level: 'recommended' | 'should_fit' | 'tight_fit' | 'not_recommended' | 'incompatible' | 'unknown';
  label: string;
  weightsBytes: number;
  expectedBytes: number;
  budgetBytes: number | null;
  windowTokens: number | null;
  gpuResident: boolean | null;
  explanation: string;
  assumptions: string[];
}

export interface CatalogVariant {
  id: string;
  format: 'mlx' | 'gguf';
  quantization: string | null;
  quantizationSource: string | null;
  files: { path: string; bytes: number }[];
  bytes: number;
  modelRef: string;
  fit: FitEstimate;
  local: 'missing' | 'partial' | 'present';
  localBytes: number;
  installed: boolean;
  blocked: string | null;
}

export interface CatalogEntry {
  repository: string;
  name: string;
  author: string | null;
  url: string;
  revision: string | null;
  format: 'mlx' | 'gguf';
  backend: string;
  baseModel: string | null;
  architecture: string | null;
  architectureSource: string | null;
  parameters: number | null;
  parametersSource: string | null;
  contextLength: number | null;
  contextSource: string | null;
  license: string | null;
  downloads: number | null;
  likes: number | null;
  gated: boolean;
  pipelineTag: string | null;
  vision: boolean;
  variants: CatalogVariant[];
  bestFit: CatalogVariant['fit']['level'];
  notes: string[];
}

export interface CatalogFilters {
  compatibleOnly: boolean;
  family?: string;
  minParameters?: number;
  maxParameters?: number;
  quantization?: string;
  minContext?: number;
  maxBytes?: number;
}

/** A download as the core's state machine reports it. */
export type DownloadState =
  | { state: 'preparing' }
  | { state: 'downloading'; bytes: number; total: number }
  | { state: 'verifying'; bytes: number; total: number }
  | { state: 'completed'; total: number }
  | { state: 'failed'; kind: string; message: string; bytes: number }
  | { state: 'cancelled'; bytes: number };

export interface DownloadView {
  downloadId: string;
  state: DownloadState;
  file?: string;
  /** Known before the first progress event so the local tab can show it. */
  format?: 'mlx' | 'gguf';
  totalBytes?: number;
  fileCount?: number;
  /** Set when the download finished: what to do, if anything, before use. */
  modelRef?: string;
  ready?: boolean;
  nextStep?: string | null;
}

/** A model on this machine, as `_pwr/local_models` lists it. */
export interface LocalModel {
  modelRef: string;
  format: 'mlx' | 'gguf';
  backend: 'mlx' | 'llama';
  path: string;
  bytes: number;
  files: number;
  partial: boolean;
  inUse: boolean;
  usable: boolean;
}

export interface ModelSamplingField {
  name: string;
  /** Null when nothing sets it and the engine applies none (the penalties). */
  value: number | null;
  source: string | { kind: string; url?: string; revision?: string; declared_source?: string };
  automatic: number | null;
  automaticSource: string | { kind: string; url?: string; revision?: string; declared_source?: string };
  override: number | null;
}

export interface ModelSamplingView {
  modelRef: string;
  backend: 'mlx';
  fields: ModelSamplingField[];
  overrides: Record<string, number>;
}

/** The MLX engine's Python environment, as the desktop shell found it. */
export interface EngineStatus {
  /** This platform runs models on MLX, so it needs the environment. */
  needed: boolean;
  /** An Apple-silicon Mac. */
  supported: boolean;
  ready: boolean;
  source: 'environment' | 'installed' | 'checkout' | null;
  python: string | null;
  /** Where an install goes. */
  location: string;
  packages: string[];
  pythonVersion: string;
}

/** One step of the engine install, or a line it printed. */
export interface EngineProgress {
  step: number;
  total: number;
  label: string;
  line: string | null;
}

/** The person, as they describe themselves in Settings (`_pwr/profile`). */
export interface Profile {
  name?: string;
  role?: string;
  about?: string;
  language?: string;
  style?: string;
  memoryEnabled: boolean;
}

export type MemoryScope = 'global' | 'workspace';

/** One remembered fact (`_pwr/memory`). */
export interface Memory {
  id: string;
  text: string;
  createdAt: string;
  source?: string;
}

export interface MemoryList {
  global: Memory[];
  workspace: Memory[];
  /** The project's instructions file, when there is one. */
  instructions: { path: string; chars: number } | null;
}

/** A fact the model proposed to remember, waiting for the person. */
export interface MemoryProposal {
  key: string;
  sessionId: string | null;
  text: string;
  scope: MemoryScope;
}

/** A workspace PWR keeps a wiki for (`_pwr/projects`). */
export interface KnownProject {
  name: string;
  path: string;
  updatedAt: string;
  summary?: string;
}
