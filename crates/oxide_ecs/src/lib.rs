#![allow(clippy::type_complexity)]

extern crate self as oxide_ecs;

pub use oxide_ecs_derive::{Component, Resource, ScheduleLabel};

pub mod component {
    pub trait Component: 'static {}
}

pub mod resource {
    pub trait Resource: 'static {}
}

pub mod entity {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct Entity {
        pub(crate) index: u32,
        pub(crate) generation: u32,
    }

    impl Entity {
        pub fn index(self) -> u32 {
            self.index
        }

        pub fn generation(self) -> u32 {
            self.generation
        }
    }
}

pub mod query {
    use std::marker::PhantomData;

    pub struct With<T>(pub(crate) PhantomData<T>);
    pub struct Without<T>(pub(crate) PhantomData<T>);

    impl<T> Default for With<T> {
        fn default() -> Self {
            Self(PhantomData)
        }
    }

    impl<T> Default for Without<T> {
        fn default() -> Self {
            Self(PhantomData)
        }
    }
}

pub mod schedule {
    use super::world::World;

    pub trait ScheduleLabel: 'static {}

    pub type SystemFn = fn(&mut World);

    #[derive(Default)]
    pub struct Schedule {
        systems: Vec<SystemFn>,
    }

    impl Schedule {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn add_system(&mut self, system: SystemFn) {
            self.systems.push(system);
        }

        pub fn run(&mut self, world: &mut World) {
            for system in &self.systems {
                system(world);
            }
        }
    }
}

pub mod system {
    use std::ops::{Deref, DerefMut};

    use crate::entity::Entity;
    use crate::world::{EntityMut, World};

    pub trait SystemParam {}

    pub struct Commands<'w> {
        world: &'w mut World,
    }

    impl<'w> Commands<'w> {
        pub fn new(world: &'w mut World) -> Self {
            Self { world }
        }

        pub fn spawn<B>(&mut self, bundle: B) -> EntityMut<'_>
        where
            B: crate::world::Bundle,
        {
            self.world.spawn(bundle)
        }

        pub fn entity(&mut self, entity: Entity) -> EntityMut<'_> {
            self.world.entity_mut(entity)
        }
    }

    pub struct Res<'w, T>(pub &'w T);

    impl<'w, T> Deref for Res<'w, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            self.0
        }
    }

    pub struct ResMut<'w, T>(pub &'w mut T);

    impl<'w, T> Deref for ResMut<'w, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            self.0
        }
    }

    impl<'w, T> DerefMut for ResMut<'w, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.0
        }
    }

    pub struct Query<T>(pub std::marker::PhantomData<T>);
}

pub mod world {
    use super::component::Component;
    use super::entity::Entity;
    use super::query::{With, Without};
    use std::any::{Any, TypeId};
    use std::collections::HashMap;
    use std::marker::PhantomData;

    trait StorageDyn: Any {
        fn remove_entity(&mut self, entity: Entity);
        fn as_any(&self) -> &dyn Any;
        fn as_any_mut(&mut self) -> &mut dyn Any;
    }

    struct Storage<T: Component> {
        sparse: Vec<Option<usize>>,
        dense_entities: Vec<Entity>,
        dense_data: Vec<T>,
    }

    impl<T: Component> Default for Storage<T> {
        fn default() -> Self {
            Self {
                sparse: Vec::new(),
                dense_entities: Vec::new(),
                dense_data: Vec::new(),
            }
        }
    }

    impl<T: Component> Storage<T> {
        fn ensure_sparse_capacity(&mut self, entity: Entity) {
            let index = entity.index as usize;
            if self.sparse.len() <= index {
                self.sparse.resize(index + 1, None);
            }
        }

        fn insert(&mut self, entity: Entity, component: T) {
            self.ensure_sparse_capacity(entity);
            let index = entity.index as usize;

            if let Some(dense_index) = self.sparse[index] {
                if self.dense_entities.get(dense_index).copied() == Some(entity) {
                    self.dense_data[dense_index] = component;
                    return;
                }
            }

            let dense_index = self.dense_data.len();
            self.dense_entities.push(entity);
            self.dense_data.push(component);
            self.sparse[index] = Some(dense_index);
        }

        fn get(&self, entity: Entity) -> Option<&T> {
            let dense_index = self.sparse.get(entity.index as usize).copied().flatten()?;
            if self.dense_entities.get(dense_index).copied() == Some(entity) {
                self.dense_data.get(dense_index)
            } else {
                None
            }
        }

        fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
            let dense_index = self.sparse.get(entity.index as usize).copied().flatten()?;
            if self.dense_entities.get(dense_index).copied() == Some(entity) {
                self.dense_data.get_mut(dense_index)
            } else {
                None
            }
        }

        fn get_mut_ptr(&mut self, entity: Entity) -> Option<*mut T> {
            self.get_mut(entity).map(|value| value as *mut T)
        }

        fn remove(&mut self, entity: Entity) -> Option<T> {
            let index = entity.index as usize;
            let dense_index = self.sparse.get(index).copied().flatten()?;

            if self.dense_entities.get(dense_index).copied() != Some(entity) {
                return None;
            }

            self.sparse[index] = None;

            let removed_entity = self.dense_entities.swap_remove(dense_index);
            let removed_component = self.dense_data.swap_remove(dense_index);

            debug_assert_eq!(removed_entity, entity);

            if dense_index < self.dense_entities.len() {
                let moved_entity = self.dense_entities[dense_index];
                self.sparse[moved_entity.index as usize] = Some(dense_index);
            }

            Some(removed_component)
        }

        fn entities(&self) -> &[Entity] {
            &self.dense_entities
        }

        fn values(&self) -> impl Iterator<Item = &T> {
            self.dense_data.iter()
        }

        fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
            self.dense_data.iter_mut()
        }

        fn contains_entity(&self, entity: Entity) -> bool {
            self.get(entity).is_some()
        }
    }

    impl<T: Component> StorageDyn for Storage<T> {
        fn remove_entity(&mut self, entity: Entity) {
            self.remove(entity);
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    pub trait Bundle {
        fn insert_into(self, world: &mut World, entity: Entity);
    }

    impl<C: Component> Bundle for C {
        fn insert_into(self, world: &mut World, entity: Entity) {
            world.insert_component(entity, self);
        }
    }

    macro_rules! impl_bundle_tuple {
        ($($name:ident),+) => {
            impl<$($name: Component),+> Bundle for ($($name,)+) {
                #[allow(non_snake_case)]
                fn insert_into(self, world: &mut World, entity: Entity) {
                    let ($($name,)+) = self;
                    $(world.insert_component(entity, $name);)+
                }
            }
        };
    }

    impl_bundle_tuple!(A, B);
    impl_bundle_tuple!(A, B, C);
    impl_bundle_tuple!(A, B, C, D);
    impl_bundle_tuple!(A, B, C, D, E);

    pub struct World {
        next_index: u32,
        generations: Vec<u32>,
        free_indices: Vec<u32>,
        storages: HashMap<TypeId, Box<dyn StorageDyn>>,
        resources: HashMap<TypeId, Box<dyn Any>>,
        non_send_resources: HashMap<TypeId, Box<dyn Any>>,
    }

    impl Default for World {
        fn default() -> Self {
            Self::new()
        }
    }

    impl World {
        pub fn new() -> Self {
            Self {
                next_index: 0,
                generations: Vec::new(),
                free_indices: Vec::new(),
                storages: HashMap::new(),
                resources: HashMap::new(),
                non_send_resources: HashMap::new(),
            }
        }

        pub fn spawn<B: Bundle>(&mut self, bundle: B) -> EntityMut<'_> {
            let entity = self.alloc_entity();
            bundle.insert_into(self, entity);
            EntityMut {
                world: self,
                entity,
            }
        }

        pub fn entity_mut(&mut self, entity: Entity) -> EntityMut<'_> {
            assert!(self.contains(entity), "entity {:?} is not alive", entity);
            EntityMut {
                world: self,
                entity,
            }
        }

        pub fn contains(&self, entity: Entity) -> bool {
            self.generations
                .get(entity.index as usize)
                .map(|g| *g == entity.generation)
                .unwrap_or(false)
        }

        pub fn despawn(&mut self, entity: Entity) -> bool {
            if !self.contains(entity) {
                return false;
            }
            for storage in self.storages.values_mut() {
                storage.remove_entity(entity);
            }
            if let Some(generation) = self.generations.get_mut(entity.index as usize) {
                *generation = generation.saturating_add(1);
            }
            self.free_indices.push(entity.index);
            true
        }

        pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
            if !self.contains(entity) {
                return None;
            }
            self.storage::<T>()?.get(entity)
        }

        pub fn get_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
            if !self.contains(entity) {
                return None;
            }
            self.storage_mut::<T>()?.get_mut(entity)
        }

        pub fn remove<T: Component>(&mut self, entity: Entity) -> Option<T> {
            if !self.contains(entity) {
                return None;
            }
            self.storage_mut::<T>()?.remove(entity)
        }

        pub fn insert_resource<T: 'static>(&mut self, value: T) {
            self.resources.insert(TypeId::of::<T>(), Box::new(value));
        }

        pub fn resource<T: 'static>(&self) -> &T {
            self.resources
                .get(&TypeId::of::<T>())
                .and_then(|boxed| boxed.downcast_ref::<T>())
                .expect("resource not found")
        }

        pub fn resource_mut<T: 'static>(&mut self) -> &mut T {
            self.resources
                .get_mut(&TypeId::of::<T>())
                .and_then(|boxed| boxed.downcast_mut::<T>())
                .expect("resource not found")
        }

        pub fn init_resource<T: Default + 'static>(&mut self) {
            if !self.resources.contains_key(&TypeId::of::<T>()) {
                self.insert_resource(T::default());
            }
        }

        pub fn insert_non_send_resource<T: 'static>(&mut self, value: T) {
            self.non_send_resources
                .insert(TypeId::of::<T>(), Box::new(value));
        }

        pub fn get_non_send_resource<T: 'static>(&self) -> Option<&T> {
            self.non_send_resources
                .get(&TypeId::of::<T>())
                .and_then(|boxed| boxed.downcast_ref::<T>())
        }

        pub fn get_non_send_resource_mut<T: 'static>(&mut self) -> Option<&mut T> {
            self.non_send_resources
                .get_mut(&TypeId::of::<T>())
                .and_then(|boxed| boxed.downcast_mut::<T>())
        }

        pub fn query<Q>(&mut self) -> QueryState<Q> {
            QueryState {
                marker: PhantomData,
            }
        }

        pub fn query_filtered<Q, F>(&mut self) -> FilteredQueryState<Q, F> {
            FilteredQueryState {
                marker: PhantomData,
            }
        }

        fn alloc_entity(&mut self) -> Entity {
            if let Some(index) = self.free_indices.pop() {
                let generation = self.generations[index as usize];
                return Entity { index, generation };
            }

            let index = self.next_index;
            self.next_index += 1;
            self.generations.push(0);
            Entity {
                index,
                generation: 0,
            }
        }

        fn insert_component<T: Component>(&mut self, entity: Entity, component: T) {
            assert!(self.contains(entity), "entity {:?} is not alive", entity);
            self.ensure_storage::<T>().insert(entity, component);
        }

        fn ensure_storage<T: Component>(&mut self) -> &mut Storage<T> {
            let type_id = TypeId::of::<T>();
            self.storages
                .entry(type_id)
                .or_insert_with(|| Box::new(Storage::<T>::default()));
            self.storage_mut::<T>().expect("storage created")
        }

        fn storage<T: Component>(&self) -> Option<&Storage<T>> {
            self.storages
                .get(&TypeId::of::<T>())
                .and_then(|storage| storage.as_any().downcast_ref::<Storage<T>>())
        }

        fn storage_mut<T: Component>(&mut self) -> Option<&mut Storage<T>> {
            self.storages
                .get_mut(&TypeId::of::<T>())
                .and_then(|storage| storage.as_any_mut().downcast_mut::<Storage<T>>())
        }
    }

    pub struct EntityMut<'w> {
        world: &'w mut World,
        entity: Entity,
    }

    impl<'w> EntityMut<'w> {
        pub fn id(&self) -> Entity {
            self.entity
        }

        pub fn insert<B: Bundle>(&mut self, bundle: B) -> &mut Self {
            bundle.insert_into(self.world, self.entity);
            self
        }

        pub fn get<T: Component>(&self) -> Option<&T> {
            self.world.get::<T>(self.entity)
        }

        pub fn get_mut<T: Component>(&mut self) -> Option<&mut T> {
            self.world.get_mut::<T>(self.entity)
        }

        pub fn remove<T: Component>(&mut self) -> Option<T> {
            self.world.remove::<T>(self.entity)
        }
    }

    pub struct QueryState<Q> {
        marker: PhantomData<Q>,
    }

    impl<T: Component> QueryState<&T> {
        pub fn iter<'w>(&'w mut self, world: &'w World) -> impl Iterator<Item = &'w T> {
            world
                .storage::<T>()
                .map(|storage| storage.values())
                .into_iter()
                .flatten()
        }
    }

    impl<T: Component> QueryState<&mut T> {
        pub fn iter_mut<'w>(&'w mut self, world: &'w mut World) -> impl Iterator<Item = &'w mut T> {
            world
                .storage_mut::<T>()
                .map(|storage| storage.values_mut())
                .into_iter()
                .flatten()
        }
    }

    pub struct TupleIter2<'w, A: Component, B: Component> {
        entities: Vec<Entity>,
        index: usize,
        a: Option<&'w Storage<A>>,
        b: Option<&'w Storage<B>>,
    }

    impl<'w, A: Component, B: Component> Iterator for TupleIter2<'w, A, B> {
        type Item = (&'w A, &'w B);

        fn next(&mut self) -> Option<Self::Item> {
            let (Some(a_storage), Some(b_storage)) = (self.a, self.b) else {
                return None;
            };

            while self.index < self.entities.len() {
                let entity = self.entities[self.index];
                self.index += 1;
                if let (Some(a), Some(b)) = (a_storage.get(entity), b_storage.get(entity)) {
                    return Some((a, b));
                }
            }
            None
        }
    }

    pub struct TupleIterMut2<'w, A: Component, B: Component> {
        entities: Vec<Entity>,
        index: usize,
        a: *mut Storage<A>,
        b: *mut Storage<B>,
        marker: PhantomData<&'w mut (A, B)>,
    }

    impl<'w, A: Component, B: Component> Iterator for TupleIterMut2<'w, A, B> {
        type Item = (&'w mut A, &'w mut B);

        fn next(&mut self) -> Option<Self::Item> {
            if self.a.is_null() || self.b.is_null() {
                return None;
            }

            while self.index < self.entities.len() {
                let entity = self.entities[self.index];
                self.index += 1;
                unsafe {
                    let a_storage = &mut *self.a;
                    let b_storage = &mut *self.b;
                    let a_ptr = match a_storage.get_mut_ptr(entity) {
                        Some(value) => value,
                        None => continue,
                    };
                    let b_ptr = match b_storage.get_mut_ptr(entity) {
                        Some(value) => value,
                        None => continue,
                    };
                    return Some((&mut *a_ptr, &mut *b_ptr));
                }
            }
            None
        }
    }

    impl<A: Component, B: Component> QueryState<(&A, &B)> {
        pub fn iter<'w>(&'w mut self, world: &'w World) -> TupleIter2<'w, A, B> {
            let a_storage = world.storage::<A>();
            let b_storage = world.storage::<B>();

            let entities = a_storage
                .map(|storage| storage.entities().to_vec())
                .unwrap_or_default();

            TupleIter2 {
                entities,
                index: 0,
                a: a_storage,
                b: b_storage,
            }
        }
    }

    impl<A: Component, B: Component> QueryState<(&mut A, &mut B)> {
        pub fn iter_mut<'w>(&'w mut self, world: &'w mut World) -> TupleIterMut2<'w, A, B> {
            assert_ne!(
                TypeId::of::<A>(),
                TypeId::of::<B>(),
                "duplicate mutable query type"
            );

            let a_type = TypeId::of::<A>();
            let b_type = TypeId::of::<B>();

            let a_storage = {
                let Some(a_dyn) = world.storages.get_mut(&a_type) else {
                    return TupleIterMut2 {
                        entities: Vec::new(),
                        index: 0,
                        a: std::ptr::null_mut(),
                        b: std::ptr::null_mut(),
                        marker: PhantomData,
                    };
                };

                a_dyn
                    .as_any_mut()
                    .downcast_mut::<Storage<A>>()
                    .expect("storage type mismatch") as *mut Storage<A>
            };

            let b_storage = {
                let Some(b_dyn) = world.storages.get_mut(&b_type) else {
                    return TupleIterMut2 {
                        entities: Vec::new(),
                        index: 0,
                        a: std::ptr::null_mut(),
                        b: std::ptr::null_mut(),
                        marker: PhantomData,
                    };
                };

                b_dyn
                    .as_any_mut()
                    .downcast_mut::<Storage<B>>()
                    .expect("storage type mismatch") as *mut Storage<B>
            };

            if a_storage.is_null() || b_storage.is_null() {
                return TupleIterMut2 {
                    entities: Vec::new(),
                    index: 0,
                    a: std::ptr::null_mut(),
                    b: std::ptr::null_mut(),
                    marker: PhantomData,
                };
            }

            let entities = unsafe { (&*a_storage).entities().to_vec() };

            TupleIterMut2 {
                entities,
                index: 0,
                a: a_storage,
                b: b_storage,
                marker: PhantomData,
            }
        }
    }

    pub struct FilteredQueryState<Q, F> {
        marker: PhantomData<(Q, F)>,
    }

    pub struct EntityWithWithoutIter {
        entities: Vec<Entity>,
        index: usize,
    }

    impl Iterator for EntityWithWithoutIter {
        type Item = Entity;

        fn next(&mut self) -> Option<Self::Item> {
            if self.index >= self.entities.len() {
                return None;
            }
            let item = self.entities[self.index];
            self.index += 1;
            Some(item)
        }
    }

    impl<T: Component, U: Component> FilteredQueryState<Entity, (With<T>, Without<U>)> {
        pub fn iter<'w>(&'w mut self, world: &'w World) -> EntityWithWithoutIter {
            let mut entities = Vec::new();
            if let Some(with_storage) = world.storage::<T>() {
                for entity in with_storage.entities().iter().copied() {
                    let has_without = world
                        .storage::<U>()
                        .map(|storage| storage.contains_entity(entity))
                        .unwrap_or(false);
                    if !has_without {
                        entities.push(entity);
                    }
                }
            }
            EntityWithWithoutIter { entities, index: 0 }
        }
    }
}

