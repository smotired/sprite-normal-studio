use studio_math::Vec2;
use studio_math::bounds::bounds_of;

use super::{ObjectBuffers, Bounds, Shape, ShapeVertex, ShapeParams, ShapeParamChange, MAX_OBJECT_ID, hull_distance, merge_bounds};

impl ObjectBuffers {
    /// Get a clone of a shape by its ID. Returns an error if the shape does not exist.
    pub fn get_shape_info(&self, shape_id: u16) -> anyhow::Result<Shape> {
        if shape_id >= self.shape_count() { anyhow::bail!("Shape {} does not exist!", shape_id); }
        Ok(self.shapes.borrow().items[shape_id as usize])
    }

    /// Get a clone of a shape vertex by its ID. Returns an error if the vertex does not exist.
    pub fn get_vertex_info(&self, vertex_id: u16) -> anyhow::Result<ShapeVertex> {
        if vertex_id >= self.vertex_count() { anyhow::bail!("Shape vertex {} does not exist!", vertex_id); }
        Ok(self.shape_vertices.borrow().items[vertex_id as usize])
    }

    /// Get the ID of the zone a shape vertex is in
    pub fn zone_of_vertex(&self, vertex_id: u16) -> anyhow::Result<u16> {
        self.get_vertex_info(vertex_id)?;
        let zones = &self.zones.borrow().items;
        zones.iter()
            .position(|zone| {
                let (start, count) = zone.vertices();
                vertex_id >= start && vertex_id < start + count
            })
            .map(|zone_id| zone_id as u16)
            .ok_or_else(|| anyhow::anyhow!("Shape vertex {} is not in any zone!", vertex_id))
    }

    /// Get the IDs of every shape in a zone
    pub fn zone_shape_ids(&self, zone_id: u16) -> anyhow::Result<Vec<u16>> {
        let (start, count) = self.get_zone_info(zone_id)?.shapes();
        Ok((start..start + count).collect())
    }

    /// Get the IDs of every shape vertex in a zone
    pub fn zone_vertex_ids(&self, zone_id: u16) -> anyhow::Result<Vec<u16>> {
        let (start, count) = self.get_zone_info(zone_id)?.vertices();
        Ok((start..start + count).collect())
    }

    /// Get the IDs of the vertices a shape is made from
    pub fn shape_vertex_ids(&self, shape_id: u16) -> anyhow::Result<Vec<u16>> {
        let shape = self.get_shape_info(shape_id)?;
        let (start, _) = self.get_zone_info(shape.zone_id())?.vertices();
        Ok(shape.local_vertices().into_iter().map(|local| start + local).collect())
    }

    /// Get the positions of the vertices a shape is made from
    pub fn shape_positions(&self, shape_id: u16) -> anyhow::Result<Vec<Vec2>> {
        self.shape_vertex_ids(shape_id)?.into_iter()
            .map(|vertex_id| Ok(self.get_vertex_info(vertex_id)?.position()))
            .collect()
    }

    /// Get the IDs of every shape that uses any of the vertices, sorted
    pub fn shapes_using_vertices(&self, vertex_ids: &[u16]) -> anyhow::Result<Vec<u16>> {
        let mut shape_ids = vec![];
        for &vertex_id in vertex_ids {
            for shape_id in self.zone_shape_ids(self.zone_of_vertex(vertex_id)?)? {
                if self.shape_vertex_ids(shape_id)?.contains(&vertex_id) { shape_ids.push(shape_id); }
            }
        }
        shape_ids.sort();
        shape_ids.dedup();
        Ok(shape_ids)
    }

    /// Get the bounding box of everything a shape affects: its vertices, out to its reach
    pub fn shape_reach_bounds(&self, shape_id: u16) -> anyhow::Result<Option<Bounds>> {
        let reach = Vec2::square(self.get_shape_info(shape_id)?.params().reach());
        Ok(bounds_of(self.shape_positions(shape_id)?).map(|(min, max)| (min - reach, max + reach)))
    }

