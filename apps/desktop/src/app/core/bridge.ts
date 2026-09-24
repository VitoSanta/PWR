// The only way the interface reaches the machine: the Tauri bridge in
// `src-tauri/src/lib.rs`, which owns the `pwr serve --stdio` process.
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';

export interface Started {
  core: string;
  workspace: string;
  /** Which start of the core this is; exits of earlier ones are ignored. */
  generation: number;
}

export const inTauri = (): boolean =>
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export const bridge = {
  defaultWorkspace: () => invoke<string>('default_workspace'),
  start: (workspace: string) => invoke<Started>('core_start', { workspace }),
  send: (message: unknown) => invoke<void>('core_send', { message }),
  restoreFile: (workspace: string, path: string, content: string, remove: boolean) =>
    invoke<void>('restore_workspace_file', { workspace, path, content, remove }),
  stop: () => invoke<void>('core_stop'),
  onMessage: (handler: (message: any) => void): Promise<UnlistenFn> =>
    listen<any>('acp', (event) => handler(event.payload)),
  onLog: (handler: (line: string) => void): Promise<UnlistenFn> =>
    listen<string>('core-log', (event) => handler(event.payload)),
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
  pickFolder: async (title: string): Promise<string | null> => {
    const picked = await open({ multiple: false, directory: true, title });
    return typeof picked === 'string' ? picked : null;
  },
};
