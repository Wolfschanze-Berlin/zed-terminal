use git::repository::UpstreamTrackingStatus;
use git::status::GitSummary;
use gpui::{Context, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window};
use project::git_store::{GitStoreEvent, Repository, RepositoryEvent};
use ui::{Color, Icon, IconName, IconSize, Label, LabelSize, Tooltip, prelude::*};
use workspace::{StatusItemView, Workspace, item::ItemHandle};

pub struct GitStatusIndicator {
    branch_name: Option<String>,
    tracking_status: Option<UpstreamTrackingStatus>,
    summary: GitSummary,
    _subscriptions: Vec<Subscription>,
}

impl GitStatusIndicator {
    pub fn new(workspace: &Workspace, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let project = workspace.project();
        let git_store = project.read(cx).git_store().clone();

        let git_store_subscription = cx.subscribe_in(
            &git_store,
            window,
            |this, git_store, event, _window, cx| match event {
                GitStoreEvent::RepositoryUpdated(
                    _,
                    RepositoryEvent::StatusesChanged | RepositoryEvent::BranchChanged,
                    _,
                )
                | GitStoreEvent::ActiveRepositoryChanged(_)
                | GitStoreEvent::RepositoryAdded
                | GitStoreEvent::RepositoryRemoved(_) => {
                    this.update_from_repo(git_store.read(cx).active_repository(), cx);
                }
                _ => {}
            },
        );

        let mut this = Self {
            branch_name: None,
            tracking_status: None,
            summary: GitSummary::default(),
            _subscriptions: vec![git_store_subscription],
        };

        this.update_from_repo(git_store.read(cx).active_repository(), cx);
        this
    }

    fn update_from_repo(
        &mut self,
        active_repo: Option<Entity<Repository>>,
        cx: &mut Context<Self>,
    ) {
        if let Some(repo) = active_repo {
            let repo = repo.read(cx);
            self.branch_name = repo.branch.as_ref().map(|b| b.name().to_string());
            self.tracking_status = repo.branch.as_ref().and_then(|b| b.tracking_status());
            self.summary = repo.status_summary();
        } else {
            self.branch_name = None;
            self.tracking_status = None;
            self.summary = GitSummary::default();
        }
        cx.notify();
    }
}

impl Render for GitStatusIndicator {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let container = h_flex().gap_2();

        let Some(branch_name) = &self.branch_name else {
            return container;
        };

        let branch_section = h_flex()
            .gap_1()
            .child(
                Icon::new(IconName::GitBranch)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(
                Label::new(branch_name.clone())
                    .size(LabelSize::Small)
                    .color(Color::Default),
            );

        let tracking_section = self.tracking_status.map(|status| {
            h_flex()
                .gap_0p5()
                .when(status.ahead > 0, |el| {
                    el.child(
                        h_flex()
                            .gap_0p5()
                            .child(
                                Icon::new(IconName::ArrowUp)
                                    .size(IconSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .child(
                                Label::new(status.ahead.to_string())
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            ),
                    )
                })
                .when(status.behind > 0, |el| {
                    el.child(
                        h_flex()
                            .gap_0p5()
                            .child(
                                Icon::new(IconName::ArrowDown)
                                    .size(IconSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .child(
                                Label::new(status.behind.to_string())
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            ),
                    )
                })
        });

        let changed_files = self.summary.count;
        let diff_section = if changed_files > 0 {
            let staged = self.summary.index.added + self.summary.index.modified;
            let unstaged =
                self.summary.worktree.added + self.summary.worktree.modified + self.summary.untracked;
            let deleted = self.summary.index.deleted + self.summary.worktree.deleted;

            Some(
                h_flex()
                    .gap_1()
                    .when(staged > 0, |el| {
                        el.child(
                            h_flex()
                                .gap_0p5()
                                .child(
                                    Label::new(format!("+{staged}"))
                                        .size(LabelSize::XSmall)
                                        .color(Color::Created),
                                ),
                        )
                    })
                    .when(unstaged > 0, |el| {
                        el.child(
                            h_flex()
                                .gap_0p5()
                                .child(
                                    Label::new(format!("~{unstaged}"))
                                        .size(LabelSize::XSmall)
                                        .color(Color::Modified),
                                ),
                        )
                    })
                    .when(deleted > 0, |el| {
                        el.child(
                            h_flex()
                                .gap_0p5()
                                .child(
                                    Label::new(format!("-{deleted}"))
                                        .size(LabelSize::XSmall)
                                        .color(Color::Deleted),
                                ),
                        )
                    })
                    .when(self.summary.conflict > 0, |el| {
                        el.child(
                            h_flex()
                                .gap_0p5()
                                .child(
                                    Label::new(format!("!{}", self.summary.conflict))
                                        .size(LabelSize::XSmall)
                                        .color(Color::Error),
                                ),
                        )
                    }),
            )
        } else {
            None
        };

        let tooltip_text = self.build_tooltip_text();

        container
            .child(
                h_flex()
                    .gap_1()
                    .child(branch_section)
                    .children(tracking_section)
                    .children(diff_section)
                    .id("git-status-indicator")
                    .tooltip(Tooltip::text(tooltip_text)),
            )
    }
}

impl GitStatusIndicator {
    fn build_tooltip_text(&self) -> String {
        let mut parts = Vec::new();

        if let Some(branch) = &self.branch_name {
            parts.push(format!("Branch: {branch}"));
        }

        if let Some(status) = &self.tracking_status {
            if status.ahead > 0 || status.behind > 0 {
                let mut tracking = Vec::new();
                if status.ahead > 0 {
                    tracking.push(format!("{} ahead", status.ahead));
                }
                if status.behind > 0 {
                    tracking.push(format!("{} behind", status.behind));
                }
                parts.push(tracking.join(", "));
            }
        }

        if self.summary.count > 0 {
            let staged = self.summary.index.added + self.summary.index.modified;
            let unstaged = self.summary.worktree.added
                + self.summary.worktree.modified
                + self.summary.untracked;
            let deleted = self.summary.index.deleted + self.summary.worktree.deleted;

            let mut changes = Vec::new();
            if staged > 0 {
                changes.push(format!("{staged} staged"));
            }
            if unstaged > 0 {
                changes.push(format!("{unstaged} unstaged"));
            }
            if deleted > 0 {
                changes.push(format!("{deleted} deleted"));
            }
            if self.summary.conflict > 0 {
                changes.push(format!("{} conflicts", self.summary.conflict));
            }
            parts.push(changes.join(", "));
        } else {
            parts.push("Clean".to_string());
        }

        parts.join("\n")
    }
}

impl StatusItemView for GitStatusIndicator {
    fn set_active_pane_item(
        &mut self,
        _active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
}
