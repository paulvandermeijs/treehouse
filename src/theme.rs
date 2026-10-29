//! Catppuccin Mocha and Latte, as the design specifies them.
//!
//! The gpui-kit theme carries most colours. The design also names a few
//! Catppuccin roles the kit has no slot for (Mantle, Subtext 1, the darker
//! diff greens and reds in Latte); [`Palette`] holds those.

use gpui_kit::component::{ActiveTheme as _, Theme, ThemeMode, ThemeRegistry};
use gpui_kit::{App, Global, Hsla, SharedString, Window, rgb};
use std::borrow::Cow;
use std::sync::Mutex;

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
    refresh_installed_fonts(cx);
    apply(Config::global(cx).theme, None, cx);
}

/// The installed font families, and the fonts in use: the settings' when
/// they're installed, else the last ones that were, so a half-typed name
/// doesn't flash through fallback fonts.
struct Fonts {
    installed: Vec<String>,
    ui: SharedString,
    code: SharedString,
}

impl Global for Fonts {}

/// Lists the installed fonts again; Settings does when it opens, so a font
/// installed since is found.
pub fn refresh_installed_fonts(cx: &mut App) {
    let installed = cx.text_system().all_font_names();
    match cx.try_global::<Fonts>() {
        Some(_) => cx.global_mut::<Fonts>().installed = installed,
        None => cx.set_global(Fonts {
            installed,
            ui: SANS.into(),
            code: MONO.into(),
        }),
    }
}

pub fn installed_fonts(cx: &App) -> &[String] {
    cx.try_global::<Fonts>()
        .map(|f| f.installed.as_slice())
        .unwrap_or_default()
}

/// What a font setting resolves to.
#[derive(Debug, PartialEq)]
pub enum FontStatus {
    BuiltIn,
    Installed,
    /// Installed under this spelling.
    Suggest(String),
    Missing,
}

pub fn font_status(typed: &str, installed: &[String]) -> FontStatus {
    let typed = typed.trim();
    if typed.is_empty() {
        return FontStatus::BuiltIn;
    }
    if installed.iter().any(|name| name == typed) {
        return FontStatus::Installed;
    }
    let squash = |s: &str| -> String {
        s.chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let wanted = squash(typed);
    match installed.iter().find(|name| squash(name) == wanted) {
        Some(name) => FontStatus::Suggest(name.clone()),
        None => FontStatus::Missing,
    }
}

/// The font a setting uses now: its own when installed, the built-in one
/// when empty, else the last one in use.
fn resolve(typed: &str, built_in: &str, last: &SharedString, installed: &[String]) -> SharedString {
    match font_status(typed, installed) {
        FontStatus::BuiltIn => built_in.into(),
        FontStatus::Installed => typed.trim().to_string().into(),
        FontStatus::Suggest(_) | FontStatus::Missing => last.clone(),
    }
}

/// Puts the interface and code fonts from the settings on the theme.
pub fn apply_fonts(cx: &mut App) {
    if cx.try_global::<Fonts>().is_none() {
        refresh_installed_fonts(cx);
    }
    let config = Config::global(cx);
    let fonts = cx.global::<Fonts>();
    let ui = resolve(&config.ui_font, SANS, &fonts.ui, &fonts.installed);
    let code = resolve(&config.code_font, MONO, &fonts.code, &fonts.installed);
    let fonts = cx.global_mut::<Fonts>();
    fonts.ui = ui.clone();
    fonts.code = code.clone();
    if let Ok(mut in_use) = IN_USE.lock() {
        *in_use = Some((ui.clone(), code.clone()));
    }
    Theme::update(cx, |theme| {
        theme.font_family = ui;
        theme.mono_font_family = code;
    });
}

/// The interface and code fonts in use, for drawing code that has a
/// palette but no `App` at hand.
static IN_USE: Mutex<Option<(SharedString, SharedString)>> = Mutex::new(None);

/// The interface font in use.
pub fn sans() -> SharedString {
    IN_USE
        .lock()
        .ok()
        .and_then(|f| f.as_ref().map(|(ui, _)| ui.clone()))
        .unwrap_or(SharedString::new_static(SANS))
}

/// The code font in use.
pub fn mono() -> SharedString {
    IN_USE
        .lock()
        .ok()
        .and_then(|f| f.as_ref().map(|(_, code)| code.clone()))
        .unwrap_or(SharedString::new_static(MONO))
}

/// Switches between Mocha and Latte, or follows the system appearance.
pub fn apply(choice: ThemeChoice, window: Option<&mut Window>, cx: &mut App) {
    // Tell macOS too, so its own drawing (the title bar buttons, menus,
    // native dialogs) matches the theme rather than the system setting.
    cx.set_window_appearance(match choice {
        ThemeChoice::Mocha => Some(gpui_kit::WindowAppearance::Dark),
        ThemeChoice::Latte => Some(gpui_kit::WindowAppearance::Light),
        ThemeChoice::System => None,
    });
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
    // Changing the mode loads the theme file's fonts again.
    apply_fonts(cx);
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
    /// Uncommitted changes in the sidebar.
    pub peach: Hsla,
    /// Text on a yellow tint; darker than Yellow in Latte for contrast.
    pub yellow_text: Hsla,
    /// Commands you run with `!`: Catppuccin Yellow, a note to take care
    /// that doesn't read as an error the way Maroon did.
    pub shell: Hsla,
    /// Text in shell colour; darker than Shell in Latte for contrast.
    pub shell_text: Hsla,
    /// The icon on the shell badge.
    pub on_shell: Hsla,
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
            peach: c(0xfab387),
            yellow_text: c(0xf9e2af),
            shell: c(0xf9e2af),
            shell_text: c(0xf9e2af),
            on_shell: c(0x11111b),
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
            peach: c(0xfe640b),
            yellow_text: c(0x8a5a10),
            shell: c(0xdf8e1d),
            shell_text: c(0x8a5a10),
            on_shell: c(0x4c4f69),
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

#[cfg(test)]
mod tests {
    use super::{FontStatus, font_status, resolve};
    use gpui_kit::SharedString;

    fn installed() -> Vec<String> {
        ["IBM Plex Mono", "JetBrains Mono", "Inter"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn font_names_match_exactly_or_suggest_the_systems_spelling() {
        let installed = installed();
        assert_eq!(font_status("  ", &installed), FontStatus::BuiltIn);
        assert_eq!(font_status(" Inter ", &installed), FontStatus::Installed);
        assert_eq!(
            font_status("jetbrainsmono", &installed),
            FontStatus::Suggest("JetBrains Mono".into())
        );
        assert_eq!(
            font_status("Berkeley Mono", &installed),
            FontStatus::Missing
        );
    }

    #[test]
    fn a_missing_font_keeps_the_last_one_in_use() {
        let installed = installed();
        let last = SharedString::from("JetBrains Mono");
        assert_eq!(
            resolve("", "IBM Plex Mono", &last, &installed),
            "IBM Plex Mono"
        );
        assert_eq!(
            resolve("Inter", "IBM Plex Mono", &last, &installed),
            "Inter"
        );
        assert_eq!(
            resolve("Inte", "IBM Plex Mono", &last, &installed),
            "JetBrains Mono"
        );
    }
}
