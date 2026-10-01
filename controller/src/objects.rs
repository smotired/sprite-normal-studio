mod zone;
mod point;

use vector::{Vec2, Vec3};
use wgpu::{Buffer, Device, Queue};

use zone::Zone;
use point::{ControlPoint, ControlPointMode};

use crate::Controller;

const MAX_OBJECT_ID: usize = 65535;
const MIN_BUFFER_SIZE: usize = 32;

/// States of the Zones buffer and Points buffer.
pub type BufferStates = ((Buffer, usize), (Buffer, usize));

struct VecWithBuffer<T> where T : bytemuck::Pod + bytemuck::Zeroable {
    /// Items in the vector
    items: Vec<T>,

    /// Buffer storing these items
    buffer: Buffer,

    /// Amount of items the buffer can hold before a resize
    buffer_size: usize,

    /// The label for the buffer
    label: &'static str,
}

impl<T> VecWithBuffer<T> where T : bytemuck::Pod + bytemuck::Zeroable {
    pub fn new(device: &Device, label: &'static str) -> Self {
        let items = vec![];
        let (buffer, buffer_size) = create_buffer(device, &items, &label);
        Self {
            items,
            buffer,
            buffer_size,
            label,
        }
    }

    /// Get the buffer and its size. Caller should keep track of buffer sizes and recreate bind groups if they change.
    pub fn get_buffer(&mut self, device: &Device) -> (Buffer, usize) {
        let required_space = &self.items.len().max(MIN_BUFFER_SIZE);
        let maintain_range = (self.buffer_size >> 2)..=(self.buffer_size);

        // Recreate the buffer if the length has grown too large or too small
        if !(maintain_range).contains(required_space) {
            println!("Need to recreate {} | Buffer size: {} |  Item count: {}", self.label, self.buffer_size, self.items.len());
            (self.buffer, self.buffer_size) = create_buffer(device, &self.items, self.label);
        }

        (self.buffer.clone(), self.buffer_size)
    }

    /// Write the items to the buffer.
    pub fn write(&self, queue: &Queue) {
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.items[..]));
    }
}

/// Manages the buffers for control points and shapes
pub struct ObjectBuffers {
    /// List of zone paths
    zones: VecWithBuffer<Zone>,

    /// List of control points
    points: VecWithBuffer<ControlPoint>,
}

impl ObjectBuffers {
    pub fn new(device: &Device) -> Self {
        let mut objects = Self {
            zones: VecWithBuffer::new(device, "Zones Buffer"),
            points: VecWithBuffer::new(device, "Control Points Buffer"),
        };

        // TEMP: Add an initial zone with a point of each type
        let zone_id = objects.create_zone().unwrap();

        let pt0_id = objects.create_point(zone_id, Vec2::new(100.0, 50.0)).unwrap();
        objects.update_point(
            pt0_id, 
            None, 
            Some(ControlPointMode::Continuous),
            Some(Vec2::hz(-50.0)),
            Some(Vec2::hz(50.0)),
        ).unwrap();

        let pt1_id = objects.create_point(zone_id, Vec2::new(100.0, 100.0)).unwrap();
        objects.update_point(
            pt1_id, 
            Some(Vec2::new(150.0, 150.0)), 
            None,
            None,
            None,
        ).unwrap();

        let pt2_id = objects.create_point(zone_id, Vec2::new(50.0, 150.0)).unwrap();
        objects.update_point(
            pt2_id, 
            None,
            Some(ControlPointMode::Broken),
            Some(Vec2::vt(-30.0)),
            None,
        ).unwrap();

        objects
    }

    /// Add a zone to the zones buffer with a random (for now) offset, and return its ID.
    pub fn create_zone(&mut self) -> anyhow::Result<u16> {
        if self.zones.items.len() >= MAX_OBJECT_ID {
            anyhow::bail!("No room to create another zone!");
        }

        let zone_id = self.zones.items.len() as u16;
        self.zones.items.push(Zone::new(self.points.items.len() as u16, Vec3::random_on_hemisphere()));
        Ok(zone_id)
    }

    /// Add a linear control point to a zone and return its ID
    pub fn create_point(&mut self, zone_id: u16, position: Vec2) -> anyhow::Result<u16> {
        if self.points.items.len() >= MAX_OBJECT_ID {
            anyhow::bail!("No room to create another control point!");
        }
        if zone_id as usize >= self.zones.items.len() {
            anyhow::bail!("Zone {} does not exist!", zone_id);
        }
        
        // Add the point
        let point_id = self.zones.items[zone_id as usize].add_point();
        let point = ControlPoint::new_solo(point_id, zone_id, position);
        self.points.items.push(point);

        // Push the rest of the points and zones backwards
        for i in (point_id as usize + 1)..(self.points.items.len()) {
            ControlPoint::update_id(i as u16, i as u16 + 1, &mut self.points.items)?;
        }
        for i in (zone_id as usize + 1)..(self.zones.items.len()) {
            self.zones.items[i].add_offset(1);
        }

        Ok(point_id)
    }

    pub fn update_point(&mut self, point_id: u16, position: Option<Vec2>, mode: Option<ControlPointMode>, left_handle: Option<Vec2>, right_handle: Option<Vec2>) -> anyhow::Result<()> {
        if let Some(position) = position {
            ControlPoint::set_position(point_id, position, &mut self.points.items)?;
        }
        if let Some(mode) = mode {
            ControlPoint::set_handle_mode(point_id, mode, &mut self.points.items)?;
        }
        if let Some(left_handle) = left_handle {
            ControlPoint::set_left_handle(point_id, left_handle, &mut self.points.items)?;
        }
        if let Some(right_handle) = right_handle {
            ControlPoint::set_right_handle(point_id, right_handle, &mut self.points.items)?;
        }

        Ok(())
    }

    /// Get references to the buffers and their sizes. Recreates the buffers if needed.
    /// The caller should keep track of the previous buffer sizes and recreate the bind group if they differ.
    pub fn get_buffers(&mut self, device: &Device) -> BufferStates {
        (
            self.zones.get_buffer(device),
            self.points.get_buffer(device),
        )
    }

    /// Add a command to write the current lists to the buffers. Assumes the buffers have already been sized.
    pub fn write_buffers(&self, queue: &Queue) {
        self.zones.write(queue);
        self.points.write(queue);
    }

    pub fn object_counts(&self) -> (usize, usize) { (self.zones.items.len(), self.points.items.len()) }
}

/// Create a buffer for use with a VecWithBuffer. Return the buffer and its size of the buffer.
fn create_buffer<T>(device: &Device, vector: &Vec<T>, label: &str) -> (Buffer, usize) where T : bytemuck::Pod + bytemuck::Zeroable {
    // Make buffer the smallest power of 2 above 32 that will fit
    let item_count = {
        let mut size = MIN_BUFFER_SIZE as usize;
        let target = vector.len();
        while size < target { size = size << 1 }
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
    pub fn write_object_buffers(&self, queue: &Queue) { self.objects.write_buffers(queue); }
}