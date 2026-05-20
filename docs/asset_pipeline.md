# Asset Pipeline Roadmap

Oxide supports direct importer paths for development plus first-class native
asset documents for scenes and materials.

## Current Shape

- `oxide_asset` owns generic handles, typed storage, async loading, load
  status, typed path identity, labeled sub-asset identity, and dependency path
  metadata for hot reload and importer invalidation.
- `oxide_renderer` owns GPU resources and optional glTF/image importer helpers.
- `oxide_renderer` owns versioned `.oxmat` material documents.
- `oxide_scene` owns scene descriptors, renderable scene components, and
  versioned `.oxscene` documents.
- `oxide_engine` re-exports scene/material/asset APIs and wires importer
  systems into the app lifecycle.

## Direction

- Import external formats such as glTF, PNG, and JPEG as tooling inputs.
- Convert imported data into Oxide-owned runtime assets.
- Keep runtime systems depending on Oxide asset handles and render components,
  not on external file-format identities.
- Prefer versioned Oxide documents for game-authored scenes and materials.

## First Runtime Asset Types

- `.oxscene`: scene descriptor with entities, transforms, lights, cameras,
  scene-declared dependency paths, and render intent.
  `load_scene_descriptor(...)` accepts both legacy raw scene JSON and wrapped
  `.oxscene`; `save_scene_descriptor(...)` writes the wrapper.
- `.oxmesh`: preprocessed mesh buffers and metadata.
- `.oxmat`: material descriptor targeting Oxide renderer pipelines.
  `load_material_descriptor(...)` accepts legacy JSON/RON/TOML and wrapped
  `.oxmat`; `save_material_descriptor(...)` writes the wrapper.

## Runtime Scene Loading

Use `request_oxscene_spawn(world, path)` to asynchronously load a native scene
document and spawn it once ready. Spawned root entities are available through
`take_spawned_oxscene_roots(world, handle)`. The system is installed by
`DefaultPlugins`, so it does not require glTF importer features.
Use `scene_instance_id(world, root)` on a returned root when the same descriptor
may be spawned more than once; scoped scene path and tag helpers can then target
entities inside that loaded copy. Use `despawn_scene_instance(world, instance)`
to unload one spawned copy without touching other instances of the same scene.

Use `reload_oxscene_path(world, path)` to reload a known scene path into its
existing handle, or `reload_changed_oxscenes(world, changed_paths)` to reload
scene assets whose source path or dependency paths match file watcher output.
Reloading updates `SceneDescriptorAssets` and preserves the handle; it does not
spawn duplicate roots. Queue the returned handle with `queue_oxscene_spawn` only
when the app intentionally wants a fresh instance of the reloaded descriptor.
For normal development hot reload, prefer
`reload_changed_native_assets(world, changed_paths)` or
`poll_native_asset_reloads(world)`. These helpers route one changed-path list
through Oxide's native `.oxscene` and `.oxmat` reload systems and return a
`NativeAssetReloadSummary` containing affected scene and material handles.
Scene documents can declare relative or absolute dependency paths in
`scene.dependencies`; the runtime records them against the loaded scene handle
so changes to referenced material, sprite, or imported data files can invalidate
the scene without app code manually updating the `AssetServer`.

`examples/minimal_game` demonstrates this path with
`assets/scenes/starter.oxscene`.

## glTF Import Handles

With the `gltf-import` feature enabled, `request_gltf_scene_spawn(...)` loads a
glTF source through `AssetServer` path identity. When the load resolves, Oxide
publishes each imported mesh into `MeshCache` as a labeled `Mesh3D` asset using
the glTF source path plus the mesh label emitted by the importer. Spawned nodes
keep the lightweight `GltfMeshRef` index and also receive `MeshFilter` when a
stable mesh handle is available.

This keeps the compatibility glTF path useful for development while moving the
runtime shape toward Oxide-owned handles and caches.

## Runtime Material Descriptor Loading

Use `request_material_descriptor_load(server, path)` to asynchronously load a
`.oxmat`, JSON, RON, or TOML material descriptor into
`MaterialDescriptorAssets`. `DefaultPlugins` installs
`material_descriptor_asset_system`, which publishes ready descriptors and
records dependencies for file shaders and texture paths. Loaded descriptors are
also registered into `SceneMaterialLibrary` by `MaterialDescriptor::name`, so
`RenderMaterial::Named("stone".to_string())` can resolve through the automatic
scene renderer. `base_color` is preserved on the registered material and
multiplied with each renderable's tint during scene rendering.

```rust
let handle = request_material_descriptor_load(
    &mut world.resource_mut::<AssetServerResource>().server,
    "assets/materials/stone.oxmat",
);
```

Use `reload_material_descriptor_path(server, path)` to refresh a known
descriptor path in place, or `reload_changed_material_descriptors(server,
changed_paths)` when a watcher reports changed descriptor, shader, or texture
paths. Dependency paths in descriptors are resolved relative to the descriptor
file, which keeps authored material folders portable.

The direct importer features stay enabled by default for examples that still use
external tooling formats.

## Dependency Tracking

`AssetServer` can record secondary source paths for any typed asset handle.
It also supports labels for sub-assets imported from one container file:

```rust
let mesh = server.load_labeled_path_async(
    "assets/level.gltf",
    Some("Mesh0"),
    |path, label| load_mesh_from_container(path, label.unwrap()),
);

let same_mesh = server.handle_for_labeled_path::<MeshAsset>(
    "assets/level.gltf",
    Some("Mesh0"),
);
```

When an importer parses one container and already has ready runtime assets, it
can publish all outputs without spawning one loader per sub-asset:

```rust
let mut meshes = Assets::<MeshAsset>::new();
let parsed = import_level_gltf("assets/level.gltf")?;

for mesh in parsed.meshes {
    server.insert_loaded_labeled_path(
        &mut meshes,
        "assets/level.gltf",
        Some(mesh.name.as_str()),
        mesh.asset,
    );
}
```

Labeled and unlabeled handles are distinct, but a watcher change for
`assets/level.gltf` still matches every loaded label from that source. Use
`asset_source(handle)` or `asset_label(handle)` when tools need to display or
persist where a handle came from.

For secondary files, record dependencies as before:

```rust
server.set_asset_dependencies(
    &scene_handle,
    [
        "assets/materials/stone.oxmat",
        "assets/textures/stone.png",
    ],
);

let affected = server.handles_for_changed_path::<SceneDescriptor>(
    "assets/materials/stone.oxmat",
);
```

This is intentionally path-based and format-agnostic. Scene, material, mesh,
and importer systems can add dependency edges without pulling a graph crate into
the runtime, and hot-reload code can query affected typed handles when a source
file changes.

`Assets<T>` also tracks per-handle revisions. A loaded handle starts at revision
`1`, replacements increment the revision, and `changed_since(handle, revision)`
lets renderer/editor caches cheaply decide whether a stable handle now points at
new data. Use `get_mut_mark_changed` for direct in-place edits that should
invalidate dependent caches. `changes()` and `drain_changes()` report added,
modified, and removed handles so systems can rebuild only affected GPU/editor
caches without scanning the whole asset collection every frame.
When more than one cache needs the same records, keep an
`AssetChangeCursor<T>` per cache and call `read(&assets)`; this advances only
that cursor and leaves the retained change log available for other systems.

When a changed path maps to a known typed asset path, use
`reload_path_async(path, loader)` to keep the existing handle and publish the
replacement value through `poll_ready` or `poll_loaded`. This preserves handles
already stored in entities, components, UI models, or scene resources.
