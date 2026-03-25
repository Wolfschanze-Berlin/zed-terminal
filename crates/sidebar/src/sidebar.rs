use editor::Editor;
use gpui::{
    Action as _, AnyElement, App, Context, Entity, FocusHandle, Focusable, ListState, Pixels,
    Render, SharedString, WeakEntity, Window, list, prelude::*, px,
};
use menu::{
    Cancel, Confirm, SelectChild, SelectFirst, SelectLast, SelectNext, SelectParent,
    SelectPrevious,
};
use project::Event as ProjectEvent;
use recent_projects::sidebar_recent_projects::SidebarRecentProjects;
use ui::utils::platform_title_bar_height;

use settings::Settings as _;
use std::collections::HashSet;
use std::rc::Rc;
use theme::ActiveTheme;
use ui::{
    ContextMenu, Divider, HighlightedLabel, KeyBinding, PopoverMenu, PopoverMenuHandle, Tab,
    TintColor, Tooltip, WithScrollbar, prelude::*,
};
use util::path_list::PathList;
use workspace::{
    AddFolderToProject, FocusWorkspaceSidebar, MultiWorkspace, MultiWorkspaceEvent, Open,
    Sidebar as WorkspaceSidebar, ToggleWorkspaceSidebar, Workspace, WorkspaceId,
};

use zed_actions::OpenRecent;
use zed_actions::editor::{MoveDown, MoveUp};

use zed_actions::agents_sidebar::FocusSidebarFilter;

const DEFAULT_WIDTH: Pixels = px(300.0);
const MIN_WIDTH: Pixels = px(200.0);
const MAX_WIDTH: Pixels = px(800.0);

fn fuzzy_match_positions(query: &str, candidate: &str) -> Option<Vec<usize>> {
    let mut positions = Vec::new();
    let mut query_chars = query.chars().peekable();

    for (byte_idx, candidate_char) in candidate.char_indices() {
        if let Some(&query_char) = query_chars.peek() {
            if candidate_char.eq_ignore_ascii_case(&query_char) {
                positions.push(byte_idx);
                query_chars.next();
            }
        } else {
            break;
        }
    }

    if query_chars.peek().is_none() {
        Some(positions)
    } else {
        None
    }
}

fn workspace_path_list(workspace: &Entity<Workspace>, cx: &App) -> PathList {
    PathList::new(&workspace.read(cx).root_paths(cx))
}

fn workspace_label_from_path_list(path_list: &PathList) -> SharedString {
    let mut names = Vec::with_capacity(path_list.paths().len());
    for abs_path in path_list.paths() {
        if let Some(name) = abs_path.file_name() {
            names.push(name.to_string_lossy().to_string());
        }
    }
    if names.is_empty() {
        "Empty Workspace".into()
    } else {
        names.join(", ").into()
    }
}

#[derive(Clone)]
enum ListEntry {
    ProjectHeader {
        path_list: PathList,
        label: SharedString,
        workspace: Entity<Workspace>,
        highlight_positions: Vec<usize>,
        is_active: bool,
    },
}

#[derive(Default)]
struct SidebarContents {
    entries: Vec<ListEntry>,
    project_header_indices: Vec<usize>,
    has_open_projects: bool,
}

pub struct Sidebar {
    multi_workspace: WeakEntity<MultiWorkspace>,
    width: Pixels,
    focus_handle: FocusHandle,
    filter_editor: Entity<Editor>,
    list_state: ListState,
    contents: SidebarContents,
    selection: Option<usize>,
    collapsed_groups: HashSet<PathList>,
    recent_projects_popover_handle: PopoverMenuHandle<SidebarRecentProjects>,
    project_header_menu_ix: Option<usize>,
}

