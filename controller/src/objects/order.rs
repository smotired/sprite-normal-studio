use super::ObjectBuffers;
use super::save::saved_zones;

/// How to change the order of the selected zones. Zones later in the order are in front.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ZoneOrderChange {
    /// Just in front of the next zone in front of them that they overlap
    Forward,

    /// Just behind the next zone behind them that they overlap
    Backward,

    /// In front of every other zone
    ToFront,

    /// Behind every other zone
    ToBack,
}

impl ObjectBuffers {
    /// Put the zones in a new order, listing every zone ID once from back to front.
    /// Every ID changes, so the objects are rebuilt in the new order the same way they are saved and loaded.
    pub fn reorder_zones(&mut self, new_order: &[u16]) -> anyhow::Result<()> {
        let mut sorted = new_order.to_vec();
        sorted.sort();
        if sorted != (0..self.zone_count()).collect::<Vec<_>>() { anyhow::bail!("A new zone order must list every zone once!"); }

        let saved = saved_zones(
            new_order,
            &self.zones.borrow().items,
            &self.points.borrow().items,
            &self.point_siblings.borrow().items,
            &self.shapes.borrow().items,
            &self.shape_vertices.borrow().items,
        );
        self.load(saved.validate()?);
        Ok(())
    }

    /// Returns true if the bounding boxes of two zones overlap. Boxes that only touch at an edge don't.
    pub fn zones_overlap(&self, a: u16, b: u16) -> anyhow::Result<bool> {
        let (Some((a_min, a_max)), Some((b_min, b_max))) = (self.zone_bounds(a)?, self.zone_bounds(b)?) else { return Ok(false); };
        Ok(a_min.x < b_max.x && b_min.x < a_max.x && a_min.y < b_max.y && b_min.y < a_max.y)
    }

    /// Get the new order of every zone after changing the order of some of them, or None if they wouldn't move
    pub fn changed_zone_order(&self, selected: &[u16], change: ZoneOrderChange) -> anyhow::Result<Option<Vec<u16>>> {
        let order: Vec<u16> = (0..self.zone_count()).collect();
        let mut error = None;
        let overlaps = |a: u16, b: u16| self.zones_overlap(a, b).unwrap_or_else(|e| { error = Some(e); false });
        let new_order = match change {
            ZoneOrderChange::Forward => forward_order(&order, selected, overlaps),
            ZoneOrderChange::Backward => backward_order(&order, selected, overlaps),
            ZoneOrderChange::ToFront => Some(to_front(&order, selected)),
            ZoneOrderChange::ToBack => Some(to_back(&order, selected)),
        };
        if let Some(e) = error { return Err(e); }
        Ok(new_order.filter(|new_order| *new_order != order))
    }
}

/// Move the selected zones together, keeping their order, to just in front of the closest unselected zone in front of all of them that overlaps any of them.
/// Returns None if nothing in front of them overlaps them.
pub fn forward_order(order: &[u16], selected: &[u16], mut overlaps: impl FnMut(u16, u16) -> bool) -> Option<Vec<u16>> {
    let front = order.iter().rposition(|zone| selected.contains(zone))?;
    let target = order[front + 1..].iter()
        .find(|&&zone| !selected.contains(&zone) && selected.iter().any(|&chosen| overlaps(chosen, zone)))?;
    Some(move_block(order, selected, *target, true))
}

/// Move the selected zones together, keeping their order, to just behind the closest unselected zone behind all of them that overlaps any of them.
/// Returns None if nothing behind them overlaps them.
pub fn backward_order(order: &[u16], selected: &[u16], mut overlaps: impl FnMut(u16, u16) -> bool) -> Option<Vec<u16>> {
    let back = order.iter().position(|zone| selected.contains(zone))?;
    let target = order[..back].iter().rev()
        .find(|&&zone| !selected.contains(&zone) && selected.iter().any(|&chosen| overlaps(chosen, zone)))?;
    Some(move_block(order, selected, *target, false))
}

/// Move the selected zones together, keeping their order, in front of every other zone
pub fn to_front(order: &[u16], selected: &[u16]) -> Vec<u16> {
    let (chosen, rest): (Vec<u16>, Vec<u16>) = order.iter().partition(|zone| selected.contains(zone));
    rest.into_iter().chain(chosen).collect()
}

/// Move the selected zones together, keeping their order, behind every other zone
pub fn to_back(order: &[u16], selected: &[u16]) -> Vec<u16> {
    let (chosen, rest): (Vec<u16>, Vec<u16>) = order.iter().partition(|zone| selected.contains(zone));
    chosen.into_iter().chain(rest).collect()
}

/// Move the selected zones together, keeping their order, to just in front of or behind another zone
fn move_block(order: &[u16], selected: &[u16], target: u16, in_front: bool) -> Vec<u16> {
    let (chosen, mut rest): (Vec<u16>, Vec<u16>) = order.iter().partition(|zone| selected.contains(zone));
    let at = rest.iter().position(|&zone| zone == target).expect("the target isn't selected") + in_front as usize;
    rest.splice(at..at, chosen);
    rest
}

#[cfg(test)]
mod tests {
    use studio_math::Vec2;

    use super::*;
    use crate::objects::test_utils::{add_square, add_zone};
    use crate::objects::ShapeParams;

