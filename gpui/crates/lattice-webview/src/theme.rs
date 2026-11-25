//! The host's theme as an island's page receives it.

use gpui::{App, Hsla, Rgba};
use lattice_ui::{
    ActiveLattice as _, Appearance, CodeTheme, editor_theme,
    palette::{Accent, Palette, Tint},
};
use serde_json::{Value, json};

/// How an island's page takes the host's theme (`host:theme`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IslandTheme {
    /// The palette's colours under the Tauri app's variable names, set on the
    /// root element: a page written for the GPUI app (the HTML viewer's).
    #[default]
    Variables,
    /// The root element's `data-theme`, `data-tint` and `data-accent`, and a
    /// custom accent's `--accent`, exactly as the Tauri app sets them
    /// (`src/settings/use-appearance.ts`): a page that loads the app's own
    /// stylesheets, whose every token then resolves as it does there.
    Document,
}

fn css(color: Hsla) -> String {
    let rgba = Rgba::from(color);
    format!(
        "rgba({}, {}, {}, {:.3})",
        (rgba.r * 255.).round(),
        (rgba.g * 255.).round(),
        (rgba.b * 255.).round(),
        rgba.a
    )
}

/// The host's theme for a page that takes it as `mode` says.
pub fn theme_payload(mode: IslandTheme, cx: &App) -> Value {
    match mode {
        IslandTheme::Variables => variables(cx),
        IslandTheme::Document => document(
            cx.palette(),
            cx.lattice().code_theme,
            cx.lattice().code_theme_variant,
        ),
    }
}

/// The root element's attributes the Tauri app sets for this palette
/// (`src/settings/use-appearance.ts`): its stylesheets resolve every token
/// from them. A custom accent is a colour the CSS cannot derive: it comes
/// fitted, as `--accent`. So is a chosen theme (`docs/themes.md`): its base
/// variables come as `vars`, set inline as the Tauri app sets them, or the
/// page (the Agent's Synara host among them) would stay on the stock palette.
/// The code theme comes as its token roles' `--syntax-*` colours, as the
/// Tauri app sets them on the root, and `data-code-theme-kind`, the way its
/// ground goes; the Markdown editor's code blocks paint with them. A fixed
/// `variant` paints them as the source editor's Zed theme does: the code
/// theme in that appearance, the chosen theme's syntax laid over it.
fn document(palette: &Palette, code_theme: CodeTheme, variant: Option<Appearance>) -> Value {
    let theme = if palette.is_dark() { "dark" } else { "light" };
    let tint = match palette.tint {
        Tint::Graphite => "graphite",
        Tint::Paper => "paper",
        Tint::Sage => "sage",
        Tint::Mist => "mist",
        Tint::Dusk => "dusk",
    };
    let accent = match palette.accent_preset {
        Accent::Graphite => "graphite",
        Accent::Blue => "blue",
        Accent::Purple => "purple",
        Accent::Pink => "pink",
        Accent::Orange => "orange",
        Accent::Green => "green",
        Accent::Teal => "teal",
        Accent::Custom(_) => "custom",
        Accent::Theme if theme_accent(palette) => "custom",
        Accent::Theme => "graphite",
    };
    let code_palette = match variant {
        Some(appearance) if appearance != palette.appearance => {
            Palette::new(appearance, palette.tint, palette.accent_preset)
                .with_theme(palette.theme.clone())
        }
        _ => palette.clone(),
    };
    let (code_kind, syntax) = editor_theme::syntax_properties(code_theme, &code_palette);
    // The chosen theme's `--syntax-*` are in `syntax` already, through the
    // code palette, in the variant's appearance.
    let themed = palette
        .themed
        .iter()
        .flat_map(|themed| themed.css_variables())
        .filter(|(name, _)| name != "--accent" && !name.starts_with("--syntax-"));
    let vars: serde_json::Map<String, Value> = syntax
        .into_iter()
        .map(|(name, color)| (name.to_owned(), css(color)))
        .chain(themed)
        .map(|(name, value)| (name, Value::String(value)))
        .collect();
    let code_kind = match code_kind {
        Appearance::Light => "light",
        Appearance::Dark => "dark",
    };
    json!({
        "appearance": theme,
        "dataset": { "theme": theme, "tint": tint, "accent": accent, "codeThemeKind": code_kind },
        "accent": (matches!(palette.accent_preset, Accent::Custom(_)) || theme_accent(palette))
            .then(|| css(palette.accent)),
        "vars": vars,
    })
}

/// Whether the accent is the chosen theme's own colour, which the CSS
/// cannot derive and so is sent like a custom one.
fn theme_accent(palette: &Palette) -> bool {
    palette.accent_preset == Accent::Theme
        && palette
            .themed
            .as_ref()
            .is_some_and(|themed| themed.accent.is_some())
}

