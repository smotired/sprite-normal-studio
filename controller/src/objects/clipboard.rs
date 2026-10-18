use super::{ObjectsHandle, ControlPoint, SavedObjects, MAX_OBJECT_ID};
use super::save::saved_zones;

impl ObjectsHandle {
    /// Get some zones in the form they are saved to a file, to be pasted later.
    /// Their points stay siblings with each other, but forget their siblings in zones that aren't copied.
    /// Handles that synced to those siblings keep their current values.
    pub fn copy_zones(&self, zone_ids: &[u16]) -> anyhow::Result<SavedObjects> {
        let mut zone_ids = zone_ids.to_vec();
        zone_ids.sort();
        zone_ids.dedup();

        let mut point_ids = vec![];
        for &zone_id in &zone_ids {
            let (start, count) = self.get_zone_info(zone_id)?.range();
            point_ids.extend(start..start + count);
        }

        // Separate a copy of the lists, so the copied points only know about each other
        let mut points = self.points.borrow().items.clone();
        let mut siblings = self.point_siblings.borrow().items.clone();
        ControlPoint::separate_siblings(&point_ids, &mut points, &mut siblings)?;
        Ok(saved_zones(&zone_ids, &self.zones.borrow().items, &points, &siblings, &self.shapes.borrow().items, &self.shape_vertices.borrow().items))
    }

    /// Add copied zones and their shapes after every existing zone, in the same place they were copied from.
    /// Returns the IDs of the new zones.
    pub fn paste_zones(&mut self, saved: &SavedObjects) -> anyhow::Result<Vec<u16>> {
        let loaded = saved.validate()?;
        let (zone_offset, point_offset) = (self.zone_count(), self.point_count());
        let (shape_offset, vertex_offset) = (self.shape_count(), self.vertex_count());
        let (zone_count, point_count) = (loaded.zones.len(), loaded.points.len());
        let (shape_count, vertex_count) = (loaded.shapes.len(), loaded.shape_vertices.len());
        if zone_offset as usize + zone_count > MAX_OBJECT_ID as usize { anyhow::bail!("No room to paste {} zones!", zone_count); }
        if point_offset as usize + point_count > MAX_OBJECT_ID as usize { anyhow::bail!("No room to paste {} control points!", point_count); }
        if shape_offset as usize + shape_count > MAX_OBJECT_ID as usize { anyhow::bail!("No room to paste {} shapes!", shape_count); }
        if vertex_offset as usize + vertex_count > MAX_OBJECT_ID as usize { anyhow::bail!("No room to paste {} shape vertices!", vertex_count); }

        // Shapes refer to vertices by their index in the zone, so only their zones need shifting
        self.shapes.borrow_mut().items.extend(loaded.shapes.into_iter().map(|mut shape| {
            shape.offset_zone(zone_offset);
            shape
        }));
        self.shape_vertices.borrow_mut().items.extend(loaded.shape_vertices);

        // The loaded objects are numbered from 0, so shift every ID past the existing objects
        let siblings = &mut self.point_siblings.borrow_mut().items;
        for (i, &sibling_id) in loaded.point_siblings[..point_count].iter().enumerate() {
            siblings[point_offset as usize + i] = sibling_id + point_offset;
        }
        self.points.borrow_mut().items.extend(loaded.points.into_iter().map(|mut point| {
            point.offset_ids(point_offset, zone_offset);
            point
        }));
        self.zones.borrow_mut().items.extend(loaded.zones.into_iter().map(|mut zone| {
            zone.offset_ranges(point_offset, shape_offset, vertex_offset);
            zone
        }));

        Ok((zone_offset..zone_offset + zone_count as u16).collect())
    }
}

#[cfg(test)]
mod tests {
    use studio_math::{Vec2, Vec3};

    use super::*;
    use super::super::test_utils::add_square;

    /// A square with a triangle branching off its left edge, which shares the square's points 0 and 3
    fn branched() -> ObjectsHandle {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let (zone_id, _) = objects.create_branching_zone(0).unwrap();
        objects.create_point(zone_id, Vec2::new(-10.0, 5.0)).unwrap();
        objects.complete_branching_zone(0, zone_id, 3).unwrap();
        objects
    }

    /// Get the IDs of the siblings of a point in other zones
    fn sibling_zones(objects: &ObjectsHandle, point_id: u16) -> Vec<u16> {
        objects.get_siblings(point_id).into_iter().map(|id| objects.get_point_info(id).unwrap().zone_id()).collect()
    }

    /// Pasting connected zones keeps them connected to each other in the same way, but not to the originals
    #[test]
    fn paste_connected() {
        let mut objects = branched();
        assert_eq!(objects.object_counts(), (2, 7));
        let copied = objects.copy_zones(&[0, 1]).unwrap();

        assert_eq!(objects.paste_zones(&copied).unwrap(), vec![2, 3]);
        assert_eq!(objects.object_counts(), (4, 14));
        assert_eq!(objects.get_zone_info(2).unwrap().range(), (7, 4));
        assert_eq!(objects.get_zone_info(3).unwrap().range(), (11, 3));
        assert_eq!(objects.get_point_info(9).unwrap().position(), Vec2::new(10.0, 10.0));
        assert_eq!(sibling_zones(&objects, 7), vec![3]);
        assert_eq!(sibling_zones(&objects, 10), vec![3]);
        assert_eq!(sibling_zones(&objects, 0), vec![1]);

        // Copying the pasted zones gives exactly what was copied, so the siblings and syncs point at the new points
        assert_eq!(objects.copy_zones(&[2, 3]).unwrap(), copied);
    }

