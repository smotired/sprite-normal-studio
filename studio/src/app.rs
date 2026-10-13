mod file;
mod editor;
mod input;
mod menu;
mod new_project;
mod recent;
mod relink;
mod sidebar;
mod status;
mod toolbar;
mod welcome;
mod workspace;

use std::path::PathBuf;

use controller::Controller;
use eframe::egui;
use eframe::wgpu::FilterMode;
use rendering::Renderer;

use crate::app::file::loader::SpriteFileSelection;

/// The project that is open in the editor
struct Project {
    /// The current sprite file, which is not modified
    sprite: SpriteFileSelection,
    /// The current normal map file, which is not modified
    normal: SpriteFileSelection,

    /// Path to the workspace file. If it hasn't been saved yet, this is only where Save As starts.
    path: PathBuf,
    /// If the workspace file was opened from or saved to the path
    saved: bool,

    /// What the workspace file holds, or would hold for a project that hasn't been saved yet, to detect unsaved changes
    clean: workspace::WorkspaceFile,
    /// If saving would change the workspace file, updated every frame
    dirty: bool,
}

/// Defines application state
pub struct StudioApp {
    /// The open project, or None to show the Welcome screen
    project: Option<Project>,

    /// Recently opened or saved workspace files
    recent: recent::RecentFiles,

    /// The New Project dialog, if it is open
    new_project_dialog: Option<new_project::NewProjectDialog>,

    /// The Missing Files dialog, if a workspace file is being opened whose images couldn't be loaded
    relink_dialog: Option<relink::RelinkDialog>,

    /// Selections that are no longer used, whose textures are freed at the start of the next frame,
    /// since the current frame may already be drawing them
    retired_selections: Vec<SpriteFileSelection>,

    /// Controller object
    controller: Controller,

    /// Renderer device
    renderer: Renderer,
    /// Render state
    render_state: eframe::egui_wgpu::RenderState,
    /// Renderer viewport texture ID
    viewport_texture_id: egui::TextureId,

    /// If the dialog with the full tool error text is open
    show_error_dialog: bool,

    /// Sidebar sections for each editor tool
    tool_sidebars: sidebar::ToolSidebars,

    /// If the window may close even though there are unsaved changes, because the user chose not to save them
    allow_close: bool,
}

/// Default initializer for application state
impl StudioApp {
    pub fn new(render_state: eframe::egui_wgpu::RenderState) -> Self {
        let mut controller = Controller::new(&render_state.device);

        let renderer = Renderer::new(&render_state.device, controller.selection_buffer(), controller.object_buffers(&render_state.device));
        let viewport_texture_id = render_state.renderer.write()
            .register_native_texture(&render_state.device, renderer.view(), FilterMode::Nearest);

        Self {
            project: None,
            recent: recent::RecentFiles::load(),
            new_project_dialog: None,
            relink_dialog: None,
            retired_selections: Vec::new(),
            controller,
            renderer,
            render_state,
            viewport_texture_id,
            show_error_dialog: false,
            tool_sidebars: Default::default(),
            allow_close: false,
        }
    }

    fn update_input_textures(&mut self) {
        let Some(project) = &self.project else { return; };
        self.controller.set_inputs(&self.render_state.device, project.sprite.texture(), project.normal.texture());
        self.renderer.set_inputs(&self.render_state.device, project.sprite.texture(), self.controller.output());
    }

    /// Replace the open project, or close it with None to go back to the Welcome screen
    fn set_project(&mut self, ctx: &egui::Context, project: Option<Project>) {
        if let Some(old) = std::mem::replace(&mut self.project, project) {
            self.retired_selections.extend([old.sprite, old.normal]);
        }
        self.show_error_dialog = false;
        self.update_input_textures();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
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

        // Ask before closing the window with unsaved changes. The close is cancelled, then sent again if it's OK.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.has_unsaved_changes() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.confirm_discard(&ctx) {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // The menu bar spans the whole window, above everything else
        let menu_command = self.show_menu_bar(ui);

        // Workspace shortcuts work everywhere, unless a dialog is open and should get the keys instead (i.e. escape to close it)
        let modal_open = self.show_error_dialog || self.new_project_dialog.is_some() || self.relink_dialog.is_some();
        let mut workspace_command = menu_command;
        if !modal_open {
            workspace_command = workspace_command.or(self.process_workspace_shortcuts(ui));
        }

        if self.project.is_some() {
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
    }
}

impl StudioApp {
    /// Show the editor for the open project
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
        self.controller.update(&self.render_state.device, &self.render_state.queue);

        // Render the main editor component and handle input events on the image itself
        let response = self.show_editor(ui);
        self.process_editor_input(ui, response);

        self.show_error_dialog(ui);
    }
}