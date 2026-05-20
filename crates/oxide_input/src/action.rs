use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use oxide_ecs::resource::Resource;
use oxide_ecs::system::{Res, ResMut};
use winit::keyboard::KeyCode;

use crate::{ButtonState, KeyboardInput, MouseButton, MouseInput};

/// A physical input source that can drive a gameplay action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputTrigger {
    /// A physical keyboard key, independent of keyboard layout.
    Key(KeyCode),
    /// A mouse button.
    Mouse(MouseButton),
}

impl From<KeyCode> for InputTrigger {
    fn from(key: KeyCode) -> Self {
        Self::Key(key)
    }
}

impl From<MouseButton> for InputTrigger {
    fn from(button: MouseButton) -> Self {
        Self::Mouse(button)
    }
}

/// Bindings from game-defined actions to one or more physical inputs.
///
/// `A` is usually a small game enum such as `MoveForward`, `Jump`, or `Fire`.
/// Bindings are data-only; call [`sync_action_input_system`] each frame to
/// update the matching [`ActionInput`] resource from `KeyboardInput` and
/// `MouseInput`.
pub struct ActionBindings<A> {
    bindings: HashMap<A, Vec<InputTrigger>>,
}

impl<A: 'static> Resource for ActionBindings<A> {}

impl<A> Default for ActionBindings<A> {
    fn default() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }
}

impl<A> ActionBindings<A>
where
    A: Eq + Hash,
{
    /// Creates an empty action binding map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a keyboard trigger for an action.
    pub fn bind_key(&mut self, action: A, key: KeyCode) -> &mut Self {
        self.bind(action, InputTrigger::Key(key))
    }

    /// Adds a mouse trigger for an action.
    pub fn bind_mouse(&mut self, action: A, button: MouseButton) -> &mut Self {
        self.bind(action, InputTrigger::Mouse(button))
    }

    /// Adds any supported trigger for an action.
    pub fn bind(&mut self, action: A, trigger: InputTrigger) -> &mut Self {
        self.bindings.entry(action).or_default().push(trigger);
        self
    }

    /// Replaces every trigger for an action.
    pub fn set_triggers<I>(&mut self, action: A, triggers: I) -> &mut Self
    where
        I: IntoIterator<Item = InputTrigger>,
    {
        self.bindings.insert(action, triggers.into_iter().collect());
        self
    }

    /// Removes one trigger from an action.
    ///
    /// Returns `true` when the action had the trigger. Empty action entries are
    /// removed.
    pub fn unbind(&mut self, action: &A, trigger: InputTrigger) -> bool {
        let Some(triggers) = self.bindings.get_mut(action) else {
            return false;
        };
        let previous_len = triggers.len();
        triggers.retain(|candidate| *candidate != trigger);
        let removed = triggers.len() != previous_len;
        if triggers.is_empty() {
            self.bindings.remove(action);
        }
        removed
    }

    /// Removes a trigger from every action and returns the number of actions changed.
    pub fn unbind_trigger(&mut self, trigger: InputTrigger) -> usize {
        let mut changed = 0;
        self.bindings.retain(|_, triggers| {
            let previous_len = triggers.len();
            triggers.retain(|candidate| *candidate != trigger);
            if triggers.len() != previous_len {
                changed += 1;
            }
            !triggers.is_empty()
        });
        changed
    }

    /// Removes every trigger for an action.
    pub fn clear_action(&mut self, action: &A) -> Option<Vec<InputTrigger>> {
        self.bindings.remove(action)
    }

    /// Returns all triggers currently bound to an action.
    pub fn triggers(&self, action: &A) -> &[InputTrigger] {
        self.bindings.get(action).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Iterates over every action and its triggers.
    pub fn iter(&self) -> impl Iterator<Item = (&A, &[InputTrigger])> {
        self.bindings
            .iter()
            .map(|(action, triggers)| (action, triggers.as_slice()))
    }

    /// Iterates actions currently using `trigger`.
    pub fn actions_for_trigger(&self, trigger: InputTrigger) -> impl Iterator<Item = &A> {
        self.bindings
            .iter()
            .filter_map(move |(action, triggers)| triggers.contains(&trigger).then_some(action))
    }

    /// Returns true when more than one action uses `trigger`.
    pub fn has_conflict(&self, trigger: InputTrigger) -> bool {
        self.actions_for_trigger(trigger).take(2).count() > 1
    }

    /// Returns every trigger currently used by more than one action.
    pub fn conflicting_triggers(&self) -> Vec<InputTrigger> {
        let mut seen = HashSet::new();
        let mut conflicts = HashSet::new();
        for triggers in self.bindings.values() {
            let unique_triggers: HashSet<_> = triggers.iter().copied().collect();
            for trigger in unique_triggers {
                if !seen.insert(trigger) {
                    conflicts.insert(trigger);
                }
            }
        }
        conflicts.into_iter().collect()
    }

    /// Returns `true` when no actions are bound.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

/// A physical input source and scalar contribution for a gameplay axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisTrigger {
    pub trigger: InputTrigger,
    pub scale: f32,
}

impl AxisTrigger {
    /// Creates an axis trigger contribution.
    pub fn new(trigger: impl Into<InputTrigger>, scale: f32) -> Self {
        Self {
            trigger: trigger.into(),
            scale,
        }
    }
}

/// Bindings from game-defined axes to one or more scaled physical inputs.
///
/// `A` is usually a small game enum such as `MoveX`, `MoveY`, or `LookX`.
/// Positive and negative keys can be bound to the same axis, then
/// [`sync_axis_input_system`] computes a normalized axis value each frame.
pub struct AxisBindings<A> {
    bindings: HashMap<A, Vec<AxisTrigger>>,
}

impl<A: 'static> Resource for AxisBindings<A> {}

impl<A> Default for AxisBindings<A> {
    fn default() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }
}

