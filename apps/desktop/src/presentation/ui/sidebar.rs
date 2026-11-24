//! The sidebar: the team, the rooms, and how much is left.
//!
//! Rows carry one status mark each. There are no count badges and no "needs
//! you" summaries above the list — a bot's state belongs on its own row.

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Styled, Window, div, prelude::*, px,
};

use crate::domain::model::{Bot, Chat, Presence};
use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::{ContextTarget, Shell};
use crate::presentation::ui::widgets;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContextAction {
    Pin,
    Rename,
    Delete,
}

impl Shell {
    fn room_mascot_mark(&self, chat: &Chat, theme: &Theme, size: f32) -> AnyElement {
        let members: Vec<_> = chat
            .members
            .iter()
            .take(4)
            .filter_map(|id| self.workspace.bot(*id).map(|bot| (*id, bot)))
            .collect();
        if members.is_empty() {
            return widgets::room_mark(theme.ink_secondary, size, theme.is_dark());
        }

        let mascot_size = size * 0.58;
        let positions: [(f32, f32); 4] = match members.len() {
            1 => [(0.21, 0.21), (0.0, 0.0), (0.0, 0.0), (0.0, 0.0)],
            2 => [(0.0, 0.21), (0.42, 0.21), (0.0, 0.0), (0.0, 0.0)],
            3 => [(0.0, 0.02), (0.42, 0.02), (0.21, 0.42), (0.0, 0.0)],
            _ => [(0.0, 0.0), (0.42, 0.0), (0.0, 0.42), (0.42, 0.42)],
        };

        let mut mark = div().relative().flex_shrink_0().size(px(size));
        for (index, (bot_id, bot)) in members.into_iter().enumerate() {
            let mascot_id = (chat.id as usize)
                .wrapping_mul(1_000_003)
                .wrapping_add((bot_id as usize).wrapping_mul(257))
                .wrapping_add(index + 40_000_000);
            let (x, y) = positions[index];
            mark = mark.child(
                div()
                    .absolute()
                    .left(px(size * x))
                    .top(px(size * y))
                    .size(px(mascot_size))
                    .child(widgets::mascot(
                        bot.mascot,
                        theme.identity(bot.color),
                        bot.presence,
                        mascot_size,
                        mascot_id,
                    )),
            );
        }
        mark.into_any_element()
    }

    fn run_context_action(
        &mut self,
        target: ContextTarget,
        action: ContextAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = None;
        match action {
            ContextAction::Rename => match target {
                ContextTarget::Bot(id) => self.open_edit_bot(id, window, cx),
                ContextTarget::Team(id) => self.open_edit_team(id, window, cx),
            },
            ContextAction::Pin => {
                match target {
                    ContextTarget::Bot(id) => {
                        if let Some(bot) = self.workspace.bot_mut(id) {
                            bot.pinned = !bot.pinned;
                        }
                    }
                    ContextTarget::Team(id) => {
                        if let Some(team) =
                            self.workspace.chats.iter_mut().find(|chat| chat.id == id)
                        {
                            team.pinned = !team.pinned;
                        }
                    }
                }
                self.persist_workspace();
                cx.notify();
            }
            ContextAction::Delete => self.delete_context_target(target, cx),
        }
    }

