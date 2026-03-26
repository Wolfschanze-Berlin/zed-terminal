use std::path::PathBuf;
use std::sync::Arc;

use chrono::{Datelike, NaiveDate, NaiveDateTime, Utc};
use gpui::{
    App, BorderStyle, Bounds, Context, EventEmitter, FocusHandle, Focusable, Hsla, SharedString,
    actions, canvas, px, quad,
};
use log::error;
use serde::Deserialize;
use ui::{Tooltip, prelude::*};
use workspace::item::{Item, ItemEvent};
use workspace::Workspace;

actions!(analytics_panel, [Open]);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _cx| {
        workspace.register_action(|workspace, _: &Open, window, cx| {
            let view = cx.new(|cx| AnalyticsPanel::new(cx));
            workspace.add_item_to_center(Box::new(view), window, cx);
        });
    })
    .detach();
}

// ── Data types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct SessionRecord {
    editor: String,
    #[serde(default)]
    #[allow(dead_code)]
    model: String,
    tokens_in: u64,
    tokens_out: u64,
    cost: f64,
    timestamp: String,
    #[serde(default)]
    duration_s: u64,
}

#[derive(Debug, Clone, Default)]
struct AnalyticsSummary {
    total_sessions: usize,
    total_tokens_in: u64,
    total_tokens_out: u64,
    total_cost: f64,
    total_duration_s: u64,
    editor_counts: Vec<(String, usize)>,
    daily_sessions: Vec<(NaiveDate, usize)>,
}

// ── Data directory ──────────────────────────────────────────────────

fn agentics_sessions_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".agentics")
        .join("sessions")
}

fn load_sessions_from_disk() -> Vec<SessionRecord> {
    let dir = agentics_sessions_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut records = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(record) = serde_json::from_str::<SessionRecord>(line) {
                    records.push(record);
                }
            }
        }
    }
    records
}

fn build_summary(records: &[SessionRecord]) -> AnalyticsSummary {
    if records.is_empty() {
        return AnalyticsSummary::default();
    }

    let mut editor_map = std::collections::HashMap::<String, usize>::new();
    let mut day_map = std::collections::HashMap::<NaiveDate, usize>::new();
    let mut total_tokens_in = 0u64;
    let mut total_tokens_out = 0u64;
    let mut total_cost = 0.0f64;
    let mut total_duration_s = 0u64;

    for record in records {
        total_tokens_in += record.tokens_in;
        total_tokens_out += record.tokens_out;
        total_cost += record.cost;
        total_duration_s += record.duration_s;

        *editor_map.entry(record.editor.clone()).or_default() += 1;

        if let Ok(dt) = NaiveDateTime::parse_from_str(&record.timestamp, "%Y-%m-%dT%H:%M:%S") {
            *day_map.entry(dt.date()).or_default() += 1;
        } else if let Ok(dt) =
            NaiveDateTime::parse_from_str(&record.timestamp, "%Y-%m-%dT%H:%M:%S%.f")
        {
            *day_map.entry(dt.date()).or_default() += 1;
        } else if let Ok(date) = NaiveDate::parse_from_str(&record.timestamp, "%Y-%m-%d") {
            *day_map.entry(date).or_default() += 1;
        }
    }

    let mut editor_counts: Vec<(String, usize)> = editor_map.into_iter().collect();
    editor_counts.sort_by(|a, b| b.1.cmp(&a.1));

    let mut daily_sessions: Vec<(NaiveDate, usize)> = day_map.into_iter().collect();
    daily_sessions.sort_by_key(|(date, _)| *date);

    AnalyticsSummary {
        total_sessions: records.len(),
        total_tokens_in,
        total_tokens_out,
        total_cost,
        total_duration_s,
        editor_counts,
        daily_sessions,
    }
}

// ── View ────────────────────────────────────────────────────────────

pub struct AnalyticsPanel {
    focus_handle: FocusHandle,
    summary: Option<AnalyticsSummary>,
    loading: bool,
    _load_task: Option<gpui::Task<()>>,
}

impl AnalyticsPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut panel = Self {
            focus_handle: cx.focus_handle(),
            summary: None,
            loading: true,
            _load_task: None,
        };
        panel.reload_data(cx);
        panel
    }

    fn reload_data(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        cx.notify();

        let task = cx.background_spawn(async move { load_sessions_from_disk() });

        self._load_task = Some(cx.spawn(async move |this, cx| {
            let records = task.await;
            let summary = build_summary(&records);
            if let Err(err) = this.update(cx, |this, cx| {
                this.summary = Some(summary);
                this.loading = false;
                cx.notify();
            }) {
                error!("failed to update analytics panel: {err}");
            }
        }));
    }
}

// ── Rendering helpers ───────────────────────────────────────────────

