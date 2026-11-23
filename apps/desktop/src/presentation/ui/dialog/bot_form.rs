//! The new/edit bot form: identity fields, mascot, runtime picker.

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement, Styled, div, prelude::*, px,
};

use crate::domain::model::{MascotShape, Presence};
use crate::presentation::theme::{Theme, metrics, text as typo};
use crate::presentation::ui::shell::Shell;
use crate::presentation::ui::widgets;

use super::{BotForm, Dialog, Field, PickerHost};

impl Shell {
    pub(super) fn bot_form_dialog(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        form: &BotForm,
    ) -> AnyElement {
        let editing = form.editing();
        let header = self.dialog_header(
            theme,
            cx,
            if editing { "Edit bot" } else { "New bot" },
            Some(if editing {
                "The chat, the sidebar and the panel follow these fields the moment you save."
                    .to_string()
            } else {
                "It answers in this chat. You can change any of this later in its panel."
                    .to_string()
            }),
        );

        let name = self.labelled(
            theme,
            Field::Name.label(),
            None,
            self.form_field(theme, cx, form, Field::Name, false),
        );
        let role = self.labelled(
            theme,
            Field::Role.label(),
            None,
            self.form_field(theme, cx, form, Field::Role, false),
        );

        let hover = theme.hover_wash();
        let folder_active = form.active == Field::Folder;
        let folder_text = form.folder.trim().to_string();
        let folder_empty = folder_text.is_empty();

        let folder_control = div()
            .id("form-folder-picker")
            .h(px(metrics::CONTROL_LG))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .rounded(px(metrics::RADIUS_CONTROL))
            .bg(theme.inset)
            .border_1()
            .border_color(if folder_active {
                theme.edge
            } else {
                theme.hairline
            })
            .cursor_pointer()
            .track_focus(&form.focus[Field::Folder.index()])
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(|this, _event, window, cx| {
                this.focus_field(Field::Folder, window, cx);
                this.pick_working_folder(window, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .min_w_0()
                    .flex_1()
                    .child(widgets::icon("folder", 16.0, theme.ink_secondary))
                    .child(
                        div()
                            .truncate()
                            .font_family(theme.font_mono.clone())
                            .text_size(px(typo::BODY))
                            .text_color(if folder_empty {
                                theme.ink_tertiary
                            } else {
                                theme.ink
                            })
                            .child(if folder_empty {
                                "~/my-project".to_string()
                            } else {
                                folder_text
                            }),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(8.0))
                    .py(px(2.0))
                    .rounded_full()
                    .bg(theme.control)
                    .text_size(px(typo::TINY))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.ink_secondary)
                    .child("Browse..."),
            )
            .into_any_element();

        let folder = self.labelled(theme, "Working folder", None, folder_control);
        let instructions = self.labelled(
            theme,
            Field::Instructions.label(),
            Some("Optional · Shift+Enter for a new line"),
            self.form_field(theme, cx, form, Field::Instructions, false),
        );

        let runtime_name = self.engine.runtime_label(&form.runtime);
        let current_icon = crate::application::engine::Engine::runtime_icon(&form.runtime);
        let is_provider_open = form.provider.open;

        let provider_trigger = div()
            .id("form-provider-trigger")
            .h(px(metrics::CONTROL_LG))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(metrics::RADIUS_CONTROL))
            .bg(theme.inset)
            .border_1()
            .border_color(if is_provider_open {
                theme.edge
            } else {
                theme.hairline
            })
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |this, _event, _window, cx| {
                if let Some(Dialog::Bot(form)) = this.dialog.as_mut() {
                    form.provider.open = !form.provider.open;
                }
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(widgets::icon(current_icon, 16.0, theme.ink))
                    .child(
                        div()
                            .text_size(px(typo::BODY))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.ink)
                            .child(if runtime_name.is_empty() {
                                "Select Provider".to_string()
                            } else {
                                runtime_name
                            }),
                    ),
            )
            .child(widgets::icon(
                if is_provider_open {
                    "x"
                } else {
                    "chevron-down"
                },
                12.0,
                theme.ink_tertiary,
            ));