    /// Overlaps from a list of pairs, either way around
    fn overlapping(pairs: &'static [(u16, u16)]) -> impl FnMut(u16, u16) -> bool {
        move |a, b| pairs.contains(&(a, b)) || pairs.contains(&(b, a))
    }

    /// Forward skips zones in front that don't overlap, and stops just in front of the first one that does
    #[test]
    fn forward_skips() {
        let order = [0, 1, 2, 3];
        assert_eq!(forward_order(&order, &[0], overlapping(&[(0, 2)])), Some(vec![1, 2, 0, 3]));
        assert_eq!(forward_order(&order, &[0], overlapping(&[])), None);
        assert_eq!(forward_order(&order, &[3], overlapping(&[(3, 0)])), None);
    }

    /// Backward is the mirror image
    #[test]
    fn backward_skips() {
        let order = [0, 1, 2, 3];
        assert_eq!(backward_order(&order, &[3], overlapping(&[(3, 1)])), Some(vec![0, 3, 1, 2]));
        assert_eq!(backward_order(&order, &[0], overlapping(&[(0, 1)])), None);
    }

    /// Selected zones move as a block, keeping their order, past the first zone overlapping any of them
    #[test]
    fn block_keeps_order() {
        let order = [0, 1, 2, 3, 4];
        assert_eq!(forward_order(&order, &[2, 0], overlapping(&[(0, 4)])), Some(vec![1, 3, 4, 0, 2]));
        assert_eq!(backward_order(&order, &[1, 3], overlapping(&[(3, 0)])), Some(vec![1, 3, 0, 2, 4]));
    }

    /// To front and to back move the block all the way
    #[test]
    fn front_and_back() {
        let order = [0, 1, 2, 3];
        assert_eq!(to_front(&order, &[2, 0]), vec![1, 3, 0, 2]);
        assert_eq!(to_back(&order, &[3, 1]), vec![1, 3, 0, 2]);
    }

    /// Zones overlap by their bounding boxes, even where their paths don't, but boxes that only touch don't
    #[test]
    fn zones_overlapping() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(5.0, 5.0), (15.0, 5.0), (15.0, 15.0), (5.0, 15.0)]);
        add_zone(&mut objects, &[(10.0, 0.0), (20.0, 0.0), (20.0, -10.0)]);
        add_zone(&mut objects, &[(9.0, 1.0), (30.0, 1.0), (30.0, -20.0)]);
        assert!(objects.zones_overlap(0, 1).unwrap());
        assert!(!objects.zones_overlap(0, 2).unwrap());
        assert!(objects.zones_overlap(0, 3).unwrap());
    }

    /// Reordering keeps every zone's points, siblings and shapes, just numbered in the new order
    #[test]
    fn reorder_keeps_objects() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (branch, _) = objects.create_branching_zone(1).unwrap();
        objects.create_point(branch, Vec2::new(20.0, 5.0)).unwrap();
        let vertex = objects.add_shape_vertex(0, Vec2::new(5.0, 5.0)).unwrap();
        objects.create_shape(0, &[vertex], ShapeParams::default(), 0).unwrap();
        add_zone(&mut objects, &[(30.0, 0.0), (40.0, 0.0), (35.0, 10.0)]);

        // Each zone keeps its points and shapes, in its new place
        let positions = |saved: &crate::SavedObjects, zone: usize| saved.zones[zone].points.iter().map(|point| point.position).collect::<Vec<_>>();
        let before = objects.save();
        objects.reorder_zones(&[2, 0, 1]).unwrap();
        let after = objects.save();
        for (new, old) in [(0, 2), (1, 0), (2, 1)] {
            assert_eq!(positions(&after, new), positions(&before, old));
            assert_eq!(after.zones[new].shapes, before.zones[old].shapes);
            assert_eq!(after.zones[new].shape_vertices, before.zones[old].shape_vertices);
        }

        // The branch is still joined to the corner of the square it branched from, which now has a different ID
        let branch_start = objects.get_zone_info(2).unwrap().range().0;
        let sibling = objects.get_sibling(branch_start).unwrap();
        assert_ne!(sibling, branch_start);
        assert_eq!(objects.get_point_info(sibling).unwrap().position(), Vec2::new(10.0, 0.0));
        assert_eq!(objects.get_zone_info(1).unwrap().range(), (3, 4));

        assert!(objects.reorder_zones(&[0, 1]).is_err());
        assert!(objects.reorder_zones(&[0, 0, 1]).is_err());
    }

    /// Changing the order finds the new order, or None if nothing would move
    #[test]
    fn changed_order() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(50.0, 50.0), (60.0, 50.0), (60.0, 60.0)]);
        add_zone(&mut objects, &[(5.0, 5.0), (15.0, 5.0), (15.0, 15.0), (5.0, 15.0)]);
        assert_eq!(objects.changed_zone_order(&[0], ZoneOrderChange::Forward).unwrap(), Some(vec![1, 2, 0]));
        assert_eq!(objects.changed_zone_order(&[2], ZoneOrderChange::Forward).unwrap(), None);
        assert_eq!(objects.changed_zone_order(&[2], ZoneOrderChange::ToFront).unwrap(), None);
        assert_eq!(objects.changed_zone_order(&[2], ZoneOrderChange::ToBack).unwrap(), Some(vec![2, 0, 1]));
    }
}
