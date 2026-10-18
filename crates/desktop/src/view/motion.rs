use super::*;
use gpui_kit::base::motion::{Transition, transition};
use std::time::Duration;

#[derive(Default)]
struct ButtonState {
    hovered: bool,
    pressed: bool,
}

#[derive(IntoElement)]
pub(super) struct AnimatedButton {
    button: Button,
    id: ElementId,
    disabled: bool,
}

pub(super) fn animated_button(
    button: Button,
    id: (&'static str, impl Into<ElementId>),
    disabled: bool,
) -> AnimatedButton {
    AnimatedButton {
        button,
        id: (id.1.into(), id.0).into(),
        disabled,
    }
}

impl RenderOnce for AnimatedButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ButtonState::default());
        let current = state.read(cx);
        let target = if self.disabled {
            1.
        } else if current.pressed {
            0.65
        } else if current.hovered {
            0.86
        } else {
            1.
        };
        let opacity = transition(
            self.id.clone(),
            target,
            Transition::new(Duration::from_millis(120))
                .ease(gpui_kit::component::animation::ease_out_cubic),
            window,
            cx,
        );
        let disabled = self.disabled;
        div()
            .id(self.id)
            .flex()
            .opacity(opacity)
            .on_hover(
                window.listener_for(&state, move |state, hovered, window, _| {
                    state.hovered = *hovered && !disabled;
                    if !*hovered {
                        state.pressed = false;
                    }
                    window.refresh();
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                window.listener_for(&state, move |state, _, window, _| {
                    state.pressed = !disabled;
                    window.refresh();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                window.listener_for(&state, |state, _, window, _| {
                    state.pressed = false;
                    window.refresh();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                window.listener_for(&state, |state, _, window, _| {
                    state.pressed = false;
                    window.refresh();
                }),
            )
            .child(self.button)
    }
}
