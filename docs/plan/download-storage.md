# Model-download storage boundary

The downloader accepts a trusted destination root and registry plan. Remote responses provide bytes, not filesystem authority. Each file destination must be a nonempty relative path beneath that root, without parent or absolute components after removing the root prefix.

## Unix implementation

`crates/pwr-models/src/download/storage.rs` pins the selected root once, before HTTP. An exclusive nonblocking advisory lock on that directory serializes cooperating PWR downloads for the whole operation; a busy destination produces a conflict. The lock is released by descriptor lifetime, including process termination. It does not prevent an unrelated process from ignoring the lock.

Each subdirectory is opened relative to an already pinned descriptor with `DIRECTORY | NOFOLLOW`. Missing subdirectories are created relative to that descriptor. Partial reads/writes, hashing, removal and no-replacement publication use the pinned file parent, including retry and resume. The writer validates its opened descriptor before truncation and refuses non-regular, multiply-linked or unexpectedly resized resumed files. Hashing uses the opened regular-file descriptor off the async event loop.

The named root and parent identities are checked around verification/publication. A directory moved or replaced while HTTP or hashing is in flight is not reported as a successful named installation. Publication never replaces a destination that another writer installed in the meantime and compares device/inode identity with the verified file. A conflicting path remains for diagnosis rather than being deleted as though PWR still owned it.

The revision marker is written to an exclusively created temporary file beneath the pinned root, flushed, and renamed within the same directory. Replacing a marker symlink replaces the link entry itself and never writes into its target. A missing/unwritable marker leaves revision provenance unknown; it does not turn unverified weights into verified weights.

## Tested scope and limits

The third-candidate full workspace run reports 1,601 passed and 6 ignored, with the Docker socket skip recorded; Clippy passes. Local HTTP/filesystem regressions cover normal download, resumed and interrupted transfer, wrong bytes, late partial symlink/hard-link substitution, concurrent destination creation, partial replacement after hashing, root replacement during HTTP, linked revision marker, nested symlink escape, out-of-root/parent destinations, and exclusive-root ownership/release.

The trusted root is established at operation start. Arbitrary concurrent same-user processes that disregard advisory locks can still mutate permitted files or replace names after any final check; this is not a proof against a malicious host or a compromised account. Disk failure/power-loss durability and network-filesystem behavior require separate qualification. A filesystem that cannot perform the no-replacement hard-link publication fails without replacing the destination.

Windows retains a separate fallback and needs native testing. Unix descriptor/lock evidence must not be reported as Windows containment or release qualification. This repair alone does not pass G1 or the full release gate.
