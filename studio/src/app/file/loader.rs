use std::path::PathBuf;
use eframe::egui::{self, Image, Response, TextureId, Ui, Widget};
use eframe::egui_wgpu::{RenderState, wgpu};

/// Defines a selection of a sprite or normal map file and its associated texture on the GPU.
pub struct SpriteFileSelection {
    label: String,
    path: Option<PathBuf>,
    texture_id: TextureId,
    size: (u32, u32),
}

impl SpriteFileSelection {
    /// Create a new SpriteFileSelection with nothing selected.
    /// label: The label for the texture itself
    pub fn new(label: String, rs: &RenderState, path: Option<PathBuf>) -> Self {
        let (width, height) = (200, 150); // default preview texture size

        // Create a blank texture
        let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&label),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING, // we just need to copy it into the device
            view_formats: &[], // would love to put srgb here but unfortunately not supported everywhere
        });

        // Create a texture view and ID
        let view = texture.create_view(&Default::default());
        let texture_id = rs.renderer.write().register_native_texture(&rs.device, &view, wgpu::FilterMode::Nearest);

        // Set up the object with the blank texture.
        let mut selection = Self {
            label,
            path: None,
            texture_id,
            size: (width, height),
        };

        // Try to select the image, or just leave it as empty
        if let Some(path) = path {
            selection.select(path, rs);
        }

        selection
    }

    /// Attempt to select a file and put it into the texture.
    /// On failure, return an Err and leave the selection unmodified.
    fn try_select(&mut self, path: PathBuf, rs: &RenderState) -> anyhow::Result<(u32, u32)> {
        // Try to open the image file or return the error
        let image = image::open(&path)?.to_rgba8();

        // No failures beyond this point
        self.path = Some(path); // image exists so we can set path

        // Get image dimensions 
        self.size = image.dimensions();
        let (width, height) = self.size;
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };

        // Create a new texture
        let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&self.label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage:
                wgpu::TextureUsages::COPY_DST |       // we need this to write to it below
                wgpu::TextureUsages::TEXTURE_BINDING, // it is a texture
            view_formats: &[],
        });

        // Write to the texture
        rs.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &image,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width), // RGBA8 = 4 bytes per pixel
                rows_per_image: Some(height),
            },
            size,
        );

        // Update the texture ID
        let view = texture.create_view(&Default::default());
        rs.renderer.write().update_egui_texture_from_wgpu_texture(
            &rs.device, &view, wgpu::FilterMode::Nearest, self.texture_id,
        );

        Ok((width, height))
    }

    /// Attempt to select a file and load it into the texture.
    /// If selection fails, leave the selection unmodified.
    pub fn select(&mut self, path: PathBuf, rs: &RenderState) -> (u32, u32) {
        if let Ok(size) = self.try_select(path, rs) {
            size
        } else { self.size }
    }

    /// Attempt to select a file and load it into the texture.
    /// If selection fails, creates an image with the same size as the reference, but filled with a color.
    pub fn select_or_fill(&mut self, path: PathBuf, rs: &RenderState, size: (u32, u32), color: [u8; 4]) {
        if let Err(_err) = self.try_select(path.clone(), rs) {
            let (width, height) = size;
            self.size = (width, height);
            self.path = Some(path); // Keep invalid path, we will save to it.
            let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };

            // Create a new texture
            let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&self.label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage:
                    wgpu::TextureUsages::COPY_DST |       // we need this to write to it below
                    wgpu::TextureUsages::TEXTURE_BINDING, // it is a texture
                view_formats: &[],
            });

            // Create the data for the texture, which should all be the u32 color
            let image = image::RgbaImage::from_pixel(width, height, image::Rgba(color));

            // Write to the texture
            rs.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &image,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width), // RGBA8 = 4 bytes per pixel
                    rows_per_image: Some(height),
                },
                size,
            );

            // Update the texture ID
            let view = texture.create_view(&Default::default());
            rs.renderer.write().update_egui_texture_from_wgpu_texture(
                &rs.device, &view, wgpu::FilterMode::Nearest, self.texture_id,
            );
        }
    }
}

pub struct SpriteFileSelectionDisplay<'a> {
    value: &'a SpriteFileSelection,
}

impl<'a> SpriteFileSelectionDisplay<'a> {
    pub fn new(value: &'a SpriteFileSelection) -> Self {
        Self { value }
    }
}

impl Widget for SpriteFileSelectionDisplay<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        // Add the image widget or default label
        if let Some(path) = &self.value.path {
            let path = path.as_path().file_name().unwrap().to_str().unwrap().to_string();

            // Sprite should be no more than available x wide and no more than 300px tall
            let dim = {
                let mut width = ui.available_width();
                let aspect_ratio = {
                    let (tw, th) = self.value.size;
                    let (tw, th) = (tw as f32, th as f32);
                    tw / th
                };
                let mut height = width / aspect_ratio;
    
                if height > 200.0 {
                    height = 200.0;
                    width = height * aspect_ratio;
                }

                eframe::egui::vec2(width, height)
            };

            // Display the image
            ui.vertical_centered(|ui| {
                ui.add(Image::new(egui::load::SizedTexture::new(self.value.texture_id, dim)))
            })
            .inner
            .on_hover_text_at_pointer(path)
        } else {
            ui.label(format!("No {} selected.", self.value.label))
        }
    }
}