impl<A> AxisBindings<A>
where
    A: Eq + Hash,
{
    /// Creates an empty axis binding map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one scaled keyboard contribution for an axis.
    pub fn bind_key(&mut self, axis: A, key: KeyCode, scale: f32) -> &mut Self {
        self.bind(axis, AxisTrigger::new(key, scale))
    }

    /// Adds one scaled mouse-button contribution for an axis.
    pub fn bind_mouse(&mut self, axis: A, button: MouseButton, scale: f32) -> &mut Self {
        self.bind(axis, AxisTrigger::new(button, scale))
    }

    /// Adds conventional negative and positive keyboard bindings for an axis.
    pub fn bind_key_pair(&mut self, axis: A, negative: KeyCode, positive: KeyCode) -> &mut Self
    where
        A: Clone,
    {
        self.bind_key(axis.clone(), negative, -1.0)
            .bind_key(axis, positive, 1.0)
    }

    /// Adds any supported scaled trigger for an axis.
    pub fn bind(&mut self, axis: A, trigger: AxisTrigger) -> &mut Self {
        self.bindings.entry(axis).or_default().push(trigger);
        self
    }

    /// Replaces every trigger contribution for an axis.
    pub fn set_triggers<I>(&mut self, axis: A, triggers: I) -> &mut Self
    where
        I: IntoIterator<Item = AxisTrigger>,
    {
        self.bindings.insert(axis, triggers.into_iter().collect());
        self
    }

    /// Removes one physical trigger from an axis regardless of scale.
    ///
    /// Returns `true` when the axis had the trigger. Empty axis entries are
    /// removed.
    pub fn unbind(&mut self, axis: &A, trigger: InputTrigger) -> bool {
        let Some(triggers) = self.bindings.get_mut(axis) else {
            return false;
        };
        let previous_len = triggers.len();
        triggers.retain(|candidate| candidate.trigger != trigger);
        let removed = triggers.len() != previous_len;
        if triggers.is_empty() {
            self.bindings.remove(axis);
        }
        removed
    }

    /// Removes a physical trigger from every axis and returns the number of axes changed.
    pub fn unbind_trigger(&mut self, trigger: InputTrigger) -> usize {
        let mut changed = 0;
        self.bindings.retain(|_, triggers| {
            let previous_len = triggers.len();
            triggers.retain(|candidate| candidate.trigger != trigger);
            if triggers.len() != previous_len {
                changed += 1;
            }
            !triggers.is_empty()
        });
        changed
    }

    /// Removes every trigger for an axis.
    pub fn clear_axis(&mut self, axis: &A) -> Option<Vec<AxisTrigger>> {
        self.bindings.remove(axis)
    }

    /// Returns all triggers currently bound to an axis.
    pub fn triggers(&self, axis: &A) -> &[AxisTrigger] {
        self.bindings.get(axis).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Iterates over every axis and its triggers.
    pub fn iter(&self) -> impl Iterator<Item = (&A, &[AxisTrigger])> {
        self.bindings
            .iter()
            .map(|(axis, triggers)| (axis, triggers.as_slice()))
    }

    /// Iterates axes currently using `trigger`.
    pub fn axes_for_trigger(&self, trigger: InputTrigger) -> impl Iterator<Item = &A> {
        self.bindings.iter().filter_map(move |(axis, triggers)| {
            triggers
                .iter()
                .any(|candidate| candidate.trigger == trigger)
                .then_some(axis)
        })
    }

    /// Returns true when more than one axis uses `trigger`.
    pub fn has_conflict(&self, trigger: InputTrigger) -> bool {
        self.axes_for_trigger(trigger).take(2).count() > 1
    }

    /// Returns every physical trigger currently used by more than one axis.
    pub fn conflicting_triggers(&self) -> Vec<InputTrigger> {
        let mut seen = HashSet::new();
        let mut conflicts = HashSet::new();
        for triggers in self.bindings.values() {
            let unique_triggers: HashSet<_> =
                triggers.iter().map(|trigger| trigger.trigger).collect();
            for trigger in unique_triggers {
                if !seen.insert(trigger) {
                    conflicts.insert(trigger);
                }
            }
        }
        conflicts.into_iter().collect()
    }

    /// Returns `true` when no axes are bound.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

/// Active input context stack for mode-specific action and axis maps.
///
/// Contexts are game-defined values such as `Gameplay`, `Menu`, or `Editor`.
/// The first active context is treated as the lowest-priority layer and the
/// last active context as the highest-priority layer. Contextual bindings merge
/// all active layers with global bindings when synchronized.
pub struct InputContexts<C> {
    active: Vec<C>,
}

impl<C: 'static> Resource for InputContexts<C> {}

impl<C> Default for InputContexts<C> {
    fn default() -> Self {
        Self { active: Vec::new() }
    }
}

impl<C> InputContexts<C>
where
    C: Eq,
{
    /// Creates an empty context stack.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a context stack with one active context.
    pub fn with_context(context: C) -> Self {
        Self {
            active: vec![context],
        }
    }

    /// Replaces the stack with one active context.
    pub fn set(&mut self, context: C) -> &mut Self {
        self.active.clear();
        self.active.push(context);
        self
    }

    /// Pushes a context above the current active contexts.
    ///
    /// Existing copies of the same context are removed first so each context
    /// appears at most once in the stack.
    pub fn push(&mut self, context: C) -> &mut Self {
        self.active.retain(|candidate| *candidate != context);
        self.active.push(context);
        self
    }

    /// Removes one context from the stack and returns whether it was active.
    pub fn remove(&mut self, context: &C) -> bool {
        let previous_len = self.active.len();
        self.active.retain(|candidate| candidate != context);
        self.active.len() != previous_len
    }

    /// Pops the highest-priority context.
    pub fn pop(&mut self) -> Option<C> {
        self.active.pop()
    }

    /// Clears all active contexts.
    pub fn clear(&mut self) {
        self.active.clear();
    }

    /// Returns true when `context` is active.
    pub fn is_active(&self, context: &C) -> bool {
        self.active.contains(context)
    }

    /// Iterates active contexts from lowest to highest priority.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &C> {
        self.active.iter()
    }

    /// Returns the highest-priority active context.
    pub fn current(&self) -> Option<&C> {
        self.active.last()
    }

    /// Returns true when no contexts are active.
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }
}

