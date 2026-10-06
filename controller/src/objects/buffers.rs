use wgpu::{Buffer, Device, Queue};

use super::{ObjectBuffers, BufferStates, MIN_BUFFER_SIZE};
use crate::Controller;

pub(super) struct VecWithBuffer<T> where T : bytemuck::Pod + bytemuck::Zeroable {
    /// Items in the vector
    pub(super) items: Vec<T>,

    /// Buffer storing these items. Only None when created without a GPU for tests.
    buffer: Option<Buffer>,

    /// Amount of items the buffer can hold before a resize
    buffer_size: usize,

    /// The label for the buffer
    label: &'static str,
}

impl<T> VecWithBuffer<T> where T : bytemuck::Pod + bytemuck::Zeroable {
    pub fn new(device: &Device, label: &'static str) -> Self {
        let items = vec![];
        let (buffer, buffer_size) = create_buffer(device, &items, label);
        Self {
            items,
            buffer: Some(buffer),
            buffer_size,
            label,
        }
    }

    /// Create a list with no GPU buffer behind it, for testing the CPU side.
    #[cfg(test)]
    pub fn headless(label: &'static str) -> Self {
        Self { items: vec![], buffer: None, buffer_size: MIN_BUFFER_SIZE, label }
    }

    /// Get the buffer and its size. Caller should keep track of buffer sizes and recreate bind groups if they change.
    pub fn get_buffer(&mut self, device: &Device) -> (Buffer, usize) {
        let required_space = &self.items.len().max(MIN_BUFFER_SIZE);
        let maintain_range = (self.buffer_size >> 2)..=(self.buffer_size);

        // Recreate the buffer if the length has grown too large or too small
        if !(maintain_range).contains(required_space) {
            println!("Need to recreate {} | Buffer size: {} |  Item count: {}", self.label, self.buffer_size, self.items.len());
            let (buffer, buffer_size) = create_buffer(device, &self.items, self.label);
            (self.buffer, self.buffer_size) = (Some(buffer), buffer_size);
        }

        (self.buffer.clone().expect("Buffer should exist when a device is available"), self.buffer_size)
    }

    /// Write the items to the buffer.
    pub fn write(&self, queue: &Queue) {
        if let Some(buffer) = &self.buffer {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&self.items[..]));
        }
    }
}

impl ObjectBuffers {
    /// Get references to the buffers and their sizes. Recreates the buffers if needed.
    /// The caller should keep track of the previous buffer sizes and recreate the bind group if they differ.
    pub fn get_buffers(&mut self, device: &Device) -> BufferStates {
        (
            self.zones.borrow_mut().get_buffer(device),
            self.points.borrow_mut().get_buffer(device),
            self.point_siblings.borrow_mut().get_buffer(device),
        )
    }

    /// Add a command to write the current lists to the buffers. Assumes the buffers have already been sized.
    pub fn write_buffers(&self, queue: &Queue) {
        self.zones.borrow().write(queue);
        self.points.borrow().write(queue);
        self.point_siblings.borrow().write(queue);
    }
}

/// Create a buffer for use with a VecWithBuffer. Return the buffer and its size of the buffer.
fn create_buffer<T>(device: &Device, vector: &[T], label: &str) -> (Buffer, usize) where T : bytemuck::Pod + bytemuck::Zeroable {
    // Make buffer the smallest power of 2 above 32 that will fit
    let item_count = {
        let mut size = MIN_BUFFER_SIZE;
        let target = vector.len();
        while size < target { size <<= 1 }
        size
    };

    // Create the buffer
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (item_count * bytemuck::bytes_of(&T::zeroed()).len()) as u64,
        mapped_at_creation: false,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });

    (buffer, item_count)
}

impl Controller {
    /// Get references to the buffers and their sizes. Recreates the buffers if needed.
    /// The caller should keep track of the previous buffer sizes and recreate the bind group if they differ.
    pub fn object_buffers(&mut self, device: &Device) -> BufferStates { self.objects.get_buffers(device) }

    /// Add a command to write the current object lists to the buffers. Assumes the buffers have already been sized.
    pub fn write_object_buffers(&self, queue: &Queue) {
        // Write the object buffers
        self.objects.write_buffers(queue);

        // Pack selected points into selection buffer
        let mut packed_bytes = vec![0; 8192];
        for &point_id in self.tool.selection() {
            let byte_index = point_id / 8;
            let bit_index = point_id % 8;
            packed_bytes[byte_index as usize] |= 1 << bit_index;
        }

        queue.write_buffer(&self.selection_buffer, 0, &packed_bytes);
    }
}