        let provider_section = div()
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_2))
            .child(provider_trigger)
            .when(is_provider_open, |this| {
                this.child(self.provider_picker_view(
                    theme,
                    cx,
                    &form.provider,
                    &form.runtime,
                    PickerHost::Form,
                ))
            })
            .into_any_element();

        let mut swatches: Vec<AnyElement> = Vec::new();
        for slot in 0..8 {
            let color = theme.identity(slot);
            let selected = form.color == slot;
            swatches.push(
                div()
                    .id(("swatch", slot))
                    .p(px(2.0))
                    .rounded_full()
                    .border_1()
                    .border_color(if selected { theme.ink } else { theme.hairline })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        if let Some(Dialog::Bot(form)) = this.dialog.as_mut() {
                            form.color = slot;
                        }
                        cx.notify();
                    }))
                    .child(widgets::mascot(
                        form.mascot,
                        color,
                        Presence::Resting,
                        22.0,
                        100 + slot,
                    ))
                    .into_any_element(),
            );
        }

        let mut mascot_buttons: Vec<AnyElement> = Vec::new();
        for shape in MascotShape::ALL {
            let selected = form.mascot == shape;
            let shape_index = shape.index();
            mascot_buttons.push(
                div()
                    .id(("mascot-pick", shape_index))
                    .p(px(2.0))
                    .rounded(px(metrics::RADIUS_CONTROL))
                    .border_1()
                    .border_color(if selected { theme.ink } else { theme.hairline })
                    .bg(if selected {
                        theme.hover_wash()
                    } else {
                        gpui::transparent_black()
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        if let Some(Dialog::Bot(form)) = this.dialog.as_mut() {
                            form.mascot = shape;
                        }
                        cx.notify();
                    }))
                    .child(widgets::mascot(
                        shape,
                        theme.identity(form.color),
                        if selected {
                            Presence::Working
                        } else {
                            Presence::Resting
                        },
                        26.0,
                        200 + shape_index,
                    ))
                    .into_any_element(),
            );
        }

        // Text fields come first: name, role, folder and instructions stay in
        // the first viewport, above the mascot/color pickers.
        let identity_fields = div()
            .flex()
            .gap(px(metrics::SPACE_3))
            .child(div().flex_1().child(name))
            .child(div().w(px(150.0)).child(role))
            .into_any_element();

        let body = div()
            .id("dialog-body")
            .flex_1()
            // While the provider popover is open it lives inside this scroll
            // area; freezing the scroll keeps it attached to its trigger.
            .when(!form.provider.open, |this| this.overflow_y_scroll())
            .px(px(metrics::SPACE_4))
            .pb(px(metrics::SPACE_4))
            .flex()
            .flex_col()
            .gap(px(metrics::SPACE_3))
            .child(self.labelled(theme, "Provider", None, provider_section))
            .child(identity_fields)
            .child(folder)
            .child(instructions)
            .child(
                self.labelled(
                    theme,
                    "Mascot Shape",
                    Some("Choose from 26 expressive agent silhouettes"),
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(metrics::SPACE_1))
                        .children(mascot_buttons)
                        .into_any_element(),
                ),
            )
            .child(
                self.labelled(
                    theme,
                    "Color",
                    Some("Its mark, and its name in the transcript"),
                    div()
                        .flex()
                        .items_center()
                        .gap(px(metrics::SPACE_2))
                        // Live preview: the pick and the mood read at a glance.
                        .child(widgets::mascot(
                            form.mascot,
                            theme.identity(form.color),
                            Presence::Working,
                            40.0,
                            999,
                        ))
                        .child(div().flex().gap(px(metrics::SPACE_1)).children(swatches))
                        .into_any_element(),
                ),
            )
            .into_any_element();

        let can_submit = form.can_submit();
        let accent = theme.accent;
        let on_accent = theme.on_accent;
        let control = theme.control;
        let footer = div()
            .flex_shrink_0()
            .px(px(metrics::SPACE_4))
            .py(px(metrics::SPACE_3))
            .flex()
            .items_center()
            .gap(px(metrics::SPACE_2))
            .border_t_1()
            .border_color(theme.hairline)
            .child(
                div()
                    .flex_1()
                    .text_size(px(typo::TINY))
                    .text_color(theme.ink_tertiary)
                    .child(if editing {
                        "Enter saves · Tab moves · Esc cancels"
                    } else {
                        "Enter creates · Tab moves · Esc closes"
                    }),
            )
            .child(widgets::segment(
                ("form-cancel", 0),
                "Cancel",
                false,
                theme,
                cx.listener(|this, _event, window, cx| this.close_dialog(window, cx)),
            ))
            .child(
                div()
                    .id("form-create")
                    .h(px(26.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .bg(if can_submit { accent } else { control })
                    .when(can_submit, |this| this.cursor_pointer())
                    .text_size(px(typo::TINY))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if can_submit {
                        on_accent
                    } else {
                        theme.ink_tertiary
                    })
                    .on_click(
                        cx.listener(|this, _event, window, cx| this.commit_bot_form(window, cx)),
                    )
                    .child(if editing {
                        "Save changes"
                    } else {
                        "Create bot"
                    })
                    .into_any_element(),
            )
            .into_any_element();

        self.dialog_shell(theme, cx, 516.0, header, body, footer)
    }
}