/// Action bindings split into global and context-specific maps.
pub struct ContextualActionBindings<A, C> {
    global: ActionBindings<A>,
    contexts: HashMap<C, ActionBindings<A>>,
}

impl<A: 'static, C: 'static> Resource for ContextualActionBindings<A, C> {}

impl<A, C> Default for ContextualActionBindings<A, C> {
    fn default() -> Self {
        Self {
            global: ActionBindings::default(),
            contexts: HashMap::new(),
        }
    }
}

impl<A, C> ContextualActionBindings<A, C>
where
    A: Eq + Hash,
    C: Eq + Hash,
{
    /// Creates an empty contextual action binding map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns bindings that are active in every context.
    pub fn global(&self) -> &ActionBindings<A> {
        &self.global
    }

    /// Returns mutable bindings that are active in every context.
    pub fn global_mut(&mut self) -> &mut ActionBindings<A> {
        &mut self.global
    }

    /// Returns bindings for one context, if any are registered.
    pub fn context(&self, context: &C) -> Option<&ActionBindings<A>> {
        self.contexts.get(context)
    }

    /// Returns mutable bindings for one context, creating it if needed.
    pub fn context_mut(&mut self, context: C) -> &mut ActionBindings<A> {
        self.contexts.entry(context).or_default()
    }

    /// Adds a global trigger for an action.
    pub fn bind_global(&mut self, action: A, trigger: InputTrigger) -> &mut Self {
        self.global.bind(action, trigger);
        self
    }

    /// Adds a global keyboard trigger for an action.
    pub fn bind_global_key(&mut self, action: A, key: KeyCode) -> &mut Self {
        self.bind_global(action, key.into())
    }

    /// Adds a global mouse trigger for an action.
    pub fn bind_global_mouse(&mut self, action: A, button: MouseButton) -> &mut Self {
        self.bind_global(action, button.into())
    }

    /// Adds a context-specific trigger for an action.
    pub fn bind(&mut self, context: C, action: A, trigger: InputTrigger) -> &mut Self {
        self.context_mut(context).bind(action, trigger);
        self
    }

    /// Adds a context-specific keyboard trigger for an action.
    pub fn bind_key(&mut self, context: C, action: A, key: KeyCode) -> &mut Self {
        self.bind(context, action, key.into())
    }

    /// Adds a context-specific mouse trigger for an action.
    pub fn bind_mouse(&mut self, context: C, action: A, button: MouseButton) -> &mut Self {
        self.bind(context, action, button.into())
    }

    /// Removes all bindings for one context.
    pub fn clear_context(&mut self, context: &C) -> Option<ActionBindings<A>> {
        self.contexts.remove(context)
    }

    /// Iterates registered context binding maps.
    pub fn iter_contexts(&self) -> impl Iterator<Item = (&C, &ActionBindings<A>)> {
        self.contexts.iter()
    }
}