    /// Get the bounding box of everything several shapes affect. Returns None if there are no shapes.
    pub fn shapes_reach_bounds(&self, shape_ids: &[u16]) -> anyhow::Result<Option<Bounds>> {
        let mut bounds = None;
        for &shape_id in shape_ids {
            bounds = merge_bounds(bounds, self.shape_reach_bounds(shape_id)?);
        }
        Ok(bounds)
    }

    /// Get the closest shape vertex to a position, in one zone or all of them. Returns the zone ID and vertex ID.
    pub fn get_closest_vertex(&self, zone_id: Option<u16>, position: Vec2) -> anyhow::Result<Option<(u16, u16)>> {
        let zone_ids: Vec<u16> = match zone_id {
            Some(zone_id) => vec![zone_id],
            None => (0..self.zone_count()).collect(),
        };

        let mut closest: Option<(f32, u16, u16)> = None;
        for zone_id in zone_ids {
            for vertex_id in self.zone_vertex_ids(zone_id)? {
                let distance = self.get_vertex_info(vertex_id)?.position().distance(position);
                if closest.is_none_or(|(best, _, _)| distance < best) {
                    closest = Some((distance, zone_id, vertex_id));
                }
            }
        }
        Ok(closest.map(|(_, zone_id, vertex_id)| (zone_id, vertex_id)))
    }

    /// Get the shape whose core is closest to a position, if it is within a tolerance, in one zone or all of them.
    /// Positions inside several cores find the last of those shapes. Shapes still being drawn can't be found.
    pub fn get_shape_at(&self, zone_id: Option<u16>, position: Vec2, tolerance: f32) -> anyhow::Result<Option<u16>> {
        let zone_ids: Vec<u16> = match zone_id {
            Some(zone_id) => vec![zone_id],
            None => (0..self.zone_count()).collect(),
        };

        let mut closest: Option<(f32, u16)> = None;
        for zone_id in zone_ids {
            for shape_id in self.zone_shape_ids(zone_id)? {
                let shape = self.get_shape_info(shape_id)?;
                if shape.is_pending() { continue; }

                let (hull, _) = hull_distance(&self.shape_positions(shape_id)?, position);
                let distance = (hull - shape.params().inner_radius).max(0.0);
                if distance <= tolerance && closest.is_none_or(|(best, _)| distance <= best) {
                    closest = Some((distance, shape_id));
                }
            }
        }
        Ok(closest.map(|(_, shape_id)| shape_id))
    }

    /// Add a shape vertex to the end of a zone's vertices, and return its ID.
    pub fn add_shape_vertex(&mut self, zone_id: u16, position: Vec2) -> anyhow::Result<u16> {
        if self.vertex_count() == MAX_OBJECT_ID { anyhow::bail!("No room to create another shape vertex!"); }
        self.get_zone_info(zone_id)?;

        let zones = &mut self.zones.borrow_mut().items;
        let vertex_id = zones[zone_id as usize].add_vertex();
        for zone in &mut zones[zone_id as usize + 1..] {
            zone.add_vertex_offset(1);
        }
        self.shape_vertices.borrow_mut().items.insert(vertex_id as usize, ShapeVertex::new(position));
        Ok(vertex_id)
    }

    /// Add a shape in a new group of its own to the end of a zone's shapes, from 1 to 3 different vertices in that zone, and return its ID.
    pub fn create_shape(&mut self, zone_id: u16, vertex_ids: &[u16], params: ShapeParams, flags: u8) -> anyhow::Result<u16> {
        let group = self.unused_group(zone_id)?;
        let (start, count) = self.get_zone_info(zone_id)?.shapes();
        self.insert_shape(zone_id, start + count, vertex_ids, params, group, flags)
    }

    /// Add a shape to an existing group in a zone, right after the group's other shapes, from 1 to 3 different vertices in that zone.
    /// It takes the params of the group, so the group stays one surface. Returns its ID.
    pub fn create_shape_in_group(&mut self, zone_id: u16, vertex_ids: &[u16], group: u16, flags: u8) -> anyhow::Result<u16> {
        let mut last = None;
        for shape_id in self.zone_shape_ids(zone_id)? {
            let shape = self.get_shape_info(shape_id)?;
            if shape.group() == group { last = Some((shape_id, shape.params())); }
        }
        let Some((last_id, params)) = last else { anyhow::bail!("Zone {} has no shape group {}!", zone_id, group); };
        self.insert_shape(zone_id, last_id + 1, vertex_ids, params, group, flags)
    }

