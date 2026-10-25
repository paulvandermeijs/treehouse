//! Catppuccin Mocha and Latte, as the design specifies them.
//!
//! The gpui-kit theme carries most colours. The design also names a few
//! Catppuccin roles the kit has no slot for (Mantle, Subtext 1, the darker
//! diff greens and reds in Latte); [`Palette`] holds those.

use gpui_kit::component::{ActiveTheme as _, Theme, ThemeMode, ThemeRegistry};
use gpui_kit::{App, Hsla, Window, rgb};
use std::borrow::Cow;

use crate::config::{Config, ThemeChoice};

const THEMES: &str = include_str!("../assets/themes/treehouse.json");
const MOCHA: &str = "Treehouse Mocha";
const LATTE: &str = "Treehouse Latte";

const FONTS: [&[u8]; 8] = [
    include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-Italic.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-Bold.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Medium.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Italic.ttf"),
];

pub const MONO: &str = "IBM Plex Mono";
pub const SANS: &str = "IBM Plex Sans";

pub fn init(cx: &mut App) {
    let fonts = FONTS.iter().map(|bytes| Cow::Borrowed(*bytes)).collect();
    if let Err(err) = cx.text_system().add_fonts(fonts) {
        eprintln!("treehouse: failed to load IBM Plex: {err}");
    }

    let registry = ThemeRegistry::global_mut(cx);
    registry
        .load_themes_from_str(THEMES)
        .expect("the bundled Treehouse themes parse");
    let mocha = registry.themes().get(MOCHA).cloned();
    let latte = registry.themes().get(LATTE).cloned();
    if let (Some(mocha), Some(latte)) = (mocha, latte) {
        Theme::update(cx, |theme| {
            theme.apply_config(&latte);
            theme.apply_config(&mocha);
        });
    }
    apply(Config::global(cx).theme, None, cx);
}

/// Switches between Mocha and Latte, or follows the system appearance.
pub fn apply(choice: ThemeChoice, window: Option<&mut Window>, cx: &mut App) {
    let mode = match choice {
        ThemeChoice::Mocha => ThemeMode::Dark,
        ThemeChoice::Latte => ThemeMode::Light,
        ThemeChoice::System => match window {
            Some(window) => {
                if matches!(
                    window.appearance(),
                    gpui_kit::WindowAppearance::Dark | gpui_kit::WindowAppearance::VibrantDark
                ) {
                    ThemeMode::Dark
                } else {
                    ThemeMode::Light
                }
            }
            None => ThemeMode::Dark,
        },
    };
    Theme::change(mode, None, cx);
    let size = Config::global(cx).text_size;
    Theme::update(cx, |theme| theme.font_size = gpui_kit::px(size));
    cx.refresh_windows();
}

/// The current theme's name for the menu that switches it.
pub fn toggle_label(cx: &App) -> &'static str {
    if cx.theme().is_dark() {
        "Switch to Latte"
    } else {
        "Switch to Mocha"
    }
}

/// Catppuccin roles the design uses beyond the kit's theme slots.
#[derive(Clone, Copy)]
pub struct Palette {
    pub mantle: Hsla,
    pub base: Hsla,
    pub surface0: Hsla,
    pub surface1: Hsla,
    pub subtext1: Hsla,
    pub subtext0: Hsla,
    pub text: Hsla,
    pub blue: Hsla,
    pub green: Hsla,
    pub red: Hsla,
    pub yellow: Hsla,
    /// Text on a yellow tint; darker than Yellow in Latte for contrast.
    pub yellow_text: Hsla,
    /// Shell commands (`!`): Catppuccin Maroon.
    pub shell: Hsla,
    /// Text in shell colour; darker than Shell in Latte for contrast.
    pub shell_text: Hsla,
    /// Text colour of added lines; darker than Green in Latte for contrast.
    pub add: Hsla,
    pub delete: Hsla,
    pub on_accent: Hsla,
}

pub fn palette(cx: &App) -> Palette {
    let c = |hex: u32| -> Hsla { rgb(hex).into() };
    if cx.theme().is_dark() {
        Palette {
            mantle: c(0x181825),
            base: c(0x1e1e2e),
            surface0: c(0x313244),
            surface1: c(0x45475a),
            subtext1: c(0xbac2de),
            subtext0: c(0xa6adc8),
            text: c(0xcdd6f4),
            blue: c(0x89b4fa),
            green: c(0xa6e3a1),
            red: c(0xf38ba8),
            yellow: c(0xf9e2af),
            yellow_text: c(0xf9e2af),
            shell: c(0xeba0ac),
            shell_text: c(0xeba0ac),
            add: c(0xa6e3a1),
            delete: c(0xf38ba8),
            on_accent: c(0x11111b),
        }
    } else {
        Palette {
            mantle: c(0xe6e9ef),
            base: c(0xeff1f5),
            surface0: c(0xccd0da),
            surface1: c(0xbcc0cc),
            subtext1: c(0x5c5f77),
            subtext0: c(0x5c5f77),
            text: c(0x4c4f69),
            blue: c(0x1e66f5),
            green: c(0x40a02b),
            red: c(0xd20f39),
            yellow: c(0xdf8e1d),
            yellow_text: c(0x8a5a10),
            shell: c(0xe64553),
            shell_text: c(0xc4283d),
            add: c(0x2f7a1f),
            delete: c(0xb80d32),
            on_accent: c(0xeff1f5),
        }
    }
}

/// `color-mix(in srgb, a p%, b)`, which the design uses for tints.
pub fn mix(a: Hsla, b: Hsla, amount: f32) -> Hsla {
    let a = a.to_rgb();
    let b = b.to_rgb();
    let lerp = |x: f32, y: f32| x * amount + y * (1.0 - amount);
    gpui_kit::Rgba {
        r: lerp(a.r, b.r),
        g: lerp(a.g, b.g),
        b: lerp(a.b, b.b),
        a: lerp(a.a, b.a),
    }
    .into()
}
