mod zone;
mod point;
mod exterior_path;
mod buffers;
mod query;
mod create;
mod branching;
mod modify;

use std::{cell::RefCell, rc::Rc};

use wgpu::{Buffer, Device};

use zone::Zone;
pub use point::ControlPoint;
pub use point::ControlPointMode;
use buffers::VecWithBuffer;

const MAX_OBJECT_ID: u16 = 65535;
const MIN_BUFFER_SIZE: usize = 32;

/// States of the Zones buffer and Points buffer.
pub type BufferStates = ((Buffer, usize), (Buffer, usize), (Buffer, usize));

/// Manages the buffers for control points and shapes
#[derive(Clone)]
pub struct ObjectBuffers {
    /// List of zone paths
    zones: Rc<RefCell<VecWithBuffer<Zone>>>,

    /// List of control points
    points: Rc<RefCell<VecWithBuffer<ControlPoint>>>,

    /// IDs of control points' siblings. These don't actually affect
    /// the point at all during rendering and it's better for alignment if we don't
    /// pass them to the GPU.
    point_siblings: Rc<RefCell<VecWithBuffer<u16>>>,
}

impl ObjectBuffers {
    pub fn zone_count(&self) -> u16 { self.zones.borrow().items.len() as u16 }
    pub fn point_count(&self) -> u16 { self.points.borrow().items.len() as u16 }

    pub fn new(device: &Device) -> Self {
        let mut siblings_buffer = VecWithBuffer::new(device, "Point Siblings Buffer");
        siblings_buffer.items.resize(65536, 0);

        Self {
            zones: Rc::new(RefCell::new(VecWithBuffer::new(device, "Zones Buffer"))),
            points: Rc::new(RefCell::new(VecWithBuffer::new(device, "Control Points Buffer"))),
            point_siblings: Rc::new(RefCell::new(siblings_buffer)),
        }
    }

    /// Get a clone of a zone by its ID. Returns None if the zone does not exist.
    pub fn get_zone_info(&self, zone_id: u16) -> Option<Zone> {
        if zone_id >= self.zone_count() {
            None
        } else {
            Some(self.zones.borrow().items[zone_id as usize])
        }
    }

    /// Get a clone of a control point by its ID. Returns None if the point does not exist.
    pub fn get_point_info(&self, point_id: u16) -> Option<ControlPoint> {
        if point_id >= self.point_count() {
            None
        } else {
            Some(self.points.borrow().items[point_id as usize])
        }
    }

    pub fn get_sibling(&self, point_id: u16) -> anyhow::Result<u16> {
        if point_id >= self.point_count() { anyhow::bail!("Point {} does not exist!", point_id); }
        Ok(self.point_siblings.borrow().items[point_id as usize])
    }

    /// Find a sibling to the control point in the selected zone.
    pub fn sibling_in_zone(&self, point_id: u16, zone_id: u16) -> Option<u16> {
        ControlPoint::find_in_zone(point_id, zone_id, &self.points.borrow().items, &self.point_siblings.borrow().items).unwrap()
    }

    pub fn object_counts(&self) -> (usize, usize) { (self.zone_count() as usize, self.point_count() as usize) }
}
