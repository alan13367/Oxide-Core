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

    /// Returns `true` when no axes are bound.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
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
}
