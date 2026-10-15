use super::{ObjectBuffers, ControlPoint, Shape, ShapeVertex};
use super::zone::Zone;

/// A run of items that changed. Spans before the end of the shorter list have equal old and new lengths,
/// but the tail span holds the items that were removed or added, so its lengths can differ.
struct Span<T> {
    start: usize,
    old: Vec<T>,
    new: Vec<T>,
}

/// The changes made to one object list, which can be reverted or applied again.
pub(super) struct ListDiff<T> {
    old_len: usize,
    new_len: usize,
    spans: Vec<Span<T>>,
}

impl<T> ListDiff<T> where T : bytemuck::Pod + bytemuck::Zeroable {
    /// Find the changes between two versions of a list. Returns None if nothing changed.
    pub(super) fn between(before: &[T], after: &[T]) -> Option<Self> {
        let shared_len = before.len().min(after.len());
        let mut spans = vec![];
        let mut index = 0;
        while index < shared_len {
            if bytemuck::bytes_of(&before[index]) == bytemuck::bytes_of(&after[index]) { index += 1; continue; }

            // Extend the span over every consecutive changed item
            let start = index;
            while index < shared_len && bytemuck::bytes_of(&before[index]) != bytemuck::bytes_of(&after[index]) { index += 1; }
            spans.push(Span { start, old: before[start..index].to_vec(), new: after[start..index].to_vec() });
        }

        // Items removed or added at the end
        if before.len() != after.len() {
            spans.push(Span { start: shared_len, old: before[shared_len..].to_vec(), new: after[shared_len..].to_vec() });
        }

        (!spans.is_empty()).then_some(Self { old_len: before.len(), new_len: after.len(), spans })
    }

    /// Put a list back the way it was before the changes
    pub(super) fn revert(&self, items: &mut Vec<T>) {
        items.resize(self.old_len, T::zeroed());
        for span in &self.spans {
            items[span.start..span.start + span.old.len()].copy_from_slice(&span.old);
        }
    }

    /// Make the changes to a list again
    pub(super) fn apply(&self, items: &mut Vec<T>) {
        items.resize(self.new_len, T::zeroed());
        for span in &self.spans {
            items[span.start..span.start + span.new.len()].copy_from_slice(&span.new);
        }
    }

    /// Combine diffs that were made one after another into one diff, given the list as it is after the last of them.
    /// Returns None if the list ended up the way it was before the first diff.
    pub(super) fn collapse(mut diffs: Vec<Self>, items: &[T]) -> Option<Self> {
        if diffs.len() <= 1 { return diffs.pop(); }

        // Revert every diff to find the list from before the first one
        let mut before = items.to_vec();
        for diff in diffs.iter().rev() { diff.revert(&mut before); }
        Self::between(&before, items)
    }
}

/// The changes made to every object list by one action, which can be reverted or applied again.
/// Lists that weren't changed are None.
pub struct ObjectDiff {
    pub(super) zones: Option<ListDiff<Zone>>,
    pub(super) points: Option<ListDiff<ControlPoint>>,
    pub(super) point_siblings: Option<ListDiff<u16>>,
    pub(super) shapes: Option<ListDiff<Shape>>,
    pub(super) shape_vertices: Option<ListDiff<ShapeVertex>>,
}

impl ObjectDiff {
    /// Returns true if nothing changed
    pub fn is_empty(&self) -> bool {
        self.zones.is_none() && self.points.is_none() && self.point_siblings.is_none()
            && self.shapes.is_none() && self.shape_vertices.is_none()
    }

    /// Put the objects back the way they were before the changes
    pub fn revert(&self, objects: &ObjectBuffers) {
        if let Some(diff) = &self.zones { diff.revert(&mut objects.zones.borrow_mut().items); }
        if let Some(diff) = &self.points { diff.revert(&mut objects.points.borrow_mut().items); }
        if let Some(diff) = &self.point_siblings { diff.revert(&mut objects.point_siblings.borrow_mut().items); }
        if let Some(diff) = &self.shapes { diff.revert(&mut objects.shapes.borrow_mut().items); }
        if let Some(diff) = &self.shape_vertices { diff.revert(&mut objects.shape_vertices.borrow_mut().items); }
    }

    /// Make the changes to the objects again
    pub fn apply(&self, objects: &ObjectBuffers) {
        if let Some(diff) = &self.zones { diff.apply(&mut objects.zones.borrow_mut().items); }
        if let Some(diff) = &self.points { diff.apply(&mut objects.points.borrow_mut().items); }
        if let Some(diff) = &self.point_siblings { diff.apply(&mut objects.point_siblings.borrow_mut().items); }
        if let Some(diff) = &self.shapes { diff.apply(&mut objects.shapes.borrow_mut().items); }
        if let Some(diff) = &self.shape_vertices { diff.apply(&mut objects.shape_vertices.borrow_mut().items); }
    }