/// Axis bindings split into global and context-specific maps.
pub struct ContextualAxisBindings<A, C> {
    global: AxisBindings<A>,
    contexts: HashMap<C, AxisBindings<A>>,
}

impl<A: 'static, C: 'static> Resource for ContextualAxisBindings<A, C> {}

impl<A, C> Default for ContextualAxisBindings<A, C> {
    fn default() -> Self {
        Self {
            global: AxisBindings::default(),
            contexts: HashMap::new(),
        }
    }
}

impl<A, C> ContextualAxisBindings<A, C>
where
    A: Eq + Hash,
    C: Eq + Hash,
{
    /// Creates an empty contextual axis binding map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns bindings that are active in every context.
    pub fn global(&self) -> &AxisBindings<A> {
        &self.global
    }

    /// Returns mutable bindings that are active in every context.
    pub fn global_mut(&mut self) -> &mut AxisBindings<A> {
        &mut self.global
    }

    /// Returns bindings for one context, if any are registered.
    pub fn context(&self, context: &C) -> Option<&AxisBindings<A>> {
        self.contexts.get(context)
    }

    /// Returns mutable bindings for one context, creating it if needed.
    pub fn context_mut(&mut self, context: C) -> &mut AxisBindings<A> {
        self.contexts.entry(context).or_default()
    }

    /// Adds a global scaled trigger for an axis.
    pub fn bind_global(&mut self, axis: A, trigger: AxisTrigger) -> &mut Self {
        self.global.bind(axis, trigger);
        self
    }

    /// Adds a global keyboard contribution for an axis.
    pub fn bind_global_key(&mut self, axis: A, key: KeyCode, scale: f32) -> &mut Self {
        self.bind_global(axis, AxisTrigger::new(key, scale))
    }

    /// Adds a context-specific scaled trigger for an axis.
    pub fn bind(&mut self, context: C, axis: A, trigger: AxisTrigger) -> &mut Self {
        self.context_mut(context).bind(axis, trigger);
        self
    }

    /// Adds a context-specific keyboard contribution for an axis.
    pub fn bind_key(&mut self, context: C, axis: A, key: KeyCode, scale: f32) -> &mut Self {
        self.bind(context, axis, AxisTrigger::new(key, scale))
    }

    /// Adds context-specific negative and positive keyboard bindings for an axis.
    pub fn bind_key_pair(
        &mut self,
        context: C,
        axis: A,
        negative: KeyCode,
        positive: KeyCode,
    ) -> &mut Self
    where
        A: Clone,
        C: Clone,
    {
        self.bind_key(context.clone(), axis.clone(), negative, -1.0)
            .bind_key(context, axis, positive, 1.0)
    }

    /// Removes all bindings for one context.
    pub fn clear_context(&mut self, context: &C) -> Option<AxisBindings<A>> {
        self.contexts.remove(context)
    }

    /// Iterates registered context binding maps.
    pub fn iter_contexts(&self) -> impl Iterator<Item = (&C, &AxisBindings<A>)> {
        self.contexts.iter()
    }
}

/// Per-frame state for semantic gameplay actions.
pub struct ActionInput<A> {
    pressed: HashSet<A>,
    just_pressed: HashSet<A>,
    just_released: HashSet<A>,
}

impl<A: 'static> Resource for ActionInput<A> {}

impl<A> Default for ActionInput<A> {
    fn default() -> Self {
        Self {
            pressed: HashSet::new(),
            just_pressed: HashSet::new(),
            just_released: HashSet::new(),
        }
    }
}

