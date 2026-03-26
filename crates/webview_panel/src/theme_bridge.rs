use gpui::{Hsla, Rgba};
use theme::ThemeColors;

fn hsla_to_css(color: Hsla) -> String {
    let rgba: Rgba = color.into();
    format!(
        "rgba({},{},{},{:.3})",
        (rgba.r * 255.0) as u8,
        (rgba.g * 255.0) as u8,
        (rgba.b * 255.0) as u8,
        rgba.a,
    )
}

/// Generate a JavaScript snippet that injects (or updates) a `<style>` element
/// with Zed theme colors mapped to CSS custom properties on `:root`.
pub fn build_theme_css_script(colors: &ThemeColors) -> String {
    let vars = [
        ("--zed-background", colors.background),
        ("--zed-surface-background", colors.surface_background),
        (
            "--zed-elevated-surface-background",
            colors.elevated_surface_background,
        ),
        ("--zed-panel-background", colors.panel_background),
        ("--zed-text", colors.text),
        ("--zed-text-muted", colors.text_muted),
        ("--zed-text-placeholder", colors.text_placeholder),
        ("--zed-text-disabled", colors.text_disabled),
        ("--zed-text-accent", colors.text_accent),
        ("--zed-icon", colors.icon),
        ("--zed-icon-muted", colors.icon_muted),
        ("--zed-border", colors.border),
        ("--zed-border-variant", colors.border_variant),
        ("--zed-border-focused", colors.border_focused),
        ("--zed-border-selected", colors.border_selected),
        ("--zed-border-disabled", colors.border_disabled),
        ("--zed-element-background", colors.element_background),
        ("--zed-element-hover", colors.element_hover),
        ("--zed-element-active", colors.element_active),
        ("--zed-element-selected", colors.element_selected),
        ("--zed-element-disabled", colors.element_disabled),
        ("--zed-status-bar-background", colors.status_bar_background),
        ("--zed-tab-bar-background", colors.tab_bar_background),
        ("--zed-tab-active-background", colors.tab_active_background),
        (
            "--zed-tab-inactive-background",
            colors.tab_inactive_background,
        ),
    ]
    .into_iter()
    .map(|(name, color)| format!("{name}: {}", hsla_to_css(color)))
    .collect::<Vec<_>>()
    .join("; ");

    format!(
        r#"(function(){{var s=document.getElementById('__zed_theme')||document.createElement('style');s.id='__zed_theme';s.textContent=':root {{ {vars} }}';if(!s.parentNode)document.head.appendChild(s);}})();"#,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsla_to_css_formats_correctly() {
        let color = gpui::hsla(0.0, 1.0, 0.5, 1.0);
        let css = hsla_to_css(color);
        assert!(css.starts_with("rgba("));
        assert!(css.ends_with(")"));
        assert!(css.contains(",1.000"));
    }
}
