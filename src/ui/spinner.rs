//! The design's spinner: an arc on a faint track that grows and shrinks as it
//! turns, after svg-spinners' `ring-resize` (MIT). gpui-kit's own spinner
//! turns an icon, which can't draw a track or change the arc's length.

use std::sync::OnceLock;
use std::time::Instant;

use gpui_kit::*;

/// The SVG's geometry, in its 24-unit view box.
const VIEW: f32 = 24.;
const RADIUS: f32 = 9.5;
const STROKE: f32 = 3.5;
/// The arc's longest length, in view box units along the circle.
const DASH: f32 = 42.;
const TURN_SECS: f32 = 2.;
const GROW_SECS: f32 = 1.5;

#[derive(IntoElement)]
pub struct Spinner {
    size: Pixels,
    color: Hsla,
}

impl Spinner {
    pub fn new(size: Pixels, color: Hsla) -> Self {
        Self { size, color }
    }
}

impl RenderOnce for Spinner {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let color = self.color;
        div().size(self.size).flex_none().child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| {
                    let still = cx.reduce_motion();
                    // One clock for every spinner, so they turn together.
                    static START: OnceLock<Instant> = OnceLock::new();
                    let t = START.get_or_init(Instant::now).elapsed().as_secs_f32();
                    let (turn, start, end) = if still {
                        (0.75, 0., DASH / 2.)
                    } else {
                        let (start, end) = arc_at(t % GROW_SECS / GROW_SECS);
                        ((t % TURN_SECS) / TURN_SECS, start, end)
                    };
                    paint(bounds, color, turn, start, end, window);
                    if !still {
                        window.request_animation_frame();
                    }
                },
            )
            .size_full(),
        )
    }
}

/// Where the arc starts and ends along the circle, in view box units, at
/// `phase` (0 to 1) of the grow cycle: the SVG's dash keyframes.
fn arc_at(phase: f32) -> (f32, f32) {
    let circumference = std::f32::consts::TAU * RADIUS;
    let (length, offset) = if phase < 0.475 {
        let k = ease_in_out(phase / 0.475);
        (DASH * k, 16. * k)
    } else if phase < 0.95 {
        let k = ease_in_out((phase - 0.475) / 0.475);
        (DASH, 16. + (59. - 16.) * k)
    } else {
        (DASH, 59.)
    };
    // The dash runs off the end of the circle's path rather than wrapping,
    // which is what shrinks it.
    (offset, (offset + length).min(circumference))
}

/// CSS's `ease-in-out`, cubic-bezier(0.42, 0, 0.58, 1).
fn ease_in_out(x: f32) -> f32 {
    let bezier = |t: f32, a: f32, b: f32| {
        let u = 1. - t;
        3. * u * u * t * a + 3. * u * t * t * b + t * t * t
    };
    // Find t for x by bisection; x(t) only grows.
    let (mut lo, mut hi) = (0f32, 1f32);
    for _ in 0..20 {
        let mid = (lo + hi) / 2.;
        if bezier(mid, 0.42, 0.58) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bezier((lo + hi) / 2., 0., 1.)
}

fn paint(
    bounds: Bounds<Pixels>,
    color: Hsla,
    turn: f32,
    start: f32,
    end: f32,
    window: &mut Window,
) {
    let scale = f32::from(bounds.size.width.min(bounds.size.height)) / VIEW;
    let center = bounds.center();
    let radius = RADIUS * scale;
    let width = STROKE * scale;
    // The circle's path starts at three o'clock and runs clockwise; the
    // whole arc then turns by `turn`.
    let at = |units: f32| {
        let angle = (units / RADIUS) + turn * std::f32::consts::TAU;
        point(
            center.x + px(radius * angle.cos()),
            center.y + px(radius * angle.sin()),
        )
    };

    let mut track = PathBuilder::stroke(px(width));
    let steps = 48;
    let circumference = std::f32::consts::TAU * RADIUS;
    track.move_to(at(0.));
    for i in 1..=steps {
        track.line_to(at(circumference * i as f32 / steps as f32));
    }
    track.close();
    if let Ok(path) = track.build() {
        window.paint_path(path, color.opacity(0.3));
    }

    if end - start > 0.01 {
        let mut arc = PathBuilder::stroke(px(width));
        let steps = ((end - start) / circumference * 48.).ceil().max(2.) as usize;
        arc.move_to(at(start));
        for i in 1..=steps {
            arc.line_to(at(start + (end - start) * i as f32 / steps as f32));
        }
        if let Ok(path) = arc.build() {
            window.paint_path(path, color);
        }
    }
    // Round caps: lyon's line cap isn't reachable through GPUI, so each end
    // gets a dot as wide as the stroke.
    for units in [start, end] {
        let c = at(units);
        let r = px(width / 2.);
        let mut dot = PathBuilder::fill();
        dot.move_to(point(c.x + r, c.y));
        dot.arc_to(point(r, r), px(0.), false, true, point(c.x - r, c.y));
        dot.arc_to(point(r, r), px(0.), false, true, point(c.x + r, c.y));
        dot.close();
        if let Ok(path) = dot.build() {
            window.paint_path(path, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DASH, RADIUS, arc_at, ease_in_out};

    #[test]
    fn eases_like_css() {
        assert!(ease_in_out(0.).abs() < 1e-3);
        assert!((ease_in_out(0.5) - 0.5).abs() < 1e-3);
        assert!((ease_in_out(1.) - 1.).abs() < 1e-3);
        assert!(ease_in_out(0.1) < 0.1);
    }

    #[test]
    fn arc_grows_then_runs_off_the_end() {
        let (start, end) = arc_at(0.);
        assert!(start.abs() < 1e-3 && end.abs() < 1e-3);
        let (start, end) = arc_at(0.475);
        assert!((end - start - DASH).abs() < 1e-3);
        let (start, end) = arc_at(0.97);
        assert!(end - start < 1.);
        assert!((end - std::f32::consts::TAU * RADIUS).abs() < 1e-3);
    }
}
