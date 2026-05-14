use oxide_engine::prelude::*;
use oxide_physics::prelude::*;

const ARENA_HALF_SIZE: f32 = 24.0;
const PLAYER_SPEED: f32 = 7.0;
const PLAYER_JUMP_SPEED: f32 = 6.2;
const PLAYER_EYE_HEIGHT: f32 = 1.15;
const GRAVITY: f32 = -18.0;
const MOUSE_SENSITIVITY: f32 = 0.0022;
const TITLE_FONT: &str = "zombie:title";
const HUD_FONT: &str = "zombie:hud";
const GUN_SPRITE: &str = "zombie:gun";
const GUN_FIRE_SPRITE: &str = "zombie:gun_fire";
const ZOMBIE_SPRITE: &str = "zombie:walker";

#[derive(Clone, Copy, Debug)]
struct Health {
    current: f32,
    max: f32,
}

impl Component for Health {}

impl Health {
    fn new(max: f32) -> Self {
        Self { current: max, max }
    }

    fn damage(&mut self, amount: f32) {
        self.current = (self.current - amount).max(0.0);
    }

    fn is_dead(self) -> bool {
        self.current <= 0.0
    }
}

#[derive(Clone, Copy, Debug)]
struct Zombie {
    speed: f32,
    attack_range: f32,
    attack_damage: f32,
    attack_cooldown: f32,
    attack_timer: f32,
}

impl Component for Zombie {}