    /// Copying a zone without the zones it is connected to forgets them, keeping its handles as they are
    #[test]
    fn copy_forgets_unselected() {
        let mut objects = branched();
        objects.update_point(4, None, None, Some(Vec2::new(-1.0, 2.0)), None).unwrap();
        let full = objects.save();
        let copied = objects.copy_zones(&[1]).unwrap();

        assert!(copied.sibling_groups.is_empty());
        for (point, original) in copied.zones[0].points.iter().zip(&full.zones[1].points) {
            assert_eq!(point.left_sync, None);
            assert_eq!(point.right_sync, None);
            assert_eq!((point.position, point.left_handle, point.right_handle), (original.position, original.left_handle, original.right_handle));
        }

        // The originals are untouched, and the pasted zone is on its own
        assert_eq!(objects.save(), full);
        let zones = objects.paste_zones(&copied).unwrap();
        assert_eq!(zones, vec![2]);
        for point_id in 7..10 {
            assert!(objects.get_siblings(point_id).is_empty());
        }
    }

    /// Pasting zones with no points, e.g. from clipboard text edited by hand, is an error and changes nothing
    #[test]
    fn paste_empty_zone_errors() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let saved: SavedObjects = serde_json::from_str(r#"{ "zones": [{ "normal": { "x": 0.0, "y": 0.0, "z": 1.0 }, "points": [] }] }"#).unwrap();
        assert!(objects.paste_zones(&saved).is_err());
        assert_eq!(objects.object_counts(), (1, 4));
        assert!(objects.get_closest_path_point(Vec2::new(5.0, 0.0), 1.0).unwrap().is_some());
    }

    /// Pasted zones keep their shape and normal
    #[test]
    fn paste_keeps_normal() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        objects.set_zone_normals(&[0], Vec3::new(0.5, 0.0, 0.5)).unwrap();
        let copied = objects.copy_zones(&[0]).unwrap();
        objects.paste_zones(&copied).unwrap();

        assert_eq!(objects.get_zone_info(1).unwrap().normal(), Vec3::new(0.5, 0.0, 0.5));
        for i in 0..4 {
            assert_eq!(objects.get_point_info(4 + i).unwrap().position(), objects.get_point_info(i).unwrap().position());
            assert_eq!(objects.get_point_info(4 + i).unwrap().zone_id(), 1);
        }
    }

    /// Pasted zones bring their shapes, which follow the new zones and keep using their own vertices
    #[test]
    fn paste_keeps_shapes() {
        let mut objects = branched();
        let vertex = objects.add_shape_vertex(1, Vec2::new(-5.0, 5.0)).unwrap();
        objects.create_shape(1, &[vertex], crate::objects::ShapeParams::default(), 0).unwrap();
        let vertices: Vec<u16> = [(2.0, 2.0), (8.0, 8.0)].into_iter().map(|pos| objects.add_shape_vertex(0, Vec2::from(pos)).unwrap()).collect();
        objects.create_shape(0, &vertices, crate::objects::ShapeParams::default(), 0).unwrap();
        let copied = objects.copy_zones(&[0, 1]).unwrap();

        objects.paste_zones(&copied).unwrap();
        assert_eq!((objects.shape_count(), objects.vertex_count()), (4, 6));
        assert_eq!(objects.get_zone_info(2).unwrap().shapes(), (2, 1));
        assert_eq!(objects.get_zone_info(3).unwrap().vertices(), (5, 1));
        assert_eq!(objects.get_shape_info(3).unwrap().zone_id(), 3);
        assert_eq!(objects.shape_positions(2).unwrap(), vec![Vec2::new(2.0, 2.0), Vec2::new(8.0, 8.0)]);
        assert_eq!(objects.shape_positions(3).unwrap(), vec![Vec2::new(-5.0, 5.0)]);
        assert_eq!(objects.copy_zones(&[2, 3]).unwrap(), copied);
    }

    /// Copying zones that don't exist is an error
    #[test]
    fn copy_missing_zone() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        assert!(objects.copy_zones(&[1]).is_err());
    }

    /// Pasting more points than there is room for is an error, and changes nothing
    #[test]
    fn paste_too_many() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let mut copied = objects.copy_zones(&[0]).unwrap();
        let point = copied.zones[0].points[0].clone();
        copied.zones[0].points = vec![point; MAX_OBJECT_ID as usize - 3];

        assert!(objects.paste_zones(&copied).is_err());
        assert_eq!(objects.object_counts(), (1, 4));
    }

    /// Pasted zones go in front of every zone, keeping the order they were in, whatever order they were picked in
    #[test]
    fn paste_keeps_order() {
        let mut objects = ObjectsHandle::headless();
        for x in [0.0, 20.0, 40.0] {
            super::super::test_utils::add_zone(&mut objects, &[(x, 0.0), (x + 10.0, 0.0), (x + 5.0, 10.0)]);
        }
        let copied = objects.copy_zones(&[2, 0]).unwrap();
        assert_eq!(objects.paste_zones(&copied).unwrap(), vec![3, 4]);
        let first = |zone_id: u16| objects.get_point_info(objects.get_zone_info(zone_id).unwrap().range().0).unwrap().position();
        assert_eq!(first(3), Vec2::new(0.0, 0.0));
        assert_eq!(first(4), Vec2::new(40.0, 0.0));
    }

    /// Feathered curves keep their width when copied and pasted
    #[test]
    fn paste_keeps_feather() {
        let mut objects = branched();
        objects.set_feather(&[3], 9).unwrap();
        let copied = objects.copy_zones(&[0, 1]).unwrap();
        let pasted = objects.paste_zones(&copied).unwrap();
        let start = objects.get_zone_info(pasted[0]).unwrap().range().0;
        assert_eq!(objects.get_point_info(start + 3).unwrap().feather(), 9);
    }
}
