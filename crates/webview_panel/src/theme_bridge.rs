use gpui::{Hsla, Rgba};
use theme::{StatusColors, ThemeColors};

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
    build_theme_css_script_with_status(colors, None)
}

fn build_theme_css_script_with_status(
    colors: &ThemeColors,
    status: Option<&StatusColors>,
) -> String {
    let mut vars: Vec<(&str, Hsla)> = vec![
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
        ("--zed-icon-disabled", colors.icon_disabled),
        ("--zed-icon-placeholder", colors.icon_placeholder),
        ("--zed-icon-accent", colors.icon_accent),
        ("--zed-border", colors.border),
        ("--zed-border-variant", colors.border_variant),
        ("--zed-border-focused", colors.border_focused),
        ("--zed-border-selected", colors.border_selected),
        ("--zed-border-transparent", colors.border_transparent),
        ("--zed-border-disabled", colors.border_disabled),
        ("--zed-element-background", colors.element_background),
        ("--zed-element-hover", colors.element_hover),
        ("--zed-element-active", colors.element_active),
        ("--zed-element-selected", colors.element_selected),
        (
            "--zed-element-selection-background",
            colors.element_selection_background,
        ),
        ("--zed-element-disabled", colors.element_disabled),
        ("--zed-drop-target-background", colors.drop_target_background),
        (
            "--zed-ghost-element-background",
            colors.ghost_element_background,
        ),
        ("--zed-ghost-element-hover", colors.ghost_element_hover),
        ("--zed-ghost-element-active", colors.ghost_element_active),
        (
            "--zed-ghost-element-selected",
            colors.ghost_element_selected,
        ),
        (
            "--zed-ghost-element-disabled",
            colors.ghost_element_disabled,
        ),
        ("--zed-status-bar-background", colors.status_bar_background),
        ("--zed-title-bar-background", colors.title_bar_background),
        ("--zed-toolbar-background", colors.toolbar_background),
        ("--zed-tab-bar-background", colors.tab_bar_background),
        ("--zed-tab-active-background", colors.tab_active_background),
        (
            "--zed-tab-inactive-background",
            colors.tab_inactive_background,
        ),
        (
            "--zed-search-match-background",
            colors.search_match_background,
        ),
        ("--zed-panel-focused-border", colors.panel_focused_border),
        (
            "--zed-scrollbar-thumb-background",
            colors.scrollbar_thumb_background,
        ),
        (
            "--zed-scrollbar-thumb-hover-background",
            colors.scrollbar_thumb_hover_background,
        ),
        ("--zed-scrollbar-thumb-border", colors.scrollbar_thumb_border),
        (
            "--zed-scrollbar-track-background",
            colors.scrollbar_track_background,
        ),
        (
            "--zed-scrollbar-track-border",
            colors.scrollbar_track_border,
        ),
        ("--zed-editor-background", colors.editor_background),
        ("--zed-editor-foreground", colors.editor_foreground),
        ("--zed-terminal-background", colors.terminal_background),
        ("--zed-terminal-foreground", colors.terminal_foreground),
        ("--zed-link-text-hover", colors.link_text_hover),
    ];

    if let Some(status) = status {
        vars.extend_from_slice(&[
            ("--zed-conflict", status.conflict),
            ("--zed-conflict-background", status.conflict_background),
            ("--zed-conflict-border", status.conflict_border),
            ("--zed-created", status.created),
            ("--zed-created-background", status.created_background),
            ("--zed-created-border", status.created_border),
            ("--zed-deleted", status.deleted),
            ("--zed-deleted-background", status.deleted_background),
            ("--zed-deleted-border", status.deleted_border),
            ("--zed-error", status.error),
            ("--zed-error-background", status.error_background),
            ("--zed-error-border", status.error_border),
            ("--zed-hidden", status.hidden),
            ("--zed-hidden-background", status.hidden_background),
            ("--zed-hidden-border", status.hidden_border),
            ("--zed-hint", status.hint),
            ("--zed-hint-background", status.hint_background),
            ("--zed-hint-border", status.hint_border),
            ("--zed-ignored", status.ignored),
            ("--zed-ignored-background", status.ignored_background),
            ("--zed-ignored-border", status.ignored_border),
            ("--zed-info", status.info),
            ("--zed-info-background", status.info_background),
            ("--zed-info-border", status.info_border),
            ("--zed-modified", status.modified),
            ("--zed-modified-background", status.modified_background),
            ("--zed-modified-border", status.modified_border),
            ("--zed-predictive", status.predictive),
            ("--zed-predictive-background", status.predictive_background),
            ("--zed-predictive-border", status.predictive_border),
            ("--zed-renamed", status.renamed),
            ("--zed-renamed-background", status.renamed_background),
            ("--zed-renamed-border", status.renamed_border),
            ("--zed-success", status.success),
            ("--zed-success-background", status.success_background),
            ("--zed-success-border", status.success_border),
            ("--zed-unreachable", status.unreachable),
            (
                "--zed-unreachable-background",
                status.unreachable_background,
            ),
            ("--zed-unreachable-border", status.unreachable_border),
            ("--zed-warning", status.warning),
            ("--zed-warning-background", status.warning_background),
            ("--zed-warning-border", status.warning_border),
        ]);
    }

    let css_vars = vars
        .into_iter()
        .map(|(name, color)| format!("{name}: {}", hsla_to_css(color)))
        .collect::<Vec<_>>()
        .join("; ");

    format!(
        r#"(function(){{var s=document.getElementById('__zed_theme')||document.createElement('style');s.id='__zed_theme';s.textContent=':root {{ {css_vars} }}';if(!s.parentNode)document.head.appendChild(s);}})();"#,
    )
}