impl Sidebar {
    pub fn new(
        multi_workspace: Entity<MultiWorkspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus_in(&focus_handle, window, Self::focus_in)
            .detach();

        let filter_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_use_modal_editing(true);
            editor.set_placeholder_text("Search…", window, cx);
            editor
        });

        cx.subscribe_in(
            &multi_workspace,
            window,
            |this, _multi_workspace, event: &MultiWorkspaceEvent, window, cx| match event {
                MultiWorkspaceEvent::ActiveWorkspaceChanged => {
                    this.update_entries(cx);
                }
                MultiWorkspaceEvent::WorkspaceAdded(workspace) => {
                    this.subscribe_to_workspace(workspace, window, cx);
                    this.update_entries(cx);
                }
                MultiWorkspaceEvent::WorkspaceRemoved(_) => {
                    this.update_entries(cx);
                }
            },
        )
        .detach();

        cx.subscribe(&filter_editor, |this: &mut Self, _, event, cx| {
            if let editor::EditorEvent::BufferEdited = event {
                let query = this.filter_editor.read(cx).text(cx);
                if !query.is_empty() {
                    this.selection.take();
                }
                this.update_entries(cx);
                if !query.is_empty() {
                    this.select_first_entry();
                }
            }
        })
        .detach();

        let workspaces = multi_workspace.read(cx).workspaces().to_vec();
        cx.defer_in(window, move |this, window, cx| {
            for workspace in &workspaces {
                this.subscribe_to_workspace(workspace, window, cx);
            }
            this.update_entries(cx);
        });

        Self {
            multi_workspace: multi_workspace.downgrade(),
            width: DEFAULT_WIDTH,
            focus_handle,
            filter_editor,
            list_state: ListState::new(0, gpui::ListAlignment::Top, px(1000.)),
            contents: SidebarContents::default(),
            selection: None,
            collapsed_groups: HashSet::new(),
            recent_projects_popover_handle: PopoverMenuHandle::default(),
            project_header_menu_ix: None,
        }
    }

    fn subscribe_to_workspace(
        &mut self,
        workspace: &Entity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let project = workspace.read(cx).project().clone();
        cx.subscribe_in(
            &project,
            window,
            |this, _project, event, _window, cx| match event {
                ProjectEvent::WorktreeAdded(_)
                | ProjectEvent::WorktreeRemoved(_)
                | ProjectEvent::WorktreeOrderChanged => {
                    this.update_entries(cx);
                }
                _ => {}
            },
        )
        .detach();

        let git_store = workspace.read(cx).project().read(cx).git_store().clone();
        cx.subscribe_in(
            &git_store,
            window,
            |this, _, event: &project::git_store::GitStoreEvent, _window, cx| {
                if matches!(
                    event,
                    project::git_store::GitStoreEvent::RepositoryUpdated(
                        _,
                        project::git_store::RepositoryEvent::GitWorktreeListChanged,
                        _,
                    )
                ) {
                    this.update_entries(cx);
                }
            },
        )
        .detach();
    }

    fn rebuild_contents(&mut self, cx: &App) {
        let Some(multi_workspace) = self.multi_workspace.upgrade() else {
            return;
        };
        let mw = multi_workspace.read(cx);
        let workspaces = mw.workspaces().to_vec();
        let active_workspace = mw.workspaces().get(mw.active_workspace_index()).cloned();

        let query = self.filter_editor.read(cx).text(cx);

        let has_open_projects = workspaces
            .iter()
            .any(|ws| !workspace_path_list(ws, cx).paths().is_empty());

        let active_ws_index = active_workspace
            .as_ref()
            .and_then(|active| workspaces.iter().position(|ws| ws == active));

        let mut entries = Vec::new();
        let mut project_header_indices: Vec<usize> = Vec::new();

        for (ws_index, workspace) in workspaces.iter().enumerate() {
            let path_list = workspace_path_list(workspace, cx);
            if path_list.paths().is_empty() {
                continue;
            }

            let label = workspace_label_from_path_list(&path_list);
            let is_active = active_ws_index == Some(ws_index);

            if !query.is_empty() {
                let workspace_highlight_positions =
                    fuzzy_match_positions(&query, &label).unwrap_or_default();
                if workspace_highlight_positions.is_empty() {
                    continue;
                }

                project_header_indices.push(entries.len());
                entries.push(ListEntry::ProjectHeader {
                    path_list,
                    label,
                    workspace: workspace.clone(),
                    highlight_positions: workspace_highlight_positions,
                    is_active,
                });
            } else {
                project_header_indices.push(entries.len());
                entries.push(ListEntry::ProjectHeader {
                    path_list,
                    label,
                    workspace: workspace.clone(),
                    highlight_positions: Vec::new(),
                    is_active,
                });
            }
        }

        self.contents = SidebarContents {
            entries,
            project_header_indices,
            has_open_projects,
        };
    }

    fn update_entries(&mut self, cx: &mut Context<Self>) {
        let Some(multi_workspace) = self.multi_workspace.upgrade() else {
            return;
        };
        if !multi_workspace.read(cx).multi_workspace_enabled(cx) {
            return;
        }

        let scroll_position = self.list_state.logical_scroll_top();

        self.rebuild_contents(cx);

        self.list_state.reset(self.contents.entries.len());
        self.list_state.scroll_to(scroll_position);

        cx.notify();
    }

    fn select_first_entry(&mut self) {
        self.selection = if self.contents.entries.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    fn render_list_entry(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = self.contents.entries.get(ix) else {
            return div().into_any_element();
        };
        let is_focused = self.focus_handle.is_focused(window);
        let is_selected = is_focused && self.selection == Some(ix);

        let is_group_header_after_first =
            ix > 0 && matches!(entry, ListEntry::ProjectHeader { .. });

        let rendered = match entry {
            ListEntry::ProjectHeader {
                path_list,
                label,
                workspace,
                highlight_positions,
                is_active,
            } => self.render_project_header(
                ix,
                false,
                path_list,
                label,
                workspace,
                highlight_positions,
                *is_active,
                is_selected,
                cx,
            ),
        };

        if is_group_header_after_first {
            v_flex()
                .w_full()
                .border_t_1()
                .border_color(cx.theme().colors().border.opacity(0.5))
                .child(rendered)
                .into_any_element()
        } else {
            rendered
        }
    }

    fn render_project_header(
        &self,
        ix: usize,
        is_sticky: bool,
        path_list: &PathList,
        label: &SharedString,
        workspace: &Entity<Workspace>,
        highlight_positions: &[usize],
        is_active: bool,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id_prefix = if is_sticky { "sticky-" } else { "" };
        let id = SharedString::from(format!("{id_prefix}project-header-{ix}"));
        let group_name = SharedString::from(format!("{id_prefix}header-group-{ix}"));

        let is_collapsed = self.collapsed_groups.contains(path_list);
        let disclosure_icon = if is_collapsed {
            IconName::ChevronRight
        } else {
            IconName::ChevronDown
        };

        let workspace_for_remove = workspace.clone();
        let workspace_for_menu = workspace.clone();
        let workspace_for_open = workspace.clone();

        let path_list_for_toggle = path_list.clone();

        let label = if highlight_positions.is_empty() {
            Label::new(label.clone())
                .color(Color::Muted)
                .into_any_element()
        } else {
            HighlightedLabel::new(label.clone(), highlight_positions.to_vec())
                .color(Color::Muted)
                .into_any_element()
        };

        let color = cx.theme().colors();
        let hover_color = color
            .element_active
            .blend(color.element_background.opacity(0.2));

        h_flex()
            .id(id)
            .group(&group_name)
            .h(Tab::content_height(cx))
            .w_full()
            .pl_1p5()
            .pr_1()
            .border_1()
            .map(|this| {
                if is_selected {
                    this.border_color(color.border_focused)
                } else {
                    this.border_color(gpui::transparent_black())
                }
            })
            .justify_between()
            .hover(|s| s.bg(hover_color))
            .child(
                h_flex()
                    .relative()
                    .min_w_0()
                    .w_full()
                    .gap_1p5()
                    .child(
                        h_flex().size_4().flex_none().justify_center().child(
                            Icon::new(disclosure_icon)
                                .size(IconSize::Small)
                                .color(Color::Custom(cx.theme().colors().icon_muted.opacity(0.5))),
                        ),
                    )
                    .child(label),
            )
            .child({
                h_flex()
                    .when(self.project_header_menu_ix != Some(ix), |this| {
                        this.visible_on_hover(group_name)
                    })
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(self.render_project_header_menu(
                        ix,
                        id_prefix,
                        &workspace_for_menu,
                        &workspace_for_remove,
                        cx,
                    ))
                    .when(!is_active, |this| {
                        this.child(
                            IconButton::new(
                                SharedString::from(format!(
                                    "{id_prefix}project-header-open-workspace-{ix}",
                                )),
                                IconName::Focus,
                            )
                            .icon_size(IconSize::Small)
                            .icon_color(Color::Muted)
                            .tooltip(Tooltip::text("Activate Workspace"))
                            .on_click(cx.listener({
                                move |this, _, _window, cx| {
                                    if let Some(multi_workspace) = this.multi_workspace.upgrade() {
                                        multi_workspace.update(cx, |multi_workspace, cx| {
                                            multi_workspace
                                                .activate(workspace_for_open.clone(), cx);
                                        });
                                    }
                                }
                            })),
                        )
                    })
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.selection = None;
                this.toggle_collapse(&path_list_for_toggle, window, cx);
            }))
            .into_any_element()
    }

    fn render_project_header_menu(
        &self,
        ix: usize,
        id_prefix: &str,
        workspace: &Entity<Workspace>,
        workspace_for_remove: &Entity<Workspace>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let workspace_for_menu = workspace.clone();
        let workspace_for_remove = workspace_for_remove.clone();
        let multi_workspace = self.multi_workspace.clone();
        let this = cx.weak_entity();

        PopoverMenu::new(format!("{id_prefix}project-header-menu-{ix}"))
            .on_open(Rc::new({
                let this = this.clone();
                move |_window, cx| {
                    this.update(cx, |sidebar, cx| {
                        sidebar.project_header_menu_ix = Some(ix);
                        cx.notify();
                    })
                    .ok();
                }
            }))
            .menu(move |window, cx| {
                let workspace = workspace_for_menu.clone();
                let workspace_for_remove = workspace_for_remove.clone();
                let multi_workspace = multi_workspace.clone();

                let menu = ContextMenu::build_persistent(window, cx, move |menu, _window, cx| {
                    let worktrees: Vec<_> = workspace
                        .read(cx)
                        .visible_worktrees(cx)
                        .map(|worktree| {
                            let worktree_read = worktree.read(cx);
                            let id = worktree_read.id();
                            let name: SharedString =
                                worktree_read.root_name().as_unix_str().to_string().into();
                            (id, name)
                        })
                        .collect();

                    let worktree_count = worktrees.len();

                    let mut menu = menu
                        .header("Project Folders")
                        .end_slot_action(Box::new(menu::EndSlot));

                    for (worktree_id, name) in &worktrees {
                        let worktree_id = *worktree_id;
                        let workspace_for_worktree = workspace.clone();
                        let workspace_for_remove_worktree = workspace_for_remove.clone();
                        let multi_workspace_for_worktree = multi_workspace.clone();

                        let remove_handler = move |window: &mut Window, cx: &mut App| {
                            if worktree_count <= 1 {
                                if let Some(mw) = multi_workspace_for_worktree.upgrade() {
                                    let ws = workspace_for_remove_worktree.clone();
                                    mw.update(cx, |multi_workspace, cx| {
                                        if let Some(index) = multi_workspace
                                            .workspaces()
                                            .iter()
                                            .position(|w| *w == ws)
                                        {
                                            multi_workspace.remove_workspace(index, window, cx);
                                        }
                                    });
                                }
                            } else {
                                workspace_for_worktree.update(cx, |workspace, cx| {
                                    workspace.project().update(cx, |project, cx| {
                                        project.remove_worktree(worktree_id, cx);
                                    });
                                });
                            }
                        };

                        menu = menu.entry_with_end_slot_on_hover(
                            name.clone(),
                            None,
                            |_, _| {},
                            IconName::Close,
                            "Remove Folder".into(),
                            remove_handler,
                        );
                    }

                    let workspace_for_add = workspace.clone();
                    let multi_workspace_for_add = multi_workspace.clone();
                    let menu = menu.separator().entry(
                        "Add Folder to Project",
                        Some(Box::new(AddFolderToProject)),
                        move |window, cx| {
                            if let Some(mw) = multi_workspace_for_add.upgrade() {
                                mw.update(cx, |mw, cx| {
                                    mw.activate(workspace_for_add.clone(), cx);
                                });
                            }
                            workspace_for_add.update(cx, |workspace, cx| {
                                workspace.add_folder_to_project(&AddFolderToProject, window, cx);
                            });
                        },
                    );

                    let workspace_count = multi_workspace
                        .upgrade()
                        .map_or(0, |mw| mw.read(cx).workspaces().len());
                    let menu = if workspace_count > 1 {
                        let workspace_for_move = workspace.clone();
                        let multi_workspace_for_move = multi_workspace.clone();
                        menu.entry(
                            "Move to New Window",
                            Some(Box::new(
                                zed_actions::agents_sidebar::MoveWorkspaceToNewWindow,
                            )),
                            move |window, cx| {
                                if let Some(mw) = multi_workspace_for_move.upgrade() {
                                    mw.update(cx, |multi_workspace, cx| {
                                        if let Some(index) = multi_workspace
                                            .workspaces()
                                            .iter()
                                            .position(|w| *w == workspace_for_move)
                                        {
                                            multi_workspace
                                                .move_workspace_to_new_window(index, window, cx);
                                        }
                                    });
                                }
                            },
                        )
                    } else {
                        menu
                    };

                    let workspace_for_remove = workspace_for_remove.clone();
                    let multi_workspace_for_remove = multi_workspace.clone();
                    menu.separator()
                        .entry("Remove Project", None, move |window, cx| {
                            if let Some(mw) = multi_workspace_for_remove.upgrade() {
                                let ws = workspace_for_remove.clone();
                                mw.update(cx, |multi_workspace, cx| {
                                    if let Some(index) =
                                        multi_workspace.workspaces().iter().position(|w| *w == ws)
                                    {
                                        multi_workspace.remove_workspace(index, window, cx);
                                    }
                                });
                            }
                        })
                });

                let this = this.clone();
                window
                    .subscribe(&menu, cx, move |_, _: &gpui::DismissEvent, _window, cx| {
                        this.update(cx, |sidebar, cx| {
                            sidebar.project_header_menu_ix = None;
                            cx.notify();
                        })
                        .ok();
                    })
                    .detach();

                Some(menu)
            })
            .trigger(
                IconButton::new(
                    SharedString::from(format!("{id_prefix}-ellipsis-menu-{ix}")),
                    IconName::Ellipsis,
                )
                .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                .icon_size(IconSize::Small)
                .icon_color(Color::Muted),
            )
            .anchor(gpui::Corner::TopRight)
            .offset(gpui::Point {
                x: px(0.),
                y: px(1.),
            })
    }

    fn render_sticky_header(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let scroll_top = self.list_state.logical_scroll_top();

        let &header_idx = self
            .contents
            .project_header_indices
            .iter()
            .rev()
            .find(|&&idx| idx <= scroll_top.item_ix)?;

        let needs_sticky = header_idx < scroll_top.item_ix
            || (header_idx == scroll_top.item_ix && scroll_top.offset_in_item > px(0.));

        if !needs_sticky {
            return None;
        }

        let entry = self.contents.entries.get(header_idx)?;
        let ListEntry::ProjectHeader {
            path_list,
            label,
            workspace,
            highlight_positions,
            is_active,
        } = entry;

        let is_focused = self.focus_handle.is_focused(window);
        let is_selected = is_focused && self.selection == Some(header_idx);

        let header_element = self.render_project_header(
            header_idx,
            true,
            path_list,
            label,
            workspace,
            highlight_positions,
            *is_active,
            is_selected,
            cx,
        );

        let top_offset = self
            .contents
            .project_header_indices
            .iter()
            .find(|&&idx| idx > header_idx)
            .and_then(|&next_idx| {
                let bounds = self.list_state.bounds_for_item(next_idx)?;
                let viewport = self.list_state.viewport_bounds();
                let y_in_viewport = bounds.origin.y - viewport.origin.y;
                let header_height = bounds.size.height;
                (y_in_viewport < header_height).then_some(y_in_viewport - header_height)
            })
            .unwrap_or(px(0.));

        let color = cx.theme().colors();
        let background = color
            .title_bar_background
            .blend(color.panel_background.opacity(0.2));

        let element = v_flex()
            .absolute()
            .top(top_offset)
            .left_0()
            .w_full()
            .bg(background)
            .border_b_1()
            .border_color(color.border.opacity(0.5))
            .child(header_element)
            .shadow_xs()
            .into_any_element();

        Some(element)
    }

    fn toggle_collapse(
        &mut self,
        path_list: &PathList,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.collapsed_groups.contains(path_list) {
            self.collapsed_groups.remove(path_list);
        } else {
            self.collapsed_groups.insert(path_list.clone());
        }
        self.update_entries(cx);
    }

    fn focus_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_handle.is_focused(window) {
            return;
        }

        if self.selection.is_none() {
            self.filter_editor.focus_handle(cx).focus(window, cx);
        }
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.reset_filter_editor_text(window, cx) {
            self.update_entries(cx);
        } else {
            self.selection = None;
            self.filter_editor.focus_handle(cx).focus(window, cx);
            cx.notify();
        }
    }

    fn focus_sidebar_filter(
        &mut self,
        _: &FocusSidebarFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection = None;
        self.filter_editor.focus_handle(cx).focus(window, cx);

        if vim_mode_setting::VimModeSetting::get_global(cx).0 {
            if let Ok(action) = cx.build_action("vim::SwitchToInsertMode", None) {
                window.dispatch_action(action, cx);
            }
        }

        cx.notify();
    }

    fn reset_filter_editor_text(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.filter_editor.update(cx, |editor, cx| {
            if editor.buffer().read(cx).len(cx).0 > 0 {
                editor.set_text("", window, cx);
                true
            } else {
                false
            }
        })
    }

    fn has_filter_query(&self, cx: &App) -> bool {
        !self.filter_editor.read(cx).text(cx).is_empty()
    }

    fn editor_move_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        self.select_next(&SelectNext, window, cx);
        if self.selection.is_some() {
            self.focus_handle.focus(window, cx);
        }
    }

    fn editor_move_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        self.select_previous(&SelectPrevious, window, cx);
        if self.selection.is_some() {
            self.focus_handle.focus(window, cx);
        }
    }

    fn editor_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection.is_none() {
            self.select_next(&SelectNext, window, cx);
        }
        if self.selection.is_some() {
            self.focus_handle.focus(window, cx);
        }
    }

    fn select_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        let next = match self.selection {
            Some(ix) if ix + 1 < self.contents.entries.len() => ix + 1,
            Some(_) if !self.contents.entries.is_empty() => 0,
            None if !self.contents.entries.is_empty() => 0,
            _ => return,
        };
        self.selection = Some(next);
        self.list_state.scroll_to_reveal_item(next);
        cx.notify();
    }

    fn select_previous(
        &mut self,
        _: &SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.selection {
            Some(0) => {
                self.selection = None;
                self.filter_editor.focus_handle(cx).focus(window, cx);
                cx.notify();
            }
            Some(ix) => {
                self.selection = Some(ix - 1);
                self.list_state.scroll_to_reveal_item(ix - 1);
                cx.notify();
            }
            None if !self.contents.entries.is_empty() => {
                let last = self.contents.entries.len() - 1;
                self.selection = Some(last);
                self.list_state.scroll_to_reveal_item(last);
                cx.notify();
            }
            None => {}
        }
    }

    fn select_first(&mut self, _: &SelectFirst, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.contents.entries.is_empty() {
            self.selection = Some(0);
            self.list_state.scroll_to_reveal_item(0);
            cx.notify();
        }
    }

    fn select_last(&mut self, _: &SelectLast, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(last) = self.contents.entries.len().checked_sub(1) {
            self.selection = Some(last);
            self.list_state.scroll_to_reveal_item(last);
            cx.notify();
        }
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.selection else { return };
        let Some(entry) = self.contents.entries.get(ix) else {
            return;
        };

        match entry {
            ListEntry::ProjectHeader { path_list, .. } => {
                let path_list = path_list.clone();
                self.toggle_collapse(&path_list, window, cx);
            }
        }
    }

    fn expand_selected_entry(
        &mut self,
        _: &SelectChild,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.selection else { return };

        if let Some(ListEntry::ProjectHeader { path_list, .. }) = self.contents.entries.get(ix) {
            if self.collapsed_groups.contains(path_list) {
                let path_list = path_list.clone();
                self.collapsed_groups.remove(&path_list);
                self.update_entries(cx);
            } else if ix + 1 < self.contents.entries.len() {
                self.selection = Some(ix + 1);
                self.list_state.scroll_to_reveal_item(ix + 1);
                cx.notify();
            }
        }
    }

    fn collapse_selected_entry(
        &mut self,
        _: &SelectParent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.selection else { return };

        if let Some(ListEntry::ProjectHeader { path_list, .. }) = self.contents.entries.get(ix) {
            if !self.collapsed_groups.contains(path_list) {
                let path_list = path_list.clone();
                self.collapsed_groups.insert(path_list);
                self.update_entries(cx);
            }
        }
    }

    fn toggle_selected_fold(
        &mut self,
        _: &editor::actions::ToggleFold,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.selection else { return };

        if let Some(ListEntry::ProjectHeader { path_list, .. }) = self.contents.entries.get(ix) {
            let path_list = path_list.clone();
            if self.collapsed_groups.contains(&path_list) {
                self.collapsed_groups.remove(&path_list);
            } else {
                self.collapsed_groups.insert(path_list);
            }
            self.update_entries(cx);
        }
    }

    fn fold_all(
        &mut self,
        _: &editor::actions::FoldAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for entry in &self.contents.entries {
            let ListEntry::ProjectHeader { path_list, .. } = entry;
            self.collapsed_groups.insert(path_list.clone());
        }
        self.update_entries(cx);
    }

    fn unfold_all(
        &mut self,
        _: &editor::actions::UnfoldAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.collapsed_groups.clear();
        self.update_entries(cx);
    }

    fn render_filter_input(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .min_w_0()
            .flex_1()
            .capture_action(
                cx.listener(|this, _: &editor::actions::Newline, window, cx| {
                    this.editor_confirm(window, cx);
                }),
            )
            .child(self.filter_editor.clone())
    }

    fn render_recent_projects_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let multi_workspace = self.multi_workspace.upgrade();

        let workspace = multi_workspace
            .as_ref()
            .map(|mw| mw.read(cx).workspace().downgrade());

        let focus_handle = workspace
            .as_ref()
            .and_then(|ws| ws.upgrade())
            .map(|w| w.read(cx).focus_handle(cx))
            .unwrap_or_else(|| cx.focus_handle());

        let sibling_workspace_ids: HashSet<WorkspaceId> = multi_workspace
            .as_ref()
            .map(|mw| {
                mw.read(cx)
                    .workspaces()
                    .iter()
                    .filter_map(|ws| ws.read(cx).database_id())
                    .collect()
            })
            .unwrap_or_default();

        let popover_handle = self.recent_projects_popover_handle.clone();

        PopoverMenu::new("sidebar-recent-projects-menu")
            .with_handle(popover_handle)
            .menu(move |window, cx| {
                workspace.as_ref().map(|ws| {
                    SidebarRecentProjects::popover(
                        ws.clone(),
                        sibling_workspace_ids.clone(),
                        focus_handle.clone(),
                        window,
                        cx,
                    )
                })
            })
            .trigger_with_tooltip(
                IconButton::new("open-project", IconName::OpenFolder)
                    .icon_size(IconSize::Small)
                    .selected_style(ButtonStyle::Tinted(TintColor::Accent)),
                |_window, cx| {
                    Tooltip::for_action(
                        "Add Project",
                        &OpenRecent {
                            create_new_window: false,
                        },
                        cx,
                    )
                },
            )
            .offset(gpui::Point {
                x: px(-2.0),
                y: px(-2.0),
            })
            .anchor(gpui::Corner::BottomRight)
    }

    fn render_no_results(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_query = self.has_filter_query(cx);
        let message = if has_query {
            "No projects match your search."
        } else {
            "No projects open"
        };

        v_flex()
            .id("sidebar-no-results")
            .p_4()
            .size_full()
            .items_center()
            .justify_center()
            .child(
                Label::new(message)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
    }

    fn render_empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("sidebar-empty-state")
            .p_4()
            .size_full()
            .items_center()
            .justify_center()
            .gap_1()
            .track_focus(&self.focus_handle(cx))
            .child(
                Button::new("open_project", "Open Project")
                    .full_width()
                    .key_binding(KeyBinding::for_action(&workspace::Open::default(), cx))
                    .on_click(|_, window, cx| {
                        window.dispatch_action(
                            Open {
                                create_new_window: false,
                            }
                            .boxed_clone(),
                            cx,
                        );
                    }),
            )
            .child(
                h_flex()
                    .w_1_2()
                    .gap_2()
                    .child(Divider::horizontal())
                    .child(Label::new("or").size(LabelSize::XSmall).color(Color::Muted))
                    .child(Divider::horizontal()),
            )
            .child(
                Button::new("clone_repo", "Clone Repository")
                    .full_width()
                    .on_click(|_, window, cx| {
                        window.dispatch_action(git::Clone.boxed_clone(), cx);
                    }),
            )
    }

    fn render_sidebar_header(
        &self,
        no_open_projects: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let has_query = self.has_filter_query(cx);
        let traffic_lights = cfg!(target_os = "macos") && !window.is_fullscreen();
        let header_height = platform_title_bar_height(window);

        h_flex()
            .h(header_height)
            .mt_px()
            .pb_px()
            .map(|this| {
                if traffic_lights {
                    this.pl(px(ui::utils::TRAFFIC_LIGHT_PADDING))
                } else {
                    this.pl_1p5()
                }
            })
            .pr_1p5()
            .gap_1()
            .when(!no_open_projects, |this| {
                this.border_b_1()
                    .border_color(cx.theme().colors().border)
                    .when(traffic_lights, |this| {
                        this.child(Divider::vertical().color(ui::DividerColor::Border))
                    })
                    .child(
                        div().ml_1().child(
                            Icon::new(IconName::MagnifyingGlass)
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                    .child(self.render_filter_input(cx))
                    .child(
                        h_flex()
                            .gap_1()
                            .when(
                                self.selection.is_some()
                                    && !self.filter_editor.focus_handle(cx).is_focused(window),
                                |this| this.child(KeyBinding::for_action(&FocusSidebarFilter, cx)),
                            )
                            .when(has_query, |this| {
                                this.child(
                                    IconButton::new("clear_filter", IconName::Close)
                                        .icon_size(IconSize::Small)
                                        .tooltip(Tooltip::text("Clear Search"))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.reset_filter_editor_text(window, cx);
                                            this.update_entries(cx);
                                        })),
                                )
                            }),
                    )
            })
    }

    fn render_sidebar_toggle_button(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        IconButton::new("sidebar-close-toggle", IconName::ThreadsSidebarLeftOpen)
            .icon_size(IconSize::Small)
            .tooltip(Tooltip::element(move |_window, cx| {
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .justify_between()
                            .child(Label::new("Toggle Sidebar"))
                            .child(KeyBinding::for_action(&ToggleWorkspaceSidebar, cx)),
                    )
                    .child(
                        h_flex()
                            .pt_1()
                            .gap_2()
                            .border_t_1()
                            .border_color(cx.theme().colors().border_variant)
                            .justify_between()
                            .child(Label::new("Focus Sidebar"))
                            .child(KeyBinding::for_action(&FocusWorkspaceSidebar, cx)),
                    )
                    .into_any_element()
            }))
            .on_click(|_, window, cx| {
                if let Some(multi_workspace) = window.root::<MultiWorkspace>().flatten() {
                    multi_workspace.update(cx, |multi_workspace, cx| {
                        multi_workspace.close_sidebar(window, cx);
                    });
                }
            })
    }
}