    fn delete_context_target(&mut self, target: ContextTarget, cx: &mut Context<Self>) {
        let mut remove_chat_ids = Vec::new();
        let removed_bot = match target {
            ContextTarget::Bot(id) => Some(id),
            ContextTarget::Team(_) => None,
        };
        match target {
            ContextTarget::Bot(id) => {
                self.workspace.bots.retain(|bot| bot.id != id);
                if let Some(chat) = self
                    .workspace
                    .chats
                    .iter()
                    .find(|chat| chat.bot == Some(id))
                {
                    remove_chat_ids.push(chat.id);
                }
                for team in self
                    .workspace
                    .chats
                    .iter_mut()
                    .filter(|chat| chat.is_room())
                {
                    team.members.retain(|member| *member != id);
                    if team.members.is_empty() {
                        team.preview = "Add bots to start this team".into();
                    } else {
                        team.preview = format!("{} bots in this team", team.members.len()).into();
                    }
                }
            }
            ContextTarget::Team(id) => remove_chat_ids.push(id),
        }
        let old_selected_id = self
            .workspace
            .chats
            .get(self.workspace.selected)
            .map(|chat| chat.id);
        let selected_is_removed = old_selected_id.is_some_and(|id| remove_chat_ids.contains(&id));
        if selected_is_removed {
            self.draft.clear();
        }
        for chat_id in &remove_chat_ids {
            self.drafts.remove(chat_id);
        }
        if self
            .run_chat
            .is_some_and(|id| remove_chat_ids.contains(&id))
            || self.run_bot == removed_bot
        {
            if let Some(id) = self.run_chat {
                self.engine.stop(id);
            }
            self.run_task.take();
            self.run_chat = None;
            self.run_bot = None;
            self.status = None;
        }
        self.workspace
            .chats
            .retain(|chat| !remove_chat_ids.contains(&chat.id));
        self.workspace.selected = old_selected_id
            .and_then(|id| self.workspace.chats.iter().position(|chat| chat.id == id))
            .unwrap_or_else(|| {
                self.workspace
                    .selected
                    .min(self.workspace.chats.len().saturating_sub(1))
            });
        if self.workspace.chats.is_empty() {
            self.panel_open = false;
        }
        if selected_is_removed {
            self.draft = self
                .workspace
                .chats
                .get(self.workspace.selected)
                .and_then(|chat| self.drafts.remove(&chat.id))
                .unwrap_or_default();
        }
        self.persist_workspace();
        cx.notify();
    }

    pub(crate) fn view_sidebar(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w(px(metrics::SIDEBAR_WIDTH))
            .ml(px(metrics::SPACE_2))
            .mt(px(metrics::SPACE_2))
            .mb(px(metrics::SPACE_2))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .rounded(px(metrics::RADIUS_CARD))
            .border_1()
            .border_color(theme.edge)
            .bg(theme.panel)
            .child(self.sidebar_header(theme, cx))
            .when(self.searching || !self.search.is_empty(), |this| {
                this.child(self.sidebar_search(theme, cx))
            })
            .child(if self.searching || !self.search.is_empty() {
                self.sidebar_list(theme, cx)
            } else {
                self.sidebar_home(theme, cx)
            })
            .child(self.sidebar_footer(theme, cx))
            .into_any_element()
    }

