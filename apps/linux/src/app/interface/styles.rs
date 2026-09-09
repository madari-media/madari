//! Bundled fonts and application stylesheet.
use super::*;

pub(in crate::app) fn install_style() {
    // Register bundled fonts in this process only; no system font configuration changes.
    let fonts = data_directory().join("fonts");
    if madari_native::private_directory(&fonts).is_ok()
        && let Some(map) = gtk::Label::new(None).pango_context().font_map()
    {
        for (name, bytes) in [
            (
                "DMSans-Variable.ttf",
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/fonts/DMSans-Variable.ttf"
                ))
                .as_slice(),
            ),
            (
                "DMSans-Italic-Variable.ttf",
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/fonts/DMSans-Italic-Variable.ttf"
                ))
                .as_slice(),
            ),
        ] {
            let path = fonts.join(name);
            if std::fs::write(&path, bytes).is_ok() {
                let _ = map.add_font_file(&path);
            }
        }
        let _ = std::fs::write(
            fonts.join("OFL.txt"),
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts/OFL.txt")),
        );
    }
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("styles.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