impl Zombie {
    fn basic(wave: u32) -> Self {
        Self {
            speed: 2.2 + wave as f32 * 0.18,
            attack_range: 1.15,
            attack_damage: 12.0,
            attack_cooldown: 0.85,
            attack_timer: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ZombieHitZone {
    Body,
    Head,
}

#[derive(Clone, Copy, Debug)]
struct ZombieShotHit {
    entity: Entity,
    point: Vec3,
    normal: Vec3,
    zone: ZombieHitZone,
}

#[derive(Clone, Copy, Debug)]
struct ImpactMarker {
    ttl: f32,
}

impl Component for ImpactMarker {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShooterMode {
    StartMenu,
    Playing,
    Paused,
    GameOver,
}

#[derive(Clone, Debug)]
struct ShooterState {
    player_entity: Entity,
    camera_entity: Entity,
    gun_entity: Entity,
    mode: ShooterMode,
    yaw: f32,
    pitch: f32,
    vertical_velocity: f32,
    jump_held: bool,
    enter_held: bool,
    escape_held: bool,
    reload_held: bool,
    fire_timer: f32,
    muzzle_flash_timer: f32,
    ammo: u32,
    max_ammo: u32,
    reserve_ammo: u32,
    player_health: f32,
    kills: u32,
    wave: u32,
}

impl Resource for ShooterState {}

struct ZombieShooter {
    world: World,
}

impl App for ZombieShooter {
    fn configure(world: &mut World) {
        world.init_resource::<Time>();
        world.init_resource::<KeyboardInput>();
        world.init_resource::<MouseInput>();
    }

    fn init(window: &Window, renderer: Renderer) -> Self {
        let mut world = World::new();
        Self::configure(&mut world);

        world.insert_resource(RendererResource::new(renderer));
        world.insert_resource(WindowResource::new(
            window.size().width,
            window.size().height,
        ));
        apply_cursor_capture(window, &mut world, false);

        spawn_lighting(&mut world);
        spawn_arena(&mut world);
        let player_entity = spawn_player(&mut world);
        let camera_entity = spawn_camera(&mut world);
        let gun_entity = spawn_gun(&mut world);
        install_zombie_fonts(&mut world);
        install_zombie_sprites(&mut world);

        world.insert_resource(ShooterState {
            player_entity,
            camera_entity,
            gun_entity,
            mode: ShooterMode::StartMenu,
            yaw: 0.0,
            pitch: 0.0,
            vertical_velocity: 0.0,
            jump_held: false,
            enter_held: false,
            escape_held: false,
            reload_held: false,
            fire_timer: 0.0,
            muzzle_flash_timer: 0.0,
            ammo: 12,
            max_ammo: 12,
            reserve_ammo: 48,
            player_health: 100.0,
            kills: 0,
            wave: 1,
        });
        spawn_wave(&mut world, 1);

        tracing::info!("Zombie shooter initialized.");
        tracing::info!(
            "Controls: WASD move, mouse look, Space jump, left mouse shoot, R reload/restart, Escape pause."
        );

        Self { world }
    }

    fn world(&self) -> &World {
        &self.world
    }

    fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    fn update(&mut self) {
        let dt = self.world.resource::<Time>().delta_secs().min(0.05);
        update_gameplay(&mut self.world, dt);
        update_game_ui(&mut self.world);
    }

    fn on_event(&mut self, _event: EngineEvent) {}
}

fn apply_cursor_capture(window: &Window, world: &mut World, captured: bool) {
    set_cursor_capture_requested(world, captured);
    window.set_cursor_visible(!captured);
    if let Err(err) = window.set_cursor_grabbed(captured) {
        tracing::warn!("Failed to update cursor capture: {err}");
    }
    if captured {
        if let Err(err) = window.center_cursor() {
            tracing::warn!("Failed to center cursor for FPS input: {err}");
        }
    }
}

fn set_cursor_capture_requested(world: &mut World, captured: bool) {
    if world.contains_resource::<MouseInput>() {
        world
            .resource_mut::<MouseInput>()
            .set_cursor_grabbed(captured);
    }
}

fn install_zombie_fonts(world: &mut World) {
    if let Err(err) = load_game_font(
        world,
        TITLE_FONT,
        "/System/Library/Fonts/Supplemental/Impact.ttf",
    ) {
        tracing::warn!("Falling back to built-in title font: {err}");
    }

    if let Err(err) = load_game_font(
        world,
        HUD_FONT,
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    ) {
        tracing::warn!("Falling back to built-in HUD font: {err}");
    }
}

fn install_zombie_sprites(world: &mut World) {
    for (id, image) in [
        (GUN_SPRITE, gun_sprite(false)),
        (GUN_FIRE_SPRITE, gun_sprite(true)),
        (ZOMBIE_SPRITE, zombie_sprite()),
    ] {
        match image {
            Ok(image) => {
                register_sprite(world, id, image);
            }
            Err(err) => {
                tracing::warn!("Failed to register sprite {id}: {err}");
            }
        }
    }
}

fn zombie_sprite() -> Result<SpriteImage, SpriteImageError> {
    SpriteImage::from_ascii(
        &[
            "      gggg      ",
            "     gGGGGg     ",
            "    gGEEGGg     ",
            "    gGGGGGg     ",
            "     gDDg       ",
            "   gggGGggg     ",
            "  gGgGGGGgGg    ",
            " gGggGGGGggGg   ",
            " gGgGGGGGGgGg   ",
            "  ggGGGGGGgg    ",
            "    gGggGg      ",
            "   gGg  gGg     ",
            "  gGg    gGg    ",
            "  gg      gg    ",
            " gG        Gg   ",
            "                ",
        ],
        &[
            (' ', [0, 0, 0, 0]),
            ('g', [45, 128, 42, 255]),
            ('G', [82, 191, 70, 255]),
            ('E', [234, 244, 196, 255]),
            ('D', [73, 46, 42, 255]),
        ],
    )
}

fn gun_sprite(firing: bool) -> Result<SpriteImage, SpriteImageError> {
    let muzzle = if firing { 'F' } else { ' ' };
    let rows = [
        "                                ",
        "                                ",
        "                                ",
        "                    bbbb        ",
        "                 bbbbbbbbbb     ",
        "              bbbbbbbbbbbbbb    ",
        "          dddddddddbbbbbbbbb    ",
        "      dddddddddddddddddbbb      ",
        "   DDDDDDDDDDDDDDDDDDDDD       ",
        "  DDDDDDDDDDDDDDDDDDDDD        ",
        "      DDDDDDDDDDDD             ",
        "          DDDDDDD              ",
        "             DDDD              ",
        "              DD               ",
        "                                ",
        "                                ",
    ];
    let mut rows: Vec<String> = rows.iter().map(|row| row.to_string()).collect();
    if firing {
        rows[6].replace_range(29..30, &muzzle.to_string());
        rows[7].replace_range(28..29, &muzzle.to_string());
        rows[8].replace_range(27..28, &muzzle.to_string());
    }
    let row_refs: Vec<&str> = rows.iter().map(String::as_str).collect();

    SpriteImage::from_ascii(
        &row_refs,
        &[
            (' ', [0, 0, 0, 0]),
            ('D', [16, 18, 21, 255]),
            ('d', [39, 43, 48, 255]),
            ('b', [74, 78, 82, 255]),
            ('F', [255, 218, 80, 255]),
        ],
    )
}

fn play_menu_sound(world: &World) {
    play_tone(world, AudioTone::sine(660.0, 0.08, 0.18));
}

fn play_pause_sound(world: &World) {
    play_tone(world, AudioTone::sine(330.0, 0.1, 0.16));
}

fn play_shot_sound(world: &World) {
    if let Some(audio) = world
        .contains_resource::<Audio>()
        .then(|| world.resource::<Audio>())
    {
        audio.play_tone(AudioTone {
            waveform: AudioWaveform::Noise,
            frequency_hz: 1.0,
            duration_secs: 0.045,
            volume: 0.28,
        });
        audio.play_tone(AudioTone {
            waveform: AudioWaveform::Square,
            frequency_hz: 110.0,
            duration_secs: 0.055,
            volume: 0.14,
        });
    }
}

fn play_reload_sound(world: &World) {
    play_tone(world, AudioTone::sine(440.0, 0.12, 0.14));
}

fn play_hit_sound(world: &World) {
    play_tone(
        world,
        AudioTone {
            waveform: AudioWaveform::Square,
            frequency_hz: 180.0,
            duration_secs: 0.06,
            volume: 0.16,
        },
    );
}

fn play_game_over_sound(world: &World) {
    play_tone(
        world,
        AudioTone {
            waveform: AudioWaveform::Saw,
            frequency_hz: 95.0,
            duration_secs: 0.35,
            volume: 0.2,
        },
    );
}

fn play_tone(world: &World, tone: AudioTone) {
    if world.contains_resource::<Audio>() {
        world.resource::<Audio>().play_tone(tone);
    }
}

fn update_gameplay(world: &mut World, dt: f32) {
    let Some(mut physics) = world.remove_resource::<PhysicsWorld>() else {
        return;
    };
    let Some(mut state) = world.remove_resource::<ShooterState>() else {
        world.insert_resource(physics);
        return;
    };

    let (enter_pressed, escape_pressed, reload_pressed) = read_menu_inputs(world, &mut state);

    match state.mode {
        ShooterMode::StartMenu => {
            if enter_pressed || world.resource::<MouseInput>().left_pressed {
                state.mode = ShooterMode::Playing;
                set_cursor_capture_requested(world, true);
                play_menu_sound(world);
            }
            world.insert_resource(state);
            world.insert_resource(physics);
            return;
        }
        ShooterMode::Paused => {
            if escape_pressed || enter_pressed || world.resource::<MouseInput>().left_pressed {
                state.mode = ShooterMode::Playing;
                set_cursor_capture_requested(world, true);
                play_menu_sound(world);
            }
            world.insert_resource(state);
            world.insert_resource(physics);
            return;
        }
        ShooterMode::GameOver => {
            if reload_pressed || enter_pressed {
                reset_game(world, &mut state, &mut physics);
                state.mode = ShooterMode::Playing;
                set_cursor_capture_requested(world, true);
                play_menu_sound(world);
            }
            world.insert_resource(state);
            world.insert_resource(physics);
            return;
        }
        ShooterMode::Playing => {}
    }

    if escape_pressed {
        state.mode = ShooterMode::Paused;
        set_cursor_capture_requested(world, false);
        play_pause_sound(world);
        world.insert_resource(state);
        world.insert_resource(physics);
        return;
    }

    update_player(world, &physics, &mut state, dt);
    update_gun(world, &mut physics, &mut state, dt, reload_pressed);
    update_zombies(world, &mut physics, &mut state, dt);
    cleanup_dead_zombies(world, &mut physics);
    update_impact_markers(world, dt);

    if active_zombie_count(world) == 0 {
        state.wave = state.wave.saturating_add(1);
        spawn_wave(world, state.wave);
    }

    if state.player_health <= 0.0 {
        state.player_health = 0.0;
        state.mode = ShooterMode::GameOver;
        set_cursor_capture_requested(world, false);
        play_game_over_sound(world);
        tracing::info!("Game over. Press R to restart.");
    }

    world.insert_resource(state);
    world.insert_resource(physics);
}

fn read_menu_inputs(world: &World, state: &mut ShooterState) -> (bool, bool, bool) {
    let keyboard = world.resource::<KeyboardInput>();
    let enter = consume_pressed(keyboard.pressed(KeyCode::Enter), &mut state.enter_held);
    let escape = consume_pressed(keyboard.pressed(KeyCode::Escape), &mut state.escape_held);
    let reload = consume_pressed(keyboard.pressed(KeyCode::KeyR), &mut state.reload_held);
    (enter, escape, reload)
}

fn consume_pressed(current: bool, held: &mut bool) -> bool {
    let pressed = current && !*held;
    *held = current;
    pressed
}

fn update_player(world: &mut World, physics: &PhysicsWorld, state: &mut ShooterState, dt: f32) {
    let (move_forward, move_back, move_left, move_right, jump, mouse_delta) = {
        let keyboard = world.resource::<KeyboardInput>();
        let mouse = world.resource::<MouseInput>();
        (
            keyboard.pressed(KeyCode::KeyW),
            keyboard.pressed(KeyCode::KeyS),
            keyboard.pressed(KeyCode::KeyA),
            keyboard.pressed(KeyCode::KeyD),
            keyboard.pressed(KeyCode::Space),
            mouse.delta(),
        )
    };

    state.yaw -= mouse_delta.0 * MOUSE_SENSITIVITY;
    state.pitch -= mouse_delta.1 * MOUSE_SENSITIVITY;
    state.pitch = state.pitch.clamp(-1.35, 1.25);

    let view_rotation = Quat::from_rotation_y(state.yaw) * Quat::from_rotation_x(state.pitch);
    let flat_rotation = Quat::from_rotation_y(state.yaw);
    let forward = flat_rotation * Vec3::NEG_Z;
    let right = flat_rotation * Vec3::X;

    let mut movement = Vec3::ZERO;
    if move_forward {
        movement += forward;
    }
    if move_back {
        movement -= forward;
    }
    if move_right {
        movement += right;
    }
    if move_left {
        movement -= right;
    }
    if movement.length_squared() > f32::EPSILON {
        movement = movement.normalize() * PLAYER_SPEED;
    }

    let Some(mut controller) = world.remove::<CharacterControllerComponent>(state.player_entity)
    else {
        return;
    };

    if controller.grounded && state.vertical_velocity < 0.0 {
        state.vertical_velocity = -0.6;
    }
    if jump && !state.jump_held && controller.grounded {
        state.vertical_velocity = PLAYER_JUMP_SPEED;
    }
    state.jump_held = jump;
    state.vertical_velocity += GRAVITY * dt;

    let player_position = world
        .get::<TransformComponent>(state.player_entity)
        .map(|transform| transform.transform.position)
        .unwrap_or(Vec3::new(0.0, 1.0, 8.0));
    let desired_velocity = movement + Vec3::Y * state.vertical_velocity;
    let displacement = controller.move_and_slide(physics, player_position, desired_velocity, dt);

    let mut new_player_position = player_position + displacement;
    new_player_position.x = new_player_position
        .x
        .clamp(-ARENA_HALF_SIZE + 1.0, ARENA_HALF_SIZE - 1.0);
    new_player_position.z = new_player_position
        .z
        .clamp(-ARENA_HALF_SIZE + 1.0, ARENA_HALF_SIZE - 1.0);

    if controller.grounded && state.vertical_velocity < 0.0 {
        state.vertical_velocity = -0.6;
    }

    if let Some(transform) = world.get_mut::<TransformComponent>(state.player_entity) {
        transform.transform_mut().position = new_player_position;
    }
    world.entity_mut(state.player_entity).insert(controller);

    let camera_position = new_player_position + Vec3::Y * PLAYER_EYE_HEIGHT;
    let camera_forward = view_rotation * Vec3::NEG_Z;
    if let Some(camera) = world.get_mut::<CameraComponent>(state.camera_entity) {
        camera.0.position = camera_position;
        camera.0.target = camera_position + camera_forward;
        camera.0.up = view_rotation * Vec3::Y;
    }

    update_gun_transform(world, state, camera_position, view_rotation);
}

fn update_gun(
    world: &mut World,
    physics: &mut PhysicsWorld,
    state: &mut ShooterState,
    dt: f32,
    reload_pressed: bool,
) {
    state.fire_timer = (state.fire_timer - dt).max(0.0);
    state.muzzle_flash_timer = (state.muzzle_flash_timer - dt).max(0.0);

    if (reload_pressed || (state.ammo == 0 && state.reserve_ammo > 0)) && reload_weapon(state) {
        play_reload_sound(world);
    }

    let firing = world.resource::<MouseInput>().left_pressed;
    if !firing || state.fire_timer > 0.0 || state.ammo == 0 {
        update_gun_tint(world, state);
        return;
    }

    state.fire_timer = 0.16;
    state.muzzle_flash_timer = 0.06;
    state.ammo = state.ammo.saturating_sub(1);
    play_shot_sound(world);

    let Some(camera) = world.get::<CameraComponent>(state.camera_entity).copied() else {
        return;
    };

    let shot_origin = camera.0.position;
    let shot_direction = camera.0.forward();
    let max_distance = 80.0;

    if let Some(hit) =
        find_zombie_shot_hit(world, physics, shot_origin, shot_direction, max_distance)
    {
        spawn_impact_marker(world, hit.point + hit.normal * 0.03);
        if let Some(ratio) =
            damage_zombie(world, physics, state, hit.entity, shot_direction, hit.zone)
        {
            play_hit_sound(world);
            if let Some(sprite) = world.get_mut::<SpriteBillboard>(hit.entity) {
                sprite.tint = [0.72 - ratio * 0.4, 0.16 + ratio * 0.56, 0.12, 1.0];
            }
        }
    } else if let Some(hit) = physics.raycast(shot_origin, shot_direction, max_distance) {
        spawn_impact_marker(world, hit.point + hit.normal * 0.03);
        if let Some(ratio) = damage_zombie(
            world,
            physics,
            state,
            hit.entity,
            shot_direction,
            ZombieHitZone::Body,
        ) {
            play_hit_sound(world);
            if let Some(sprite) = world.get_mut::<SpriteBillboard>(hit.entity) {
                sprite.tint = [0.72 - ratio * 0.4, 0.16 + ratio * 0.56, 0.12, 1.0];
            }
        }
    }

    update_gun_tint(world, state);
}

fn damage_zombie(
    world: &mut World,
    physics: &mut PhysicsWorld,
    state: &mut ShooterState,
    entity: Entity,
    direction: Vec3,
    zone: ZombieHitZone,
) -> Option<f32> {
    let damage = match zone {
        ZombieHitZone::Body => 34.0,
        ZombieHitZone::Head => 68.0,
    };

    let health_ratio = {
        let health = world.get_mut::<Health>(entity)?;
        health.damage(damage);
        let ratio = (health.current / health.max).clamp(0.0, 1.0);
        if health.is_dead() {
            state.kills = state.kills.saturating_add(1);
        }
        ratio
    };

    if let Some(body) = world
        .get::<RigidBodyComponent>(entity)
        .and_then(|body| body.handle)
        .and_then(|handle| physics.body_mut(handle))
    {
        let impulse = if zone == ZombieHitZone::Head {
            28.0
        } else {
            20.0
        };
        body.apply_impulse(direction.normalize_or_zero() * impulse);
    }

    Some(health_ratio)
}

fn find_zombie_shot_hit(
    world: &mut World,
    physics: &PhysicsWorld,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<ZombieShotHit> {
    let direction = direction.normalize_or_zero();
    if direction.length_squared() <= f32::EPSILON {
        return None;
    }

    let blocker_distance = physics
        .raycast_all(origin, direction, max_distance)
        .into_iter()
        .filter(|hit| world.get::<Zombie>(hit.entity).is_none())
        .map(|hit| hit.distance)
        .fold(max_distance, f32::min);

    let zombie_entities: Vec<Entity> = {
        let mut query = world.query::<(Entity, &Zombie)>();
        query.iter(world).map(|(entity, _)| entity).collect()
    };

    let mut closest: Option<ZombieShotHit> = None;
    let mut closest_distance = blocker_distance;

    for entity in zombie_entities {
        if world
            .get::<Health>(entity)
            .map(|health| health.is_dead())
            .unwrap_or(true)
        {
            continue;
        }

        let Some(transform) = world.get::<TransformComponent>(entity) else {
            continue;
        };
        let Some(sprite) = world.get::<SpriteBillboard>(entity) else {
            continue;
        };

        let scale = transform.transform.scale;
        let center = transform.transform.position;
        let visual_width = sprite.size.x * scale.x.abs().max(0.001);
        let visual_height = sprite.size.y * scale.y.abs().max(0.001);
        let body_center = center - Vec3::Y * visual_height * 0.05;
        let head_center = center + Vec3::Y * visual_height * 0.36;
        let body_radius = visual_width * 0.55;
        let head_radius = visual_width * 0.36;

        for (zone, hit_center, radius) in [
            (ZombieHitZone::Head, head_center, head_radius),
            (ZombieHitZone::Body, body_center, body_radius),
        ] {
            let Some((distance, point, normal)) =
                ray_sphere_hit(origin, direction, hit_center, radius, closest_distance)
            else {
                continue;
            };
            closest_distance = distance;
            closest = Some(ZombieShotHit {
                entity,
                point,
                normal,
                zone,
            });
        }
    }

    closest
}

fn ray_sphere_hit(
    origin: Vec3,
    direction: Vec3,
    center: Vec3,
    radius: f32,
    max_distance: f32,
) -> Option<(f32, Vec3, Vec3)> {
    let offset = origin - center;
    let half_b = offset.dot(direction);
    let c = offset.length_squared() - radius * radius;
    let discriminant = half_b * half_b - c;
    if discriminant < 0.0 {
        return None;
    }

    let root = discriminant.sqrt();
    let mut distance = -half_b - root;
    if distance < 0.0 {
        distance = -half_b + root;
    }
    if !(0.0..=max_distance).contains(&distance) {
        return None;
    }

    let point = origin + direction * distance;
    let normal = (point - center).normalize_or_zero();
    Some((distance, point, normal))
}

fn reload_weapon(state: &mut ShooterState) -> bool {
    if state.ammo >= state.max_ammo || state.reserve_ammo == 0 {
        return false;
    }

    let needed = state.max_ammo - state.ammo;
    let loaded = needed.min(state.reserve_ammo);
    state.ammo += loaded;
    state.reserve_ammo -= loaded;
    true
}

fn update_zombies(
    world: &mut World,
    physics: &mut PhysicsWorld,
    state: &mut ShooterState,
    dt: f32,
) {
    let player_position = world
        .get::<TransformComponent>(state.player_entity)
        .map(|transform| transform.transform.position)
        .unwrap_or(Vec3::ZERO);

    let zombie_entities: Vec<Entity> = {
        let mut query = world.query::<(Entity, &Zombie)>();
        query.iter(world).map(|(entity, _)| entity).collect()
    };

    for entity in zombie_entities {
        if world
            .get::<Health>(entity)
            .map(|health| health.is_dead())
            .unwrap_or(true)
        {
            continue;
        }

        let zombie_position = world
            .get::<TransformComponent>(entity)
            .map(|transform| transform.transform.position)
            .unwrap_or(Vec3::ZERO);
        let mut to_player = player_position - zombie_position;
        to_player.y = 0.0;
        let distance = to_player.length();

        let mut next_position = zombie_position;
        let mut did_attack = false;
        if let Some(zombie) = world.get_mut::<Zombie>(entity) {
            zombie.attack_timer = (zombie.attack_timer - dt).max(0.0);
            if distance <= zombie.attack_range {
                if zombie.attack_timer <= 0.0 {
                    state.player_health -= zombie.attack_damage;
                    zombie.attack_timer = zombie.attack_cooldown;
                    did_attack = true;
                }
            } else if distance > f32::EPSILON {
                next_position += to_player.normalize() * zombie.speed * dt;
            }
        }

        if did_attack {
            if let Some(sprite) = world.get_mut::<SpriteBillboard>(entity) {
                sprite.tint = [1.0, 0.16, 0.08, 1.0];
            }
        }

        if let Some(transform) = world.get_mut::<TransformComponent>(entity) {
            let mut edited = transform.transform;
            edited.position = next_position;
            if distance > 0.01 {
                edited.rotation = Quat::from_rotation_y(to_player.x.atan2(to_player.z));
            }
            transform.set_transform(edited);
        }

        if let Some(handle) = world
            .get::<RigidBodyComponent>(entity)
            .and_then(|body| body.handle)
        {
            physics.set_body_pose(handle, next_position, Quat::IDENTITY);
        }
    }
}

fn cleanup_dead_zombies(world: &mut World, physics: &mut PhysicsWorld) {
    let zombie_entities: Vec<Entity> = {
        let mut query = world.query::<(Entity, &Zombie)>();
        query.iter(world).map(|(entity, _)| entity).collect()
    };
    let dead: Vec<Entity> = zombie_entities
        .into_iter()
        .filter(|entity| {
            world
                .get::<Health>(*entity)
                .map(|health| health.is_dead())
                .unwrap_or(true)
        })
        .collect();

    for entity in dead {
        if let Some(handle) = world
            .get::<RigidBodyComponent>(entity)
            .and_then(|body| body.handle)
        {
            physics.remove_body(handle);
        }
        let _ = world.despawn(entity);
    }
}

fn update_impact_markers(world: &mut World, dt: f32) {
    let mut expired = Vec::new();
    let mut query = world.query::<(Entity, &mut ImpactMarker)>();
    for (entity, marker) in query.iter_mut(world) {
        marker.ttl -= dt;
        if marker.ttl <= 0.0 {
            expired.push(entity);
        }
    }

    for entity in expired {
        let _ = world.despawn(entity);
    }
}

fn update_game_ui(world: &mut World) {
    if !world.contains_resource::<GameUi>() || !world.contains_resource::<ShooterState>() {
        return;
    }

    let state = world.resource::<ShooterState>().clone();
    let zombie_count = active_zombie_count(world);
    let ui = world.resource_mut::<GameUi>();
    ui.clear();
    ui.set_distance(0.72);

    match state.mode {
        ShooterMode::StartMenu => {
            ui.panel(
                "start_menu_panel",
                GameUiAnchor::Center,
                [0.0, 0.0],
                [0.46, 0.34],
                [0.03, 0.04, 0.05, 1.0],
            );
            ui.bar(
                "start_menu_title_mark",
                GameUiAnchor::Center,
                [0.0, 0.09],
                [0.28, 0.035],
                1.0,
                [0.32, 0.72, 0.28, 1.0],
            );
            ui.text(
                "start_menu_title",
                GameUiAnchor::Center,
                [0.0, 0.13],
                "ZOMBIE SHOOTER",
                menu_title_style(),
            );
            ui.button(
                "start_menu_start",
                GameUiAnchor::Center,
                [0.0, -0.04],
                [0.24, 0.07],
                true,
            );
            ui.text(
                "start_menu_start_text",
                GameUiAnchor::Center,
                [0.0, -0.04],
                "ENTER / CLICK START",
                menu_button_style(),
            );
            ui.text(
                "start_menu_help",
                GameUiAnchor::Center,
                [0.0, -0.14],
                "WASD MOVE  MOUSE AIM  SPACE JUMP",
                menu_hint_style(),
            );
        }
        ShooterMode::Playing => {
            let health_ratio = (state.player_health / 100.0).clamp(0.0, 1.0);
            let reserve_ratio = (state.reserve_ammo as f32 / 48.0).clamp(0.0, 1.0);
            let wave_pressure =
                (zombie_count as f32 / (3 + state.wave.min(5)) as f32).clamp(0.0, 1.0);

            ui.reticle("reticle", 0.045, 0.004, [1.0, 0.95, 0.62, 1.0]);
            draw_weapon_sprite(ui, state.muzzle_flash_timer > 0.0);
            ui.text(
                "health_label",
                GameUiAnchor::TopLeft,
                [0.05, -0.04],
                format!("HEALTH {:.0}", state.player_health),
                hud_left_style(),
            );
            ui.bar(
                "health_bar",
                GameUiAnchor::TopLeft,
                [0.18, -0.08],
                [0.26, 0.035],
                health_ratio,
                [0.72, 0.18, 0.14, 1.0],
            );
            ui.text(
                "wave_label",
                GameUiAnchor::TopRight,
                [-0.05, -0.04],
                format!("WAVE {}  LEFT {}", state.wave, zombie_count),
                hud_right_style(),
            );
            ui.counter(
                "ammo_counter",
                GameUiAnchor::BottomRight,
                [-0.26, 0.09],
                state.ammo,
                state.max_ammo,
                [0.95, 0.78, 0.22, 1.0],
            );
            ui.text(
                "ammo_label",
                GameUiAnchor::BottomRight,
                [-0.05, 0.13],
                format!("AMMO {}/{}", state.ammo, state.max_ammo),
                hud_right_style(),
            );
            ui.bar(
                "reserve_ammo_bar",
                GameUiAnchor::BottomRight,
                [-0.18, 0.05],
                [0.2, 0.018],
                reserve_ratio,
                [0.44, 0.58, 0.78, 1.0],
            );
            ui.text(
                "reserve_label",
                GameUiAnchor::BottomRight,
                [-0.05, 0.025],
                format!("RESERVE {}", state.reserve_ammo),
                hud_right_small_style(),
            );
            ui.bar(
                "wave_pressure_bar",
                GameUiAnchor::TopRight,
                [-0.18, -0.08],
                [0.24, 0.026],
                wave_pressure,
                [0.34, 0.72, 0.28, 1.0],
            );
        }
        ShooterMode::Paused => {
            ui.panel(
                "pause_menu_panel",
                GameUiAnchor::Center,
                [0.0, 0.0],
                [0.4, 0.28],
                [0.03, 0.04, 0.05, 1.0],
            );
            ui.button(
                "pause_resume",
                GameUiAnchor::Center,
                [0.0, 0.02],
                [0.22, 0.065],
                true,
            );
            ui.text(
                "pause_title",
                GameUiAnchor::Center,
                [0.0, 0.1],
                "PAUSED",
                menu_title_style(),
            );
            ui.text(
                "pause_resume_text",
                GameUiAnchor::Center,
                [0.0, 0.02],
                "ENTER / CLICK RESUME",
                menu_button_style(),
            );
            ui.button(
                "pause_hint",
                GameUiAnchor::Center,
                [0.0, -0.08],
                [0.16, 0.035],
                false,
            );
            ui.text(
                "pause_hint_text",
                GameUiAnchor::Center,
                [0.0, -0.08],
                "ESC ALSO RESUMES",
                menu_hint_style(),
            );
        }
        ShooterMode::GameOver => {
            ui.panel(
                "game_over_panel",
                GameUiAnchor::Center,
                [0.0, 0.0],
                [0.44, 0.3],
                [0.08, 0.02, 0.02, 1.0],
            );
            ui.bar(
                "game_over_mark",
                GameUiAnchor::Center,
                [0.0, 0.08],
                [0.28, 0.035],
                1.0,
                [0.78, 0.14, 0.1, 1.0],
            );
            ui.text(
                "game_over_title",
                GameUiAnchor::Center,
                [0.0, 0.12],
                "GAME OVER",
                GameTextStyle::new(0.052, [1.0, 0.7, 0.62, 1.0])
                    .with_font(TITLE_FONT)
                    .with_tracking(0.03),
            );
            ui.text(
                "game_over_stats",
                GameUiAnchor::Center,
                [0.0, 0.02],
                format!("KILLS {}  WAVE {}", state.kills, state.wave),
                menu_hint_style(),
            );
            ui.button(
                "game_over_restart",
                GameUiAnchor::Center,
                [0.0, -0.06],
                [0.24, 0.07],
                true,
            );
            ui.text(
                "game_over_restart_text",
                GameUiAnchor::Center,
                [0.0, -0.06],
                "R / ENTER RESTART",
                menu_button_style(),
            );
        }
    }
}

fn draw_weapon_sprite(ui: &mut GameUi, firing: bool) {
    let rows = weapon_ui_rows(firing);
    let width = rows[0].chars().count() as f32;
    let height = rows.len() as f32;
    let pixel = [0.01, 0.016];
    let center = [0.22, 0.19];

    for (row, pixels) in rows.iter().enumerate() {
        for (column, ch) in pixels.chars().enumerate() {
            let Some(color) = weapon_pixel_color(ch) else {
                continue;
            };
            let x = center[0] + (column as f32 + 0.5 - width * 0.5) * pixel[0];
            let y = center[1] + (height * 0.5 - row as f32 - 0.5) * pixel[1];
            ui.panel(
                format!("weapon_pixel_{row}_{column}"),
                GameUiAnchor::BottomCenter,
                [x, y],
                pixel,
                color,
            );
        }
    }
}

fn weapon_ui_rows(firing: bool) -> Vec<String> {
    let mut rows = vec![vec![' '; 34]; 24];

    weapon_span(&mut rows, 4, 12, 22, 'm');
    weapon_span(&mut rows, 5, 9, 25, 'M');
    weapon_span(&mut rows, 6, 6, 29, 'K');
    weapon_span(&mut rows, 7, 5, 32, 'K');
    weapon_span(&mut rows, 8, 8, 32, 'K');
    weapon_span(&mut rows, 9, 10, 32, 'K');
    weapon_span(&mut rows, 10, 13, 33, 'k');
    weapon_span(&mut rows, 11, 15, 33, 'k');
    weapon_span(&mut rows, 12, 17, 33, 'K');
    weapon_span(&mut rows, 13, 19, 33, 'K');
    weapon_span(&mut rows, 14, 20, 33, 'K');
    weapon_span(&mut rows, 15, 21, 33, 'K');
    weapon_span(&mut rows, 16, 22, 33, 'K');
    weapon_span(&mut rows, 17, 22, 31, 'K');
    weapon_span(&mut rows, 18, 21, 29, 'k');
    weapon_span(&mut rows, 19, 21, 28, 'k');
    weapon_span(&mut rows, 20, 22, 28, 'K');
    weapon_span(&mut rows, 21, 23, 28, 'K');
    weapon_span(&mut rows, 22, 24, 28, 'K');

    if firing {
        weapon_span(&mut rows, 6, 2, 5, 'F');
        weapon_span(&mut rows, 7, 1, 4, 'f');
        weapon_span(&mut rows, 8, 3, 6, 'F');
    }

    rows.into_iter()
        .map(|row| row.into_iter().collect())
        .collect()
}

fn weapon_span(rows: &mut [Vec<char>], row: usize, start: usize, end: usize, ch: char) {
    let Some(row) = rows.get_mut(row) else {
        return;
    };
    let width = row.len();
    for pixel in row.iter_mut().take(end.min(width)).skip(start.min(width)) {
        *pixel = ch;
    }
}

fn weapon_pixel_color(ch: char) -> Option<[f32; 4]> {
    match ch {
        'K' => Some([0.035, 0.04, 0.045, 1.0]),
        'k' => Some([0.02, 0.024, 0.028, 1.0]),
        'M' => Some([0.28, 0.29, 0.3, 1.0]),
        'm' => Some([0.43, 0.44, 0.45, 1.0]),
        'D' => Some([0.035, 0.04, 0.045, 1.0]),
        'd' => Some([0.16, 0.17, 0.18, 1.0]),
        'b' => Some([0.34, 0.35, 0.36, 1.0]),
        'F' => Some([1.0, 0.82, 0.16, 1.0]),
        'f' => Some([1.0, 0.35, 0.08, 1.0]),
        _ => None,
    }
}

fn menu_title_style() -> GameTextStyle {
    GameTextStyle::new(0.046, [0.93, 1.0, 0.78, 1.0])
        .with_font(TITLE_FONT)
        .with_tracking(0.03)
}

fn menu_button_style() -> GameTextStyle {
    GameTextStyle::new(0.024, [0.05, 0.05, 0.04, 1.0])
        .with_font(HUD_FONT)
        .with_tracking(0.02)
        .with_line_height(1.0)
}

fn menu_hint_style() -> GameTextStyle {
    GameTextStyle::new(0.021, [0.76, 0.82, 0.76, 1.0])
        .with_font(HUD_FONT)
        .with_tracking(0.02)
        .with_wrap_width(0.36)
}

fn hud_left_style() -> GameTextStyle {
    GameTextStyle::new(0.022, [0.9, 0.94, 0.86, 1.0])
        .with_font(HUD_FONT)
        .with_horizontal_align(TextHorizontalAlign::Left)
        .with_vertical_align(TextVerticalAlign::Top)
}

fn hud_right_style() -> GameTextStyle {
    GameTextStyle::new(0.022, [0.9, 0.94, 0.86, 1.0])
        .with_font(HUD_FONT)
        .with_horizontal_align(TextHorizontalAlign::Right)
        .with_vertical_align(TextVerticalAlign::Top)
}

fn hud_right_small_style() -> GameTextStyle {
    GameTextStyle::new(0.018, [0.72, 0.8, 0.86, 1.0])
        .with_font(HUD_FONT)
        .with_horizontal_align(TextHorizontalAlign::Right)
        .with_vertical_align(TextVerticalAlign::Top)
}

fn reset_game(world: &mut World, state: &mut ShooterState, physics: &mut PhysicsWorld) {
    let zombies: Vec<Entity> = {
        let mut query = world.query::<(Entity, &Zombie)>();
        query.iter(world).map(|(entity, _)| entity).collect()
    };
    for entity in zombies {
        if let Some(handle) = world
            .get::<RigidBodyComponent>(entity)
            .and_then(|body| body.handle)
        {
            physics.remove_body(handle);
        }
        let _ = world.despawn(entity);
    }

    let impacts: Vec<Entity> = {
        let mut query = world.query::<(Entity, &ImpactMarker)>();
        query.iter(world).map(|(entity, _)| entity).collect()
    };
    for entity in impacts {
        let _ = world.despawn(entity);
    }

    if let Some(transform) = world.get_mut::<TransformComponent>(state.player_entity) {
        transform.set_transform(Transform::from_position(Vec3::new(0.0, 1.0, 8.0)));
    }

    state.yaw = 0.0;
    state.pitch = 0.0;
    state.vertical_velocity = 0.0;
    state.jump_held = false;
    state.fire_timer = 0.0;
    state.muzzle_flash_timer = 0.0;
    state.ammo = state.max_ammo;
    state.reserve_ammo = 5s;
    state.player_health = 100.0;
    state.kills = 0;
    state.wave = 1;
    spawn_wave(world, state.wave);
}

fn spawn_lighting(world: &mut World) {
    world.spawn((
        Name("Ambient Light".to_string()),
        AmbientLight::new(Vec3::new(0.42, 0.48, 0.56), 0.35),
    ));
    world.spawn((
        Name("Moon Key Light".to_string()),
        DirectionalLight::new(
            Vec3::new(-0.4, -1.0, -0.25),
            Vec3::new(0.82, 0.9, 1.0),
            0.95,
        ),
    ));
}

fn spawn_arena(world: &mut World) {
    let descriptor = zombie_world_descriptor();
    let spawned = spawn_world_descriptor(world, &descriptor);

    if let Some(terrain) = spawned.terrain {
        world.entity_mut(terrain).insert((
            RigidBodyComponent::static_body(),
            ColliderComponent::cuboid(Vec3::new(ARENA_HALF_SIZE, 0.1, ARENA_HALF_SIZE)),
            CollisionLayers::in_layer(collision_layer::STATIC),
        ));
    }

    for (object, entity) in descriptor
        .objects
        .iter()
        .zip(spawned.objects.iter().copied())
    {
        if object.solid {
            let size = Vec3::from_array(object.size);
            world.entity_mut(entity).insert((
                RigidBodyComponent::static_body(),
                ColliderComponent::cuboid(size * 0.5),
                CollisionLayers::in_layer(collision_layer::STATIC),
            ));
        }
    }
}

fn zombie_world_descriptor() -> SceneWorldDescriptor {
    let wall = [0.22, 0.27, 0.32, 1.0];
    let cover = [0.38, 0.33, 0.27, 1.0];
    SceneWorldDescriptor {
        terrain: TerrainDescriptor {
            width: ARENA_HALF_SIZE * 2.0,
            depth: ARENA_HALF_SIZE * 2.0,
            columns: 56,
            rows: 56,
            height_scale: 1.0,
            tint: [0.34, 0.4, 0.34, 1.0],
            waves: vec![
                TerrainWaveDescriptor {
                    direction: [1.0, 0.25],
                    frequency: 5.0,
                    amplitude: 0.05,
                    phase: 0.2,
                },
                TerrainWaveDescriptor {
                    direction: [-0.2, 1.0],
                    frequency: 8.0,
                    amplitude: 0.025,
                    phase: 1.4,
                },
            ],
        },
        objects: vec![
            world_object(
                "North Wall",
                [0.0, 2.0, -ARENA_HALF_SIZE],
                [ARENA_HALF_SIZE * 2.0, 4.0, 0.5],
                wall,
            ),
            world_object(
                "South Wall",
                [0.0, 2.0, ARENA_HALF_SIZE],
                [ARENA_HALF_SIZE * 2.0, 4.0, 0.5],
                wall,
            ),
            world_object(
                "West Wall",
                [-ARENA_HALF_SIZE, 2.0, 0.0],
                [0.5, 4.0, ARENA_HALF_SIZE * 2.0],
                wall,
            ),
            world_object(
                "East Wall",
                [ARENA_HALF_SIZE, 2.0, 0.0],
                [0.5, 4.0, ARENA_HALF_SIZE * 2.0],
                wall,
            ),
            world_object("Cover 1", [-8.0, 0.6, -6.0], [2.5, 1.2, 2.5], cover),
            world_object("Cover 2", [7.0, 0.6, -4.0], [2.5, 1.2, 2.5], cover),
            world_object("Cover 3", [-5.0, 0.6, 7.0], [2.5, 1.2, 2.5], cover),
            world_object("Cover 4", [10.0, 0.6, 8.0], [2.5, 1.2, 2.5], cover),
        ],
    }
}

fn world_object(
    name: &str,
    position: [f32; 3],
    size: [f32; 3],
    tint: [f32; 4],
) -> WorldObjectDescriptor {
    WorldObjectDescriptor {
        name: name.to_string(),
        position,
        size,
        tint,
        solid: true,
    }
}

fn spawn_player(world: &mut World) -> Entity {
    world
        .spawn((
            Name("Player".to_string()),
            TransformComponent::from_position(Vec3::new(0.0, 1.0, 8.0)),
            GlobalTransform::default(),
            CharacterControllerComponent::new(0.35, 0.85).with_step_offset(0.45),
        ))
        .id()
}

fn spawn_camera(world: &mut World) -> Entity {
    let mut camera = CameraComponent::new();
    camera.0.position = Vec3::new(0.0, 2.15, 8.0);
    camera.0.target = Vec3::new(0.0, 2.15, 7.0);
    camera.0.fov = 70.0_f32.to_radians();
    world
        .spawn((
            Name("Player Camera".to_string()),
            TransformComponent::from_position(camera.0.position),
            GlobalTransform::default(),
            camera,
        ))
        .id()
}

fn spawn_gun(world: &mut World) -> Entity {
    world
        .spawn((
            Name("Gun".to_string()),
            TransformComponent::new(Transform {
                position: Vec3::new(0.25, 1.8, 7.4),
                scale: Vec3::ONE,
                ..Default::default()
            }),
            GlobalTransform::default(),
            SpriteBillboard::new(GUN_SPRITE, Vec2::new(0.52, 0.3))
                .with_facing(SpriteFacing::Camera)
                .with_depth(SpriteDepthMode::World),
        ))
        .id()
}

fn update_gun_transform(
    world: &mut World,
    state: &ShooterState,
    camera_position: Vec3,
    view_rotation: Quat,
) {
    let recoil = if state.muzzle_flash_timer > 0.0 {
        -0.035
    } else {
        0.0
    };
    let forward = view_rotation * Vec3::NEG_Z;
    let right = view_rotation * Vec3::X;
    let up = view_rotation * Vec3::Y;
    let gun_position = camera_position + forward * 0.95 + right * 0.34 - up * (0.28 - recoil);

    if let Some(transform) = world.get_mut::<TransformComponent>(state.gun_entity) {
        transform.set_transform(Transform {
            position: gun_position,
            rotation: view_rotation,
            scale: Vec3::ONE,
        });
    }
}

fn update_gun_tint(world: &mut World, state: &ShooterState) {
    if let Some(sprite) = world.get_mut::<SpriteBillboard>(state.gun_entity) {
        if state.muzzle_flash_timer > 0.0 {
            sprite.sprite = SpriteId::from(GUN_FIRE_SPRITE);
            sprite.tint = [1.0, 0.95, 0.74, 1.0];
        } else {
            sprite.sprite = SpriteId::from(GUN_SPRITE);
            sprite.tint = [1.0, 1.0, 1.0, 1.0];
        };
    }
}

fn spawn_wave(world: &mut World, wave: u32) {
    let count = 3 + wave.min(5);
    for i in 0..count {
        let angle = (i as f32 / count as f32) * std::f32::consts::TAU + wave as f32 * 0.37;
        let radius = 12.0 + (i % 3) as f32 * 3.0;
        let position = Vec3::new(angle.cos() * radius, 0.55, angle.sin() * radius);
        spawn_zombie(world, position, wave);
    }
    tracing::info!("Spawned wave {wave} with {count} zombies.");
}

fn spawn_zombie(world: &mut World, position: Vec3, wave: u32) -> Entity {
    let entity = world
        .spawn((
            Name(format!("Zombie Wave {wave}")),
            TransformComponent::new(Transform {
                position,
                scale: Vec3::new(0.9, 1.7, 0.9),
                ..Default::default()
            }),
            GlobalTransform::default(),
            SpriteBillboard::new(ZOMBIE_SPRITE, Vec2::new(1.25, 1.85))
                .with_tint([0.9, 1.0, 0.86, 1.0])
                .with_facing(SpriteFacing::YBillboard)
                .with_depth(SpriteDepthMode::World),
        ))
        .id();
    world.entity_mut(entity).insert((
        RigidBodyComponent::kinematic_position_based(),
        ColliderComponent::sphere(0.55),
        CollisionLayers::in_layer(collision_layer::ENEMY),
        Health::new(100.0),
        Zombie::basic(wave),
    ));
    entity
}

fn spawn_impact_marker(world: &mut World, position: Vec3) {
    world.spawn((
        Name("Impact".to_string()),
        TransformComponent::new(Transform {
            position,
            scale: Vec3::splat(0.13),
            ..Default::default()
        }),
        GlobalTransform::default(),
        RenderMesh::new(
            MeshPrimitive::Sphere {
                segments: 8,
                rings: 8,
            },
            RenderMaterial::default(),
        )
        .with_tint([1.0, 0.84, 0.18, 1.0]),
        ImpactMarker { ttl: 0.18 },
    ));
}

fn active_zombie_count(world: &mut World) -> usize {
    let mut query = world.query::<&Zombie>();
    query.iter(world).count()
}

fn main() {
    tracing_subscriber::fmt::init();
    app::<ZombieShooter>()
        .add_plugins(DefaultPlugins)
        .add_plugins(SceneAuthoringPlugins)
        .add_plugin(AudioPlugin)
        .add_plugin(PhysicsPlugin)
        .run();
}