pub mod prelude {
    pub use crate::component::Component;
    pub use crate::entity::Entity;
    pub use crate::query::{With, Without};
    pub use crate::resource::Resource;
    pub use crate::schedule::ScheduleLabel;
    pub use crate::system::{Commands, Query, Res, ResMut, SystemParam};
    pub use crate::world::World;
}

#[cfg(test)]
mod tests {
    use crate::query::{With, Without};
    use crate::world::World;
    use crate::{Component, Resource};

    #[derive(Component, Debug, PartialEq)]
    struct Position(i32);

    #[derive(Component, Debug, PartialEq)]
    struct Velocity(i32);

    #[derive(Resource, Default)]
    struct Tick(u64);

    #[test]
    fn spawn_insert_remove_and_despawn_work() {
        let mut world = World::new();
        let entity = world.spawn((Position(1), Velocity(2))).id();

        assert_eq!(world.get::<Position>(entity).map(|v| v.0), Some(1));
        assert_eq!(world.get::<Velocity>(entity).map(|v| v.0), Some(2));

        let removed = world.remove::<Velocity>(entity);
        assert_eq!(removed.map(|v| v.0), Some(2));
        assert!(world.get::<Velocity>(entity).is_none());

        assert!(world.despawn(entity));
        assert!(!world.contains(entity));
        assert!(world.get::<Position>(entity).is_none());
    }

    #[test]
    fn resources_and_non_send_resources_work() {
        let mut world = World::new();
        world.init_resource::<Tick>();
        world.resource_mut::<Tick>().0 = 7;
        assert_eq!(world.resource::<Tick>().0, 7);

        world.insert_non_send_resource(String::from("watcher"));
        assert_eq!(
            world.get_non_send_resource::<String>().map(String::as_str),
            Some("watcher")
        );
    }

    #[test]
    fn query_and_filtered_query_work() {
        let mut world = World::new();
        let a = world.spawn((Position(1), Velocity(10))).id();
        let b = world.spawn(Position(2)).id();

        {
            let mut query = world.query::<(&mut Position, &mut Velocity)>();
            for (position, velocity) in query.iter_mut(&mut world) {
                position.0 += velocity.0;
            }
        }

        assert_eq!(world.get::<Position>(a).map(|v| v.0), Some(11));
        assert_eq!(world.get::<Position>(b).map(|v| v.0), Some(2));

        let mut filtered =
            world.query_filtered::<crate::entity::Entity, (With<Position>, Without<Velocity>)>();
        let entities: Vec<_> = filtered.iter(&world).collect();
        assert_eq!(entities, vec![b]);
    }
}
