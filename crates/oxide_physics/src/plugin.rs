//! Physics plugin wiring for Oxide app stages.

use oxide_engine::prelude::{App, AppBuilder, AppStage, Plugin, Window, World};

use crate::resources::PhysicsWorld;
use crate::systems::{
    ensure_colliders_system, ensure_rigid_bodies_system, initialize_body_pose_system,
    physics_step_system, prune_orphan_bodies_system, prune_orphan_colliders_system,
    sync_transforms_system,
};

pub struct PhysicsPlugin;

impl<T: App> Plugin<T> for PhysicsPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_startup_system_mut(initialize_physics_world);
        app.add_system_mut(AppStage::Update, ensure_rigid_bodies_system);
        app.add_system_mut(AppStage::Update, initialize_body_pose_system);
        app.add_system_mut(AppStage::Update, ensure_colliders_system);
        app.add_system_mut(AppStage::Update, prune_orphan_bodies_system);
        app.add_system_mut(AppStage::Update, prune_orphan_colliders_system);
        app.add_system_mut(AppStage::Update, physics_step_system);
        app.add_system_mut(AppStage::Update, sync_transforms_system);
    }
}

fn initialize_physics_world(world: &mut World, _window: &Window) {
    if !world.contains_resource::<PhysicsWorld>() {
        world.insert_resource(PhysicsWorld::default());
    }
}
