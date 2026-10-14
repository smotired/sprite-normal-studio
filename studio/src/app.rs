mod clipboard;
mod file;
mod editor;
mod input;
mod menu;
mod new_project;
mod recent;
mod relink;
mod sidebar;
mod status;
mod tabs;
mod toolbar;
mod welcome;
mod workspace;

use std::path::PathBuf;

use controller::Controller;
use eframe::egui;
use eframe::wgpu::FilterMode;
use rendering::Renderer;

use crate::app::file::loader::SpriteFileSelection;

/// A project that is open in the editor, in its own tab
struct Project {
    /// The current sprite file, which is not modified
    sprite: SpriteFileSelection,
    /// The base normal map file, which is not modified, or None to start from a flat normal map
    normal: Option<SpriteFileSelection>,

    /// Path to the workspace file. If it hasn't been saved yet, this is only where Save As starts.
    path: PathBuf,
    /// If the workspace file was opened from or saved to the path
    saved: bool,

    /// Where the generated normal map was last exported, which Export writes to again
    export_path: Option<PathBuf>,

    /// What the workspace file holds, or would hold for a project that hasn't been saved yet, to detect unsaved changes
    clean: workspace::WorkspaceFile,
    /// If saving would change the workspace file, updated every frame while it is the current project
    dirty: bool,

    /// The project's objects, history, camera and tool
    controller: Controller,
}

/// Defines application state
pub struct StudioApp {
    /// The open projects, one per tab. The Welcome screen is shown when there are none.
    projects: Vec<Project>,
    /// Index of the project whose tab is selected
    current: usize,
    /// If the selected tab changed since the tab bar was last shown, so it can be scrolled into view
    scroll_to_current: bool,

    /// Recently opened or saved workspace files
    recent: recent::RecentFiles,

    /// The New Project dialog, if it is open
    new_project_dialog: Option<new_project::NewProjectDialog>,

    /// The Missing Files dialog, if a workspace file is being opened whose images couldn't be loaded
    relink_dialog: Option<relink::RelinkDialog>,

    /// Selections that are no longer used, whose textures are freed at the start of the next frame,
    /// since the current frame may already be drawing them
    retired_selections: Vec<SpriteFileSelection>,

    /// Renderer device
    renderer: Renderer,
    /// Render state
    render_state: eframe::egui_wgpu::RenderState,
    /// Renderer viewport texture ID
    viewport_texture_id: egui::TextureId,

    /// If the dialog with the full status text is open
    show_status_dialog: bool,

    /// Sidebar sections for each editor tool
    tool_sidebars: sidebar::ToolSidebars,

    /// If the window may close even though there are unsaved changes, because the user chose not to save them
    allow_close: bool,

    /// While closing the window, the next project to ask about saving its unsaved changes
    quitting: Option<usize>,

    /// The last zones pasted from the clipboard, in any project
    last_paste: Option<clipboard::LastPaste>,
}

/// Default initializer for application state
impl StudioApp {
    pub fn new(render_state: eframe::egui_wgpu::RenderState) -> Self {
        // The renderer needs buffers to start with. Each project's controller is bound to it when its tab is selected.
        let mut controller = Controller::new(&render_state.device);
        let renderer = Renderer::new(&render_state.device, controller.selection_buffer(), controller.object_buffers(&render_state.device));
        let viewport_texture_id = render_state.renderer.write()
            .register_native_texture(&render_state.device, renderer.view(), FilterMode::Nearest);

        Self {
            projects: Vec::new(),
            current: 0,
            scroll_to_current: false,
            recent: recent::RecentFiles::load(),
            new_project_dialog: None,
            relink_dialog: None,
            retired_selections: Vec::new(),
            renderer,
            render_state,
            viewport_texture_id,
            show_status_dialog: false,
            tool_sidebars: Default::default(),
            allow_close: false,
            quitting: None,
            last_paste: None,
        }
    }

    /// The project whose tab is selected, if any
    fn project(&self) -> Option<&Project> {
        self.projects.get(self.current)
    }

    /// The project whose tab is selected, if any
    fn project_mut(&mut self) -> Option<&mut Project> {
        self.projects.get_mut(self.current)
    }

    /// Give the current project's images to its controller, after they are loaded or changed
    fn update_input_textures(&mut self) {
        let Some(project) = self.projects.get_mut(self.current) else { return; };

        // Without a base normal map, start from a flat one. The controller keeps the texture alive while it uses it.
        let flat;
        let normal = match &project.normal {
            Some(normal) => normal.texture(),
            None => {
                flat = file::loader::flat_normal_texture(&self.render_state, project.sprite.size());
                &flat
            },
        };

        project.controller.set_inputs(&self.render_state.device, project.sprite.texture(), normal);
        self.renderer.set_inputs(&self.render_state.device, project.sprite.texture(), project.controller.output());
    }

