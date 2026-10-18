use studio_math::Vec2;

use super::{Bounds, ObjectBuffers};

impl ObjectBuffers {
    /// Find the curve between two groups of siblings in every zone they are next to each other in.
    /// Returns the point that starts the curve in each zone, which may run either way. Shared edges between zones give two or more.
    pub fn shared_segment_starts(&self, group_a: &[u16], group_b: &[u16]) -> anyhow::Result<Vec<u16>> {
        let mut starts = vec![];
        for &point_id in group_a {
            let (prev_id, next_id) = self.get_neighbors(point_id)?;
            let start = if group_b.contains(&next_id) {
                Some(point_id)
            } else if group_b.contains(&prev_id) {
                Some(prev_id)
            } else {
                None
            };
            if let Some(start) = start && !starts.contains(&start) {
                starts.push(start);
            }
        }
        Ok(starts)
    }

    /// Set how far the curves starting at some points are feathered, in pixels
    pub fn set_feather(&mut self, start_ids: &[u16], width: u8) -> anyhow::Result<()> {
        let points = &mut self.points.borrow_mut().items;
        for &point_id in start_ids {
            points.get_mut(point_id as usize)
                .ok_or_else(|| anyhow::anyhow!("Point {} does not exist!", point_id))?
                .set_feather(width);
        }
        Ok(())
    }

    /// Give each point the feather of the point before it in its zone, for points inserted partway along a curve
    pub fn inherit_feather(&mut self, point_ids: &[u16]) -> anyhow::Result<()> {
        for &point_id in point_ids {
            let (prev_id, _) = self.get_neighbors(point_id)?;
            let width = self.get_point_info(prev_id)?.feather();
            self.set_feather(&[point_id], width)?;
        }
        Ok(())
    }

    /// Stop feathering curves that are no longer an edge two zones share, after separating or deleting points.
    /// Feathering blends with whatever zone is across a curve, so it should only stay on curves zones still share.
    pub fn clear_unshared_feathers(&mut self) -> anyhow::Result<()> {
        let feathered: Vec<u16> = self.points.borrow().items.iter().enumerate()
            .filter(|(_, point)| point.feather() > 0)
            .map(|(point_id, _)| point_id as u16)
            .collect();
        let group = |point_id: u16| { let mut group = self.get_siblings(point_id); group.push(point_id); group };

        let mut unshared = vec![];
        for start_id in feathered {
            let (_, next_id) = self.get_neighbors(start_id)?;
            if self.shared_segment_starts(&group(start_id), &group(next_id))?.len() < 2 {
                unshared.push(start_id);
            }
        }
        self.set_feather(&unshared, 0)
    }

    /// The widest feather of any curve, which is how far a change to a curve can reach past it
    pub fn max_feather(&self) -> u8 {
        self.points.borrow().items.iter().map(|point| point.feather()).max().unwrap_or(0)
    }

    /// The bounds of the zones with the curves starting at some points, and their siblings' zones, grown by a feather width on every side
    pub fn feathered_bounds(&self, start_ids: &[u16], width: u8) -> anyhow::Result<Option<Bounds>> {
        let mut ids = start_ids.to_vec();
        for &start_id in start_ids {
            ids.push(self.get_neighbors(start_id)?.1);
        }
        let grow = Vec2::new(width as f32, width as f32);
        Ok(self.bounds_around_points(&ids)?.map(|(min, max)| (min - grow, max + grow)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::add_square;

    /// A square with a triangle branching off its left edge, which shares the square's points 0 and 3 as the triangle's 4 and 6
    fn branched() -> ObjectBuffers {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (zone_id, _) = objects.create_branching_zone(0).unwrap();
        objects.create_point(zone_id, Vec2::new(-10.0, 5.0)).unwrap();
        objects.complete_branching_zone(0, zone_id, 3).unwrap();
        objects
    }

    /// A shared edge is found in both zones, and an edge only one zone has is found once
    #[test]
    fn shared_starts() {
        let objects = branched();
        let groups = |id: u16| { let mut group = objects.get_siblings(id); group.push(id); group };
        let mut shared = objects.shared_segment_starts(&groups(0), &groups(3)).unwrap();
        shared.sort();
        assert_eq!(shared.len(), 2);
        assert!(shared.contains(&3));
        assert_eq!(objects.shared_segment_starts(&groups(1), &groups(2)).unwrap(), vec![1]);
    }

    /// Feather widths are set, inherited by inserted points, and the widest is found
    #[test]
    fn set_and_inherit() {
        let mut objects = branched();
        assert_eq!(objects.max_feather(), 0);
        objects.set_feather(&[1], 12).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().feather(), 12);
        assert_eq!(objects.max_feather(), 12);

        let inserted = objects.insert_point(0, 1, 0.5).unwrap();
        objects.inherit_feather(&inserted).unwrap();
        assert_eq!(objects.get_point_info(inserted[0]).unwrap().feather(), 12);
        assert!(objects.set_feather(&[999], 3).is_err());
    }

    /// Bounds of a curve's zones grow by the feather width
    #[test]
    fn bounds_grow() {
        let objects = branched();
        assert_eq!(objects.feathered_bounds(&[1], 5).unwrap(), Some((Vec2::new(-5.0, -5.0), Vec2::new(15.0, 15.0))));
    }

    /// Feathering a shared edge
    fn feathered() -> ObjectBuffers {
        let mut objects = branched();
        let starts = objects.shared_segment_starts(&[0, 4], &[3, 6]).unwrap();
        assert_eq!(starts.len(), 2);
        objects.set_feather(&starts, 8).unwrap();
        objects
    }

    /// Separating the points of a feathered edge stops feathering it in both zones
    #[test]
    fn separate_clears() {
        let mut objects = feathered();
        objects.separate_points(&[0]).unwrap();
        assert_eq!(objects.max_feather(), 0);
    }

    /// Deleting the other zone stops feathering the edge it shared
    #[test]
    fn delete_zone_clears() {
        let mut objects = feathered();
        objects.delete_zone(1).unwrap();
        assert_eq!(objects.max_feather(), 0);
    }

    /// Edits that keep the edge shared keep the feather
    #[test]
    fn shared_edges_keep_feather() {
        let mut objects = feathered();
        objects.set_feather(&[1], 5).unwrap();
        objects.separate_points(&[1]).unwrap();
        objects.delete_point(2).unwrap();
        assert_eq!(objects.max_feather(), 8);
    }
}