impl<A> ActionInput<A>
where
    A: Clone + Eq + Hash,
{
    /// Creates an empty action state resource.
    pub fn new() -> Self {
        Self::default()
    }

    /// Recomputes action state from the current keyboard and mouse state.
    pub fn sync(
        &mut self,
        bindings: &ActionBindings<A>,
        keyboard: &KeyboardInput,
        mouse: &MouseInput,
    ) {
        let previous = std::mem::take(&mut self.pressed);
        let current: HashSet<A> = bindings
            .iter()
            .filter(|(_, triggers)| {
                triggers
                    .iter()
                    .any(|trigger| trigger_pressed(*trigger, keyboard, mouse))
            })
            .map(|(action, _)| action.clone())
            .collect();

        self.just_pressed = current.difference(&previous).cloned().collect();
        self.just_released = previous.difference(&current).cloned().collect();
        self.pressed = current;
    }

    /// Recomputes action state from global plus active context bindings.
    pub fn sync_contextual<C>(
        &mut self,
        bindings: &ContextualActionBindings<A, C>,
        contexts: &InputContexts<C>,
        keyboard: &KeyboardInput,
        mouse: &MouseInput,
    ) where
        C: Eq + Hash,
    {
        let previous = std::mem::take(&mut self.pressed);
        let mut current = HashSet::new();
        collect_pressed_actions(&bindings.global, keyboard, mouse, &mut current);
        for context in contexts.iter() {
            if let Some(context_bindings) = bindings.context(context) {
                collect_pressed_actions(context_bindings, keyboard, mouse, &mut current);
            }
        }

        self.just_pressed = current.difference(&previous).cloned().collect();
        self.just_released = previous.difference(&current).cloned().collect();
        self.pressed = current;
    }

    /// Clears all action state.
    pub fn clear(&mut self) {
        self.pressed.clear();
        self.just_pressed.clear();
        self.just_released.clear();
    }

    /// Returns `true` while the action is held.
    pub fn pressed(&self, action: &A) -> bool {
        self.pressed.contains(action)
    }

    /// Returns `true` for the first frame an action is held.
    pub fn just_pressed(&self, action: &A) -> bool {
        self.just_pressed.contains(action)
    }

    /// Returns `true` for the first frame after an action is released.
    pub fn just_released(&self, action: &A) -> bool {
        self.just_released.contains(action)
    }

    /// Returns the button-style state for an action.
    pub fn button_state(&self, action: &A) -> ButtonState {
        if self.just_pressed(action) {
            ButtonState::JustPressed
        } else if self.just_released(action) {
            ButtonState::JustReleased
        } else if self.pressed(action) {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        }
    }

    /// Iterates over currently pressed actions.
    pub fn pressed_actions(&self) -> impl Iterator<Item = &A> {
        self.pressed.iter()
    }
}

/// Per-frame state for semantic gameplay axes.
pub struct AxisInput<A> {
    values: HashMap<A, f32>,
}

impl<A: 'static> Resource for AxisInput<A> {}

impl<A> Default for AxisInput<A> {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
        }
    }
}

impl<A> AxisInput<A>
where
    A: Clone + Eq + Hash,
{
    /// Creates an empty axis state resource.
    pub fn new() -> Self {
        Self::default()
    }

    /// Recomputes axis values from the current keyboard and mouse state.
    pub fn sync(
        &mut self,
        bindings: &AxisBindings<A>,
        keyboard: &KeyboardInput,
        mouse: &MouseInput,
    ) {
        self.values.clear();
        for (axis, triggers) in bindings.iter() {
            let value = triggers
                .iter()
                .filter(|trigger| trigger_pressed(trigger.trigger, keyboard, mouse))
                .map(|trigger| trigger.scale)
                .sum::<f32>()
                .clamp(-1.0, 1.0);
            self.values.insert(axis.clone(), value);
        }
    }

    /// Recomputes axis values from global plus active context bindings.
    pub fn sync_contextual<C>(
        &mut self,
        bindings: &ContextualAxisBindings<A, C>,
        contexts: &InputContexts<C>,
        keyboard: &KeyboardInput,
        mouse: &MouseInput,
    ) where
        C: Eq + Hash,
    {
        self.values.clear();
        accumulate_axis_values(&bindings.global, keyboard, mouse, &mut self.values);
        for context in contexts.iter() {
            if let Some(context_bindings) = bindings.context(context) {
                accumulate_axis_values(context_bindings, keyboard, mouse, &mut self.values);
            }
        }
        for value in self.values.values_mut() {
            *value = value.clamp(-1.0, 1.0);
        }
    }

    /// Clears all axis state.
    pub fn clear(&mut self) {
        self.values.clear();
    }

    /// Returns the current value for an axis, or zero if it is unbound/inactive.
    pub fn value(&self, axis: &A) -> f32 {
        self.values.get(axis).copied().unwrap_or(0.0)
    }

    /// Returns true when the axis magnitude is above a small dead zone.
    pub fn active(&self, axis: &A) -> bool {
        self.value(axis).abs() > f32::EPSILON
    }

    /// Iterates over all tracked axis values.
    pub fn iter(&self) -> impl Iterator<Item = (&A, f32)> {
        self.values.iter().map(|(axis, value)| (axis, *value))
    }
}

