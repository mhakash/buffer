//! Auto-scroll while a drag (text or block selection, a dragged block) is held near or past
//! the top or bottom edge of a scroll area. The kit textarea only scrolls itself, and the
//! editor's grow to full height inside the page's scroll area.

use std::{cell::RefCell, rc::Rc, time::Duration};

use gpui_kit::base::AutoScroll;
use gpui_kit::{
    App, Div, EntityId, InteractiveElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, PlatformInput, Point, ScrollHandle, Stateful,
    StatefulInteractiveElement, Styled, Window, canvas, point, px,
};

#[derive(Clone, Default)]
pub struct DragScroll(Rc<RefCell<State>>);

#[derive(Default)]
struct State {
    handle: ScrollHandle,
    /// The left button went down inside the area and is still held.
    armed: bool,
    pointer: Point<Pixels>,
    /// Scroll per tick the pointer asks for (positive = down).
    speed: Option<Pixels>,
    running: bool,
    /// Our own replayed move is being dispatched: it isn't where the pointer is.
    replaying: bool,
}

impl DragScroll {
    /// Make `area` (an `overflow_y_scroll` element) follow drags past its edges.
    pub fn attach(&self, area: Stateful<Div>) -> Stateful<Div> {
        let handle = self.0.borrow().handle.clone();
        let down = self.clone();
        let paint = self.clone();
        area.track_scroll(&handle)
            .capture_any_mouse_down(move |event: &MouseDownEvent, _, _| {
                down.0.borrow_mut().armed = event.button == MouseButton::Left;
            })
            .child(
                canvas(|_, _, _| {}, move |_, _, window, _| paint.listen(window))
                    .absolute()
                    .size_0(),
            )
    }

    /// The area's scroll handle (offset, bounds).
    pub fn handle(&self) -> ScrollHandle {
        self.0.borrow().handle.clone()
    }

    /// Window-wide listeners (the pointer may leave the area, or the window, mid-drag).
    fn listen(&self, window: &mut Window) {
        let view = window.current_view();
        let this = self.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase.bubble() {
                this.on_move(event, view, window, cx);
            }
        });
        let this = self.clone();
        window.on_mouse_event(move |_: &MouseUpEvent, _, _, _| {
            let mut state = this.0.borrow_mut();
            state.armed = false;
            state.speed = None;
        });
    }

    fn on_move(&self, event: &MouseMoveEvent, view: EntityId, window: &Window, cx: &App) {
        let mut state = self.0.borrow_mut();
        if state.replaying {
            return;
        }
        if !state.armed || event.pressed_button != Some(MouseButton::Left) {
            state.armed = false;
            state.speed = None;
            return;
        }
        state.pointer = event.position;
        state.speed = AutoScroll::compute_delta(event.position.y, state.handle.bounds());
        if state.speed.is_some() && !state.running {
            state.running = true;
            drop(state);
            self.run(view, window, cx);
        }
    }

    /// Scroll every frame until the pointer leaves the edge zone or the button is released.
    fn run(&self, view: EntityId, window: &Window, cx: &App) {
        let this = self.clone();
        window
            .spawn(cx, async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    let this = this.clone();
                    let alive = cx.update(move |window, cx| this.tick(view, window, cx));
                    if !matches!(alive, Ok(true)) {
                        break;
                    }
                }
                this.0.borrow_mut().running = false;
            })
            .detach();
    }

    /// One scroll step; false once there's nothing left to do.
    fn tick(&self, view: EntityId, window: &mut Window, cx: &mut App) -> bool {
        let state = self.0.borrow();
        let Some(speed) = state.speed else {
            return false;
        };
        let offset = state.handle.offset();
        let max = state.handle.max_offset().y;
        let y = (offset.y - speed).clamp(-max, px(0.));
        if y == offset.y {
            return true;
        }
        state.handle.set_offset(point(offset.x, y));
        drop(state);
        cx.notify(view);
        // Once the scrolled content is laid out, move the selection to the edge row.
        let this = self.clone();
        window.on_next_frame(move |window, cx| this.replay(window, cx));
        true
    }

    /// Dispatch a move at the pointer, clamped into the area, so selections under it extend.
    fn replay(&self, window: &mut Window, cx: &mut App) {
        let position = {
            let mut state = self.0.borrow_mut();
            if !state.armed {
                return;
            }
            state.replaying = true;
            let b = state.handle.bounds();
            let y = state.pointer.y.clamp(b.top() + px(1.), b.bottom() - px(1.));
            point(state.pointer.x, y)
        };
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                pressed_button: Some(MouseButton::Left),
                modifiers: window.modifiers(),
            }),
            cx,
        );
        self.0.borrow_mut().replaying = false;
    }
}