    fn sidebar_header(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .h(px(metrics::HEADER_HEIGHT))
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.hairline)
            .px(px(metrics::SPACE_2))
            .flex()
            .items_center()
            .justify_start()
            .gap(px(metrics::SPACE_1))
            .child(
                div()
                    .id("sidebar-search-button")
                    .size(px(metrics::CONTROL_LG))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .bg(theme.control)
                    .hover(|style| style.bg(theme.raised_hover))
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.searching = true;
                        window.focus(&this.search_focus.clone(), cx);
                        cx.notify();
                    }))
                    .child(widgets::icon("search", 17.0, theme.ink)),
            )
            .child(
                div()
                    .id("sidebar-create-button")
                    .size(px(metrics::CONTROL_LG))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .bg(theme.control)
                    .hover(|style| style.bg(theme.raised_hover))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.create_menu_open = !this.create_menu_open;
                        cx.notify();
                    }))
                    .child(widgets::icon("plus", 19.0, theme.ink)),
            )
            .into_any_element()
    }

    /// One row of the "+" menu: glyph, name, one-line detail.
    #[allow(clippy::too_many_arguments)]
    fn menu_entry<F>(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        id: (&'static str, usize),
        glyph: &'static str,
        label: &'static str,
        detail: &'static str,
        on_click: F,
    ) -> AnyElement
    where
        F: Fn(&mut Shell, &mut Window, &mut Context<Shell>) + 'static,
    {
        let hover = theme.hover_wash();
        div()
            .id(id)
            .h(px(metrics::MENU_ENTRY_HEIGHT))
            .px(px(metrics::SPACE_3))
            .flex()
            .items_center()
            .gap(px(metrics::SPACE_3))
            .rounded(px(metrics::RADIUS_CONTROL))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |this, _event, window, cx| on_click(this, window, cx)))
            .child(widgets::icon(glyph, 16.0, theme.ink_secondary))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .w_full()
                            .text_size(px(typo::BODY))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.ink)
                            .truncate()
                            .child(label),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_size(px(typo::TINY))
                            .text_color(theme.ink_tertiary)
                            .truncate()
                            .child(detail),
                    ),
            )
            .into_any_element()
    }

    /// The "+" menu hanging under the header button: New bot, New team.
    /// Rendered at window level so its backdrop can catch clicks anywhere.
    pub(crate) fn view_create_menu(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.create_menu_open {
            return None;
        }
        Some(
            div()
                .id("create-menu-layer")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(
                    div()
                        .id("create-menu-backdrop")
                        .absolute()
                        .inset_0()
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.create_menu_open = false;
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .absolute()
                        // Under the header's "+" button: the sidebar sits 8px
                        // from the left edge and is 256 wide; the menu is 200.
                        .top(px(metrics::SPACE_2
                            + metrics::HEADER_HEIGHT
                            + metrics::SPACE_2))
                        .left(px(metrics::SPACE_2 + metrics::SIDEBAR_WIDTH
                            - metrics::SPACE_2
                            - 200.0))
                        .w(px(metrics::MENU_WIDTH))
                        .flex()
                        .flex_col()
                        .gap(px(metrics::SPACE_HALF))
                        .p(px(metrics::SPACE_HALF))
                        .rounded(px(metrics::RADIUS_CONTROL))
                        .bg(theme.menu)
                        .border_1()
                        .border_color(theme.edge)
                        .shadow_md()
                        .child(self.menu_entry(
                            theme,
                            cx,
                            ("create-menu-bot", 0),
                            "bot",
                            "New bot",
                            "A single agent with its own chat",
                            |this, window, cx| this.open_new_bot(window, cx),
                        ))
                        .child(self.menu_entry(
                            theme,
                            cx,
                            ("create-menu-team", 1),
                            "users",
                            "New team",
                            "Several bots sharing one chat",
                            |this, window, cx| this.open_new_team(window, cx),
                        )),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn view_context_menu(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (target, position) = self.context_menu?;
        let pinned = match target {
            ContextTarget::Bot(id) => self.workspace.bot(id).is_some_and(|bot| bot.pinned),
            ContextTarget::Team(id) => self
                .workspace
                .chats
                .iter()
                .find(|chat| chat.id == id)
                .is_some_and(|chat| chat.pinned),
        };
        let label = if pinned { "Unpin" } else { "Pin" };
        Some(
            div()
                .id("context-menu-layer")
                .absolute()
                .inset_0()
                .occlude()
                .child(
                    div()
                        .id("context-menu-backdrop")
                        .absolute()
                        .inset_0()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.context_menu = None;
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .absolute()
                        .left(position.x)
                        .top(position.y)
                        .w(px(metrics::MENU_WIDTH))
                        .flex()
                        .flex_col()
                        .gap(px(metrics::SPACE_HALF))
                        .p(px(metrics::SPACE_HALF))
                        .rounded(px(metrics::RADIUS_CONTROL))
                        .bg(theme.menu)
                        .border_1()
                        .border_color(theme.edge)
                        .shadow_md()
                        .child(self.context_menu_entry(
                            theme,
                            cx,
                            "Pin",
                            "pin",
                            label,
                            target,
                            ContextAction::Pin,
                        ))
                        .child(self.context_menu_entry(
                            theme,
                            cx,
                            "Edit",
                            "edit",
                            "Rename",
                            target,
                            ContextAction::Rename,
                        ))
                        .child(self.context_menu_entry(
                            theme,
                            cx,
                            "Delete",
                            "trash",
                            "Delete",
                            target,
                            ContextAction::Delete,
                        )),
                )
                .into_any_element(),
        )
    }

    #[allow(clippy::too_many_arguments)] // Explicit view dependencies keep call sites readable.
    fn context_menu_entry(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        id: &'static str,
        glyph: &'static str,
        label: &'static str,
        target: ContextTarget,
        action: ContextAction,
    ) -> AnyElement {
        div()
            .id(id)
            .h(px(metrics::MENU_ROW))
            .px(px(metrics::SPACE_3))
            .flex()
            .items_center()
            .gap(px(metrics::SPACE_2))
            .rounded(px(metrics::RADIUS_CONTROL))
            .cursor_pointer()
            .hover(|style| style.bg(theme.menu_hover))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.run_context_action(target, action, window, cx)
            }))
            .text_size(px(typo::META))
            .text_color(if action == ContextAction::Delete {
                theme.danger
            } else {
                theme.ink
            })
            .child(widgets::icon(
                glyph,
                14.0,
                if action == ContextAction::Delete {
                    theme.danger
                } else {
                    theme.ink_secondary
                },
            ))
            .child(label)
            .into_any_element()
    }

    fn sidebar_home(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut bot_rows = Vec::new();
        let mut bots = self.workspace.bots.iter().collect::<Vec<_>>();
        bots.sort_by_key(|bot| !bot.pinned);
        for bot in bots {
            let Some(index) = self.chat_index_for_bot(bot.id) else {
                continue;
            };
            let mascot = widgets::mascot(
                bot.mascot,
                theme.identity(bot.color),
                bot.presence,
                44.0,
                bot.id as usize * 1000 + 1,
            );
            bot_rows.push(self.sidebar_bot_row(
                theme,
                cx,
                index,
                self.workspace.selected == index,
                bot,
                self.workspace.chats[index].time.to_string(),
                mascot,
            ));
        }

        let mut room_indices: Vec<usize> = self
            .workspace
            .chats
            .iter()
            .enumerate()
            .filter_map(|(index, chat)| chat.is_room().then_some(index))
            .collect();
        room_indices.sort_by_key(|index| !self.workspace.chats[*index].pinned);
        let mut room_rows = Vec::new();
        for index in room_indices {
            let chat = &self.workspace.chats[index];
            let mark = self.room_mascot_mark(chat, theme, metrics::MASCOT_ROW);
            room_rows.push(self.sidebar_row(
                theme,
                cx,
                index,
                self.workspace.selected == index,
                chat.title.to_string(),
                if chat.members.is_empty() {
                    "Add bots to this team".to_string()
                } else {
                    format!("{} bots · {}", chat.members.len(), chat.preview)
                },
                mark,
                None,
                chat.time.to_string(),
            ));
        }

        let room_heading = div()
            .px(px(metrics::GUTTER))
            .pt(px(metrics::SPACE_3))
            .pb(px(metrics::SPACE_1))
            .child(widgets::section_label("Rooms", theme));

        div()
            .id("sidebar-home")
            .flex_1()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .child(
                div()
                    .px(px(metrics::GUTTER))
                    .pt(px(metrics::SPACE_2))
                    .pb(px(metrics::SPACE_1))
                    .child(widgets::section_label("Bots", theme)),
            )
            .children(bot_rows)
            .child(room_heading)
            .when(room_rows.is_empty(), |this| {
                this.child(
                    div()
                        .px(px(metrics::GUTTER))
                        .py(px(metrics::SPACE_2))
                        .text_size(px(typo::META))
                        .text_color(theme.ink_tertiary)
                        .child("Create a room to build a bot team."),
                )
            })
            .children(room_rows)
            .into_any_element()
    }

    fn sidebar_search(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let empty = self.search.is_empty();
        let color = if empty { theme.ink_tertiary } else { theme.ink };
        let focused = self.searching;
        div()
            .px(px(metrics::GUTTER))
            .pb(px(metrics::SPACE_2))
            .child(
                div()
                    .id("search-field")
                    .h(px(metrics::CONTROL_MD))
                    .px(px(metrics::SPACE_2))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(metrics::RADIUS_CONTROL))
                    .bg(theme.inset)
                    .cursor_text()
                    .border_1()
                    .border_color(if focused { theme.focus } else { theme.hairline })
                    .track_focus(&self.search_focus)
                    .on_key_down(cx.listener(Self::on_search_key))
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.searching = true;
                        window.focus(&this.search_focus.clone(), cx);
                        cx.notify();
                    }))
                    .child(widgets::icon(
                        "search",
                        13.0,
                        if focused {
                            theme.ink_secondary
                        } else {
                            theme.ink_tertiary
                        },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(typo::META))
                            .text_color(color)
                            .child(if empty {
                                "Search".to_string()
                            } else {
                                self.search.clone()
                            }),
                    ),
            )
            .into_any_element()
    }

    fn sidebar_list(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let query = self.search.to_lowercase();
        let matches = |title: &str| query.is_empty() || title.to_lowercase().contains(&query);

        let mut rows: Vec<AnyElement> = Vec::new();

        rows.push(
            div()
                .px(px(metrics::GUTTER))
                .pt(px(metrics::SPACE_2))
                .pb(px(metrics::SPACE_1))
                .child(widgets::section_label("Bots", theme))
                .into_any_element(),
        );

        let mut bots = self.workspace.bots.iter().collect::<Vec<_>>();
        bots.sort_by_key(|bot| !bot.pinned);
        for bot in bots {
            if !matches(bot.name.as_ref()) {
                continue;
            }
            let Some(index) = self.chat_index_for_bot(bot.id) else {
                continue;
            };
            let mascot = widgets::mascot(
                bot.mascot,
                theme.identity(bot.color),
                bot.presence,
                44.0,
                bot.id as usize * 1000 + 3,
            );
            let selected = index == self.workspace.selected;
            rows.push(self.sidebar_bot_row(
                theme,
                cx,
                index,
                selected,
                bot,
                self.workspace.chats[index].time.to_string(),
                mascot,
            ));
        }

        let mut room_indices: Vec<usize> = self
            .workspace
            .chats
            .iter()
            .enumerate()
            .filter(|(_, chat)| chat.is_room())
            .filter(|(_, chat)| matches(chat.title.as_ref()))
            .map(|(index, _)| index)
            .collect();
        room_indices.sort_by_key(|index| !self.workspace.chats[*index].pinned);

        if !room_indices.is_empty() {
            rows.push(
                div()
                    .px(px(metrics::GUTTER))
                    .pt(px(metrics::SPACE_3))
                    .pb(px(metrics::SPACE_1))
                    .child(widgets::section_label("Rooms", theme))
                    .into_any_element(),
            );
            for index in room_indices {
                let chat = &self.workspace.chats[index];
                let selected = index == self.workspace.selected;
                let title = chat.title.to_string();
                let preview = chat.preview.to_string();
                let time = chat.time.to_string();
                let mark = self.room_mascot_mark(chat, theme, metrics::MASCOT_ROW);
                rows.push(
                    self.sidebar_row(theme, cx, index, selected, title, preview, mark, None, time),
                );
            }
        }

        if rows.len() <= 1 {
            rows.push(
                div()
                    .px(px(metrics::GUTTER))
                    .py(px(metrics::SPACE_3))
                    .text_size(px(typo::META))
                    .text_color(theme.ink_tertiary)
                    .child("Nothing matches that. Try a shorter word.")
                    .into_any_element(),
            );
        }

        div()
            .id("chat-list")
            .flex_1()
            .overflow_y_scroll()
            .pb(px(metrics::SPACE_2))
            .children(rows)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn sidebar_row(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        index: usize,
        selected: bool,
        title: String,
        preview: String,
        mark: AnyElement,
        presence: Option<Presence>,
        time: String,
    ) -> AnyElement {
        let hover = theme.hover_wash();
        let background = if selected {
            theme.raised
        } else {
            theme.ink.opacity(0.0)
        };
        let title_color = theme.ink;
        let preview_color = if selected {
            theme.ink_secondary
        } else {
            theme.ink_tertiary
        };
        let target = self.workspace.chats.get(index).map(|chat| match chat.bot {
            Some(bot_id) => ContextTarget::Bot(bot_id),
            None => ContextTarget::Team(chat.id),
        });

        div()
            .id(("chat-row", index))
            .mx(px(metrics::SPACE_1))
            .px(px(metrics::SPACE_2))
            .h(px(metrics::SIDEBAR_ROW_HEIGHT))
            .flex()
            .items_center()
            .gap(px(metrics::SPACE_2))
            .rounded(px(metrics::RADIUS_CONTROL))
            .bg(background)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.context_menu = None;
                this.dispatch(
                    crate::presentation::ui::action::UiAction::SelectChat(index),
                    cx,
                )
            }))
            .when_some(target, |this, target| {
                this.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        this.context_menu = Some((target, event.position));
                        cx.notify();
                    }),
                )
            })
            .child(mark)
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(typo::BODY))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(title_color)
                            .child(title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(typo::META))
                            .text_color(preview_color)
                            .child(preview),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(typo::TINY))
                            .text_color(theme.ink_tertiary)
                            .child(time),
                    )
                    .when_some(presence, |this, presence| {
                        this.child(widgets::presence_dot(
                            presence,
                            theme,
                            metrics::PRESENCE_DOT,
                        ))
                    }),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)] // Row builder mirrors the visible bot-row fields.
    fn sidebar_bot_row(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        index: usize,
        selected: bool,
        bot: &Bot,
        time: String,
        mascot: AnyElement,
    ) -> AnyElement {
        let hover = theme.hover_wash();
        let background = if selected {
            theme.raised
        } else {
            theme.ink.opacity(0.0)
        };
        let target = ContextTarget::Bot(bot.id);

        div()
            .id(("chat-row", index))
            .mx(px(metrics::SPACE_1))
            .px(px(metrics::SPACE_2))
            .h(px(68.0))
            .flex()
            .items_center()
            .gap(px(metrics::SPACE_2))
            .rounded(px(metrics::RADIUS_CONTROL))
            .bg(background)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.context_menu = None;
                this.dispatch(
                    crate::presentation::ui::action::UiAction::SelectChat(index),
                    cx,
                )
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.context_menu = Some((target, event.position));
                    cx.notify();
                }),
            )
            .child(mascot)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(typo::BODY))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.ink)
                                    .child(bot.name.to_string()),
                            )
                            .when(!bot.role.is_empty(), |row| {
                                row.child(
                                    div()
                                        .max_w(px(66.0))
                                        .px(px(6.0))
                                        .py(px(2.0))
                                        .rounded_full()
                                        .bg(theme.ink.opacity(0.07))
                                        .text_size(px(typo::TINY))
                                        .text_color(theme.ink_secondary)
                                        .truncate()
                                        .child(bot.role.to_string()),
                                )
                            })
                            .child(div().flex_1())
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(typo::TINY))
                                    .text_color(theme.ink_tertiary)
                                    .child(time),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(metrics::SPACE_1))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(typo::META))
                                    .text_color(if selected {
                                        theme.ink_secondary
                                    } else {
                                        theme.ink_tertiary
                                    })
                                    .child(bot.activity.to_string()),
                            )
                            .child(widgets::presence_dot(
                                bot.presence,
                                theme,
                                metrics::PRESENCE_DOT,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn sidebar_footer(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("sidebar-actions")
            .flex_shrink_0()
            .px(px(metrics::SPACE_2))
            .py(px(metrics::SPACE_2))
            .flex()
            .items_center()
            .justify_start()
            .gap(px(metrics::SPACE_1))
            .child(
                div()
                    .id("sidebar-settings")
                    .h(px(metrics::CONTROL_LG))
                    .w_full()
                    .px(px(metrics::SPACE_2))
                    .flex()
                    .items_center()
                    .justify_start()
                    .gap(px(metrics::SPACE_2))
                    .rounded(px(metrics::RADIUS_CONTROL))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.raised_hover))
                    .on_click(
                        cx.listener(|this, _event, window, cx| this.open_settings(window, cx)),
                    )
                    .child(widgets::icon("settings-outline", 16.0, theme.ink_secondary))
                    .child(
                        div()
                            .text_size(px(typo::META))
                            .text_color(theme.ink_secondary)
                            .child("Settings"),
                    ),
            )
            .into_any_element()
    }
}
