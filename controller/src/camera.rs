/// Defines the current state of the camera.
#[derive(Copy, Clone)]
pub struct Camera {
    // Position of the pixel the camera is centered on, from the top left of the image, if scale = 1
    pub position: [i32; 2],

    // Scale of the camera. At scale 2, each screen pixel is 2 pixels of the image. Always an integer >= 1.
    pub scale: u32,
}

impl Camera {
    pub fn new(position: [i32; 2], scale: u32) -> Self {
        Self { position, scale }
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: [0, 0],
            scale: 1,
        }
    }
}