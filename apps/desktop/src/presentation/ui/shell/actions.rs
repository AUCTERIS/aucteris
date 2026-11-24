//! Workspace mutations that are not part of a run: team membership,
//! bot panel settings, panel/appearance toggles.

use gpui::{Context, PathPromptOptions, Window};
use std::path::{Path, PathBuf};

use crate::domain::model::{self, AppearancePreference, Bot, Chat, Part, Presence};
use crate::presentation::theme::Theme;
use crate::presentation::ui::dialog::BotFormSubmission;
use aucteris_harness::komet_proto::SandboxLevel;

use super::Shell;

impl Shell {
    pub(crate) fn add_allowed_folder(
        &mut self,
        bot_id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bot) = self.workspace.bot(bot_id) else {
            return;
        };
        if !crate::application::engine::Engine::supports_scoped_folders(&bot.runtime)
            || bot.sandbox != aucteris_harness::komet_proto::SandboxLevel::WorkspaceWrite
        {
            return;
        }
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Allow folder access".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = prompt.await
                && let Some(path) = paths.pop()
            {
                let _ = this.update(cx, |this, cx| {
                    let Ok(path) = path.canonicalize() else {
                        return;
                    };
                    if !path.is_dir() {
                        return;
                    }
                    let Some(bot) = this.workspace.bot_mut(bot_id) else {
                        return;
                    };
                    let private = crate::infrastructure::storage::expand_home(&bot.folder)
                        .canonicalize()
                        .unwrap_or_else(|_| {
                            crate::infrastructure::storage::expand_home(&bot.folder)
                        });
                    if path.starts_with(&private) || private.starts_with(&path) {
                        return;
                    }
                    let value = path.to_string_lossy().to_string();
                    if bot.allowed_folders.iter().any(|root| root == &value) {
                        return;
                    }
                    this.dispatch(
                        crate::presentation::ui::action::UiAction::AddAllowedFolder {
                            bot: bot_id,
                            path: value,
                        },
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    pub(crate) fn pick_attachments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chat_id = self.workspace.chat().id;
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach files".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = prompt.await {
                let paths: Vec<String> = paths
                    .into_iter()
                    .filter(|path| path.is_file())
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();
                if !paths.is_empty() {
                    let _ = this.update(cx, |this, cx| {
                        let attachments = this.draft_attachments.entry(chat_id).or_default();
                        for path in paths {
                            if !attachments.contains(&path) {
                                attachments.push(path);
                            }
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    pub(crate) fn toggle_file_diff(
        &mut self,
        chat_index: usize,
        message_index: usize,
        part_index: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(Part::Files { diff_expanded, .. }) = self
            .workspace
            .chats
            .get_mut(chat_index)
            .and_then(|chat| chat.messages.get_mut(message_index))
            .and_then(|message| message.parts.get_mut(part_index))
        {
            *diff_expanded = !*diff_expanded;
        }
        cx.notify();
    }

    pub(crate) fn open_file(&mut self, chat_index: usize, path: &str, cx: &mut Context<Self>) {
        let Some(bot_id) = self
            .workspace
            .chats
            .get(chat_index)
            .and_then(|chat| chat.bot.or_else(|| chat.members.first().copied()))
        else {
            return;
        };
        let Some(bot) = self.workspace.bots.iter().find(|bot| bot.id == bot_id) else {
            return;
        };
        let roots = std::iter::once(bot.folder.to_string())
            .chain(bot.allowed_folders.iter().cloned())
            .collect::<Vec<_>>();
        match confined_path(&roots, path, true) {
            Ok(file) => {
                cx.open_with_system(&file);
            }
            Err(error) => eprintln!("refused to open changed file: {error}"),
        }
    }

    pub(crate) fn toggle_thought(
        &mut self,
        chat_index: usize,
        message_index: usize,
        part_index: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(Part::Thought { collapsed, .. }) = self
            .workspace
            .chats
            .get_mut(chat_index)
            .and_then(|chat| chat.messages.get_mut(message_index))
            .and_then(|message| message.parts.get_mut(part_index))
        {
            *collapsed = !*collapsed;
        }
        cx.notify();
    }

    pub(crate) fn toggle_activity(
        &mut self,
        chat_index: usize,
        message_index: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(message) = self
            .workspace
            .chats
            .get_mut(chat_index)
            .and_then(|chat| chat.messages.get_mut(message_index))
        {
            message.activity_expanded = !message.activity_expanded;
        }
        cx.notify();
    }

    pub(crate) fn save_bot(&mut self, form: BotFormSubmission, cx: &mut Context<Self>) {
        if let Some(id) = form.target {
            if let Some(bot) = self.workspace.bot_mut(id) {
                bot.name = form.name.clone().into();
                bot.role = form.role.into();
                bot.runtime = form.runtime.into();
                bot.instructions = form.instructions.into();
                bot.color = form.color;
                bot.mascot = form.mascot;
            }
            if let Some(chat) = self
                .workspace
                .chats
                .iter_mut()
                .find(|chat| chat.bot == Some(id))
            {
                chat.title = form.name.into();
            }
            cx.notify();
            return;
        }

        let id = self
            .workspace
            .bots
            .iter()
            .map(|bot| bot.id)
            .max()
            .unwrap_or(0)
            + 1;
        let chat_id = self
            .workspace
            .chats
            .iter()
            .map(|chat| chat.id)
            .max()
            .unwrap_or(0)
            + 1;
        let folder = crate::infrastructure::storage::AppStorage::bot_workspace_path(
            &crate::infrastructure::storage::AppStorage::data_dir(),
            id,
        );
        self.workspace.bots.push(Bot {
            id,
            pinned: false,
            name: form.name.clone().into(),
            role: form.role.into(),
            color: form.color,
            mascot: form.mascot,
            runtime: form.runtime.into(),
            presence: Presence::Resting,
            activity: "Ready".into(),
            folder: folder.to_string_lossy().to_string().into(),
            allowed_folders: Vec::new(),
            instructions: form.instructions.into(),
            tokens: 0,
            context_limit: None,
            sandbox: SandboxLevel::WorkspaceWrite,
        });
        self.workspace.chats.push(Chat {
            id: chat_id,
            pinned: false,
            title: form.name.into(),
            bot: Some(id),
            members: Vec::new(),
            messages: Vec::new(),
            preview: "Ready for your first task".into(),
            time: model::timestamp_now(),
            session_id: None,
        });
        self.stash_draft();
        self.workspace.selected = self.workspace.chats.len() - 1;
        self.restore_draft();
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    /// Create or update the team described by the dialog.
    pub(crate) fn save_team(
        &mut self,
        target: Option<u32>,
        title: String,
        members: Vec<u32>,
        cx: &mut Context<Self>,
    ) {
        if let Some(team_id) = target {
            if let Some(team) = self
                .workspace
                .chats
                .iter_mut()
                .find(|chat| chat.id == team_id)
            {
                team.title = title.into();
                team.members = members;
                team.preview = if team.members.is_empty() {
                    "Add bots to start this team".into()
                } else {
                    format!("{} bots in this team", team.members.len()).into()
                };
            }
            cx.notify();
            return;
        }
        let id = self
            .workspace
            .chats
            .iter()
            .map(|chat| chat.id)
            .max()
            .unwrap_or(0)
            + 1;
        self.workspace.chats.push(model::Chat {
            id,
            pinned: false,
            title: title.into(),
            bot: None,
            members: members.clone(),
            messages: Vec::new(),
            preview: if members.is_empty() {
                "Add bots to start this team".into()
            } else {
                format!("{} bots in this team", members.len()).into()
            },
            time: model::timestamp_now(),
            session_id: None,
        });
        self.stash_draft();
        self.workspace.selected = self.workspace.chats.len() - 1;
        self.restore_draft();
        self.panel_open = true;
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    pub(crate) fn add_bot_to_room(&mut self, room_id: u32, bot_id: u32, cx: &mut Context<Self>) {
        let Some(room) = self
            .workspace
            .chats
            .iter_mut()
            .find(|chat| chat.id == room_id && chat.is_room())
        else {
            return;
        };
        if !room.members.contains(&bot_id) {
            room.members.push(bot_id);
            room.preview = format!("{} bots in this team", room.members.len()).into();
            cx.notify();
        }
    }

    pub(crate) fn remove_bot_from_room(
        &mut self,
        room_id: u32,
        bot_id: u32,
        cx: &mut Context<Self>,
    ) {
        let Some(room) = self
            .workspace
            .chats
            .iter_mut()
            .find(|chat| chat.id == room_id && chat.is_room())
        else {
            return;
        };
        room.members.retain(|id| *id != bot_id);
        room.preview = if room.members.is_empty() {
            "Add bots to start this team".into()
        } else {
            format!("{} bots in this team", room.members.len()).into()
        };
        cx.notify();
    }
    // ---- bot panel actions ----------------------------------------------

    pub(crate) fn reset_usage(&mut self, bot_id: u32, cx: &mut Context<Self>) {
        if let Some(bot) = self.workspace.bot_mut(bot_id) {
            bot.tokens = 0;
        }
        cx.notify();
    }

    /// Take back a file edit or an out-of-chat change, in place.
    pub(crate) fn undo_part(
        &mut self,
        chat_index: usize,
        message_index: usize,
        part_index: usize,
        cx: &mut Context<Self>,
    ) {
        let restore_root = self
            .workspace
            .chats
            .get(chat_index)
            .and_then(|chat| chat.bot.or_else(|| chat.members.first().copied()))
            .and_then(|id| self.workspace.bots.iter().find(|bot| bot.id == id))
            .map(|bot| {
                std::iter::once(bot.folder.to_string())
                    .chain(bot.allowed_folders.iter().cloned())
                    .collect::<Vec<_>>()
            });
        if let Some(chat) = self.workspace.chats.get_mut(chat_index)
            && let Some(message) = chat.messages.get_mut(message_index)
        {
            match message.parts.get_mut(part_index) {
                Some(Part::Files {
                    path,
                    old_text,
                    new_text,
                    undone,
                    undo_error,
                    ..
                }) => {
                    let result = restore_root
                        .as_deref()
                        .ok_or_else(|| "This conversation has no working folder.".to_string())
                        .and_then(|root| {
                            restore_file(root, path, old_text.as_deref(), new_text.as_deref())
                        });
                    match result {
                        Ok(()) => {
                            *undone = true;
                            *undo_error = None;
                        }
                        Err(error) => *undo_error = Some(error),
                    }
                }
                Some(Part::Note { undone, .. }) => {
                    *undone = true;
                }
                _ => {}
            }
        }
        cx.notify();
    }

    // ---- chrome ----------------------------------------------------------

    pub(crate) fn toggle_panel(&mut self, cx: &mut Context<Self>) {
        self.panel_open = !self.panel_open;
        self.panel_picker.open = false;
        cx.notify();
    }

    pub(crate) fn toggle_appearance(&mut self, cx: &mut Context<Self>) {
        let preference = match self.appearance.toggled() {
            crate::presentation::theme::Appearance::Dark => AppearancePreference::Dark,
            crate::presentation::theme::Appearance::Light => AppearancePreference::Light,
        };
        self.set_appearance(preference, cx);
    }

    pub(crate) fn set_appearance(
        &mut self,
        preference: AppearancePreference,
        cx: &mut Context<Self>,
    ) {
        self.appearance = match preference {
            AppearancePreference::Dark => crate::presentation::theme::Appearance::Dark,
            AppearancePreference::Light => crate::presentation::theme::Appearance::Light,
        };
        self.settings.appearance = Some(preference);
        let current = Theme::of(cx);
        let font_sans = current.font_sans.clone();
        let font_mono = current.font_mono.clone();
        let mut theme = Theme::for_appearance(self.appearance);
        theme.font_sans = font_sans;
        theme.font_mono = font_mono;
        cx.set_global(theme);
        cx.notify();
    }
}

fn confined_path(roots: &[String], path: &str, must_exist: bool) -> Result<PathBuf, String> {
    let (primary, additional) = roots
        .split_first()
        .ok_or_else(|| "This conversation has no working folder.".to_string())?;
    let primary = crate::infrastructure::storage::expand_home(primary)
        .canonicalize()
        .map_err(|error| format!("Cannot resolve the private workspace: {error}"))?;
    let mut canonical_roots = vec![primary.clone()];
    canonical_roots.extend(additional.iter().filter_map(|root| {
        crate::infrastructure::storage::expand_home(root)
            .canonicalize()
            .ok()
    }));
    let root = &primary;
    let candidate = Path::new(path);
    let candidate = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let resolved = if must_exist {
        candidate
            .canonicalize()
            .map_err(|error| format!("Cannot resolve the file: {error}"))?
    } else if candidate.exists() {
        candidate
            .canonicalize()
            .map_err(|error| format!("Cannot resolve the file: {error}"))?
    } else {
        let parent = candidate
            .parent()
            .ok_or_else(|| "The file path has no parent folder.".to_string())?;
        let parent = parent
            .canonicalize()
            .map_err(|error| format!("Cannot resolve the file folder: {error}"))?;
        parent.join(
            candidate
                .file_name()
                .ok_or_else(|| "The file path has no name.".to_string())?,
        )
    };
    if !canonical_roots
        .iter()
        .any(|root| resolved.starts_with(root))
    {
        return Err("The file is outside this bot’s approved folders.".into());
    }
    Ok(resolved)
}

fn restore_file(
    roots: &[String],
    path: &str,
    old_text: Option<&str>,
    new_text: Option<&str>,
) -> Result<(), String> {
    let target = confined_path(roots, path, false)?;
    let expected = new_text.ok_or_else(|| {
        "This change has no saved after-version, so it cannot be safely undone.".to_string()
    })?;
    let current = std::fs::read_to_string(&target)
        .map_err(|error| format!("Cannot read the current file: {error}"))?;
    if current != expected {
        return Err("The file changed again after this action. Nothing was overwritten.".into());
    }
    if let Some(previous) = old_text {
        std::fs::write(&target, previous)
            .map_err(|error| format!("Could not restore the previous file: {error}"))?;
    } else {
        std::fs::remove_file(&target)
            .map_err(|error| format!("Could not remove the created file: {error}"))?;
    }
    Ok(())
}
