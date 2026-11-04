//! The status bar's context ring: how full the model's context window is.

use gpui_kit::*;

#[derive(IntoElement)]
pub struct ContextRing {
    /// The share in use, 0 to 1; `None` when the window size isn't known,
    /// which draws a dashed circle instead.
    fraction: Option<f32>,
    color: Hsla,
    size: Pixels,
}

impl ContextRing {
    pub fn new(fraction: Option<f32>, color: Hsla, size: Pixels) -> Self {
        Self {
            fraction: fraction.map(|f| f.clamp(0., 1.)),
            color,
            size,
        }
    }
}

impl RenderOnce for ContextRing {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (fraction, color) = (self.fraction, self.color);
        div().size(self.size).flex_none().child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| paint(bounds, fraction, color, window),
            )
            .size_full(),
        )
    }
}

fn paint(bounds: Bounds<Pixels>, fraction: Option<f32>, color: Hsla, window: &mut Window) {
    let size = f32::from(bounds.size.width.min(bounds.size.height));
    let width = (size * 0.15).max(1.5);
    let radius = (size - width) / 2.;
    let center = bounds.center();
    // From twelve o'clock, clockwise.
    let at = |turn: f32| {
        let angle = (turn - 0.25) * std::f32::consts::TAU;
        point(
            center.x + px(radius * angle.cos()),
            center.y + px(radius * angle.sin()),
        )
    };
    let arc = |from: f32, to: f32, color: Hsla, window: &mut Window| {
        let steps = ((to - from) * 48.).ceil().max(2.) as usize;
        let mut path = PathBuilder::stroke(px(width));
        path.move_to(at(from));
        for i in 1..=steps {
            path.line_to(at(from + (to - from) * i as f32 / steps as f32));
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    };
    match fraction {
        Some(fraction) => {
            arc(0., 1., color.opacity(0.28), window);
            if fraction > 0.005 {
                arc(0., fraction, color, window);
            }
        }
        None => {
            for dash in 0..10 {
                let start = dash as f32 / 10.;
                arc(start, start + 0.05, color, window);
            }
        }
    }
}
