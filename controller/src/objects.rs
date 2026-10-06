// TODO: This file is way too long. Split up somehow.
mod zone;
mod point;

use std::collections::VecDeque;
use std::{cell::RefCell, rc::Rc};

use studio_math::bezier::bezier_point_at;
use studio_math::{Vec2, Vec3, bezier::bezier_signed_area};
use wgpu::{Buffer, Device, Queue};

use zone::Zone;
pub use point::ControlPoint;
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
        Self {
            zones: Rc::new(RefCell::new(VecWithBuffer::new(device, "Zones Buffer"))),
            points: Rc::new(RefCell::new(VecWithBuffer::new(device, "Control Points Buffer"))),
        }
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

    /// Get a clone of the zones list

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

    /// Get the closest point on any path to the given position.
    /// Returns the ID of the zone path, and the ID of the point that STARTS the path segment.
    /// Also returns the corrected position on the path, and the t value for that segment
    /// If inserting a point into the zone path, it would go after the returned point.
    pub fn get_closest_path_point(&self, position: Vec2, scale: f32) -> Option<(u16, u16, Vec2, f32)> {
        let mut best = None as Option<(u16, u16)>;
        let mut corrected = Vec2::ZERO;
        let mut t = 0.5;
        let mut min_path_dist = f32::MAX;
        for i in 0..self.zone_count() {
            // Get the amount of points in this zone
            let (start, count) = self.get_zone_info(i).unwrap().range();
            if count == 0 { panic!("Zone {} has zero points!", i); } // should never hit. could just continue but i want to catch 0-len zones

            // Check each point to the previous point on the path
            let mut start_point = self.get_point_info(start + count - 1).unwrap();
            let mut start_id = start + count - 1;
            let mut best_start_id = start_id;
            let mut min_point_dist = f32::MAX;
            let mut corrected_to_path= Vec2::ZERO;
            let mut path_t = 0.5;
            for j in 0..count {
                let end_id = start + j;
                let end_point = self.get_point_info(end_id).unwrap();
                let (distance, corrected_to_curve, curve_t) = correct_to_bezier(
                    position, 
                    start_point.position(), start_point.right_handle(),
                    end_point.left_handle(), end_point.position(),
                    scale
                );

                if distance < min_point_dist {
                    min_point_dist = distance;
                    corrected_to_path = corrected_to_curve;
                    path_t = curve_t;
                    best_start_id = start_id;
                }

                start_point = end_point;
                start_id = end_id;
            }

            // Check if this is the closest path
            if min_point_dist < min_path_dist {
                min_path_dist = min_point_dist;
                corrected = corrected_to_path;
                t = path_t;
                best = Some((i, best_start_id));
            }
        }

        // Fold in corrected position
        best.map_or(None, |(z, p)| Some((z, p, corrected, t)))
    }

    /// Add a linear control point to a zone, to have the selected ID. Does not add any siblings.
    fn insert_point_helper(&mut self, zone_id: u16, point_id: u16, position: Vec2) -> anyhow::Result<u16> {
        // Make sure it would fit in this zone
        let point_count = self.point_count();
        let zone_count = self.zone_count();
        if point_count >= MAX_OBJECT_ID { anyhow::bail!("No room to create another control point!"); }
        if zone_id >= zone_count { anyhow::bail!("Zone {} does not exist!", zone_id); }

        let (start, count) = self.get_zone_info(zone_id).unwrap().range();
        if point_id < start || point_id > start + count {
            anyhow::bail!("Invalid point id {}: must be within range {}..={}", point_id, start, start + count);
        }
        
        // Create the point
        let point = ControlPoint::new_solo(point_id, zone_id, position);
        let points = &mut self.points.borrow_mut().items;
        
        // Push point IDs ahead, then add the point
        let points_after = point_count - point_id;
        for i in 0..points_after {
            let index = point_count - i;
            ControlPoint::update_id(index - 1, index, points);
        }
        points.insert(point_id as usize, point);

        // Push zone ranges ahead
        let zones = &mut self.zones.borrow_mut().items;
        zones[zone_id as usize].add_point();
        for i in (zone_id as usize + 1)..(zone_count as usize) {
            zones[i].add_offset(1);
        }

        Ok(point_id)
    }

    /// Add a linear control point to a zone at `t` along the path that starts with start_id
    /// Returns a list of created IDs, where the target point will always be the first one.
    pub fn insert_point(&mut self, zone_id: u16, mut start_id: u16, t: f32) -> anyhow::Result<Vec<u16>> {
        // Make sure it would fit in this zone
        let point_count = self.point_count();
        let zone_count = self.zone_count();
        if point_count >= MAX_OBJECT_ID {
            anyhow::bail!("No room to create another control point!");
        }
        if zone_id >= zone_count {
            anyhow::bail!("Zone {} does not exist!", zone_id);
        }
        let (start, count) = self.get_zone_info(zone_id).unwrap().range();
        if start_id < start || start_id >= start + count {
            anyhow::bail!("Invalid start id {}: must be within range {}..{}", start_id, start, start + count);
        }


        // Get left sync target for the path end node.
        let start_info = self.get_point_info(start_id).unwrap();
        let (start_sync_id, _) = ControlPoint::get_sync_id(start_id, true, &self.points.borrow().items)?; // TODO: what to do with flips
        let mut end_id = start + (start_id - start + 1) % count;
        let end_info = self.get_point_info(end_id).unwrap();
        let (end_sync_id, _) = ControlPoint::get_sync_id(end_id, false, &self.points.borrow().items)?; // TODO: what to do with flips
        
        // First determine how many siblings need to be created, and ensure we have room.
        let mut synced_start_ids = {
            let immut_points = &self.points.borrow().items;
            let mut ids: Vec<u16> = ControlPoint::get_siblings(start_id, immut_points).into_iter()
                // Only include siblings where the endpoints sync to the same values
                .filter(|sibling_start_id| {
                    let sibling_start_id = *sibling_start_id;
                    let sibling_start_info = self.get_point_info(sibling_start_id).unwrap();

                    // If there isn't even an end sibling in this zone, exclude
                    let sibling_end_id = ControlPoint::find_in_zone(end_id, sibling_start_info.zone_id(), immut_points);
                    if sibling_end_id.is_err() || sibling_end_id.as_ref().unwrap().is_none() { return false; }
                    let sibling_end_id = sibling_end_id.unwrap().unwrap();

                    // Make sure sibling end is right after sibling start
                    let (start, count) = self.get_zone_info(sibling_start_info.zone_id()).unwrap().range();
                    let projected_sibling_end_id = start + (sibling_start_id - start + 1) % count;
                    if sibling_end_id != projected_sibling_end_id { return false; } // there is something else between

                    // Determine what the start and end sync to
                    let ss_right = ControlPoint::get_sync_id(sibling_start_id, true, immut_points); // TODO: what to do with flips
                    let se_left  = ControlPoint::get_sync_id(sibling_end_id, false, immut_points); // TODO: what to do with flips
                    if ss_right.is_err() || se_left.is_err() { return false; }

                    // We must add a synced pair here IF:
                    // The start's right syncs to the same eventual thing as the real start's right
                    // AND
                    // The end's left syncs to the same eventual thing as the real end's left
                    ss_right.unwrap().0 == start_sync_id && se_left.unwrap().0 == end_sync_id
                }).collect();
            ids.sort();
            ids
        };
        let sibling_count = synced_start_ids.len();

        // Create the point
        let position = bezier_point_at(start_info.position(), start_info.right_handle(), end_info.left_handle(), end_info.position(), t);
        let mut point_id = self.insert_point_helper(zone_id, start_id + 1, position)?;

        // Increase end_id and everything in synced_start_ids if they're after the point
        if end_id >= point_id { end_id += 1; } // only doesn't hit if start_id is the last point
        for i in 0..sibling_count {
            if synced_start_ids[i] >= point_id {
                synced_start_ids[i] += 1;
            }
        }

        // we will add the siblings backwards because it's easier. doesn't really matter.
        let mut last_sibling_id = point_id;

        // Create the synced siblings and sync them as Both
        for i in 0..sibling_count {
            let sibling_start_id = synced_start_ids[i];
            let sibling_zone_id = self.get_point_info(sibling_start_id).unwrap().zone_id();

            // Create the synced point
            let sibling_id = self.insert_point_helper(sibling_zone_id, sibling_start_id + 1, position)?;

            // Increment all tracked IDs if they're after this point
            if point_id >= sibling_id { point_id += 1; }
            if start_id >= sibling_id { start_id += 1; }
            if end_id >= sibling_id { end_id += 1; }
            for j in (i + 1)..sibling_count {
                synced_start_ids[j] += 1;
            }

            // Update the synced point
            self.points.borrow_mut().items[sibling_id as usize].force_synced(point_id);// we will update handles later so we don't need to do anything after this.
            self.points.borrow_mut().items[sibling_id as usize].set_sibling_id(last_sibling_id);
            last_sibling_id = sibling_id;
        }
        self.points.borrow_mut().items[point_id as usize].set_sibling_id(last_sibling_id);

        // Update the point to split the curve correctly (updating the siblings as well)
        // Determine mode and handles for the path to not change
        let (mode, left_handle, right_handle, start_rh, end_lh) = {
            let start_sync_point_mode = self.get_point_info(start_sync_id).unwrap().mode();
            let end_sync_point_mode = self.get_point_info(end_sync_id).unwrap().mode();

            // If both are linear, this should be linear.
            if start_sync_point_mode == ControlPointMode::Linear && end_sync_point_mode == ControlPointMode::Linear {
                (ControlPointMode::Linear, None, None, None, None)
            } 
            
            // Otherwise it should be continuous.
            else {
                let (start_r, new_l, _, new_r, end_l) = studio_math::bezier::bezier_split_at(
                    start_info.position(),
                    start_info.right_handle(),
                    end_info.left_handle(),
                    end_info.position(),
                    t
                );

                (
                    ControlPointMode::Continuous,
                    Some(new_l - position),
                    Some(new_r - position),
                    Some(start_r - start_info.position()),
                    Some(end_l - end_info.position()),
                )
            }
        };
        self.update_point(point_id, None, Some(mode), left_handle, right_handle)?; // cascades to the new watching points

        // Also update the right and left handles of previous and next points, respectively.
        // Theoretically they should be on the same lines, so even if continuous they shouldn't affect other parts of the curve.
        // Should also cascade to watching points
        if let Some(handle) = start_rh {
            self.update_point(start_id, None, None, None, Some(handle))?;
        }
        if let Some(handle) = end_lh {
            self.update_point(end_id, None, None, Some(handle), None)?;
        }

        // Return the list of created points, which is the point ID and its siblings.
        let mut created = vec![point_id];
        for sibling_id in ControlPoint::get_siblings(point_id, &mut self.points.borrow_mut().items) {
            created.push(sibling_id);
        }
        Ok(created)
    }

    /// Add a linear control point to a zone and return its ID.
    /// Assumes this is a free point in the final zone. If you need to insert a point into
    /// a different zone, use insert_point.
    pub fn create_point(&mut self, zone_id: u16, position: Vec2) -> anyhow::Result<u16> {
        // Ensure we can create a point in this zone
        if zone_id != self.zone_count() - 1 { anyhow::bail!("Can only use create_point in the final zone, not {}!", zone_id); }
        if self.point_count() >= MAX_OBJECT_ID { anyhow::bail!("No room to create another control point!"); }

        // Create the point
        let point_id = self.zones.borrow_mut().items[zone_id as usize].add_point();
        let point = ControlPoint::new_solo(point_id, zone_id, position);
        self.points.borrow_mut().items.push(point);

        Ok(point_id)
    }

    /// Add a broken control point to some other path, to create a new zone branching from this path.
    /// Returns the ID of the the created zone and point.
    pub fn create_branching_zone(&mut self, sibling_id: u16) -> anyhow::Result<(u16, u16)> {
        // Create the zone
        let zone_count = self.zone_count();
        let point_count = self.point_count();
        if zone_count >= MAX_OBJECT_ID { anyhow::bail!("No room to create another zone!"); }
        if point_count >= MAX_OBJECT_ID { anyhow::bail!("No room to create another control point!"); }
        if sibling_id >= point_count { anyhow::bail!("Sibling point {} does not exist!", sibling_id); }
        let zone_id = self.create_zone()?;

        // Create a point branching off the sibling point toward the right
        let point_id = point_count;
        let points = &mut self.points.borrow_mut().items;
        let point = ControlPoint::new_sibling_branch_start(point_id, zone_id, sibling_id, points);
        points.push(point);
        self.zones.borrow_mut().items[zone_id as usize].add_point();

        Ok((zone_id, point_id))
    }

    /// Add a broken control point to some other path, to create a new zone branching from this path.
    /// Returns the IDs of all points created to complete the zone.
    /// Takes in the point we first branched off of (not the sibling we created), the ID of the zone we are creating, and the ID of the point we are merging back into.
    pub fn complete_branching_zone(&mut self, source_point_id: u16, creating_zone_id: u16, sibling_id: u16) -> anyhow::Result<Vec<u16>> {
        // Validate
        let zone_count = self.zone_count();
        let point_count = self.point_count();
        if point_count >= MAX_OBJECT_ID { anyhow::bail!("No room to create another control point!"); }
        if sibling_id >= point_count { anyhow::bail!("Sibling point {} does not exist!", sibling_id); }
        if source_point_id >= point_count { anyhow::bail!("Source point {} does not exist!", source_point_id); }
        if creating_zone_id >= zone_count { anyhow::bail!("Current zone {} does not exist!", creating_zone_id); }
        let source_zone_id = self.get_point_info(source_point_id).unwrap().zone_id();

        // When start = end, convert first point to a free node with a broken handle instead and don't add any other points.
        // This also means we don't have to flip
        let same_zone_id = ControlPoint::find_in_zone(sibling_id, source_zone_id, &mut self.points.borrow_mut().items)?;
        if let Some(sibling_id) = same_zone_id {
            if source_point_id == sibling_id {
                let (first_id, _) = self.get_zone_info(creating_zone_id).unwrap().range();
                self.points.borrow_mut().items[first_id as usize].force_free(first_id);
                return Ok(vec![]);
            }
        }

        // Ensure the points are connected
        if !self.check_points_connected(source_point_id, sibling_id)? {
            anyhow::bail!("Cannot join control points in disconnected zones! Make the paths close together and use the merge tool.");
        }

        // Find the exterior path to take and bail if there isn't room for all the points
        let (exterior_path, requires_flip) = self.find_exterior_path(source_point_id, sibling_id, creating_zone_id)?;
        if point_count >= MAX_OBJECT_ID - exterior_path.len().max(1) as u16 { anyhow::bail!("No room to create interior control points!"); }
        // max bc we need 65535 free to flip
        // and not -1 from path length because we do still need to create the join point's sibling

        // Create a point branching off the sibling point toward the left
        let mut created_point_id = {
            let created_point_id = self.zones.borrow_mut().items[creating_zone_id as usize].add_point();
            let points = &mut self.points.borrow_mut().items;

            let point = ControlPoint::new_sibling_branch_end(created_point_id, creating_zone_id, sibling_id, points);
            points.push(point); // goes on the end of the list because we are creating a zone
            created_point_id
        };

        // Flip the path we're creating if necessary
        if requires_flip {
            (created_point_id, _) = self.flip_zone(creating_zone_id)?;
        }

        // Ensure the start and end of the path point to the right places
        {
            let (start, count) = self.get_zone_info(creating_zone_id).unwrap().range();
            let points = &mut self.points.borrow_mut().items;

            let first_seg = exterior_path.first().unwrap();
            let last_seg = exterior_path.last().unwrap();
            let (out_id, out_right) = ControlPoint::get_sync_id(first_seg.start_id, first_seg.start_handle, points)?;
            let (in_id, in_right) = ControlPoint::get_sync_id(last_seg.end_id, last_seg.end_handle, points)?;

            let last_point = start + count - 1;
            ControlPoint::retarget(last_point, true, out_id, out_right, points)?;
            ControlPoint::retarget(start, false, in_id, in_right, points)?;
            ControlPoint::resync_handles(last_point, points)?;
            ControlPoint::resync_handles(start, points)?;
        }

        // Create interior points for everything along the chosen path
        let mut created = vec![created_point_id];
        let creating_zone = &mut self.zones.borrow_mut().items[creating_zone_id as usize];
        let points = &mut self.points.borrow_mut().items;

        // Add an interior control point at the start of each segment
        let mut last_segment = &exterior_path[0]; // exists if start != end which we checked earlier
        for i in 1..exterior_path.len() {
            let segment = &exterior_path[i];

            // Add a point for the start of the segment
            let interior_id = creating_zone.add_point();
            let point = ControlPoint::new_sibling_branch_interior(
                interior_id,
                creating_zone_id,
                segment.start_id,
                last_segment.end_id,
                last_segment.end_handle,
                segment.start_id,
                segment.start_handle,
                points
            );

            points.push(point);
            created.push(interior_id);

            last_segment = segment;
        }

        Ok(created)
    }

    pub fn update_point(&mut self, point_id: u16, position: Option<Vec2>, mode: Option<ControlPointMode>, left_handle: Option<Vec2>, right_handle: Option<Vec2>) -> anyhow::Result<()> {
        let points = &mut self.points.borrow_mut().items;
        if let Some(position) = position {
            ControlPoint::set_position(point_id, position, points)?;
        }
        if let Some(mode) = mode {
            ControlPoint::set_handle_mode(point_id, mode, points)?;
        }
        
        let mode = mode.unwrap_or(points[point_id as usize].mode());

        if let Some(left_handle) = left_handle {
            ControlPoint::set_left_handle(point_id, left_handle, points)?;
            if right_handle.is_none() && mode == ControlPointMode::Continuous {
                let current_right_handle = points[point_id as usize].right_handle() - points[point_id as usize].position();
                ControlPoint::set_right_handle(point_id, -left_handle.normalized() * current_right_handle.magnitude(), points)?;
            }
        }
        if let Some(right_handle) = right_handle {
            ControlPoint::set_right_handle(point_id, right_handle, points)?;
            if left_handle.is_none() && mode == ControlPointMode::Continuous {
                let current_left_handle = points[point_id as usize].left_handle() - points[point_id as usize].position();
                ControlPoint::set_left_handle(point_id, -right_handle.normalized() * current_left_handle.magnitude(), points)?;
            }
        }

        Ok(())
    }

    pub fn update_zone_position(&mut self, zone_id: u16, first_point_position: Vec2) -> anyhow::Result<()> {
        let (start, count) = self.get_zone_info(zone_id).unwrap().range();
        let delta = first_point_position - self.get_point_info(start).unwrap().position();
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

    /// Find a sibling to the control point in the selected zone.
    pub fn sibling_in_zone(&self, point_id: u16, zone_id: u16) -> Option<u16> {
        ControlPoint::find_in_zone(point_id, zone_id, &mut self.points.borrow_mut().items).unwrap()
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

    /// Determine if two points in different zones are connected
    fn check_points_connected(&self, point1: u16, point2: u16) -> anyhow::Result<bool> {
        // Helper function to get the next point in the same zone in the given direction.
        let next = |i: u16| {
            let zone_id = self.get_point_info(i).unwrap().zone_id();
            let (start, count) = self.get_zone_info(zone_id).unwrap().range();
            start + (i - start + 1) % count
        };

        let points = &self.points.borrow().items;

        // Run breadth-first search
        // Could optimize by doing depth first search since everything in a zone shares a range
        let mut visited = [false; 65536];
        let mut queue = VecDeque::new();
        queue.push_back(point1);
        while let Some(point_id) = queue.pop_front() {
            // Skip or mark as visited
            if visited[point_id as usize] { continue; }
            visited[point_id as usize] = true;

            // If this is our target point, the points are connected.
            if point_id == point2 { return Ok(true); }

            // Enqueue siblings
            for sibling_id in ControlPoint::get_siblings(point_id, points) {
                queue.push_back(sibling_id);
            }

            // Enqueue next node
            queue.push_back(next(point_id));
        }

        // If the queue is exhausted the points are not connected
        Ok(false)
    }
    
    // Get the signed area of a zone
    fn zone_signed_area(&self, zone_id: u16, closed: bool) -> f32 {
        let (start, count) = self.get_zone_info(zone_id).unwrap().range();
        let first = self.get_point_info(start).unwrap();

        let mut prev = first;
        let mut area = 0.0;
        for i in 1..count {
            let point = self.get_point_info(start + i).unwrap();
            area += bezier_signed_area(prev.position(), prev.right_handle(), point.left_handle(), point.position());
            prev = point;
        }

        if closed {
            area += bezier_signed_area(prev.position(), prev.right_handle(), first.left_handle(), first.position());
        }
        area
    }

    /// Helper method to find an exterior path by traversing in a direction.
    /// Takes in start position, end position, and if we should initially go right form the start position.
    /// Returns a vector of path segments and a total path signed area.
    /// 
    /// Makes the following assumptions:
    /// - Connected zones do not overlap.
    /// - start_id and end_id are not on interior paths
    /// - Interior nodes are correctly only found on interior paths.
    /// - All nodes on interior paths are correctly set up as interior nodes.
    /// - A path does exist (i.e. check_points_connected was run already).
    fn exterior_path_helper(&self, start_id: u16, end_id: u16, start_right: bool) -> anyhow::Result<(Vec<ExteriorPathSegment>, f32)> {
        // Get the handle the start is syncing to, to start by going around in the start direction.
        let points = &self.points.borrow().items;
        let (mut id, mut right) = ControlPoint::get_sync_id(start_id, start_right, points)?;

        // Helper function to get the next point in the same zone in the given direction.
        let next = |i: u16, right: bool| {
            let zone_id = self.get_point_info(i).unwrap().zone_id();
            let (start, count) = self.get_zone_info(zone_id).unwrap().range();
            start + ((i - start + count) as i32 + if right { 1 } else { -1 }) as u16 % count
        };

        // Helper function to get the normalized direction of a point's handle.
        // If the handle's magnitude is 0 (e.g. the point is linear), instead returns the normalized direction to
        // the next point's opposite handle.
        let handle_dir = |i: u16, right: bool| {
            let point = self.get_point_info(i).unwrap();
            let handle = if right { point.right_handle() } else { point.left_handle() };
            let mut dir = (handle - point.position()).normalized();
            if dir.magnitude() == 0.0 {
                let neighbor = self.get_point_info(next(i, right)).unwrap();
                let handle = if right { neighbor.left_handle() } else { neighbor.right_handle() };
                dir = (handle - point.position()).normalized();
            }
            dir
        };

        // True if the edge leaving the point by the handle is shared with any other zone.
        let edge_shared = |point_id: u16, right: bool| -> anyhow::Result<bool> {
            let handle = ControlPoint::get_sync_id(point_id, right, points)?;
            for sibling_id in ControlPoint::get_siblings(point_id, points) {
                for side in [false, true] {
                    if ControlPoint::get_sync_id(sibling_id, side, points)? == handle { return Ok(true); }
                }
            }
            Ok(false)
        };

        // Get sibling IDs of the end point, which is where we will stop.
        let end_siblings = {
            let mut siblings = ControlPoint::get_siblings(end_id, points);
            siblings.insert(0, end_id);
            siblings
        };

        // Helper method to find the exterior angle between two vectors in the right direction.
        let sweep_angle = {
            // +1.0 = interior is on the left of travel, so sweep counter-clockwise from back_dir.
            // -1.0 = interior is on the right, so sweep clockwise.
            let start_zone_id = self.get_point_info(id).unwrap().zone_id();
            let sweep = if (self.zone_signed_area(start_zone_id, true) > 0.0) == right { 1.0 } else { -1.0 };

            move |from: Vec2, to: Vec2| -> f32 {
                let a = (sweep * from.cross(to)).atan2(from.dot(to));
                if a <= 1e-4 { a + std::f32::consts::TAU } else { a }
            }
        };
        
        // Loop until we find a sibling of the end point
        let mut path = vec![];
        let mut seen = std::collections::HashSet::new();
        while !end_siblings.contains(&id) {
            // Bail if we are in an infinite loop.
            if !seen.insert((id, right)) {
                anyhow::bail!("Exterior path revisited state ({id}, {right}) from {start_id} to {end_id}");
            }
            let base_next_id = next(id, right);
            
            // Create the path segment that starts at this point and ends at the next point
            let segment = ExteriorPathSegment::new(id, right, base_next_id, !right);
            path.push(segment);

            // Find the next point in the zone and its siblings
            let mut next_id = base_next_id;
            let mut next_handle = !right; // assume we will keep going the same way
            let next_siblings = ControlPoint::get_siblings(next_id, points);

            // If it has no siblings, it's definitely the next one to go to.
            // Otherwise it's definitely not the next one to go to (by assumptions).
            let mut best_angle = f32::MAX;
            if !next_siblings.is_empty() {
                // Get the direction of the path we just took
                let back_dir = handle_dir(base_next_id, !right);
                
                // Get the full sibling ring
                let mut ring = next_siblings.clone();
                ring.push(base_next_id);
                
                // Pick the direction with the smallest angle
                // let mut best_angle = f32::MAX;
                'ring: for candidate_id in ring { // 'ring is the loop handle, so we can control target of break
                    // Reaching the end point always wins
                    if end_siblings.contains(&candidate_id) {
                        next_id = candidate_id;
                        next_handle = !right;
                        best_angle = -1.0;
                        break 'ring;
                    }

                    // Check both handles
                    for out_right in [false, true] {
                        // Skip incoming edge and interior edges
                        if candidate_id == base_next_id && out_right == !right { continue; }
                        if edge_shared(candidate_id, out_right)? { continue; }

                        // Determine the exterior angle to this branch point
                        let angle = sweep_angle(back_dir, handle_dir(candidate_id, out_right));
                        if angle < best_angle {
                            best_angle = angle;
                            next_id = candidate_id;
                            next_handle = !out_right;
                        }
                    }
                }
            }
            
            // Traverse to the next point/handle
            id = next_id;
            right = !next_handle;
        }

        // Calculate area by checking each segment
        let mut signed_area = 0.0;
        for segment in &path[..] {
            let start = self.get_point_info(segment.start_id).unwrap();
            let end = self.get_point_info(segment.end_id).unwrap();

            let pos0 = start.position();
            let pos1 = if segment.start_handle { start.right_handle() } else { start.left_handle() };
            let pos2 = if segment.end_handle { end.right_handle() } else { end.left_handle() };
            let pos3 = end.position();

            signed_area += bezier_signed_area(pos0, pos1, pos2, pos3);
        }

        Ok((path, signed_area))
    }

    /// Find an exterior path for a zone along another zone.
    /// Searches for siblings from the start point to the end point, where a continuous exterior path could
    /// be created if the points from zone_id were to be added.
    /// Returns the IDs of the nodes along the path that should have interior nodes. Also returns true if the creating zone should be flipped.
    /// Does not traverse any interior nodes, meaning the path will only be on the outside.
    /// This means that as long as start_id and end_id are both on the outside of the overall zone, there is exactly one correct path.
    /// When a branch is encountered, returns the node with one handle synced, so that we can sync directly to it and have both handles synced.
    /// Bails if a path can't be found, i.e. the zones aren't connected or either point isn't on the outside.
    /// If start or end ID have siblings, traversal will start by going down to their free node.
    fn find_exterior_path(&self, start_id: u16, end_id: u16, zone_id: u16) -> anyhow::Result<(Vec<ExteriorPathSegment>, bool)> {
        // Find the signed area of the part we are creating, up to the endpoint which we take as linear
        let (creating_start, creating_count) = self.get_zone_info(zone_id).unwrap().range();
        let signed_area = {
            let mut area = self.zone_signed_area(zone_id, false);
            let before_join = self.get_point_info(creating_start + creating_count - 1).unwrap();
            let end = self.get_point_info(end_id).unwrap();
            area += bezier_signed_area(before_join.position(), before_join.right_handle(), end.position(), end.position());
            area
        };

        // Find the path along the exterior from end to start, incrementing IDs.
        // This travels along right handles and doesn't require a flip. Does not include end or start.
        let (path_1, area_1) = {
            let (path, area) = self.exterior_path_helper(end_id, start_id, true)?;
            (path, (signed_area + area).abs())
        };

        // Find the path along the exterior from end to start, decrementing IDs.
        // This travels along left handles and would require a flip. Does not include end or start.
        let (mut path_2, area_2) = {
            let (path, area) = self.exterior_path_helper(end_id, start_id, false)?;
            (path, (signed_area + area).abs())
        };
        
        // If path 2 has a smaller absolute area, flip it, pick it, and mark the zone for flipping. Area will never be equal.
        let (path, flip) =
            if area_1 < area_2 {
                (path_1, false)
            } else {
                path_2.reverse();
                for s in &mut path_2[..] { s.flip() }
                (path_2, true)
            };

        Ok((path, flip))
    }

    /// Flip the zone with the requested ID. Reverses all handles and point order.
    /// Returns the new IDs of the start and end points.
    /// Should only be used for a brand new zone, that doesn't have anything synced to it.
    fn flip_zone(&mut self, zone_id: u16) -> anyhow::Result<(u16, u16)> {
        if zone_id != self.zone_count() - 1 { anyhow::bail!("Can only run flip_zone on the final zone, not zone {}!", zone_id); }
        if self.point_count() >= MAX_OBJECT_ID { anyhow::bail!("No space to flip the points in a zone!"); } // we should have checked at the beginning of whatever function
        let (start, count) = self.get_zone_info(zone_id).unwrap().range();
        let points = &mut self.points.borrow_mut().items;

        for i in 0..(count / 2) {
            let first_id = start + i;
            let last_id = start + count - 1 - i;

            // Flip the first point
            points[first_id as usize].flip(first_id);

            // Flip the last point
            points[last_id as usize].flip(last_id);

            // Swap the points themselves
            let last_point = points[last_id as usize];
            points[last_id as usize] = points[first_id as usize];
            points[first_id as usize] = last_point;

            // Swap the sibling IDs. Use id 65535 as a swap space which we checked earlier.
            ControlPoint::update_id(last_id, 65535, points);
            ControlPoint::update_id(first_id, last_id, points);
            ControlPoint::update_id(65535, first_id, points);
        }

        // Flip middle point if it didn't get flipped
        if count % 2 == 1 {
            let middle_id = start + count / 2;
            points[middle_id as usize].flip(middle_id);
            // don't need to move it
        }

        // Resync all the handles
        for id in start..(start + count) {
            ControlPoint::resync_handles(id, points)?;
        }

        Ok((start, start + count - 1))
    }
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

/// Return scalar distance to a line segment, the closest point on that line segment, and t for that point.
fn correct_to_line_segment(pos: Vec2, pos0: Vec2, pos1: Vec2) -> (f32, Vec2, f32) {
    // Check endpoints
    let relative = pos - pos0;
    let line_dir = (pos1 - pos0).normalized();
    let t = relative.dot(line_dir);

    // If it's past an endpoint, distance = distance to endpoint
    if t < 0.0 { (relative.magnitude(), pos0, 0.0) }
    else if t > 1.0 { (pos.distance(pos1), pos1, 1.0) }

    // If it can be projected onto the line segment,
    // distance = magnitude of projection onto orthag. vector
    else { (relative.dot(line_dir.right()).abs(), pos0 + t * line_dir, t) }
}

/// Return scalar distance to a bezier curve based on shortest distance to a line segment.
fn correct_to_bezier(pos: Vec2, pos0: Vec2, pos1: Vec2, pos2: Vec2, pos3: Vec2, scale: f32) -> (f32, Vec2, f32) {
    // Split the curve into individual lines with de Casteljau's method.
    // Determine segment count from curvature. For a cubic, deviation is at most M / 8n^2.
    // Chord error = 0.25px means largest distance from polyline to curve is at most 0.25px.
    // M is the largest magnitude of d^2B(t)/dt^2 where B is the curve. Acceleration/tightness.
    let tightness = {
        let a = (pos0 - 2.0 * pos1 + pos2).magnitude();
        let b = (pos1 - 2.0 * pos2 + pos3).magnitude();
        6.0 * a.max(b)
    };
    let tolerance = 0.25 * scale;
    let segment_count = {
        let adjusted = tightness / (8.0 * tolerance);
        let true_count = adjusted.sqrt().ceil() as u32;
        true_count.clamp(1, 64)
    };

    // Draw line segments between each point and its previous point
    let mut last_point = pos0;
    let mut min_dist = (pos - pos0).magnitude(); // minimum distance to the curve
    let mut corrected = pos0;
    let mut path_t = 0.5;
    let segment_length = 1.0 / segment_count as f32;
    for i in 1..=segment_count {
        let t = i as f32 * segment_length;
        let point = studio_math::bezier::bezier_point_at(pos0, pos1, pos2, pos3, t);

        let (distance, corrected_to_line, corrected_t) = correct_to_line_segment(pos, last_point, point);
        if distance < min_dist {
            min_dist = distance;
            corrected = corrected_to_line;
            path_t = t + corrected_t * segment_length;
        }

        last_point = point;
    }

    (min_dist, corrected, path_t)
}

/// Defines an item in an exterior path that we will need to create an interior node for.
struct ExteriorPathSegment {
    /// The ID of the point that starts the curve.
    pub start_id: u16,

    /// The handle we are using to start the curve segment. right = true
    pub start_handle: bool,

    /// The ID of the point that ends the curve.
    pub end_id: u16,

    /// The handle we are using to end the curve segment. right = true
    pub end_handle: bool,
}

impl ExteriorPathSegment {
    pub fn new(start_id: u16, start_handle: bool, end_id: u16, end_handle: bool) -> Self {
        Self {
            start_id,
            start_handle,
            end_id,
            end_handle,
        }
    }

    pub fn flip(&mut self) {
        std::mem::swap(&mut self.start_id, &mut self.end_id);
        std::mem::swap(&mut self.start_handle, &mut self.end_handle);
    }
}