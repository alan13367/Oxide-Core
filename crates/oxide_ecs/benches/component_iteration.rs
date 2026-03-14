use criterion::{black_box, criterion_group, criterion_main, Criterion};
use oxide_ecs::world::World;
use oxide_ecs::Component;

#[derive(Component, Clone, Copy)]
struct Position([f32; 3]);

#[derive(Component, Clone, Copy)]
struct Velocity([f32; 3]);

fn seeded_world(entity_count: usize) -> World {
    let mut world = World::new();
    for i in 0..entity_count {
        let f = i as f32;
        world.spawn((
            Position([f, f * 0.5, f * 2.0]),
            Velocity([1.0, 0.25, -0.75]),
        ));
    }
    world
}

fn bench_single_component_iteration(c: &mut Criterion) {
    let mut world = seeded_world(100_000);
    c.bench_function("single_component_iteration_100k", |b| {
        b.iter(|| {
            let mut query = world.query::<&Position>();
            let mut sum = 0.0f32;
            for position in query.iter(&world) {
                sum += position.0[0] + position.0[1] + position.0[2];
            }
            black_box(sum);
        });
    });
}

fn bench_tuple_component_iteration(c: &mut Criterion) {
    let mut world = seeded_world(100_000);
    c.bench_function("tuple_component_iteration_100k", |b| {
        b.iter(|| {
            let mut query = world.query::<(&Position, &Velocity)>();
            let mut sum = 0.0f32;
            for (position, velocity) in query.iter(&world) {
                sum += position.0[0] * velocity.0[0];
            }
            black_box(sum);
        });
    });
}

criterion_group!(
    ecs_iteration_baseline,
    bench_single_component_iteration,
    bench_tuple_component_iteration
);
criterion_main!(ecs_iteration_baseline);
