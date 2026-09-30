use wgpu::{BindGroup, Buffer, ComputePipeline, Device, Queue, Texture, TextureView};
use controller::ViewportDataUniform;

/// Contains information about the viewport texture.
struct ViewportTexture {
    /// The current size of the viewport, in pixels.
    size: (usize, usize),

    /// The actual viewer for the viewport.
    view: TextureView,
}

/// References for the objects that have to do with WGPU rendering
struct RenderControl {
    /// The compute pipeline we use to run the shader that generates the viewport image.
    pipeline: ComputePipeline,

    /// The uniform buffer we use to send viewport data to the GPU.
    uniform_buffer: Buffer,

    /// Bind group for sending the textures and uniforms to the GPU.
    bind_group: BindGroup,

    /// TextureView for the spritesheet, passed to the renderer
    sprite_view: TextureView,

    /// TextureView for the working normal map, passed to the renderer
    normal_view: TextureView,
}

/// Primary struct for rendering the viewport.
pub struct Renderer {
    /// References to the controller objects.
    control: RenderControl,

    /// Information about the texture used for the viewport.
    texture: ViewportTexture,
}

impl Renderer {
    pub fn view(&self) -> &TextureView { &self.texture.view }

    pub fn size(&self) -> (usize, usize) { (self.texture.size.0, self.texture.size.1) }

    /// Use egui's render state to initialize our renderer.
    /// Creates our compute pipeline, texture, and buffers.
    pub fn new(device: &Device) -> Self {
        // Load the shader
        let source = format!(
            "{}\n{}\n{}\n{}\n{}",                       // Concatenate each source file into one big one
            include_str!("shader/bindings.wgsl"),       // Contains the actual bindings, including a description of each overlay flag
            include_str!("shader/helpers.wgsl"),        // Common helper functions
            include_str!("shader/spritesheet.wgsl"),    // Logic for drawing the lit spritesheet
            include_str!("shader/overlay.wgsl"),        // Logic for drawing overlay with lights, paths, shapes, etc.
            include_str!("shader/main.wgsl"),           // Main shader function which calls other stuff
        );

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport.wgsl"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });

        // Create the pipeline for the shader
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Viewport Rendering Pipeline"),
            layout: None,
            module: &shader,
            entry_point: None,
            compilation_options: Default::default(),
            cache: Default::default(),
        });

        let size: (usize, usize) = (256, 256);

        // Create the buffer for the viewport uniform
        let uniform_buffer = ViewportDataUniform::buffer(&device);
        
        // Create texture data
        let view = create_texture(&device, size);

        // Create initial input textures
        let sprite_view = create_texture(&device, (16, 16));
        let normal_view = create_texture(&device, (16, 16));

        // Create the bind group
        let bind_group = create_bind_group(
            &device,
            &pipeline,
            &view,
            &sprite_view,
            &normal_view,
            &uniform_buffer
        );

        // Set up
        let control = RenderControl { pipeline, uniform_buffer, bind_group, sprite_view, normal_view  };
        let texture = ViewportTexture { size, view };
        Self { control, texture }
    }

    /// Render the editor UI. Runs the shader and updates the texture, and returns the texture ID for use in egui.
    pub fn render(&mut self, device: &Device, queue: &Queue, uniform: ViewportDataUniform, (width, height): (usize, usize)) -> &TextureView {
        // Ensure we aren't rendering too small
        let width = width.max(16);
        let height = height.max(16);

        // Recreate texture if needed
        if (width, height) != self.texture.size {
            let view = create_texture(&device, (width, height));

            self.texture.view = view;
            self.texture.size = (width, height);

            self.control.bind_group = create_bind_group(
                &device,
                &self.control.pipeline,
                &self.texture.view,
                &self.control.sprite_view,
                &self.control.normal_view,
                &self.control.uniform_buffer
            );
        }

        // Create a command encoder
        let mut encoder = device.create_command_encoder(&Default::default());

        // Set up work groups
        {
            // We specified 16x16x16 work groups in the shader
            let blocks_x = width.div_ceil(16) as u32;
            let blocks_y = height.div_ceil(16) as u32;

            // Set up the render pass and dispatch work groups
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.control.pipeline);
            pass.set_bind_group(0, &self.control.bind_group, &[]);
            pass.dispatch_workgroups(blocks_x, blocks_y, 1);
        }

        // Write uniforms
        queue.write_buffer(&self.control.uniform_buffer, 0, uniform.bytes());

        // Submit workload
        queue.submit([encoder.finish()]);

        // Return the texture view to be updated
        &self.texture.view
    }

    /// Set the input textures for the viewport
    pub fn set_inputs(&mut self, device: &Device, sprite: &Texture, normal: &Texture)
    {
        // Get views for the textures
        self.control.sprite_view = sprite.create_view(&Default::default());
        self.control.normal_view = normal.create_view(&Default::default());

        // Must recreate the bind group
        self.control.bind_group = create_bind_group(
            device,
            &self.control.pipeline,
            &self.texture.view,
            &self.control.sprite_view,
            &self.control.normal_view,
            &self.control.uniform_buffer
        );
    }
}
    
/// Create a texture and view bindgroup for a pipeline. Should be called when the target render size changes.
fn create_texture(device: &Device, (width, height): (usize, usize)) -> TextureView {
    // Create a new texture and texture view when resizing
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("output"),
        size: wgpu::Extent3d { width: width as u32, height: height as u32, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    
    texture.create_view(&Default::default())
}

/// Create a new bind group for a pipeline. Should be called whenever the viewport is resized or texture references are changed
/// (Not just whenever the input textures are written to)
fn create_bind_group(device: &Device, pipeline: &ComputePipeline, output: &TextureView, sprite: &TextureView, normal: &TextureView, uniform: &Buffer) -> BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&output),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&sprite),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&normal),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: uniform.as_entire_binding(),
            },
        ]
    })
}