    /// Select a project's tab, drawing it in the editor. Its selection and camera are kept.
    fn activate(&mut self, ctx: &egui::Context, index: usize) {
        let Some(project) = self.projects.get_mut(index) else { return; };
        let device = &self.render_state.device;
        self.renderer.set_inputs(device, project.sprite.texture(), project.controller.output());
        self.renderer.set_objects(device, project.controller.selection_buffer(), project.controller.object_buffers(device));

        self.current = index;
        self.scroll_to_current = true;
        self.show_status_dialog = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
    }

    /// Open a project in a new tab and select it
    fn add_project(&mut self, ctx: &egui::Context, project: Project) {
        self.projects.push(project);
        self.current = self.projects.len() - 1;
        self.update_input_textures();
        self.activate(ctx, self.current);
    }

    /// Close a project's tab without asking about unsaved changes. Closing the last one goes back to the Welcome screen.
    fn close_project(&mut self, ctx: &egui::Context, index: usize) {
        if index >= self.projects.len() { return; }
        let old = self.projects.remove(index);
        self.retired_selections.push(old.sprite);
        self.retired_selections.extend(old.normal);

        if index < self.current || self.current >= self.projects.len() {
            self.current = self.current.saturating_sub(1);
        }
        if self.projects.is_empty() {
            self.show_status_dialog = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
        } else {
            self.activate(ctx, self.current);
        }
    }

    /// Select the next tab, or the previous one going backwards, wrapping around
    fn cycle_tab(&mut self, ctx: &egui::Context, backwards: bool) {
        let count = self.projects.len();
        if count == 0 { return; }
        let next = if backwards { (self.current + count - 1) % count } else { (self.current + 1) % count };
        self.activate(ctx, next);
    }

    /// While closing the window, ask about the next project with unsaved changes. Its tab is selected and drawn
    /// for a frame first, so it's clear which project is being asked about. Closes the window once every
    /// project has been asked about, or stops if the user cancels.
    fn continue_quit(&mut self, ctx: &egui::Context) {
        let Some(mut index) = self.quitting else { return; };
        while index < self.projects.len() && !self.has_unsaved_changes(index) {
            index += 1;
        }

        if index >= self.projects.len() {
            self.quitting = None;
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if index != self.current {
            self.quitting = Some(index);
            self.activate(ctx, index);
            ctx.request_repaint();
        } else if self.confirm_discard(ctx, index) {
            self.quitting = Some(index + 1);
            ctx.request_repaint();
        } else {
            self.quitting = None;
        }
    }
}

/// Root app UI
impl eframe::App for StudioApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // The last frame has been drawn, so textures that stopped being used during it can be freed
        for selection in self.retired_selections.drain(..) {
            selection.free(&self.render_state);
        }

        // Ask about each project with unsaved changes before closing the window. The close is cancelled,
        // then sent again once every project has been asked about.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.quitting.get_or_insert(0);
        }

        // The menu bar spans the whole window, above everything else, with the tabs below it
        let menu_command = self.show_menu_bar(ui);
        let tab_command = self.show_tab_bar(ui);

        // Workspace shortcuts work everywhere, unless a dialog is open and should get the keys instead (i.e. escape to close it)
        let modal_open = self.show_status_dialog || self.new_project_dialog.is_some() || self.relink_dialog.is_some();
        let mut workspace_command = menu_command.or(tab_command);
        if !modal_open {
            workspace_command = workspace_command.or(self.process_workspace_shortcuts(ui));
        }

        if !self.projects.is_empty() {
            self.show_project(ui, modal_open);
        } else {
            self.show_welcome(ui);
        }

        if let Some(command) = workspace_command {
            self.run_workspace_command(&ctx, command);
        }

        self.show_new_project_dialog(ui);
        self.show_relink_dialog(ui);

        // Everything that can change the project this frame has run
        self.update_dirty(&ctx);

        // Keep closing the window, unless a dialog is open that must be finished first
        if self.new_project_dialog.is_none() && self.relink_dialog.is_none() {
            self.continue_quit(&ctx);
        }
    }
}

impl StudioApp {
    /// Show the editor for the current project
    fn show_project(&mut self, ui: &mut egui::Ui, modal_open: bool) {
        // Side panels must be shown before the central panel, left to right, then the status bar below the workspace
        self.show_sidebar(ui);
        let toolbar_response = self.show_toolbar(ui);
        let status_response = self.show_status_bar(ui);

        // The rest of the space will now be taken up by the editor
        let full_rect = ui.max_rect();
        let viewport_rect = egui::Rect::from_min_max(
            egui::Pos2::new(toolbar_response.rect.max.x, full_rect.min.y),
            egui::Pos2::new(full_rect.max.x, status_response.rect.min.y),
        );

        // Handle app-level input events, unless a dialog is open and should get them instead
        if !modal_open {
            self.process_app_input(ui, viewport_rect);
        }

        // Call the controller's update method to regenerate any textures as needed
        if let Some(project) = self.projects.get_mut(self.current) {
            project.controller.update(&self.render_state.device, &self.render_state.queue);
        }

        // Render the main editor component and handle input events on the image itself
        let response = self.show_editor(ui);
        self.process_editor_input(ui, response);

        self.show_status_dialog(ui);
    }
}