/// The CSS variables an island's page gets from the host's theme: the
/// Tauri app's own names (`src/styles/theme.css`), so a page ported from it
/// reads the same variables it did as an iframe of the app.
fn variables(cx: &App) -> Value {
    let palette = cx.palette();
    json!({
        "appearance": if palette.is_dark() { "dark" } else { "light" },
        "vars": {
            "--bg": css(palette.window),
            "--window": css(palette.window),
            "--chrome-surface": css(palette.chrome),
            "--side-panel": css(palette.side_panel),
            "--panel": css(palette.panel),
            "--panel-strong": css(palette.raised),
            "--editor-bg": css(palette.editor),
            "--pdf-bg": css(palette.pdf_ground),
            "--text": css(palette.text),
            "--text-primary": css(palette.text),
            "--muted": css(palette.text_secondary),
            "--line": css(palette.line),
            "--line-strong": css(palette.line_strong),
            "--accent": css(palette.accent),
            "--accent-soft": css(palette.accent_soft),
            "--focus": css(palette.focus),
            "--success": css(palette.success),
            "--danger": css(palette.danger),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_page_gets_the_tauri_apps_root_attributes() {
        let palette = Palette::new(Appearance::Dark, Tint::Sage, Accent::Blue);
        let payload = document(&palette, CodeTheme::Vscode, None);
        assert_eq!(
            payload["dataset"],
            json!({ "theme": "dark", "tint": "sage", "accent": "blue", "codeThemeKind": "dark" })
        );
        assert_eq!(payload["appearance"], "dark");
        assert_eq!(payload["accent"], Value::Null);
        let custom = Palette::new(Appearance::Light, Tint::Graphite, Accent::Custom(0x3366cc));
        let payload = document(&custom, CodeTheme::Vscode, None);
        assert_eq!(payload["dataset"]["accent"], "custom");
        assert!(payload["accent"].as_str().unwrap().starts_with("rgba("));
    }

    #[test]
    fn a_document_page_gets_a_chosen_themes_variables() {
        let nord = lattice_theme::bundled()
            .into_iter()
            .find(|theme| theme.id == "app.lattice.nord")
            .expect("Nord is bundled");
        let palette = Palette::new(Appearance::Dark, Tint::Sage, Accent::Blue)
            .with_theme(Some(std::sync::Arc::new(nord)));
        let payload = document(&palette, CodeTheme::Vscode, None);
        assert_eq!(payload["dataset"]["tint"], "graphite");
        assert_eq!(payload["vars"]["--side-panel"], "#262c37");
        assert!(payload["vars"].get("--accent").is_none());
    }

    /// The Markdown island's code blocks paint with the code theme's roles,
    /// as the source editor does.
    #[test]
    fn a_document_page_gets_the_code_themes_syntax_colours() {
        let palette = Palette::new(Appearance::Light, Tint::Graphite, Accent::Graphite);
        let (_, syntax) = editor_theme::syntax_properties(CodeTheme::Vscode, &palette);
        let payload = document(&palette, CodeTheme::Vscode, None);
        for (name, color) in syntax {
            assert_eq!(payload["vars"][name], css(color), "{name}");
        }
        let dracula = document(&palette, CodeTheme::Dracula, None);
        assert_eq!(dracula["dataset"]["codeThemeKind"], "dark");
        assert_ne!(
            dracula["vars"]["--syntax-keyword"],
            payload["vars"]["--syntax-keyword"]
        );
    }

    /// A fixed variant paints the code blocks as the source editor's theme:
    /// VS Code set to Dark in a light app sends Dark+ and the dark kind, so
    /// the stylesheet's opposite-appearance ground applies.
    #[test]
    fn a_document_page_follows_the_code_themes_fixed_variant() {
        let light = Palette::new(Appearance::Light, Tint::Graphite, Accent::Graphite);
        let dark = Palette::new(Appearance::Dark, Tint::Graphite, Accent::Graphite);
        let payload = document(&light, CodeTheme::Vscode, Some(Appearance::Dark));
        assert_eq!(payload["dataset"]["theme"], "light");
        assert_eq!(payload["dataset"]["codeThemeKind"], "dark");
        let (_, syntax) = editor_theme::syntax_properties(CodeTheme::Vscode, &dark);
        for (name, color) in syntax {
            assert_eq!(payload["vars"][name], css(color), "{name}");
        }
        assert_eq!(
            document(&light, CodeTheme::Vscode, Some(Appearance::Light)),
            document(&light, CodeTheme::Vscode, None)
        );
    }

    /// An installed theme's syntax colours win in the island's code blocks
    /// (lattice#1268), in a fixed variant's appearance too, where the theme
    /// resolves its keyword as #FFDC84; on a code theme's own ground they are
    /// the source editor's, fitted to that ground.
    #[test]
    fn a_document_page_gets_an_installed_themes_syntax_colours() {
        let mut nord = lattice_theme::bundled()
            .into_iter()
            .find(|theme| theme.id == "app.lattice.nord")
            .expect("Nord is bundled");
        for variant in &mut nord.variants {
            variant
                .syntax
                .insert("keyword".into(), lattice_theme::Color::opaque(0xFFDC84));
        }
        let palette = Palette::new(Appearance::Light, Tint::Graphite, Accent::Graphite)
            .with_theme(Some(std::sync::Arc::new(nord)));
        let rgb = palette.themed.as_ref().unwrap().syntax["keyword"].rgb;
        let keyword = format!(
            "rgba({}, {}, {}, 1.000)",
            rgb >> 16,
            (rgb >> 8) & 0xFF,
            rgb & 0xFF
        );
        let payload = document(&palette, CodeTheme::Vscode, None);
        assert_eq!(payload["vars"]["--syntax-keyword"], keyword);
        let dark = document(&palette, CodeTheme::Vscode, Some(Appearance::Dark));
        assert_eq!(
            dark["vars"]["--syntax-keyword"],
            "rgba(255, 220, 132, 1.000)"
        );
        for code_theme in CodeTheme::ALL {
            let (_, syntax) = editor_theme::syntax_properties(code_theme, &palette);
            let payload = document(&palette, code_theme, None);
            assert_eq!(
                payload["vars"]["--syntax-keyword"],
                css(syntax[0].1),
                "{code_theme:?}"
            );
        }
    }
}