pub const UI_COMPONENT_CSS: &str = r#"
.zed-panel {
    background: var(--zed-panel-background);
    color: var(--zed-text);
    font-family: system-ui, -apple-system, BlinkMacSystemFont, sans-serif;
    height: 100%;
    width: 100%;
    margin: 0;
    padding: 0;
    box-sizing: border-box;
}
.zed-list {
    list-style: none;
    margin: 0;
    padding: 0;
}
.zed-list-item {
    padding: 4px 8px;
    border-bottom: 1px solid var(--zed-border-variant);
    cursor: pointer;
    transition: background 0.1s ease;
}
.zed-list-item:hover {
    background: var(--zed-ghost-element-hover);
}
.zed-list-item.active {
    background: var(--zed-ghost-element-selected);
}
.zed-badge {
    display: inline-block;
    padding: 1px 6px;
    border-radius: 4px;
    font-size: 0.75em;
    font-weight: 600;
    background: var(--zed-element-background);
    color: var(--zed-text);
}
.zed-badge.success {
    background: var(--zed-success-background, var(--zed-element-background));
    color: var(--zed-success, var(--zed-text));
}
.zed-badge.error {
    background: var(--zed-error-background, var(--zed-element-background));
    color: var(--zed-error, var(--zed-text));
}
.zed-badge.warning {
    background: var(--zed-warning-background, var(--zed-element-background));
    color: var(--zed-warning, var(--zed-text));
}
.zed-badge.info {
    background: var(--zed-info-background, var(--zed-element-background));
    color: var(--zed-info, var(--zed-text));
}
.zed-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 4px 12px;
    border: 1px solid var(--zed-border);
    border-radius: 4px;
    background: var(--zed-element-background);
    color: var(--zed-text);
    font-size: 0.85em;
    cursor: pointer;
    transition: background 0.1s ease;
}
.zed-button:hover {
    background: var(--zed-element-hover);
}
.zed-button:active {
    background: var(--zed-element-active);
}
.zed-button.primary {
    background: var(--zed-text-accent);
    color: var(--zed-background);
    border-color: var(--zed-text-accent);
}
.zed-button.primary:hover {
    opacity: 0.9;
}
.zed-input {
    padding: 4px 8px;
    border: 1px solid var(--zed-border);
    border-radius: 4px;
    background: var(--zed-background);
    color: var(--zed-text);
    font-size: 0.85em;
    outline: none;
    transition: border-color 0.1s ease;
}
.zed-input::placeholder {
    color: var(--zed-text-placeholder);
}
.zed-input:focus {
    border-color: var(--zed-border-focused);
}
.zed-card {
    background: var(--zed-surface-background);
    border: 1px solid var(--zed-border-variant);
    border-radius: 6px;
    padding: 12px;
}
.zed-divider {
    border: none;
    border-top: 1px solid var(--zed-border-variant);
    margin: 8px 0;
}
.zed-text-muted {
    color: var(--zed-text-muted);
}
.zed-text-accent {
    color: var(--zed-text-accent);
}
.zed-mono {
    font-family: "Zed Mono", "Fira Code", "Cascadia Code", "JetBrains Mono", monospace;
}
"#;

