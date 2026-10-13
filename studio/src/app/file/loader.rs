use std::path::{Path, PathBuf};
use eframe::egui::{self, Image, Response, TextureId, Ui, Widget};
use eframe::egui_wgpu::{RenderState, wgpu};

/// Color of a flat normal map, pointing straight out
pub const FLAT_NORMAL_COLOR: [u8; 4] = [128, 128, 255, 255];

/// Defines a selected sprite or normal map file and its associated texture on the GPU.
pub struct SpriteFileSelection {
    label: String,
    path: PathBuf,
    texture: wgpu::Texture,
    texture_id: TextureId,
    size: (u32, u32),
}

impl SpriteFileSelection {
    /// Clone the saved texture
    pub fn texture(&self) -> &wgpu::Texture { &self.texture }

    /// The selected file
    pub fn path(&self) -> &Path { &self.path }

    /// The size of the selected image
    pub fn size(&self) -> (u32, u32) { self.size }

    /// Open an image file and load it into a new texture.
    /// label: The label for the texture itself
    pub fn open(label: &str, path: PathBuf, rs: &RenderState) -> anyhow::Result<Self> {
        let image = image::open(&path)?.to_rgba8();
        Ok(Self::from_image(label, path, rs, &image))
    }

    /// Create a selection from an image that has already been loaded, registering a new texture ID for it
    fn from_image(label: &str, path: PathBuf, rs: &RenderState, image: &image::RgbaImage) -> Self {
        let texture = create_texture(rs, label, image);
        let view = texture.create_view(&Default::default());
        let texture_id = rs.renderer.write().register_native_texture(&rs.device, &view, wgpu::FilterMode::Nearest);

        Self {
            label: label.to_owned(),
            path,
            texture,
            texture_id,
            size: image.dimensions(),
        }
    }

    /// Replace the selected file and texture with an image that has already been loaded, keeping the same texture ID
    fn set_image(&mut self, path: PathBuf, rs: &RenderState, image: &image::RgbaImage) {
        let texture = create_texture(rs, &self.label, image);

        // Update the texture ID
        let view = texture.create_view(&Default::default());
        rs.renderer.write().update_egui_texture_from_wgpu_texture(
            &rs.device, &view, wgpu::FilterMode::Nearest, self.texture_id,
        );

        self.path = path;
        self.size = image.dimensions();
        self.texture = texture;
    }

    /// Unregister the texture from egui. The selection must not be displayed after this.
    pub fn free(self, rs: &RenderState) {
        rs.renderer.write().free_texture(&self.texture_id);
    }

    /// Attempt to select a file and put it into the texture.
    /// On failure, return an Err and leave the selection unmodified.
    pub fn try_select(&mut self, path: PathBuf, rs: &RenderState) -> anyhow::Result<(u32, u32)> {
        // Try to open the image file or return the error
        let image = image::open(&path)?.to_rgba8();

        // No failures beyond this point
        self.set_image(path, rs, &image);
        Ok(self.size)
    }
}

/// Create a flat normal map texture of a size, for when there is no base normal map
pub fn flat_normal_texture(rs: &RenderState, (width, height): (u32, u32)) -> wgpu::Texture {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba(FLAT_NORMAL_COLOR));
    create_texture(rs, "flat_normal_map", &image)
}

/// Create a texture on the GPU and write an image to it
fn create_texture(rs: &RenderState, label: &str, image: &image::RgbaImage) -> wgpu::Texture {
    let (width, height) = image.dimensions();
    let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };

    // Create a new texture
    let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage:
            wgpu::TextureUsages::COPY_DST |       // we need this to write to it below
            wgpu::TextureUsages::TEXTURE_BINDING, // it is a texture
        view_formats: &[], // would love to put srgb here but unfortunately not supported everywhere
    });

    // Write to the texture
    rs.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        image,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width), // RGBA8 = 4 bytes per pixel
            rows_per_image: Some(height),
        },
        size,
    );

    texture
}

pub struct SpriteFileSelectionDisplay<'a> {
    value: &'a SpriteFileSelection,
    max_height: f32,
}

impl<'a> SpriteFileSelectionDisplay<'a> {
    pub fn new(value: &'a SpriteFileSelection) -> Self {
        Self { value, max_height: 200.0 }
    }

    /// Set the tallest the image is shown
    pub fn max_height(mut self, max_height: f32) -> Self {
        self.max_height = max_height;
        self
    }
}

impl Widget for SpriteFileSelectionDisplay<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let name = self.value.path.file_name().unwrap_or_default().to_string_lossy().into_owned();

        // Sprite should be no more than available x wide and no more than max_height tall
        let dim = {
            let mut width = ui.available_width();
            let aspect_ratio = {
                let (tw, th) = self.value.size;
                let (tw, th) = (tw as f32, th as f32);
                tw / th
            };
            let mut height = width / aspect_ratio;

            if height > self.max_height {
                height = self.max_height;
                width = height * aspect_ratio;
            }

            eframe::egui::vec2(width, height)
        };

        // Display the image
        ui.vertical_centered(|ui| {
            ui.add(Image::new(egui::load::SizedTexture::new(self.value.texture_id, dim)))
        })
        .inner
        .on_hover_text_at_pointer(name)
    }
}