    /// Get the IDs of every shape in the same group as any of the listed shapes, sorted
    pub fn shape_groups(&self, shape_ids: &[u16]) -> anyhow::Result<Vec<u16>> {
        let mut grouped = vec![];
        for &shape_id in shape_ids {
            let shape = self.get_shape_info(shape_id)?;
            for other_id in self.zone_shape_ids(shape.zone_id())? {
                if self.get_shape_info(other_id)?.group() == shape.group() { grouped.push(other_id); }
            }
        }
        grouped.sort();
        grouped.dedup();
        Ok(grouped)
    }

    /// Get a group ID that no shape in a zone has
    fn unused_group(&self, zone_id: u16) -> anyhow::Result<u16> {
        let mut groups = vec![];
        for shape_id in self.zone_shape_ids(zone_id)? {
            groups.push(self.get_shape_info(shape_id)?.group());
        }

        // Usually one past the highest, but the lowest unused if that would overflow
        match groups.iter().max() {
            None => Ok(0),
            Some(&max) if max < u16::MAX => Ok(max + 1),
            Some(_) => (0..=u16::MAX).find(|group| !groups.contains(group)).ok_or_else(|| anyhow::anyhow!("No room for another shape group!")),
        }
    }

    /// Insert a shape into the shapes list at an ID within its zone's range, and return the ID
    fn insert_shape(&mut self, zone_id: u16, shape_id: u16, vertex_ids: &[u16], params: ShapeParams, group: u16, flags: u8) -> anyhow::Result<u16> {
        if self.shape_count() == MAX_OBJECT_ID { anyhow::bail!("No room to create another shape!"); }

        // Shapes refer to their vertices by their index in the zone
        let (start, count) = self.get_zone_info(zone_id)?.vertices();
        let mut local_vertices = vec![];
        for &vertex_id in vertex_ids {
            if vertex_id < start || vertex_id >= start + count { anyhow::bail!("Shape vertex {} is not in zone {}!", vertex_id, zone_id); }
            local_vertices.push(vertex_id - start);
        }
        let shape = Shape::new(zone_id, &local_vertices, params, group, flags)?;

        let zones = &mut self.zones.borrow_mut().items;
        zones[zone_id as usize].add_shape();
        for zone in &mut zones[zone_id as usize + 1..] {
            zone.add_shape_offset(1);
        }
        self.shapes.borrow_mut().items.insert(shape_id as usize, shape);
        Ok(shape_id)
    }

    /// Move shape vertices by a delta, which moves every shape made from them. Each vertex is only moved once.
    pub fn move_shape_vertices(&mut self, vertex_ids: &[u16], delta: Vec2) -> anyhow::Result<()> {
        let mut vertex_ids = vertex_ids.to_vec();
        vertex_ids.sort();
        vertex_ids.dedup();
        for &vertex_id in &vertex_ids { self.get_vertex_info(vertex_id)?; }

        let vertices = &mut self.shape_vertices.borrow_mut().items;
        for vertex_id in vertex_ids {
            let vertex = &mut vertices[vertex_id as usize];
            vertex.set_position(vertex.position() + delta);
        }
        Ok(())
    }

    /// Set the position of a shape vertex
    pub fn set_vertex_position(&mut self, vertex_id: u16, position: Vec2) -> anyhow::Result<()> {
        self.get_vertex_info(vertex_id)?;
        self.shape_vertices.borrow_mut().items[vertex_id as usize].set_position(position);
        Ok(())
    }

    /// Change one param of every listed shape, and the rest of their groups so each group stays one surface
    pub fn set_shape_params(&mut self, shape_ids: &[u16], change: ShapeParamChange) -> anyhow::Result<()> {
        let shape_ids = self.shape_groups(shape_ids)?;
        let shapes = &mut self.shapes.borrow_mut().items;
        for shape_id in shape_ids {
            let shape = &mut shapes[shape_id as usize];
            shape.set_params(shape.params().with(change));
        }
        Ok(())
    }