impl WorkspaceSidebar for Sidebar {
    fn width(&self, _cx: &App) -> Pixels {
        self.width
    }

    fn set_width(&mut self, width: Option<Pixels>, cx: &mut Context<Self>) {
        self.width = width.unwrap_or(DEFAULT_WIDTH).clamp(MIN_WIDTH, MAX_WIDTH);
        cx.notify();
    }

    fn has_notifications(&self, _cx: &App) -> bool {
        false
    }

    fn is_threads_list_view_active(&self) -> bool {
        true
    }

    fn prepare_for_focus(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.selection = None;
        cx.notify();
    }
}

impl Focusable for Sidebar {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _titlebar_height = ui::utils::platform_title_bar_height(window);
        let ui_font = theme::setup_ui_font(window, cx);
        let sticky_header = self.render_sticky_header(window, cx);

        let color = cx.theme().colors();
        let bg = color
            .title_bar_background
            .blend(color.panel_background.opacity(0.32));

        let no_open_projects = !self.contents.has_open_projects;
        let no_search_results = self.contents.entries.is_empty();

        v_flex()
            .id("workspace-sidebar")
            .key_context("ThreadsSidebar")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::editor_move_down))
            .on_action(cx.listener(Self::editor_move_up))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::expand_selected_entry))
            .on_action(cx.listener(Self::collapse_selected_entry))
            .on_action(cx.listener(Self::toggle_selected_fold))
            .on_action(cx.listener(Self::fold_all))
            .on_action(cx.listener(Self::unfold_all))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::focus_sidebar_filter))
            .on_action(cx.listener(|this, _: &OpenRecent, window, cx| {
                this.recent_projects_popover_handle.toggle(window, cx);
            }))
            .font(ui_font)
            .h_full()
            .w(self.width)
            .bg(bg)
            .border_r_1()
            .border_color(color.border)
            .child(self.render_sidebar_header(no_open_projects, window, cx))
            .map(|this| {
                if no_open_projects {
                    this.child(self.render_empty_state(cx))
                } else {
                    this.child(
                        v_flex()
                            .relative()
                            .flex_1()
                            .overflow_hidden()
                            .child(
                                list(
                                    self.list_state.clone(),
                                    cx.processor(Self::render_list_entry),
                                )
                                .flex_1()
                                .size_full(),
                            )
                            .when(no_search_results, |this| {
                                this.child(self.render_no_results(cx))
                            })
                            .when_some(sticky_header, |this, header| this.child(header))
                            .vertical_scrollbar_for(&self.list_state, window, cx),
                    )
                }
            })
            .child(
                h_flex()
                    .p_1()
                    .gap_1()
                    .justify_between()
                    .border_t_1()
                    .border_color(cx.theme().colors().border)
                    .child(self.render_sidebar_toggle_button(cx))
                    .child(self.render_recent_projects_button(cx)),
            )
    }
}