    /// Combine diffs that were made one after another into one diff, given the objects as they are after the last of them
    pub fn collapse(diffs: Vec<ObjectDiff>, objects: &ObjectBuffers) -> ObjectDiff {
        let (mut zones, mut points, mut point_siblings) = (vec![], vec![], vec![]);
        let (mut shapes, mut shape_vertices) = (vec![], vec![]);
        for diff in diffs {
            zones.extend(diff.zones);
            points.extend(diff.points);
            point_siblings.extend(diff.point_siblings);
            shapes.extend(diff.shapes);
            shape_vertices.extend(diff.shape_vertices);
        }
        ObjectDiff {
            zones: ListDiff::collapse(zones, &objects.zones.borrow().items),
            points: ListDiff::collapse(points, &objects.points.borrow().items),
            point_siblings: ListDiff::collapse(point_siblings, &objects.point_siblings.borrow().items),
            shapes: ListDiff::collapse(shapes, &objects.shapes.borrow().items),
            shape_vertices: ListDiff::collapse(shape_vertices, &objects.shape_vertices.borrow().items),
        }
    }
}

#[cfg(test)]
mod tests {
    use studio_math::Vec2;

    use super::*;
    use super::super::test_utils::add_square;

    /// Check that a diff turns one list into the other in both directions
    fn round_trip(before: &[u16], after: &[u16]) -> ListDiff<u16> {
        let diff = ListDiff::between(before, after).unwrap();
        let mut items = after.to_vec();
        diff.revert(&mut items);
        assert_eq!(items, before);
        diff.apply(&mut items);
        assert_eq!(items, after);
        diff
    }

    /// Identical lists have no diff
    #[test]
    fn unchanged() {
        assert!(ListDiff::<u16>::between(&[1, 2, 3], &[1, 2, 3]).is_none());
        assert!(ListDiff::<u16>::between(&[], &[]).is_none());
    }

    /// Only the changed runs are stored
    #[test]
    fn edit_middle() {
        let diff = round_trip(&[1, 2, 3, 4, 5, 6], &[1, 9, 9, 4, 9, 6]);
        assert_eq!(diff.spans.len(), 2);
        assert_eq!((diff.spans[0].start, diff.spans[0].old.as_slice()), (1, [2, 3].as_slice()));
        assert_eq!((diff.spans[1].start, diff.spans[1].new.as_slice()), (4, [9].as_slice()));
    }

    /// Added items are stored in the tail
    #[test]
    fn grow() {
        let diff = round_trip(&[1, 2], &[1, 2, 3, 4]);
        assert_eq!(diff.spans.len(), 1);
        assert_eq!(diff.spans[0].new, vec![3, 4]);
    }

    /// Removed items are stored in the tail
    #[test]
    fn shrink() {
        let diff = round_trip(&[1, 2, 3, 4], &[1, 5]);
        assert_eq!(diff.spans.len(), 2);
        assert_eq!(diff.spans[1].old, vec![3, 4]);
    }

    /// Diffs made one after another collapse into one diff covering all of them
    #[test]
    fn collapse_sequence() {
        let first = ListDiff::between(&[1u16, 2], &[1, 2, 3, 4]).unwrap();
        let second = ListDiff::between(&[1u16, 2, 3, 4], &[1, 9, 3, 4]).unwrap();
        let diff = ListDiff::collapse(vec![first, second], &[1, 9, 3, 4]).unwrap();
        let mut items = vec![1, 9, 3, 4];
        diff.revert(&mut items);
        assert_eq!(items, vec![1, 2]);
        diff.apply(&mut items);
        assert_eq!(items, vec![1, 9, 3, 4]);
    }

    /// Changes that undo each other collapse to nothing
    #[test]
    fn collapse_back_to_start() {
        let first = ListDiff::between(&[1u16, 2], &[1, 5]).unwrap();
        let second = ListDiff::between(&[1u16, 5], &[1, 2]).unwrap();
        assert!(ListDiff::collapse(vec![first, second], &[1, 2]).is_none());
    }

    /// A single diff is kept as it is, and no diffs collapse to nothing
    #[test]
    fn collapse_single() {
        let diff = ListDiff::between(&[1u16, 2], &[1, 5]).unwrap();
        let diff = ListDiff::collapse(vec![diff], &[1, 5]).unwrap();
        assert_eq!(diff.spans[0].new, vec![5]);
        assert!(ListDiff::<u16>::collapse(vec![], &[1, 5]).is_none());
    }

    /// Moving a point only records the points list, and can be reverted and applied
    #[test]
    fn move_point() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);

        objects.begin_change();
        objects.move_points(&[2], Vec2::new(5.0, 5.0)).unwrap();
        let diff = objects.finish_change();
        assert!(diff.points.is_some());
        assert!(diff.zones.is_none());
        assert!(diff.point_siblings.is_none());
        assert!(diff.shapes.is_none());
        assert!(diff.shape_vertices.is_none());

        diff.revert(&objects);
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(10.0, 10.0));
        diff.apply(&objects);
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 15.0));
    }
}
