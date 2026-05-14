# Dependency Policy

Oxide Core is built around Oxide-owned engine systems. The project may use
utility and platform crates, but must not depend on third-party game engine
building blocks for its core runtime identity.

## Disallowed Dependency Categories

Do not add dependencies that provide:

- Game engines or gameplay frameworks.
- ECS runtimes or scene graph frameworks.
- Physics engines, collision engines, or rigid-body solvers.
- Renderer/gameplay bundles that impose an engine architecture.

Examples of dependencies that should not be introduced include Bevy, Rapier,
hecs, Legion, Specs, Fyrox, ggez, macroquad, Bullet, PhysX, Box2D, and similar
engine-domain frameworks.

## Allowed Dependency Categories

Utility and platform dependencies are allowed when they stay behind Oxide-owned
APIs:

- OS/window/GPU abstraction crates such as `winit` and `wgpu`.
- Math, serialization, diagnostics, image, and asset-import utilities.
- Development and editor tooling, preferably behind features.

## Boundary Rules

- Core crates should depend on the narrowest Oxide crates they need.
- Importers and development tools should be optional features or separate crates.
- Runtime-facing APIs should use Oxide-owned types and abstractions wherever
  practical.
- New dependencies in sensitive categories should be documented in the PR and
  justified against this policy.