/// ECS system that updates [`ActionInput`] from raw keyboard and mouse input.
pub fn sync_action_input_system<A>(
    keyboard: Res<KeyboardInput>,
    mouse: Res<MouseInput>,
    bindings: Res<ActionBindings<A>>,
    mut actions: ResMut<ActionInput<A>>,
) where
    A: Clone + Eq + Hash + 'static,
{
    actions.sync(&bindings, &keyboard, &mouse);
}

/// ECS system that updates [`AxisInput`] from raw keyboard and mouse input.
pub fn sync_axis_input_system<A>(
    keyboard: Res<KeyboardInput>,
    mouse: Res<MouseInput>,
    bindings: Res<AxisBindings<A>>,
    mut axes: ResMut<AxisInput<A>>,
) where
    A: Clone + Eq + Hash + 'static,
{
    axes.sync(&bindings, &keyboard, &mouse);
}

/// ECS system that updates [`ActionInput`] from contextual action bindings.
pub fn sync_contextual_action_input_system<A, C>(
    keyboard: Res<KeyboardInput>,
    mouse: Res<MouseInput>,
    bindings: Res<ContextualActionBindings<A, C>>,
    contexts: Res<InputContexts<C>>,
    mut actions: ResMut<ActionInput<A>>,
) where
    A: Clone + Eq + Hash + 'static,
    C: Clone + Eq + Hash + 'static,
{
    actions.sync_contextual(&bindings, &contexts, &keyboard, &mouse);
}

/// ECS system that updates [`AxisInput`] from contextual axis bindings.
pub fn sync_contextual_axis_input_system<A, C>(
    keyboard: Res<KeyboardInput>,
    mouse: Res<MouseInput>,
    bindings: Res<ContextualAxisBindings<A, C>>,
    contexts: Res<InputContexts<C>>,
    mut axes: ResMut<AxisInput<A>>,
) where
    A: Clone + Eq + Hash + 'static,
    C: Clone + Eq + Hash + 'static,
{
    axes.sync_contextual(&bindings, &contexts, &keyboard, &mouse);
}

fn collect_pressed_actions<A>(
    bindings: &ActionBindings<A>,
    keyboard: &KeyboardInput,
    mouse: &MouseInput,
    current: &mut HashSet<A>,
) where
    A: Clone + Eq + Hash,
{
    current.extend(
        bindings
            .iter()
            .filter(|(_, triggers)| {
                triggers
                    .iter()
                    .any(|trigger| trigger_pressed(*trigger, keyboard, mouse))
            })
            .map(|(action, _)| action.clone()),
    );
}

fn accumulate_axis_values<A>(
    bindings: &AxisBindings<A>,
    keyboard: &KeyboardInput,
    mouse: &MouseInput,
    values: &mut HashMap<A, f32>,
) where
    A: Clone + Eq + Hash,
{
    for (axis, triggers) in bindings.iter() {
        let value = triggers
            .iter()
            .filter(|trigger| trigger_pressed(trigger.trigger, keyboard, mouse))
            .map(|trigger| trigger.scale)
            .sum::<f32>();
        if value != 0.0 {
            *values.entry(axis.clone()).or_insert(0.0) += value;
        } else {
            values.entry(axis.clone()).or_insert(0.0);
        }
    }
}

