# Zombie Shooter

Code-first FPS prototype for Oxide Core.

## Run

```bash
cargo run -p zombie_shooter
```

## Controls

- `W/A/S/D`: move
- Mouse: look
- `Space`: jump
- Left mouse: shoot
- `R`: reload, or restart after dying
- `Enter`: start/resume/restart from menu screens
- `Escape`: pause/resume

The example uses the automatic scene renderer, in-house physics, character
controller movement, raycast shooting, simple zombie chase AI, generated PNG
sprites registered through Oxide's native sprite system for the zombies and
first-person weapon, and a `SceneWorldDescriptor` for customizable heightfield
terrain plus arena blockout objects. It also uses
`GameUi` for the start menu, pause menu, game-over panel, health bar, ammo pips,
reserve ammo bar, wave pressure bar, text labels, and crosshair. On macOS it
registers system TTF fonts for the title and HUD, falling back to the built-in
Oxide bitmap font if those font files are unavailable. It also uses
`AudioPlugin` for menu, weapon, reload, hit, and game-over sound feedback.

Opening the start, pause, or game-over menu releases cursor capture and shows
the pointer. Starting or resuming play captures it again for FPS mouse look.
