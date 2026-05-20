# Asset Pipeline Roadmap

Oxide supports direct importer paths for development plus first-class native
asset documents for scenes and materials.

## Current Shape

- `oxide_asset` owns generic handles, typed storage, async loading, registered
  typed extension loaders, load status, typed path identity, labeled sub-asset
  identity, and dependency path metadata for hot reload and importer
  invalidation.
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
For normal development hot reload in apps using the automatic renderer, prefer
`reload_changed_render_assets(world, changed_paths)` or
`poll_render_asset_reloads(world)`. These helpers route one changed-path list
through Oxide's native `.oxscene`/`.oxmat` reload systems and, when renderer
resources are available, glTF scene reloads. Use the narrower
`reload_changed_native_assets(world, changed_paths)` or
`poll_native_asset_reloads(world)` when an app only wants native documents. The
native helpers return `NativeAssetReloadSummary`; the renderer-facing helpers
wrap that summary in `RenderAssetReloadSummary`.
Scene documents can declare relative or absolute dependency paths in
`scene.dependencies`; the runtime records them against the loaded scene handle
so changes to referenced material, sprite, or imported data files can invalidate
the scene without app code manually updating the `AssetServer`.
The native scene loader also records non-virtual `albedo_texture`,
`normal_texture`, and `roughness_texture` paths from top-level scene materials,
inline mesh materials, prefab materials, and prefab material overrides. Those
image files are loaded into `TextureImageAssets` with their authored labels, so
a scene material can use `"albedo_texture": "textures/crate.png"` without a
separate `.oxmat` descriptor.

`examples/minimal_game` demonstrates this path with
`assets/scenes/starter.oxscene`.

## Registered Typed Loaders

For app-owned or plugin-owned asset types, register extension loaders once on
`AssetServer` and then request typed paths without repeating the parsing closure
at every call site:

```rust
server.register_loader::<String, _>(["txt"], |path| {
    std::fs::read_to_string(path).map_err(|err| {
        AssetServerError::Message(format!("failed to read {}: {err}", path.display()))
    })
});

let handle = server.load_registered_path::<String>("assets/dialogue/intro.txt")?;
```

The registered path uses the same typed path identity, status, polling, and
reload machinery as `load_path_async`. Calling
`reload_registered_path::<T>(path)` refreshes a known path through its
registered loader while preserving the existing handle. Closure-based
`load_path_async` and `load_labeled_path_async` remain available for one-off
loads and container importers that need labels or custom dependency handling.

`DefaultPlugins` calls `register_native_asset_loaders(...)` during asset
resource initialization, so native scene and material helpers share the generic
loader registry for `.oxscene`, `.oxmat`, JSON, RON, and TOML descriptors. Tools
that construct an `AssetServer` directly can call the same helper before using
`load_registered_path` for Oxide-native assets.

## glTF Import Handles

With the `gltf-import` feature enabled, `request_gltf_scene_spawn(...)` loads a
glTF source through `AssetServer` path identity. When the load resolves, Oxide
records external glTF buffer and image URIs as dependencies on the scene handle,
so a watcher event for a sidecar `.bin` or texture file can find the owning
scene through `handles_for_changed_path::<GltfScene>(...)`. Oxide then
publishes each imported mesh into `MeshCache` as a labeled `Mesh3D` asset using
the glTF source path plus the mesh label emitted by the importer. Spawned nodes
keep the lightweight `GltfMeshRef` index and also receive `MeshFilter` when a
stable mesh handle is available.

Imported glTF materials are converted into Oxide `MaterialDescriptor` values
using the lit built-in shader and the glTF base color factor. They are published
into `MaterialDescriptorAssets` as labeled sub-assets and registered in
`SceneMaterialLibrary` by descriptor name. Spawned mesh nodes keep
`GltfMaterialRef` and receive `MaterialFilter` when a stable material descriptor
handle is available.

Imported glTF images are converted into CPU-side RGBA `TextureImage` assets and
published into `TextureImageAssets` with labels such as `image_0`. Materials that
reference a glTF base-color texture keep a virtual texture reference like
`#image_0`; these virtual references are ignored by filesystem dependency
tracking because they point at labeled sub-assets from the same imported
container, not separate files.

The automatic scene renderer can draw `MeshFilter` entities directly from
`MeshCache`. When a handle-based mesh entity also carries `RenderMesh`, the
renderer uses that component's tint and render intent for the imported mesh and
skips the built-in primitive path. If the entity has `MaterialFilter`, the
renderer resolves the descriptor handle from `MaterialDescriptorAssets` before
falling back to the copied `RenderMesh` material. This lets imported glTF
materials and reloaded `.oxmat` descriptors influence already spawned handle
meshes without duplicating geometry. `TextureImageAssets` also
maintains a label lookup, and the scene renderer uploads labeled images into a
small material texture cache. A material albedo reference such as `#image_0`
binds that uploaded texture for the relevant material batch; normal and
roughness references are bound into the same material texture set when present,
while missing slots use a neutral normal map or white fallback texture.

Spawned entities from the async glTF flow receive `GltfSceneInstance` with the
source scene handle. If the same handle is queued again after
`reload_gltf_scene_path(...)` or `reload_changed_gltf_scenes(...)`, the spawn
system removes the previous imported hierarchy before spawning the replacement.
External entities parented under the imported hierarchy are detached and
preserved.

This keeps the compatibility glTF path useful for development while moving the
runtime shape toward Oxide-owned handles and caches.

## Runtime Material Descriptor Loading

Use `request_material_descriptor_load(server, path)` to asynchronously load a
`.oxmat`, JSON, RON, or TOML material descriptor into
`MaterialDescriptorAssets`. `DefaultPlugins` installs
`material_descriptor_asset_system`, which publishes ready descriptors and
records dependencies for file shaders and texture paths. Non-virtual
albedo/normal/roughness texture paths are loaded into `TextureImageAssets` using
the authored texture path as a label, which lets renderer systems bind them
through the same material texture cache used for glTF image labels. The automatic
scene renderer binds albedo, normal, and roughness slots together and falls back
to a neutral normal map or white texture for missing slots. Loaded descriptors are
also registered into `SceneMaterialLibrary` by `MaterialDescriptor::name`, so
`RenderMaterial::Named("stone".to_string())` can resolve through the automatic
scene renderer. Entities can also store `MaterialFilter` to render directly from
a descriptor handle; this handle-backed path takes precedence over a copied
`RenderMesh` material when the descriptor asset is available. `base_color` is
preserved on the resolved material and multiplied with each renderable's tint
during scene rendering. `metallic_factor`, `roughness_factor`, and
`emissive_color` are also preserved for the automatic lit shader. See
`examples/material_filter_example` for a compact code-first scene that renders
from a descriptor handle.

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
Engine-facing systems can also publish those retained records into ECS events
with `publish_asset_change_events::<T, Store>`. `RenderPlugin` installs this
bridge for built-in material pipeline, mesh, material descriptor, texture image,
and glTF scene stores, so gameplay, editor, and renderer tooling can observe
`Events<AssetChange<T>>` with the normal `EventCursor` / `EventReader` params
without owning the asset store's change log.

When a changed path maps to a known typed asset path, use
`reload_path_async(path, loader)` to keep the existing handle and publish the
replacement value through `poll_ready` or `poll_loaded`. This preserves handles
already stored in entities, components, UI models, or scene resources.
