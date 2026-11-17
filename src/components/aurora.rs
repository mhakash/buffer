//! The drifting backdrop as its own view: each frame re-renders only this view (the cached
//! `Workspace` beside it in `Shell` is replayed), and it stands still while the window is in the
//! background.

use std::time::{Duration, Instant};

use gpui_kit::{Context, IntoElement, Render, Window};

use super::fx;

/// Drift is slow and heavily blurred: a few frames a second look continuous.
const FPS: f32 = 12.;

#[derive(Default)]
pub struct Aurora {
    /// Seconds of drift so far; advances only while animating, so pausing never jumps.
    seconds: f32,
    last: Option<Instant>,
    frame_pending: bool,
}

impl Render for Aurora {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if fx::ambient(cx) {
            let now = Instant::now();
            if let Some(last) = self.last {
                self.seconds += (now - last).as_secs_f32().min(0.5);
            }
            self.last = Some(now);
            if !self.frame_pending {
                self.frame_pending = true;
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_secs_f32(1. / FPS))
                        .await;
                    this.update(cx, |this, cx| {
                        this.frame_pending = false;
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
        } else {
            self.last = None;
        }
        fx::aurora(self.seconds, cx)
    }
}
