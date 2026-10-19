PWR distribution attribution material

PWR-LICENSE.txt: PWR's Apache-2.0 license.
frontend/: Angular-generated notices for bundled frontend code.
fonts/: license texts distributed with the bundled fonts.
rust/: license texts found in the locked target's Cargo source packages.
uv/: license texts distributed with the bundled uv executable (macOS).
inventory.json: package declarations, source references, text hashes and review gaps.

The Cargo inventory conservatively includes non-development and build dependencies;
not every listed package is necessarily linked into the final executable. Missing
texts remain listed as unresolved. uv's own license texts do not establish its
complete transitive binary attribution. Managed Python packages and model weights
are installed separately and require separate inventory and review.

This directory is attribution material, not a legal clearance or a claim that all
third-party obligations have been discharged. Do not remove unresolved items merely
to make a release check pass. The build regenerates this set from locked metadata.
