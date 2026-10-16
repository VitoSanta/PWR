# Shared workspace tools

The desktop negotiates `clientCapabilities._meta.pwr.workspaceControl`. The core then offers `workspace_interact`; headless and older clients retain their existing catalogues. Requests carry the originating conversation ID. The desktop rejects stale or inactive conversations, and queues browser operations.

## Editor

Monaco buffers live in a workspace store, independently of cards. Saves carry the original content hash and use the existing atomic writer. Disk updates reload clean buffers; dirty buffers are preserved and marked conflicted. Before model file mutations or shared terminal input, the core asks the desktop to guard unsaved buffers. Creating, renaming, searching and recoverable removal are human editor operations under `_pwr/editor`. Trash is workspace-local under `.pwr/editor-trash`; internal PWR/Git folders and paths outside the workspace are excluded. Monaco supplies editing and syntax highlighting; language servers are a subsequent integration, not currently installed by the editor.

## Terminal

The engineer's login shells remain private. The model starts an explicit executable and arguments in a new visible PTY tab, associated with its conversation. The core constructs its sandbox profile from the current tool policy; the desktop does not accept model input into private shell tabs. The process receives a clean environment with workspace HOME/TMPDIR and approved runtime paths. Input is either controlled by PWR or by the person. Taking control stops further model input; Stop sends an interrupt. Switching conversation closes model processes. Existing direct command tools remain available.

## Browser

The Browser card shows each tab as a native child web view of the PWR window (WebKit, through Tauri), placed over the card's page area and hidden while a menu or dialog covers it. Remote pages receive no Tauri IPC. The only page-to-host return path is the pending random-token `pwr-browser` response used by the DOM bridge; it cannot invoke host commands. Model operations require a visible selected tab shared with that conversation. Snapshots return bounded page text and fresh element references. Actions never retry automatically after navigation destroys the document. Password and file fields require direct user input. Popups and non-HTTP(S) navigation are denied. Cookies and site data stay in WebKit's data store, protected by macOS. A home page (Google by default) opens with the first tab and with New tab. Downloads and native dialog automation are not implemented.

## Final verification

Check stale/dirty saves, traversal and symlink denial, recoverable removal, workspace/conversation switches, terminal input ownership and confinement, browser visibility/ownership and request timeouts, and shutdown cleanup. Run frontend tests, core protocol tests, native bridge tests and release packaging after implementation. Exercise Editor, PTY and browser through the installed app without requiring model inference.
