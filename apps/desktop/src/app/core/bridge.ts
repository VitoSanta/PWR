// The only way the interface reaches the machine: the Tauri bridge in
// `src-tauri/src/lib.rs`, which owns the `pwr serve --stdio` process.
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { open, save } from '@tauri-apps/plugin-dialog';
import type { EngineProgress, EngineStatus } from './model';

export interface Started {
  core: string;
  workspace: string;
  /** Which start of the core this is; exits of earlier ones are ignored. */
  generation: number;
}

export const inTauri = (): boolean =>
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export const bridge = {
  draftsRead: (workspace: string) => invoke<import('./draft-recovery').DraftSnapshot | null>('drafts_read', { workspace }),
  draftsWrite: (workspace: string, snapshot: import('./draft-recovery').DraftSnapshot | null) => invoke<void>('drafts_write', { workspace, snapshot }),
  updateStatus: () => invoke<{configured: boolean; currentVersion: string; channel: string}>('update_status'),
  updateCheck: () => invoke<{version: string; notes: string} | null>('update_check'),
  updateDownload: () => invoke<void>('update_download'),
  updateInstall: () => invoke<void>('update_install'),
  updateRestart: () => invoke<void>('update_restart'),
  onUpdateProgress: (handler: (event: {received: number; total: number | null}) => void) => listen<{received: number; total: number | null}>('update-progress', event => handler(event.payload)),
  defaultWorkspace: () => invoke<string>('default_workspace'),
  chatHome: () => invoke<string>('chat_home_path'),
  workspaceIsTrusted: (workspace: string) => invoke<boolean>('workspace_is_trusted', { workspace }),
  trustWorkspace: (workspace: string) => invoke<void>('trust_workspace', { workspace }),
  start: (workspace: string) => invoke<Started>('core_start', { workspace }),
  send: (message: unknown) => invoke<void>('core_send', { message }),
  stop: () => invoke<void>('core_stop'),
  confirmClose: () => invoke<void>('app_close_confirmed'),
  onCloseRequested: (handler: () => void): Promise<UnlistenFn> =>
    listen('app-close-requested', () => handler()),
  openExternal: (url: string) => invoke<void>('open_external', { url }),
  engineStatus: () => invoke<EngineStatus>('engine_status'),
  engineInstall: () => invoke<void>('engine_install'),
  engineCancel: () => invoke<void>('engine_cancel'),
  debugExportChat: (destination: string, conversation: unknown) =>
    invoke<void>('debug_export_chat', { destination, conversation }),
  pickDebugExport: async (): Promise<string | null> => {
    const picked = await save({
      title: 'Export development chat diagnostic',
      defaultPath: 'pwr-chat-diagnostic.json',
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    return typeof picked === 'string' ? picked : null;
  },
  onEngineSetup: (handler: (progress: EngineProgress) => void): Promise<UnlistenFn> =>
    listen<EngineProgress>('engine-setup', (event) => handler(event.payload)),
  onMessage: (handler: (message: any, generation: number) => void): Promise<UnlistenFn> =>
    listen<{ message: any; generation: number }>('acp', (event) => handler(event.payload.message, event.payload.generation)),
  onLog: (handler: (line: string, generation: number) => void): Promise<UnlistenFn> =>
    listen<{ line: string; generation: number }>('core-log', (event) => handler(event.payload.line, event.payload.generation)),
  onExit: (handler: (generation: number) => void): Promise<UnlistenFn> =>
    listen<number>('core-exit', (event) => handler(event.payload)),
  pickFiles: async (): Promise<string[]> => {
    const picked = await open({ multiple: true, directory: false, title: 'Attach files' });
    return picked === null ? [] : Array.isArray(picked) ? picked : [picked];
  },
  pickImages: async (): Promise<string[]> => {
    const picked = await open({ multiple: true, directory: false, title: 'Attach images', filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'] }] });
    return picked === null ? [] : Array.isArray(picked) ? picked : [picked];
  },
  browserOpen: (id: string, url: string) => invoke<void>('browser_open', { id, url }),
  browserSync: (request: unknown) => invoke<void>('browser_sync', { request }),
  browserAction: (id: string, operation: string, args: unknown = {}) => invoke<any>('browser_action', { id, operation, arguments: args }),
  browserClose: (id: string) => invoke<void>('browser_close', { id }),
  onBrowserNavigation: (handler: (event: { id: string; url: string }) => void) => listen<{id:string;url:string}>('browser-navigation', event => handler(event.payload)),
  onBrowserLoaded: (handler: (event: { id: string; url: string; finished: boolean }) => void) => listen<{id:string;url:string;finished:boolean}>('browser-loaded', event => handler(event.payload)),
  onBrowserTitle: (handler: (event: { id: string; title: string }) => void) => listen<{id:string;title:string}>('browser-title', event => handler(event.payload)),
  termOpen: (cwd: string, cols: number, rows: number, launch?: unknown) => invoke<number>('term_open', { cwd, cols, rows, launch }),
  termAck: (id: number, sequence: number) => invoke<void>('term_ack', { id, sequence }),
  termWrite: (id: number, data: string) => invoke<void>('term_write', { id, data }),
  termResize: (id: number, cols: number, rows: number) => invoke<void>('term_resize', { id, cols, rows }),
  termClose: (id: number) => invoke<void>('term_close', { id }),
  onTermOutput: (handler: (output: { id: number; data: string; sequence: number }) => void): Promise<UnlistenFn> =>
    listen<{ id: number; data: string; sequence: number }>('term-output', (event) => handler(event.payload)),
  onTermExit: (handler: (id: number) => void): Promise<UnlistenFn> =>
    listen<number>('term-exit', (event) => handler(event.payload)),
  pickFolder: async (title: string): Promise<string | null> => {
    const picked = await open({ multiple: false, directory: true, title });
    return typeof picked === 'string' ? picked : null;
  },
};