    /// Remove shapes, along with any vertices no other shape uses
    pub fn delete_shapes(&mut self, shape_ids: &[u16]) -> anyhow::Result<()> {
        let mut shape_ids = shape_ids.to_vec();
        shape_ids.sort();
        shape_ids.dedup();

        // Removing a shape doesn't change the zones, so they can be found up front
        let mut zone_ids = vec![];
        for &shape_id in &shape_ids {
            zone_ids.push(self.get_shape_info(shape_id)?.zone_id());
        }
        zone_ids.dedup(); // shapes are in zone order, so sorted shapes have sorted zones

        // Remove backwards to preserve indices
        for &shape_id in shape_ids.iter().rev() {
            self.remove_shape(shape_id)?;
        }
        for zone_id in zone_ids {
            self.remove_unused_vertices(zone_id)?;
        }
        Ok(())
    }

    /// Remove shape vertices, along with every shape made from them
    pub fn delete_shape_vertices(&mut self, vertex_ids: &[u16]) -> anyhow::Result<()> {
        let mut zone_ids = vec![];
        for &vertex_id in vertex_ids {
            zone_ids.push(self.zone_of_vertex(vertex_id)?);
        }
        zone_ids.sort();
        zone_ids.dedup();

        // Every listed vertex is unused once its shapes are gone
        let shape_ids = self.shapes_using_vertices(vertex_ids)?;
        self.delete_shapes(&shape_ids)?;
        for zone_id in zone_ids {
            self.remove_unused_vertices(zone_id)?;
        }
        Ok(())
    }

    /// Merge shape vertices in the same zone into the first of them, at their average position.
    /// Shapes that used several of them use the merged vertex once, so a triangle can become a line or a point.
    /// Returns the ID of the merged vertex.
    pub fn merge_shape_vertices(&mut self, vertex_ids: &[u16]) -> anyhow::Result<u16> {
        let mut vertex_ids = vertex_ids.to_vec();
        vertex_ids.sort();
        vertex_ids.dedup();
        if vertex_ids.len() < 2 { anyhow::bail!("Select at least two shape vertices to merge them!"); }

        let zone_id = self.zone_of_vertex(vertex_ids[0])?;
        let mut sum = Vec2::ZERO;
        for &vertex_id in &vertex_ids {
            if self.zone_of_vertex(vertex_id)? != zone_id { anyhow::bail!("Only shape vertices in the same zone can be merged!"); }
            sum += self.get_vertex_info(vertex_id)?.position();
        }
        let merged_id = vertex_ids[0];
        self.set_vertex_position(merged_id, sum * (1.0 / vertex_ids.len() as f32))?;

        // Point every shape at the merged vertex instead of the others
        let (start, _) = self.get_zone_info(zone_id)?.vertices();
        let others: Vec<u16> = vertex_ids[1..].iter().map(|&vertex_id| vertex_id - start).collect();
        let merged = merged_id - start;
        for shape_id in self.zone_shape_ids(zone_id)? {
            let shape = self.get_shape_info(shape_id)?;
            if !others.iter().any(|&other| shape.uses_local(other)) { continue; }

            let mut locals = vec![];
            for local in shape.local_vertices() {
                let local = if others.contains(&local) { merged } else { local };
                if !locals.contains(&local) { locals.push(local); }
            }
            self.shapes.borrow_mut().items[shape_id as usize].set_local_vertices(&locals)?;
        }

        // The others are unused now, and come after the merged vertex so removing them doesn't change its ID
        for &vertex_id in vertex_ids[1..].iter().rev() {
            self.remove_vertex(vertex_id)?;
        }
        Ok(merged_id)
    }

