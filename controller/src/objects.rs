mod zone;
mod point;

use std::{cell::RefCell, rc::Rc};

use vector::{Vec2, Vec3};
use wgpu::{Buffer, Device, Queue};

use zone::Zone;
use point::ControlPoint;
pub use point::ControlPointMode;

use crate::Controller;

const MAX_OBJECT_ID: u16 = 65535;
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
#[derive(Clone)]
pub struct ObjectBuffers {
    /// List of zone paths
    zones: Rc<RefCell<VecWithBuffer<Zone>>>,

    /// List of control points
    points: Rc<RefCell<VecWithBuffer<ControlPoint>>>,
}

impl ObjectBuffers {
    pub fn zone_count(&self) -> u16 { self.zones.borrow().items.len() as u16 }
    pub fn point_count(&self) -> u16 { self.points.borrow().items.len() as u16 }

    pub fn new(device: &Device) -> Self {
        let mut objects = Self {
            zones: Rc::new(RefCell::new(VecWithBuffer::new(device, "Zones Buffer"))),
            points: Rc::new(RefCell::new(VecWithBuffer::new(device, "Control Points Buffer"))),
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

    /// Get a clone of a zone by its ID. Returns None if the zone does not exist.
    pub fn get_zone_info(&self, zone_id: u16) -> Option<Zone> {
        if zone_id >= self.zone_count() {
            None
        } else {
            Some(self.zones.borrow().items[zone_id as usize].clone())
        }
    }

    /// Get a clone of a control point by its ID. Returns None if the point does not exist.
    pub fn get_point_info(&self, point_id: u16) -> Option<ControlPoint> {
        if point_id >= self.point_count() {
            None
        } else {
            Some(self.points.borrow().items[point_id as usize].clone())
        }
    }

    /// Add a zone to the zones buffer with a random (for now) offset, and return its ID.
    pub fn create_zone(&mut self) -> anyhow::Result<u16> {
        let zone_id = self.zone_count();
        if zone_id >= MAX_OBJECT_ID {
            anyhow::bail!("No room to create another zone!");
        }

        self.zones.borrow_mut().items.push(Zone::new(self.point_count(), Vec3::random_on_hemisphere()));
        Ok(zone_id)
    }

    /// Get the closest point to the given position within the specified zone.
    /// Returns the ID of the closest control point to the given position within the specified zone, or None if no points exist.
    /// If no zone is specified, returns the closest point across all zones.
    /// Returns a tuple containing the zone ID and the point ID of the closest control point, or None if no points exist.
    pub fn get_closest_point(&self, zone_id: Option<u16>, position: Vec2) -> Option<(u16, u16)> {
        // Get the range of points to search for
        let (start, count) = if let Some(zone_id) = zone_id {
            self.get_zone_info(zone_id)?.range()
        } else {
            (0, self.point_count())
        };
        if count == 0 { return None; }

        // Find the closest point within the specified range
        let mut closest_id = None;
        let mut min_distance = f32::MAX;
        for i in 0..count {
            let distance = self.get_point_info(start + i).unwrap().distance(position);
            if distance < min_distance {
                min_distance = distance;
                closest_id = Some(start + i);
            }
        }
        
        closest_id.map(|point_id| (self.get_point_info(point_id).unwrap().zone_id(), point_id))
    }

    /// Add a linear control point to a zone and return its ID
    pub fn create_point(&mut self, zone_id: u16, position: Vec2) -> anyhow::Result<u16> {
        let point_count = self.point_count();
        let zone_count = self.zone_count();
        if point_count >= MAX_OBJECT_ID {
            anyhow::bail!("No room to create another control point!");
        }
        if zone_id >= zone_count {
            anyhow::bail!("Zone {} does not exist!", zone_id);
        }
        
        // Add the point
        let point_id = self.zones.borrow_mut().items[zone_id as usize].add_point();
        let point = ControlPoint::new_solo(point_id, zone_id, position);
        self.points.borrow_mut().items.push(point);

        // Push the rest of the points and zones backwards
        for i in (point_id as usize + 1)..(point_count as usize + 1) {
            ControlPoint::update_id(i as u16, i as u16, i as u16 + 1, &mut self.points.borrow_mut().items)?;
        }
        for i in (zone_id as usize + 1)..(zone_count as usize) {
            self.zones.borrow_mut().items[i].add_offset(1);
        }

        Ok(point_id)
    }

    pub fn update_point(&mut self, point_id: u16, position: Option<Vec2>, mode: Option<ControlPointMode>, left_handle: Option<Vec2>, right_handle: Option<Vec2>) -> anyhow::Result<()> {
        let points = &mut self.points.borrow_mut().items;
        if let Some(position) = position {
            ControlPoint::set_position(point_id, position, points)?;
        }
        if let Some(mode) = mode {
            ControlPoint::set_handle_mode(point_id, mode, points)?;
        }
        if let Some(left_handle) = left_handle {
            ControlPoint::set_left_handle(point_id, left_handle, points)?;
        }
        if let Some(right_handle) = right_handle {
            ControlPoint::set_right_handle(point_id, right_handle, points)?;
        }

        Ok(())
    }

    pub fn update_zone_position(&mut self, point_id: u16, position: Vec2) -> anyhow::Result<()> {
        let point = self.get_point_info(point_id).unwrap();
        let delta = position - point.position();
        let (start, count) = self.get_zone_info(point.zone_id()).unwrap().range();
        for i in 0..count {
            ControlPoint::add_position_delta(start + i, delta, &mut self.points.borrow_mut().items)?;
        }
        Ok(())
    }

    /// Remove a control point from the points list.
    /// Returns true if the path was deleted.
    fn delete_point_with_threshold(&mut self, point_id: u16, threshold: u16) -> anyhow::Result<bool> {
        let point = self.get_point_info(point_id).unwrap();
        let zone_id = point.zone_id();
        let (_, count) = self.get_zone_info(zone_id).unwrap().range();

        // If there are two or fewer points, delete the whole zone instead, as that's the minimum requirement for a path.
        if count.max(1) <= threshold { self.delete_zone(zone_id)?; return Ok(true); }
        let zone_id = zone_id as usize;

        let zone_count = self.zone_count() as usize;
        let zones = &mut self.zones.borrow_mut().items;
        
        ControlPoint::remove_point(point_id, &mut self.points.borrow_mut().items)?; // updates the list
        zones[zone_id].dec_points()?;
        
        // Pull the rest of the points and zones backwards
        for i in (zone_id as usize + 1)..zone_count {
            zones[i].add_offset(-1);
        }

        Ok(false)
    }

    pub fn delete_point(&mut self, point_id: u16) -> anyhow::Result<()> {
        self.delete_point_with_threshold(point_id, 2)?;
        Ok(())
    }

    pub fn delete_point_in_wip_path(&mut self, point_id: u16) -> anyhow::Result<bool> {
        self.delete_point_with_threshold(point_id, 1)
    }

    /// Remove a whole zone and all its points.
    pub fn delete_zone(&mut self, zone_id: u16) -> anyhow::Result<()> {
        let (start, count) = self.get_zone_info(zone_id).unwrap().range();
        
        // Remove points backwards to preserve indices
        for i in 0..count {
            let point_id = start + count - 1 - i;
            self.delete_point_with_threshold(point_id, 0)?; // threshold 0 means it won't just call this method again
        }

        // Remove the zone from the list, and pull the rest of them backward
        let new_zone_count = self.zone_count() as usize - 1;
        let zones = &mut self.zones.borrow_mut().items;
        let points = &mut self.points.borrow_mut().items;
        zones.remove(zone_id as usize);
        for i in (zone_id as usize)..new_zone_count {
            // The zone should already have had its range updated
            let (start, count) = zones[i].range();
            for j in 0..count {
                points[(start + j) as usize].set_zone_id(i as u16);
            }
        }

        Ok(())
    }

    /// Get references to the buffers and their sizes. Recreates the buffers if needed.
    /// The caller should keep track of the previous buffer sizes and recreate the bind group if they differ.
    pub fn get_buffers(&mut self, device: &Device) -> BufferStates {
        (
            self.zones.borrow_mut().get_buffer(device),
            self.points.borrow_mut().get_buffer(device),
        )
    }

    /// Add a command to write the current lists to the buffers. Assumes the buffers have already been sized.
    pub fn write_buffers(&self, queue: &Queue) {
        self.zones.borrow().write(queue);
        self.points.borrow().write(queue);
    }

    pub fn object_counts(&self) -> (usize, usize) { (self.zone_count() as usize, self.point_count() as usize) }
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