fn trigger_pressed(trigger: InputTrigger, keyboard: &KeyboardInput, mouse: &MouseInput) -> bool {
    match trigger {
        InputTrigger::Key(key) => keyboard.pressed(key),
        InputTrigger::Mouse(button) => mouse.pressed(button),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::PhysicalKey;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    enum GameAction {
        Jump,
        Fire,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    enum GameAxis {
        MoveX,
        Throttle,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    enum InputMode {
        Gameplay,
        Menu,
    }

    #[test]
    fn action_input_tracks_pressed_and_release_transitions() {
        let mut keyboard = KeyboardInput::default();
        let mouse = MouseInput::default();
        let mut bindings = ActionBindings::new();
        bindings.bind_key(GameAction::Jump, KeyCode::Space);

        let mut actions = ActionInput::new();
        actions.sync(&bindings, &keyboard, &mouse);
        assert_eq!(
            actions.button_state(&GameAction::Jump),
            ButtonState::Released
        );

        keyboard.process_event(PhysicalKey::Code(KeyCode::Space), true);
        actions.sync(&bindings, &keyboard, &mouse);
        assert!(actions.just_pressed(&GameAction::Jump));
        assert!(actions.pressed(&GameAction::Jump));

        actions.sync(&bindings, &keyboard, &mouse);
        assert!(!actions.just_pressed(&GameAction::Jump));
        assert_eq!(
            actions.button_state(&GameAction::Jump),
            ButtonState::Pressed
        );

        keyboard.process_event(PhysicalKey::Code(KeyCode::Space), false);
        actions.sync(&bindings, &keyboard, &mouse);
        assert!(actions.just_released(&GameAction::Jump));
        assert_eq!(
            actions.button_state(&GameAction::Jump),
            ButtonState::JustReleased
        );
    }

    #[test]
    fn multiple_triggers_can_drive_one_action() {
        let keyboard = KeyboardInput::default();
        let mut mouse = MouseInput::default();
        let mut bindings = ActionBindings::new();
        bindings
            .bind_key(GameAction::Fire, KeyCode::ControlLeft)
            .bind_mouse(GameAction::Fire, MouseButton::Left);

        let mut actions = ActionInput::new();
        mouse.process_button(MouseButton::Left, true);
        actions.sync(&bindings, &keyboard, &mouse);

        assert!(actions.just_pressed(&GameAction::Fire));
        assert!(actions.pressed(&GameAction::Fire));
    }

    #[test]
    fn action_bindings_support_rebinding_and_conflict_queries() {
        let mut bindings = ActionBindings::new();
        bindings
            .bind_key(GameAction::Jump, KeyCode::Space)
            .bind_key(GameAction::Fire, KeyCode::Space);

        assert!(bindings.has_conflict(KeyCode::Space.into()));
        assert_eq!(
            bindings.actions_for_trigger(KeyCode::Space.into()).count(),
            2
        );
        assert!(bindings
            .conflicting_triggers()
            .contains(&InputTrigger::Key(KeyCode::Space)));

        bindings.set_triggers(GameAction::Fire, [InputTrigger::Mouse(MouseButton::Left)]);
        assert!(!bindings.has_conflict(KeyCode::Space.into()));
        assert!(bindings.unbind(&GameAction::Jump, KeyCode::Space.into()));
        assert!(bindings.triggers(&GameAction::Jump).is_empty());
        assert_eq!(bindings.unbind_trigger(MouseButton::Left.into()), 1);
        assert!(bindings.is_empty());
    }

    #[test]
    fn contextual_action_bindings_follow_active_contexts() {
        let mut keyboard = KeyboardInput::default();
        let mouse = MouseInput::default();
        let mut contexts = InputContexts::with_context(InputMode::Gameplay);
        let mut bindings = ContextualActionBindings::new();
        bindings
            .bind_global_key(GameAction::Fire, KeyCode::F12)
            .bind_key(InputMode::Gameplay, GameAction::Jump, KeyCode::Space)
            .bind_key(InputMode::Menu, GameAction::Fire, KeyCode::Enter);

        keyboard.process_event(PhysicalKey::Code(KeyCode::Space), true);
        keyboard.process_event(PhysicalKey::Code(KeyCode::Enter), true);
        keyboard.process_event(PhysicalKey::Code(KeyCode::F12), true);

        let mut actions = ActionInput::new();
        actions.sync_contextual(&bindings, &contexts, &keyboard, &mouse);
        assert!(actions.pressed(&GameAction::Jump));
        assert!(actions.pressed(&GameAction::Fire));
        assert!(actions.just_pressed(&GameAction::Jump));

        contexts.set(InputMode::Menu);
        actions.sync_contextual(&bindings, &contexts, &keyboard, &mouse);
        assert!(!actions.pressed(&GameAction::Jump));
        assert!(actions.just_released(&GameAction::Jump));
        assert!(actions.pressed(&GameAction::Fire));

        contexts.push(InputMode::Gameplay);
        assert_eq!(contexts.current(), Some(&InputMode::Gameplay));
        actions.sync_contextual(&bindings, &contexts, &keyboard, &mouse);
        assert!(actions.pressed(&GameAction::Jump));
    }

    #[test]
    fn axis_input_tracks_key_pairs_and_release_to_zero() {
        let mut keyboard = KeyboardInput::default();
        let mouse = MouseInput::default();
        let mut bindings = AxisBindings::new();
        bindings.bind_key_pair(GameAxis::MoveX, KeyCode::KeyA, KeyCode::KeyD);

        let mut axes = AxisInput::new();
        axes.sync(&bindings, &keyboard, &mouse);
        assert_eq!(axes.value(&GameAxis::MoveX), 0.0);
        assert!(!axes.active(&GameAxis::MoveX));

        keyboard.process_event(PhysicalKey::Code(KeyCode::KeyD), true);
        axes.sync(&bindings, &keyboard, &mouse);
        assert_eq!(axes.value(&GameAxis::MoveX), 1.0);
        assert!(axes.active(&GameAxis::MoveX));

        keyboard.process_event(PhysicalKey::Code(KeyCode::KeyA), true);
        axes.sync(&bindings, &keyboard, &mouse);
        assert_eq!(axes.value(&GameAxis::MoveX), 0.0);

        keyboard.process_event(PhysicalKey::Code(KeyCode::KeyD), false);
        axes.sync(&bindings, &keyboard, &mouse);
        assert_eq!(axes.value(&GameAxis::MoveX), -1.0);

        keyboard.process_event(PhysicalKey::Code(KeyCode::KeyA), false);
        axes.sync(&bindings, &keyboard, &mouse);
        assert_eq!(axes.value(&GameAxis::MoveX), 0.0);
    }

    #[test]
    fn axis_input_clamps_multiple_trigger_contributions() {
        let mut keyboard = KeyboardInput::default();
        let mouse = MouseInput::default();
        let mut bindings = AxisBindings::new();
        bindings
            .bind_key(GameAxis::Throttle, KeyCode::KeyW, 0.75)
            .bind_key(GameAxis::Throttle, KeyCode::ArrowUp, 0.75);

        keyboard.process_event(PhysicalKey::Code(KeyCode::KeyW), true);
        keyboard.process_event(PhysicalKey::Code(KeyCode::ArrowUp), true);

        let mut axes = AxisInput::new();
        axes.sync(&bindings, &keyboard, &mouse);
        assert_eq!(axes.value(&GameAxis::Throttle), 1.0);
    }

    #[test]
    fn axis_input_accepts_mouse_button_contributions() {
        let keyboard = KeyboardInput::default();
        let mut mouse = MouseInput::default();
        let mut bindings = AxisBindings::new();
        bindings.bind_mouse(GameAxis::Throttle, MouseButton::Left, 1.0);

        let mut axes = AxisInput::new();
        mouse.process_button(MouseButton::Left, true);
        axes.sync(&bindings, &keyboard, &mouse);
        assert_eq!(axes.value(&GameAxis::Throttle), 1.0);
    }

    #[test]
    fn axis_bindings_support_rebinding_and_conflict_queries() {
        let mut bindings = AxisBindings::new();
        bindings
            .bind_key(GameAxis::MoveX, KeyCode::KeyA, -1.0)
            .bind_key(GameAxis::Throttle, KeyCode::KeyA, 1.0);

        assert!(bindings.has_conflict(KeyCode::KeyA.into()));
        assert_eq!(bindings.axes_for_trigger(KeyCode::KeyA.into()).count(), 2);
        assert!(bindings
            .conflicting_triggers()
            .contains(&InputTrigger::Key(KeyCode::KeyA)));

        bindings.set_triggers(
            GameAxis::Throttle,
            [AxisTrigger::new(MouseButton::Right, 1.0)],
        );
        assert!(!bindings.has_conflict(KeyCode::KeyA.into()));
        assert!(bindings.unbind(&GameAxis::MoveX, KeyCode::KeyA.into()));
        assert!(bindings.triggers(&GameAxis::MoveX).is_empty());
        assert_eq!(bindings.unbind_trigger(MouseButton::Right.into()), 1);
        assert!(bindings.is_empty());
    }

    #[test]
    fn contextual_axis_bindings_merge_global_and_active_context_axes() {
        let mut keyboard = KeyboardInput::default();
        let mouse = MouseInput::default();
        let mut contexts = InputContexts::with_context(InputMode::Gameplay);
        let mut bindings = ContextualAxisBindings::new();
        bindings
            .bind_global_key(GameAxis::Throttle, KeyCode::ShiftLeft, 0.25)
            .bind_key_pair(
                InputMode::Gameplay,
                GameAxis::MoveX,
                KeyCode::KeyA,
                KeyCode::KeyD,
            )
            .bind_key(InputMode::Menu, GameAxis::Throttle, KeyCode::ArrowUp, 1.0);

        keyboard.process_event(PhysicalKey::Code(KeyCode::KeyD), true);
        keyboard.process_event(PhysicalKey::Code(KeyCode::ArrowUp), true);
        keyboard.process_event(PhysicalKey::Code(KeyCode::ShiftLeft), true);

        let mut axes = AxisInput::new();
        axes.sync_contextual(&bindings, &contexts, &keyboard, &mouse);
        assert_eq!(axes.value(&GameAxis::MoveX), 1.0);
        assert_eq!(axes.value(&GameAxis::Throttle), 0.25);

        contexts.set(InputMode::Menu);
        axes.sync_contextual(&bindings, &contexts, &keyboard, &mouse);
        assert_eq!(axes.value(&GameAxis::MoveX), 0.0);
        assert_eq!(axes.value(&GameAxis::Throttle), 1.0);
    }
}