    /// Remove every shape and shape vertex from the listed zones
    pub fn clear_zone_shapes(&mut self, zone_ids: &[u16]) -> anyhow::Result<()> {
        let mut zone_ids = zone_ids.to_vec();
        zone_ids.sort();
        zone_ids.dedup();
        for &zone_id in &zone_ids { self.get_zone_info(zone_id)?; }

        for zone_id in zone_ids {
            let zone = self.get_zone_info(zone_id)?;
            let ((shape_start, shape_count), (vertex_start, vertex_count)) = (zone.shapes(), zone.vertices());
            if shape_count == 0 && vertex_count == 0 { continue; }

            self.shapes.borrow_mut().items.drain(shape_start as usize..(shape_start + shape_count) as usize);
            self.shape_vertices.borrow_mut().items.drain(vertex_start as usize..(vertex_start + vertex_count) as usize);
            let zones = &mut self.zones.borrow_mut().items;
            zones[zone_id as usize].clear_shapes();
            for zone in &mut zones[zone_id as usize + 1..] {
                zone.add_shape_offset(-(shape_count as i32));
                zone.add_vertex_offset(-(vertex_count as i32));
            }
        }
        Ok(())
    }

    /// Remove a shape from the shapes list, leaving its vertices
    fn remove_shape(&mut self, shape_id: u16) -> anyhow::Result<()> {
        let zone_id = self.get_shape_info(shape_id)?.zone_id() as usize;
        let zones = &mut self.zones.borrow_mut().items;
        zones[zone_id].dec_shapes()?;
        for zone in &mut zones[zone_id + 1..] {
            zone.add_shape_offset(-1);
        }
        self.shapes.borrow_mut().items.remove(shape_id as usize);
        Ok(())
    }

    /// Remove a shape vertex that no shape uses, and move the zone's later vertices back in its shapes
    fn remove_vertex(&mut self, vertex_id: u16) -> anyhow::Result<()> {
        let zone_id = self.zone_of_vertex(vertex_id)?;
        let removed = vertex_id - self.get_zone_info(zone_id)?.vertices().0;
        let shape_ids = self.zone_shape_ids(zone_id)?;

        let shapes = &mut self.shapes.borrow_mut().items;
        for &shape_id in &shape_ids {
            if shapes[shape_id as usize].uses_local(removed) { anyhow::bail!("Shape vertex {} is still used by shape {}!", vertex_id, shape_id); }
        }
        for &shape_id in &shape_ids {
            let shape = &mut shapes[shape_id as usize];
            let locals: Vec<u16> = shape.local_vertices().into_iter().map(|local| if local > removed { local - 1 } else { local }).collect();
            shape.set_local_vertices(&locals)?;
        }

        let zones = &mut self.zones.borrow_mut().items;
        zones[zone_id as usize].dec_vertices()?;
        for zone in &mut zones[zone_id as usize + 1..] {
            zone.add_vertex_offset(-1);
        }
        self.shape_vertices.borrow_mut().items.remove(vertex_id as usize);
        Ok(())
    }

