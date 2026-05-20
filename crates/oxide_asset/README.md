# oxide_asset

[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/alan13367/Oxide-Core#license)
[![Crates.io](https://img.shields.io/crates/v/oxide-core-asset.svg)](https://crates.io/crates/oxide-core-asset)
[![Docs](https://docs.rs/oxide-core-asset/badge.svg)](https://docs.rs/oxide-core-asset/latest/oxide_asset/)

Asset handles and runtime asset caching for the Oxide Core game engine.

`Assets<T>` stores typed values by handle and tracks a per-handle revision.
Revisions start at `1`, increment whenever `insert` replaces a loaded value,
and can be queried with `revision` or `changed_since`. Use
`get_mut_mark_changed` for in-place edits that should invalidate renderer,
editor, or importer caches. `changes` and `drain_changes` expose added,
modified, and removed handles since the last clear/drain so systems can rebuild
only affected caches. Prefer `AssetChangeCursor<T>` when multiple caches or
systems need to observe the same change log independently without a single
global drain owner.

`AssetServer` tracks typed path identity, async load status, and optional
dependency paths. Importers can record secondary files with
`set_asset_dependencies` or `add_asset_dependency`, then hot-reload systems can
ask `handles_for_changed_path::<T>(path)` which typed assets should be
reloaded when a source file changes. Use `reload_path_async` to load a
replacement value into the existing handle so entities and resources keep stable
asset references while `poll_loaded` updates `Assets<T>`.