/// Build a full initialization script that injects both theme CSS variables
/// and the UI component CSS library into a webview.
pub fn build_full_theme_script(colors: &ThemeColors, status: &StatusColors) -> String {
    let theme_script = build_theme_css_script_with_status(colors, Some(status));

    let escaped_ui_css = UI_COMPONENT_CSS.replace('\\', "\\\\").replace('\'', "\\'").replace('\n', "\\n");

    let ui_script = format!(
        r#"(function(){{var s=document.getElementById('__zed_ui')||document.createElement('style');s.id='__zed_ui';s.textContent='{escaped_ui_css}';if(!s.parentNode)document.head.appendChild(s);}})();"#,
    );

    format!("{theme_script}\n{ui_script}")
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

    #[test]
    fn build_theme_css_script_contains_expected_variables() {
        let colors = ThemeColors::light();
        let script = build_theme_css_script(&colors);
        assert!(script.contains("--zed-background"));
        assert!(script.contains("--zed-text"));
        assert!(script.contains("--zed-ghost-element-hover"));
        assert!(script.contains("--zed-scrollbar-thumb-background"));
        assert!(script.contains("__zed_theme"));
    }

    #[test]
    fn build_full_theme_script_contains_both_sections() {
        let colors = ThemeColors::light();
        let status = StatusColors::light();
        let script = build_full_theme_script(&colors, &status);
        assert!(script.contains("__zed_theme"));
        assert!(script.contains("__zed_ui"));
        assert!(script.contains("--zed-error"));
        assert!(script.contains("--zed-success"));
        assert!(script.contains("--zed-warning"));
        assert!(script.contains(".zed-panel"));
        assert!(script.contains(".zed-button"));
    }

    #[test]
    fn ui_component_css_has_all_classes() {
        assert!(UI_COMPONENT_CSS.contains(".zed-panel"));
        assert!(UI_COMPONENT_CSS.contains(".zed-list-item"));
        assert!(UI_COMPONENT_CSS.contains(".zed-list-item.active"));
        assert!(UI_COMPONENT_CSS.contains(".zed-badge"));
        assert!(UI_COMPONENT_CSS.contains(".zed-badge.success"));
        assert!(UI_COMPONENT_CSS.contains(".zed-badge.error"));
        assert!(UI_COMPONENT_CSS.contains(".zed-badge.warning"));
        assert!(UI_COMPONENT_CSS.contains(".zed-badge.info"));
        assert!(UI_COMPONENT_CSS.contains(".zed-button"));
        assert!(UI_COMPONENT_CSS.contains(".zed-button.primary"));
        assert!(UI_COMPONENT_CSS.contains(".zed-input"));
        assert!(UI_COMPONENT_CSS.contains(".zed-card"));
        assert!(UI_COMPONENT_CSS.contains(".zed-divider"));
        assert!(UI_COMPONENT_CSS.contains(".zed-text-muted"));
        assert!(UI_COMPONENT_CSS.contains(".zed-text-accent"));
        assert!(UI_COMPONENT_CSS.contains(".zed-mono"));
    }
}
