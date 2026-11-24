//! Keyboard handling: global shortcuts, the composer, the sidebar search
//! and the root key dispatcher.

use gpui::{Context, KeyDownEvent, Window};

use super::Shell;

impl Shell {
    // ---- keys ------------------------------------------------------------

    fn shortcuts(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // A dialog owns the keyboard while it is open.
        if self.dialog.is_some() {
            return;
        }
        let key = event.keystroke.key.as_str();
        match key {
            "j" => self.dispatch(
                crate::presentation::ui::action::UiAction::ToggleAppearance,
                cx,
            ),
            "i" => self.toggle_panel(cx),
            "n" => self.open_new_bot(window, cx),
            "f" => {
                self.searching = true;
                window.focus(&self.search_focus, cx);
                cx.notify();
            }
            _ => {}
        }
    }

    /// The composer's key handler: type, delete, send.
    pub(crate) fn on_composer_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        self.caret_visible = true;
        if (modifiers.control || modifiers.platform) && event.keystroke.key == "v" {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    self.draft
                        .push_str(&text.replace("\r\n", "\n").replace('\r', "\n"));
                    cx.notify();
                }
            }
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // Ctrl+Enter is the send chord when Enter is a line break.
        if modifiers.control || modifiers.platform {
            if event.keystroke.key == "enter" && !self.settings.send_on_enter {
                self.submit(window, cx);
            }
            return;
        }
        match event.keystroke.key.as_str() {
            // Shift+Enter always keeps the line break.
            "enter" if modifiers.shift => {
                self.draft.push('\n');
                cx.notify();
            }
            "enter" if self.settings.send_on_enter => self.submit(window, cx),
            "enter" => {
                self.draft.push('\n');
                cx.notify();
            }
            "backspace" => {
                self.draft.pop();
                cx.notify();
            }
            "escape" => {
                // An open panel picker eats Escape before the draft does.
                if self.panel_picker.open {
                    self.panel_picker.open = false;
                } else {
                    self.draft.clear();
                }
                cx.notify();
            }
            key => {
                let typed = event
                    .keystroke
                    .key_char
                    .clone()
                    .or_else(|| (key == "space").then(|| " ".to_string()));
                if let Some(typed) = typed.filter(|text| !text.is_empty()) {
                    self.draft.push_str(&typed);
                    cx.notify();
                }
            }
        }
    }

    /// The sidebar search's key handler.
    pub(crate) fn on_search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.platform {
            return;
        }
        match event.keystroke.key.as_str() {
            "enter" => {
                self.searching = false;
                window.focus(&self.focus, cx);
                cx.notify();
            }
            "escape" => {
                self.search.clear();
                self.searching = false;
                window.focus(&self.focus, cx);
                cx.notify();
            }
            "backspace" => {
                self.search.pop();
                cx.notify();
            }
            key => {
                let typed = event
                    .keystroke
                    .key_char
                    .clone()
                    .or_else(|| (key == "space").then(|| " ".to_string()));
                if let Some(typed) = typed.filter(|text| !text.is_empty()) {
                    self.search.push_str(&typed);
                    cx.notify();
                }
            }
        }
    }

    /// Shortcuts work from anywhere, including an empty window.
    pub(super) fn on_root_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.create_menu_open && event.keystroke.key == "escape" {
            self.create_menu_open = false;
            cx.notify();
            return;
        }
        if event.keystroke.modifiers.control || event.keystroke.modifiers.platform {
            self.shortcuts(event, window, cx);
        }
    }
}