fn format_tokens(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

fn format_cost(cost: f64) -> String {
    if cost >= 1.0 {
        format!("${:.2}", cost)
    } else {
        format!("${:.4}", cost)
    }
}

fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

/// Intensity-to-color for the heatmap (from transparent to a vivid green)
fn heatmap_color(intensity: f32, theme_fg: Hsla) -> Hsla {
    if intensity <= 0.0 {
        Hsla {
            h: theme_fg.h,
            s: 0.0,
            l: theme_fg.l,
            a: 0.08,
        }
    } else {
        Hsla {
            h: 0.38,
            s: 0.5 + intensity * 0.5,
            l: 0.55 - intensity * 0.2,
            a: 0.3 + intensity * 0.7,
        }
    }
}

fn render_stat_card(label: &str, value: String) -> impl IntoElement {
    v_flex()
        .gap_0p5()
        .child(
            Label::new(label.to_string())
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
        .child(
            Label::new(value)
                .size(LabelSize::Small)
                .color(Color::Default),
        )
}

fn render_summary(summary: &AnalyticsSummary) -> impl IntoElement {
    v_flex()
        .p_2()
        .gap_2()
        .child(
            Label::new("Summary")
                .size(LabelSize::Small)
                .color(Color::Accent),
        )
        .child(
            h_flex()
                .gap_3()
                .child(render_stat_card(
                    "Sessions",
                    summary.total_sessions.to_string(),
                ))
                .child(render_stat_card(
                    "Tokens In",
                    format_tokens(summary.total_tokens_in),
                ))
                .child(render_stat_card(
                    "Tokens Out",
                    format_tokens(summary.total_tokens_out),
                )),
        )
        .child(
            h_flex()
                .gap_3()
                .child(render_stat_card("Cost", format_cost(summary.total_cost)))
                .child(render_stat_card(
                    "Duration",
                    format_duration(summary.total_duration_s),
                )),
        )
}

fn render_editor_breakdown(editor_counts: &[(String, usize)], total: usize) -> impl IntoElement {
    let mut breakdown = v_flex().p_2().gap_1().child(
        Label::new("Editor Breakdown")
            .size(LabelSize::Small)
            .color(Color::Accent),
    );

    let bar_hues: &[f32] = &[0.6, 0.3, 0.0, 0.8, 0.15];

    for (idx, (editor, count)) in editor_counts.iter().enumerate() {
        let fraction = if total > 0 {
            *count as f32 / total as f32
        } else {
            0.0
        };
        let percentage = (fraction * 100.0) as u32;
        let hue = bar_hues[idx % bar_hues.len()];
        let bar_color = Hsla {
            h: hue,
            s: 0.6,
            l: 0.5,
            a: 0.8,
        };

        let label_text = format!("{} — {} ({}%)", editor, count, percentage);
        let captured_fraction = fraction;
        let captured_color = bar_color;

        breakdown = breakdown.child(
            v_flex()
                .gap_0p5()
                .child(
                    Label::new(label_text)
                        .size(LabelSize::XSmall)
                        .color(Color::Default),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(6.))
                        .rounded(px(3.))
                        .bg(Hsla {
                            h: 0.0,
                            s: 0.0,
                            l: 0.5,
                            a: 0.15,
                        })
                        .child(
                            canvas(
                                move |_bounds, _window, _cx| {},
                                move |bounds, _, window, _cx| {
                                    let bar_width = bounds.size.width * captured_fraction;
                                    let bar_bounds = Bounds {
                                        origin: bounds.origin,
                                        size: gpui::size(bar_width, bounds.size.height),
                                    };
                                    window.paint_quad(quad(
                                        bar_bounds,
                                        px(3.),
                                        captured_color,
                                        px(0.),
                                        Hsla::default(),
                                        BorderStyle::default(),
                                    ));
                                },
                            )
                            .size_full(),
                        ),
                ),
        );
    }

    breakdown
}

/// Renders a GitHub-style activity heatmap for the last 52 weeks.
fn render_heatmap(daily_sessions: &[(NaiveDate, usize)]) -> impl IntoElement {
    let day_map: std::collections::HashMap<NaiveDate, usize> =
        daily_sessions.iter().cloned().collect();

    let max_count = daily_sessions
        .iter()
        .map(|(_, c)| *c)
        .max()
        .unwrap_or(1)
        .max(1);

    let today = Utc::now().date_naive();
    let weeks = 52u32;
    let total_days = weeks * 7;

    let today_weekday = today.weekday().num_days_from_monday();
    let grid_end = today;
    let grid_start = grid_end - chrono::Duration::days((total_days - 1 + today_weekday) as i64);

    let mut grid = vec![vec![0.0f32; 7]; weeks as usize];
    for week in 0..weeks {
        for day in 0..7u32 {
            let date = grid_start + chrono::Duration::days((week * 7 + day) as i64);
            if date > today {
                continue;
            }
            let count = day_map.get(&date).copied().unwrap_or(0);
            grid[week as usize][day as usize] = count as f32 / max_count as f32;
        }
    }
    let grid = Arc::new(grid);

    v_flex()
        .p_2()
        .gap_1()
        .child(
            Label::new("Activity (52 weeks)")
                .size(LabelSize::Small)
                .color(Color::Accent),
        )
        .child(
            canvas(
                move |_bounds, _window, _cx| {},
                {
                    let grid = grid.clone();
                    move |bounds, _, window, cx| {
                        let cell_size = px(8.);
                        let gap = px(2.);
                        let step = cell_size + gap;

                        let theme_fg = cx.theme().colors().text;

                        for (week_idx, week) in grid.iter().enumerate() {
                            for (day_idx, &intensity) in week.iter().enumerate() {
                                let x = bounds.origin.x + step * week_idx as f32;
                                let y = bounds.origin.y + step * day_idx as f32;

                                let cell_bounds = Bounds {
                                    origin: gpui::point(x, y),
                                    size: gpui::size(cell_size, cell_size),
                                };

                                let color = heatmap_color(intensity, theme_fg);

                                window.paint_quad(quad(
                                    cell_bounds,
                                    px(1.5),
                                    color,
                                    px(0.),
                                    Hsla::default(),
                                    BorderStyle::default(),
                                ));
                            }
                        }
                    }
                },
            )
            .w(px(520.))
            .h(px(70.)),
        )
}

fn render_empty_state() -> impl IntoElement {
    v_flex()
        .p_4()
        .gap_2()
        .flex_1()
        .items_center()
        .justify_center()
        .child(
            Label::new("No session data found.")
                .size(LabelSize::Default)
                .color(Color::Muted),
        )
        .child(
            Label::new("Create ~/.agentics/sessions/ and add JSONL files with session records.")
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
        .child(
            Label::new("Format: {\"editor\":\"...\",\"model\":\"...\",\"tokens_in\":N,\"tokens_out\":N,\"cost\":N,\"timestamp\":\"...\",\"duration_s\":N}")
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
}

fn render_setup_instructions() -> impl IntoElement {
    v_flex()
        .p_4()
        .gap_2()
        .flex_1()
        .items_center()
        .justify_center()
        .child(
            Label::new("Agentlytics Setup")
                .size(LabelSize::Default)
                .color(Color::Muted),
        )
        .child(
            Label::new("The ~/.agentics/ directory does not exist yet.")
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
        .child(
            Label::new("Create it and add JSONL session files to ~/.agentics/sessions/")
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
}

// ── Trait implementations ───────────────────────────────────────────

impl Focusable for AnalyticsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<ItemEvent> for AnalyticsPanel {}

impl Item for AnalyticsPanel {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Agentlytics".into()
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::Sparkle))
    }

    fn tab_tooltip_text(&self, _cx: &App) -> Option<SharedString> {
        Some("AI Agent Usage Dashboard".into())
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        f(event.clone());
    }
}

impl Render for AnalyticsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .p_2()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                Label::new("Agentlytics")
                    .size(LabelSize::Large)
                    .color(Color::Default),
            )
            .child(
                IconButton::new("refresh-analytics", ui::IconName::ArrowCircle)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Refresh Data"))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.reload_data(cx);
                    })),
            );

        let mut panel = v_flex()
            .key_context("AnalyticsPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .child(header);

        if self.loading {
            return panel
                .child(
                    div()
                        .p_4()
                        .flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .child(
                            Label::new("Loading session data...")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                )
                .into_any_element();
        }

        let agentics_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".agentics");
        if !agentics_dir.exists() {
            return panel
                .child(render_setup_instructions())
                .into_any_element();
        }

        match &self.summary {
            Some(summary) if summary.total_sessions > 0 => {
                let summary_el = render_summary(summary);
                let heatmap_el = render_heatmap(&summary.daily_sessions);
                let editor_el =
                    render_editor_breakdown(&summary.editor_counts, summary.total_sessions);

                panel = panel.child(
                    div()
                        .id("analytics-scroll")
                        .overflow_y_scroll()
                        .flex_1()
                        .child(summary_el)
                        .child(
                            div()
                                .border_t_1()
                                .border_color(cx.theme().colors().border)
                                .child(heatmap_el),
                        )
                        .child(
                            div()
                                .border_t_1()
                                .border_color(cx.theme().colors().border)
                                .child(editor_el),
                        ),
                );

                panel.into_any_element()
            }
            _ => panel.child(render_empty_state()).into_any_element(),
        }
    }
}