    /// Remove every shape vertex in a zone that no shape uses
    fn remove_unused_vertices(&mut self, zone_id: u16) -> anyhow::Result<()> {
        let mut used = vec![];
        for shape_id in self.zone_shape_ids(zone_id)? {
            used.extend(self.shape_vertex_ids(shape_id)?);
        }

        // Remove backwards to preserve indices
        for vertex_id in self.zone_vertex_ids(zone_id)?.into_iter().rev() {
            if !used.contains(&vertex_id) { self.remove_vertex(vertex_id)?; }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::{add_square, add_zone};
    use super::super::ShapeProfile;

    /// Add a vertex at each position to a zone, and a shape made from all of them, returning the shape ID
    fn add_shape(objects: &mut ObjectBuffers, zone_id: u16, positions: &[(f32, f32)]) -> u16 {
        let vertex_ids: Vec<u16> = positions.iter()
            .map(|&position| objects.add_shape_vertex(zone_id, Vec2::from(position)).unwrap())
            .collect();
        objects.create_shape(zone_id, &vertex_ids, ShapeParams::default(), 0).unwrap()
    }

    /// Two squares, with a point shape and a line shape in the first and a point shape in the second
    fn sample() -> ObjectBuffers {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);
        add_shape(&mut objects, 1, &[(25.0, 2.0)]);
        add_shape(&mut objects, 0, &[(5.0, 5.0)]);
        add_shape(&mut objects, 0, &[(2.0, 2.0), (8.0, 2.0)]);
        objects
    }

    /// Shapes and vertices are kept in zone order, so adding to an earlier zone moves the later zone's ranges along
    #[test]
    fn create_keeps_zone_order() {
        let objects = sample();
        assert_eq!((objects.shape_count(), objects.vertex_count()), (3, 4));
        assert_eq!(objects.get_zone_info(0).unwrap().shapes(), (0, 2));
        assert_eq!(objects.get_zone_info(0).unwrap().vertices(), (0, 3));
        assert_eq!(objects.get_zone_info(1).unwrap().shapes(), (2, 1));
        assert_eq!(objects.get_zone_info(1).unwrap().vertices(), (3, 1));

        // The later zone's shape still has its own vertex
        assert_eq!(objects.shape_positions(2).unwrap(), vec![Vec2::new(25.0, 2.0)]);
        assert_eq!(objects.get_shape_info(2).unwrap().zone_id(), 1);
        assert_eq!(objects.shape_positions(1).unwrap(), vec![Vec2::new(2.0, 2.0), Vec2::new(8.0, 2.0)]);
        assert_eq!(objects.zone_of_vertex(3).unwrap(), 1);
    }

    /// Shapes can only be made from vertices in their own zone
    #[test]
    fn create_from_other_zone() {
        let mut objects = sample();
        assert!(objects.create_shape(0, &[3], ShapeParams::default(), 0).is_err());
        assert!(objects.create_shape(2, &[0], ShapeParams::default(), 0).is_err());
        assert!(objects.add_shape_vertex(2, Vec2::ZERO).is_err());
        assert_eq!(objects.shape_count(), 3);
    }

    /// Shapes can share vertices, and moving a vertex moves every shape made from it
    #[test]
    fn shared_vertices() {
        let mut objects = sample();
        objects.create_shape(0, &[0, 2], ShapeParams::default(), 0).unwrap();
        assert_eq!(objects.shapes_using_vertices(&[0]).unwrap(), vec![0, 2]);

        objects.move_shape_vertices(&[0, 0], Vec2::new(1.0, 1.0)).unwrap();
        assert_eq!(objects.shape_positions(0).unwrap(), vec![Vec2::new(6.0, 6.0)]);
        assert_eq!(objects.shape_positions(2).unwrap()[0], Vec2::new(6.0, 6.0));
    }

    /// Deleting shapes removes vertices nothing else uses, and moves later shapes' ranges back
    #[test]
    fn delete_shapes() {
        let mut objects = sample();
        objects.create_shape(0, &[0, 1], ShapeParams::default(), 0).unwrap(); // shares the point's vertex
        objects.delete_shapes(&[0, 1]).unwrap();

        assert_eq!((objects.shape_count(), objects.vertex_count()), (2, 3));
        assert_eq!(objects.get_zone_info(0).unwrap().vertices(), (0, 2));
        assert_eq!(objects.shape_positions(0).unwrap(), vec![Vec2::new(5.0, 5.0), Vec2::new(2.0, 2.0)]);
        assert_eq!(objects.get_zone_info(1).unwrap().shapes(), (1, 1));
        assert_eq!(objects.shape_positions(1).unwrap(), vec![Vec2::new(25.0, 2.0)]);
    }

    /// Deleting vertices deletes every shape made from them
    #[test]
    fn delete_vertices() {
        let mut objects = sample();
        objects.delete_shape_vertices(&[2]).unwrap();
        assert_eq!((objects.shape_count(), objects.vertex_count()), (2, 2));
        assert_eq!(objects.shape_positions(0).unwrap(), vec![Vec2::new(5.0, 5.0)]);
        assert_eq!(objects.shape_positions(1).unwrap(), vec![Vec2::new(25.0, 2.0)]);
    }

    /// Merged vertices meet in the middle, and shapes that used several of them use it once
    #[test]
    fn merge_vertices() {
        let mut objects = sample();
        let triangle = add_shape(&mut objects, 0, &[(0.0, 0.0), (4.0, 0.0), (0.0, 4.0)]);
        assert_eq!(triangle, 2);

        // Merge the line's start with two of the triangle's vertices
        let merged = objects.merge_shape_vertices(&[4, 1, 3]).unwrap();
        assert_eq!(merged, 1);
        assert_eq!(objects.get_vertex_info(1).unwrap().position(), Vec2::new(2.0, 2.0 / 3.0));
        assert_eq!(objects.shape_vertex_ids(1).unwrap(), vec![1, 2]);
        assert_eq!(objects.shape_vertex_ids(2).unwrap(), vec![1, 3]);
        assert_eq!(objects.get_zone_info(0).unwrap().vertices(), (0, 4));
        assert_eq!(objects.shape_positions(3).unwrap(), vec![Vec2::new(25.0, 2.0)]);

        assert!(objects.merge_shape_vertices(&[0]).is_err());
        assert!(objects.merge_shape_vertices(&[0, 4]).is_err()); // different zones
    }

    /// Clearing a zone removes its shapes and vertices, but not other zones'
    #[test]
    fn clear_zone_shapes() {
        let mut objects = sample();
        objects.clear_zone_shapes(&[0]).unwrap();
        assert_eq!((objects.shape_count(), objects.vertex_count()), (1, 1));
        assert_eq!(objects.get_zone_info(0).unwrap().shapes(), (0, 0));
        assert_eq!(objects.get_zone_info(1).unwrap().shapes(), (0, 1));
        assert_eq!(objects.shape_positions(0).unwrap(), vec![Vec2::new(25.0, 2.0)]);
    }

    /// Deleting a zone removes its shapes, and later shapes follow their zones to their new IDs
    #[test]
    fn delete_zone_removes_shapes() {
        let mut objects = sample();
        objects.delete_zone(0).unwrap();
        assert_eq!((objects.shape_count(), objects.vertex_count()), (1, 1));
        assert_eq!(objects.get_shape_info(0).unwrap().zone_id(), 0);
        assert_eq!(objects.get_zone_info(0).unwrap().shapes(), (0, 1));
        assert_eq!(objects.shape_positions(0).unwrap(), vec![Vec2::new(25.0, 2.0)]);
    }

    /// Params change on every listed shape, clamped into range
    #[test]
    fn set_params() {
        let mut objects = sample();
        objects.set_shape_params(&[0, 2], ShapeParamChange::Profile(ShapeProfile::Dome)).unwrap();
        objects.set_shape_params(&[0], ShapeParamChange::OuterRadius(-1.0)).unwrap();
        assert_eq!(objects.get_shape_info(0).unwrap().params().profile, ShapeProfile::Dome);
        assert_eq!(objects.get_shape_info(1).unwrap().params().profile, ShapeProfile::Cone);
        assert_eq!(objects.get_shape_info(0).unwrap().params().outer_radius, super::super::shape::MIN_OUTER_RADIUS);
        assert!(objects.set_shape_params(&[9], ShapeParamChange::Blend(1.0)).is_err());
    }

    /// New shapes get a group of their own, and shapes added to a group go right after it with its params
    #[test]
    fn groups() {
        let mut objects = sample();
        assert_eq!((objects.get_shape_info(0).unwrap().group(), objects.get_shape_info(1).unwrap().group()), (0, 1));
        assert_eq!(objects.get_shape_info(2).unwrap().group(), 0); // groups are per zone

        // Joining the point's group puts the new shape between the point and the line
        let vertex = objects.add_shape_vertex(0, Vec2::new(0.0, 9.0)).unwrap();
        let joined = objects.create_shape_in_group(0, &[0, vertex], 0, 0).unwrap();
        assert_eq!(joined, 1);
        assert_eq!(objects.get_shape_info(1).unwrap().group(), 0);
        assert_eq!(objects.get_shape_info(1).unwrap().params(), objects.get_shape_info(0).unwrap().params());
        assert_eq!(objects.shape_positions(2).unwrap(), vec![Vec2::new(2.0, 2.0), Vec2::new(8.0, 2.0)]);
        assert_eq!(objects.get_zone_info(1).unwrap().shapes(), (3, 1));
        assert_eq!(objects.shape_groups(&[1]).unwrap(), vec![0, 1]);

        // A shape made later still gets a group nobody has
        let created = objects.create_shape(0, &[vertex], ShapeParams::default(), 0).unwrap();
        assert_eq!(objects.get_shape_info(created).unwrap().group(), 2);
        assert!(objects.create_shape_in_group(0, &[vertex], 9, 0).is_err());
    }

    /// Changing a shape's params changes the rest of its group too
    #[test]
    fn params_follow_group() {
        let mut objects = sample();
        let vertex = objects.add_shape_vertex(0, Vec2::new(0.0, 9.0)).unwrap();
        objects.create_shape_in_group(0, &[vertex], 0, 0).unwrap();
        objects.set_shape_params(&[1], ShapeParamChange::Steepness(3.0)).unwrap();
        assert_eq!(objects.get_shape_info(0).unwrap().params().steepness, 3.0);
        assert_eq!(objects.get_shape_info(2).unwrap().params().steepness, 1.0);
    }

    /// Reach bounds cover the vertices out to the shape's reach
    #[test]
    fn reach_bounds() {
        let objects = sample();
        let reach = ShapeParams::default().reach();
        assert_eq!(objects.shape_reach_bounds(1).unwrap(), Some((Vec2::new(2.0 - reach, 2.0 - reach), Vec2::new(8.0 + reach, 2.0 + reach))));
        assert_eq!(objects.shapes_reach_bounds(&[]).unwrap(), None);
    }

    /// The closest vertex can be found in one zone or all of them
    #[test]
    fn closest_vertex() {
        let objects = sample();
        assert_eq!(objects.get_closest_vertex(None, Vec2::new(24.0, 0.0)).unwrap(), Some((1, 3)));
        assert_eq!(objects.get_closest_vertex(Some(0), Vec2::new(24.0, 0.0)).unwrap(), Some((0, 2)));
    }

    /// Shapes are found by their cores, the closest within the tolerance, and later shapes win where cores overlap
    #[test]
    fn shape_at() {
        let mut objects = sample();
        assert_eq!(objects.get_shape_at(None, Vec2::new(5.0, 3.0), 1.5).unwrap(), Some(1));
        assert_eq!(objects.get_shape_at(None, Vec2::new(5.0, 4.0), 1.5).unwrap(), Some(0));
        assert_eq!(objects.get_shape_at(None, Vec2::new(5.0, 3.5), 0.1).unwrap(), None);
        assert_eq!(objects.get_shape_at(Some(0), Vec2::new(25.0, 2.0), 1.0).unwrap(), None);

        // A circle around the point covers it, and comes after it
        objects.create_shape(0, &[0], ShapeParams { inner_radius: 3.0, ..Default::default() }, 0).unwrap();
        assert_eq!(objects.get_shape_at(None, Vec2::new(5.0, 5.0), 0.0).unwrap(), Some(2));
        assert_eq!(objects.get_shape_at(None, Vec2::new(5.0, 7.5), 0.0).unwrap(), Some(2));

        // Pending shapes can't be found
        objects.create_shape(1, &[3], ShapeParams::default(), super::super::shape::SHAPE_PENDING).unwrap();
        assert_eq!(objects.get_shape_at(None, Vec2::new(25.0, 2.0), 0.0).unwrap(), Some(3));
    }

    /// Shape changes are recorded, and can be reverted and applied
    #[test]
    fn undo_shape_changes() {
        let mut objects = sample();
        objects.begin_change();
        objects.delete_shapes(&[1]).unwrap();
        objects.move_shape_vertices(&[0], Vec2::new(1.0, 0.0)).unwrap();
        let diff = objects.finish_change();
        assert!(diff.shapes.is_some() && diff.shape_vertices.is_some() && diff.zones.is_some());

        diff.revert(&objects);
        assert_eq!(objects.shape_count(), 3);
        assert_eq!(objects.shape_positions(1).unwrap(), vec![Vec2::new(2.0, 2.0), Vec2::new(8.0, 2.0)]);
        assert_eq!(objects.shape_positions(0).unwrap(), vec![Vec2::new(5.0, 5.0)]);
        diff.apply(&objects);
        assert_eq!(objects.shape_count(), 2);
        assert_eq!(objects.shape_positions(0).unwrap(), vec![Vec2::new(6.0, 5.0)]);
    }
}
