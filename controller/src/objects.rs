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

    /// Create object lists with no GPU buffers behind them, for testing the CPU side.
    #[cfg(test)]
    pub fn headless() -> Self {
        let mut siblings_buffer = VecWithBuffer::headless("Point Siblings Buffer");
        siblings_buffer.items.resize(65536, 0);

        Self {
            zones: Rc::new(RefCell::new(VecWithBuffer::headless("Zones Buffer"))),
            points: Rc::new(RefCell::new(VecWithBuffer::headless("Control Points Buffer"))),
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



/// Helpers for building objects in tests
#[cfg(test)]
pub(crate) mod test_utils {
    use studio_math::Vec2;

    use super::ObjectBuffers;

    /// Create a zone with a linear point at each position, returning the zone ID
    pub(crate) fn add_zone(objects: &mut ObjectBuffers, positions: &[(f32, f32)]) -> u16 {
        let zone_id = objects.create_zone().unwrap();
        for &position in positions {
            objects.create_point(zone_id, Vec2::from(position)).unwrap();
        }
        zone_id
    }

    /// Create a counter-clockwise square zone with corners at (0, 0) and (size, size)
    pub(crate) fn add_square(objects: &mut ObjectBuffers, size: f32) -> u16 {
        add_zone(objects, &[(0.0, 0.0), (size, 0.0), (size, size), (0.0, size)])
    }
}

#[cfg(test)]
mod tests {
    use studio_math::Vec2;

    use super::*;
    use super::test_utils::{add_zone, add_square};

    /// Nothing exists in new objects
    #[test]
    fn empty() {
        let objects = ObjectBuffers::headless();
        assert_eq!(objects.object_counts(), (0, 0));
        assert!(objects.get_zone_info(0).is_none());
        assert!(objects.get_point_info(0).is_none());
        assert!(objects.get_sibling(0).is_err());
    }

    /// Counts and lookups should follow what is created
    #[test]
    fn counts_and_lookups() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0)]);

        assert_eq!(objects.object_counts(), (2, 6));
        assert_eq!(objects.get_zone_info(1).unwrap().range(), (4, 2));
        assert_eq!(objects.get_point_info(5).unwrap().position(), Vec2::new(30.0, 0.0));
        assert_eq!(objects.get_point_info(5).unwrap().zone_id(), 1);
        assert!(objects.get_zone_info(2).is_none());
        assert!(objects.get_point_info(6).is_none());
    }

    /// Points with no siblings are their own sibling
    #[test]
    fn get_sibling() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        assert_eq!(objects.get_sibling(2).unwrap(), 2);
        assert!(objects.get_sibling(4).is_err());
    }

    /// Siblings can be found by zone
    #[test]
    fn sibling_in_zone() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (zone_id, point_id) = objects.create_branching_zone(1).unwrap();

        assert_eq!(objects.sibling_in_zone(1, zone_id), Some(point_id));
        assert_eq!(objects.sibling_in_zone(point_id, 0), Some(1));
        assert_eq!(objects.sibling_in_zone(0, zone_id), None);
    }
}
