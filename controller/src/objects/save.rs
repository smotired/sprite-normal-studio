use std::collections::{HashMap, HashSet};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use studio_math::{Vec2, Vec3};

use super::{ObjectsHandle, ControlPoint, ControlPointMode, Shape, ShapeParams, ShapeProfile, ShapeVertex, Zone, MAX_OBJECT_ID};

/// Every zone and control point, as stored in a workspace file.
/// Point IDs are not stored. They are numbered in order across all zones, starting from the first point of the first zone.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedObjects {
    pub(crate) zones: Vec<SavedZone>,

    /// Groups of sibling points, each in the order of its sibling ring. Points with no siblings are left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) sibling_groups: Vec<Vec<u16>>,
}

/// A zone with its points and shapes, as stored in a workspace file
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedZone {
    pub(crate) normal: Vec3,
    pub(crate) points: Vec<SavedPoint>,

    /// Positions of the vertices this zone's shapes are made from
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) shape_vertices: Vec<Vec2>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) shapes: Vec<SavedShape>,
}

/// A shape, as stored in a workspace file
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedShape {
    /// Indices of the shape's 1 to 3 vertices in its zone's shape_vertices
    pub(crate) vertices: Vec<u16>,

    pub(crate) profile: ShapeProfile,

    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) inner_radius: f32,

    pub(crate) outer_radius: f32,
    pub(crate) steepness: f32,
    pub(crate) blend: f32,

    #[serde(default)]
    pub(crate) group: Option<u16>,
}

fn is_zero(value: &f32) -> bool { *value == 0.0 }
fn is_unfeathered(width: &u8) -> bool { *width == 0 }

impl SavedShape {
    /// Everything about the shape other than its zone and vertices
    fn params(&self) -> ShapeParams {
        ShapeParams {
            profile: self.profile,
            inner_radius: self.inner_radius,
            outer_radius: self.outer_radius,
            steepness: self.steepness,
            blend: self.blend,
        }
    }
}

/// A control point, as stored in a workspace file. Handles are relative to the position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedPoint {
    pub(crate) position: Vec2,
    pub(crate) left_handle: Vec2,
    pub(crate) right_handle: Vec2,
    pub(crate) mode: ControlPointMode,

    /// The point the left handle syncs to. Left out if it syncs to its own point without flipping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) left_sync: Option<SavedSync>,

    /// The point the right handle syncs to. Left out if it syncs to its own point without flipping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) right_sync: Option<SavedSync>,

    /// How far the outgoing curve is feathered into the zone on its other side, in pixels. Left out if it isn't.
    #[serde(default, skip_serializing_if = "is_unfeathered")]
    pub(crate) feather: u8,
}

/// The target of a handle sync, as stored in a workspace file
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedSync {
    pub(crate) point: u16,
    pub(crate) flipped: bool,
}

/// Object lists built from saved objects, which have been checked and are ready to replace the current ones
pub struct LoadedObjects {
    pub(super) zones: Vec<Zone>,
    pub(super) points: Vec<ControlPoint>,
    pub(super) point_siblings: Vec<u16>,
    pub(super) shapes: Vec<Shape>,
    pub(super) shape_vertices: Vec<ShapeVertex>,
}

impl SavedObjects {
    /// How many zones there are
    pub fn zone_count(&self) -> usize { self.zones.len() }

    /// Check the saved objects and build the object lists from them, without touching any current objects.
    pub fn validate(&self) -> anyhow::Result<LoadedObjects> {
        let zone_count = self.zones.len();
        let point_count: usize = self.zones.iter().map(|zone| zone.points.len()).sum();
        let shape_count: usize = self.zones.iter().map(|zone| zone.shapes.len()).sum();
        let vertex_count: usize = self.zones.iter().map(|zone| zone.shape_vertices.len()).sum();
        if zone_count > MAX_OBJECT_ID as usize { anyhow::bail!("Too many zones: {} (max {})", zone_count, MAX_OBJECT_ID); }
        if point_count > MAX_OBJECT_ID as usize { anyhow::bail!("Too many control points: {} (max {})", point_count, MAX_OBJECT_ID); }
        if shape_count > MAX_OBJECT_ID as usize { anyhow::bail!("Too many shapes: {} (max {})", shape_count, MAX_OBJECT_ID); }
        if vertex_count > MAX_OBJECT_ID as usize { anyhow::bail!("Too many shape vertices: {} (max {})", vertex_count, MAX_OBJECT_ID); }

        // Build the zones, points, and shapes, numbering each in order
        let mut zones = Vec::with_capacity(zone_count);
        let mut points = Vec::with_capacity(point_count);
        let mut shapes = Vec::with_capacity(shape_count);
        let mut shape_vertices = Vec::with_capacity(vertex_count);
        let mut syncs = vec![];
        for (zone_id, saved_zone) in self.zones.iter().enumerate() {
            // Every tool assumes a zone has a path, and a zero normal has no direction to light
            if saved_zone.points.len() < 2 { anyhow::bail!("Zone {} has {} points, but needs at least 2!", zone_id, saved_zone.points.len()); }
            let normal = saved_zone.normal;
            if ![normal.x, normal.y, normal.z].iter().all(|value| value.is_finite()) || normal.sq_magnitude() <= 0.0 {
                anyhow::bail!("Zone {} has a normal that isn't a finite, nonzero vector!", zone_id);
            }

            let mut zone = Zone::new(points.len() as u16, shapes.len() as u16, shape_vertices.len() as u16, saved_zone.normal);
            for saved_point in &saved_zone.points {
                let point_id = zone.add_point();
                let vectors = [saved_point.position, saved_point.left_handle, saved_point.right_handle];
                if !vectors.iter().all(|vector| vector.x.is_finite() && vector.y.is_finite()) {
                    anyhow::bail!("Point {} has a position or handle that isn't finite!", point_id);
                }
                for sync in [saved_point.left_sync, saved_point.right_sync].into_iter().flatten() {
                    if sync.point as usize >= point_count {
                        anyhow::bail!("Point {} syncs a handle to point {}, which does not exist!", point_id, sync.point);
                    }
                    syncs.push((point_id, sync.point));
                }
                points.push(ControlPoint::from_saved(point_id, zone_id as u16, saved_point));
            }

            for &position in &saved_zone.shape_vertices {
                if !position.x.is_finite() || !position.y.is_finite() { anyhow::bail!("Zone {} has a shape vertex that isn't a finite position!", zone_id); }
                zone.add_vertex();
                shape_vertices.push(ShapeVertex::new(position));
            }
            // A group is one surface, so its shapes must be next to each other. Shapes without a group are each their own.
            let (mut current, mut finished) = (None, HashSet::new());
            for saved_shape in &saved_zone.shapes {
                if saved_shape.group == current { continue; }
                if let Some(group) = saved_shape.group && finished.contains(&group) {
                    anyhow::bail!("Zone {} has shape group {} split up by other shapes!", zone_id, group);
                }
                finished.extend(current);
                current = saved_shape.group;
            }

            // Shapes without a group each get one nobody else in the zone has
            let mut next_group = saved_zone.shapes.iter().filter_map(|shape| shape.group).max().map_or(0, |max| max as u32 + 1);
            for (i, saved_shape) in saved_zone.shapes.iter().enumerate() {
                if let Some(&vertex) = saved_shape.vertices.iter().find(|&&vertex| vertex as usize >= saved_zone.shape_vertices.len()) {
                    anyhow::bail!("Shape {} in zone {} uses vertex {}, which does not exist!", i, zone_id, vertex);
                }
                let group = match saved_shape.group {
                    Some(group) => group,
                    None => {
                        let group = u16::try_from(next_group).map_err(|_| anyhow::anyhow!("Zone {} has too many shape groups!", zone_id))?;
                        next_group += 1;
                        group
                    },
                };
                let shape = Shape::new(zone_id as u16, &saved_shape.vertices, saved_shape.params(), group, 0)
                    .with_context(|| format!("Shape {} in zone {} is invalid", i, zone_id))?;
                zone.add_shape();
                shapes.push(shape);
            }
            zones.push(zone);
        }

        // Link each sibling group into a ring. Every other point is its own sibling.
        let mut point_siblings: Vec<u16> = (0..point_count as u16).collect();
        let mut grouped = vec![false; point_count];
        for group in &self.sibling_groups {
            if group.len() < 2 { anyhow::bail!("Sibling group {:?} must have at least two points!", group); }
            for (i, &point_id) in group.iter().enumerate() {
                if point_id as usize >= point_count { anyhow::bail!("Sibling group {:?} has point {}, which does not exist!", group, point_id); }
                if grouped[point_id as usize] { anyhow::bail!("Point {} is in more than one sibling group!", point_id); }
                grouped[point_id as usize] = true;
                point_siblings[point_id as usize] = group[(i + 1) % group.len()];
            }
        }

        // A handle can only sync to its own point or one of its siblings
        for (point_id, sync_id) in syncs {
            if sync_id != point_id && !ControlPoint::get_siblings(point_id, &point_siblings).contains(&sync_id) {
                anyhow::bail!("Point {} syncs a handle to point {}, which isn't its sibling!", point_id, sync_id);
            }
        }
        point_siblings.resize(65536, 0); // match ObjectsHandle::new

        Ok(LoadedObjects { zones, points, point_siblings, shapes, shape_vertices })
    }
}

impl ObjectsHandle {
    /// Get every zone, control point and shape in the form they are saved to a file
    pub fn save(&self) -> SavedObjects {
        let zone_ids: Vec<u16> = (0..self.zone_count()).collect();
        saved_zones(
            &zone_ids,
            &self.zones.borrow().items,
            &self.points.borrow().items,
            &self.point_siblings.borrow().items,
            &self.shapes.borrow().items,
            &self.shape_vertices.borrow().items,
        )
    }

    /// Replace every zone, control point and shape with ones that have been loaded from a file
    pub fn load(&self, loaded: LoadedObjects) {
        self.zones.borrow_mut().items = loaded.zones;
        self.points.borrow_mut().items = loaded.points;
        self.point_siblings.borrow_mut().items = loaded.point_siblings;
        self.shapes.borrow_mut().items = loaded.shapes;
        self.shape_vertices.borrow_mut().items = loaded.shape_vertices;
    }
}

/// Get a zone's shapes and the vertices they use in the form they are saved to a file.
/// Shapes in a path that is still being drawn are left out, along with vertices only they use.
fn saved_shapes(zone: &Zone, shapes: &[Shape], vertices: &[ShapeVertex]) -> (Vec<Vec2>, Vec<SavedShape>) {
    let (shape_start, shape_count) = zone.shapes();
    let (vertex_start, _) = zone.vertices();
    let kept: Vec<&Shape> = shapes[shape_start as usize..(shape_start + shape_count) as usize].iter()
        .filter(|shape| !shape.is_pending())
        .collect();

    // Number the vertices the kept shapes use in the order they are in the zone
    let mut used: Vec<u16> = kept.iter().flat_map(|shape| shape.local_vertices()).collect();
    used.sort();
    used.dedup();
    let file_index = |local: u16| used.binary_search(&local).expect("every used vertex is numbered") as u16;

    let saved_vertices = used.iter().map(|&local| vertices[(vertex_start + local) as usize].position()).collect();
    let saved_shapes = kept.iter().map(|shape| {
        let params = shape.params();
        SavedShape {
            vertices: shape.local_vertices().into_iter().map(file_index).collect(),
            profile: params.profile,
            inner_radius: params.inner_radius,
            outer_radius: params.outer_radius,
            steepness: params.steepness,
            blend: params.blend,
            group: Some(shape.group()),
        }
    }).collect();
    (saved_vertices, saved_shapes)
}

/// Get some zones with their points and shapes in the form they are saved to a file, numbering points in order across the zones.
/// Every handle sync and sibling of the zones' points should be in the zones too, or the saved objects won't load.
pub(super) fn saved_zones(zone_ids: &[u16], zones: &[Zone], points: &[ControlPoint], siblings: &[u16], shapes: &[Shape], vertices: &[ShapeVertex]) -> SavedObjects {
    // Number points in zone order, in case the points list doesn't already follow it
    let mut file_ids = HashMap::new();
    let mut point_ids = vec![];
    for &zone_id in zone_ids {
        let (start, count) = zones[zone_id as usize].range();
        for point_id in start..start + count {
            file_ids.insert(point_id, point_ids.len() as u16);
            point_ids.push(point_id);
        }
    }
    let file_id = |point_id: u16| file_ids.get(&point_id).copied().unwrap_or(point_id);

    let saved_zones = zone_ids.iter().map(|&zone_id| {
        let zone = zones[zone_id as usize];
        let (start, count) = zone.range();
        let (shape_vertices, zone_shapes) = saved_shapes(&zone, shapes, vertices);
        SavedZone {
            normal: zone.normal(),
            points: (start..start + count)
                .map(|point_id| points[point_id as usize].saved(point_id, file_id))
                .collect(),
            shape_vertices,
            shapes: zone_shapes,
        }
    }).collect();

    // Follow each sibling ring once, starting from its first point
    let mut sibling_groups = vec![];
    let mut visited = HashSet::new();
    for &point_id in &point_ids {
        let mut group = vec![];
        let mut id = point_id;
        while file_ids.contains_key(&id) && visited.insert(id) {
            group.push(file_id(id));
            id = siblings[id as usize];
        }
        if group.len() > 1 { sibling_groups.push(group); }
    }

    SavedObjects { zones: saved_zones, sibling_groups }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::{add_square, add_zone};

    /// Get the raw bytes of every object list, to check two sets of objects are identical
    fn raw(objects: &ObjectsHandle) -> [Vec<u8>; 5] {
        [
            bytemuck::cast_slice(&objects.zones.borrow().items).to_vec(),
            bytemuck::cast_slice(&objects.points.borrow().items).to_vec(),
            bytemuck::cast_slice(&objects.point_siblings.borrow().items).to_vec(),
            bytemuck::cast_slice(&objects.shapes.borrow().items).to_vec(),
            bytemuck::cast_slice(&objects.shape_vertices.borrow().items).to_vec(),
        ]
    }

    /// Build objects with a branching zone, curved points, a custom normal, and shapes sharing a vertex
    fn sample() -> ObjectsHandle {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let (branch, _) = objects.create_branching_zone(1).unwrap();
        objects.create_point(branch, Vec2::new(20.0, 5.0)).unwrap();
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (25.0, 10.0)]);
        objects.set_point_mode(0, ControlPointMode::Continuous).unwrap();
        objects.zones.borrow_mut().items[2].set_normal(Vec3::new(0.5, 0.0, 0.5));

        let dome = ShapeParams { profile: ShapeProfile::Dome, inner_radius: 2.0, ..Default::default() };
        let vertices: Vec<u16> = [(2.0, 2.0), (8.0, 2.0), (5.0, 8.0)].into_iter()
            .map(|position| objects.add_shape_vertex(0, Vec2::from(position)).unwrap())
            .collect();
        objects.create_shape(0, &vertices, ShapeParams::default(), 0).unwrap();
        objects.create_shape(0, &[vertices[2], vertices[0]], dome, 0).unwrap();
        let vertex = objects.add_shape_vertex(2, Vec2::new(25.0, 3.0)).unwrap();
        objects.create_shape(2, &[vertex], dome, 0).unwrap();
        objects
    }

    /// Saving, writing to JSON, reading it back and loading gives identical objects
    #[test]
    fn round_trip() {
        let objects = sample();
        let json = serde_json::to_string(&objects.save()).unwrap();
        let saved: SavedObjects = serde_json::from_str(&json).unwrap();

        let loaded = ObjectsHandle::headless();
        loaded.load(saved.validate().unwrap());
        assert_eq!(raw(&loaded), raw(&objects));
        assert_eq!(loaded.save(), objects.save());
    }

    /// Branching zones are saved as sibling groups
    #[test]
    fn sibling_groups() {
        let saved = sample().save();
        assert!(!saved.sibling_groups.is_empty());
        for group in &saved.sibling_groups {
            assert!(group.len() >= 2);
        }
    }

    /// Handles that sync to their own point are left out of the file
    #[test]
    fn self_sync_omitted() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let json = serde_json::to_string(&objects.save()).unwrap();
        assert!(!json.contains("sync"));
        assert!(!json.contains("sibling_groups"));
        assert!(json.contains("\"mode\":\"linear\""));
    }

    /// Point IDs are numbered across zones in order
    #[test]
    fn numbering() {
        let saved: SavedObjects = serde_json::from_value(serde_json::json!({
            "zones": [
                { "normal": { "x": 0.0, "y": 0.0, "z": 1.0 }, "points": [point(0.0), point(1.0)] },
                { "normal": { "x": 0.0, "y": 0.0, "z": 1.0 }, "points": [point(2.0), point(3.0)] },
            ],
            "sibling_groups": [[1, 2]],
        })).unwrap();

        let objects = ObjectsHandle::headless();
        objects.load(saved.validate().unwrap());
        assert_eq!(objects.object_counts(), (2, 4));
        assert_eq!(objects.get_zone_info(1).unwrap().range(), (2, 2));
        assert_eq!(objects.get_point_info(2).unwrap().zone_id(), 1);
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(2.0, 0.0));
        assert_eq!(objects.get_siblings(1), vec![2]);
        assert_eq!(objects.get_siblings(0), Vec::<u16>::new());
    }

    /// A linear point at (x, 0) as JSON
    fn point(x: f32) -> serde_json::Value {
        serde_json::json!({
            "position": { "x": x, "y": 0.0 },
            "left_handle": { "x": 0.0, "y": 0.0 },
            "right_handle": { "x": 0.0, "y": 0.0 },
            "mode": "linear",
        })
    }

    /// Saved objects with one zone of `count` points and the given sibling groups
    fn with_groups(count: usize, sibling_groups: Vec<Vec<u16>>) -> SavedObjects {
        let points = (0..count).map(|i| serde_json::from_value(point(i as f32)).unwrap()).collect();
        SavedObjects { zones: vec![SavedZone { normal: Vec3::FORWARD, points, shape_vertices: vec![], shapes: vec![] }], sibling_groups }
    }

    /// Syncing to a point that doesn't exist is an error
    #[test]
    fn invalid_sync() {
        let mut saved = with_groups(3, vec![]);
        saved.zones[0].points[1].left_sync = Some(SavedSync { point: 9999, flipped: false });
        assert!(saved.validate().is_err());
    }

    /// Sibling groups must have real points, at least two of them, and can't share points
    #[test]
    fn invalid_sibling_groups() {
        assert!(with_groups(3, vec![vec![0, 1]]).validate().is_ok());
        assert!(with_groups(3, vec![vec![0]]).validate().is_err());
        assert!(with_groups(3, vec![vec![0, 3]]).validate().is_err());
        assert!(with_groups(3, vec![vec![0, 1], vec![1, 2]]).validate().is_err());
        assert!(with_groups(3, vec![vec![0, 0]]).validate().is_err());
    }

    /// Zones need at least two points and a usable normal, and points need finite positions and handles
    #[test]
    fn invalid_zones_and_points() {
        assert!(with_groups(2, vec![]).validate().is_ok());
        assert!(with_groups(1, vec![]).validate().is_err());
        assert!(with_groups(0, vec![]).validate().is_err());

        for normal in [Vec3::new(0.0, 0.0, 0.0), Vec3::new(f32::NAN, 0.0, 1.0)] {
            let mut saved = with_groups(3, vec![]);
            saved.zones[0].normal = normal;
            assert!(saved.validate().is_err());
        }

        let mut saved = with_groups(3, vec![]);
        saved.zones[0].points[1].position.x = f32::NAN;
        assert!(saved.validate().is_err());
        let mut saved = with_groups(3, vec![]);
        saved.zones[0].points[2].right_handle.y = f32::INFINITY;
        assert!(saved.validate().is_err());
    }

    /// Handles can sync to their own point or a sibling, but not to any other point
    #[test]
    fn sync_must_be_sibling() {
        let mut saved = with_groups(3, vec![vec![0, 2]]);
        saved.zones[0].points[0].left_sync = Some(SavedSync { point: 2, flipped: true });
        assert!(saved.validate().is_ok());
        saved.zones[0].points[0].left_sync = Some(SavedSync { point: 1, flipped: true });
        assert!(saved.validate().is_err());
    }

    /// Shapes in a group must be next to each other in their zone
    #[test]
    fn split_shape_group() {
        let shape = |group: Option<u16>| SavedShape { vertices: vec![0], profile: ShapeProfile::Cone, inner_radius: 0.0, outer_radius: 5.0, steepness: 1.0, blend: 1.0, group };
        let with_shapes = |groups: &[Option<u16>]| {
            let mut saved = with_groups(3, vec![]);
            saved.zones[0].shape_vertices = vec![Vec2::ZERO];
            saved.zones[0].shapes = groups.iter().map(|&group| shape(group)).collect();
            saved
        };
        assert!(with_shapes(&[Some(0), Some(0), Some(1), None, Some(2)]).validate().is_ok());
        assert!(with_shapes(&[Some(0), Some(1), Some(0)]).validate().is_err());
        assert!(with_shapes(&[Some(0), None, Some(0)]).validate().is_err());
    }

    /// Too many points is an error
    #[test]
    fn too_many_points() {
        let points = vec![serde_json::from_value(point(0.0)).unwrap(); MAX_OBJECT_ID as usize + 1];
        let saved = SavedObjects { zones: vec![SavedZone { normal: Vec3::FORWARD, points, shape_vertices: vec![], shapes: vec![] }], sibling_groups: vec![] };
        assert!(saved.validate().is_err());
    }

    /// Shapes keep their vertices, params and zones through saving and loading
    #[test]
    fn shapes_saved() {
        let saved = sample().save();
        assert_eq!(saved.zones[0].shape_vertices.len(), 3);
        assert_eq!(saved.zones[0].shapes[1].vertices, vec![2, 0]);
        assert_eq!(saved.zones[0].shapes[1].profile, ShapeProfile::Dome);
        assert_eq!(saved.zones[2].shapes[0].vertices, vec![0]);
        assert!(saved.zones[1].shapes.is_empty());

        let loaded = ObjectsHandle::headless();
        loaded.load(saved.validate().unwrap());
        assert_eq!(loaded.get_zone_info(2).unwrap().shapes(), (2, 1));
        assert_eq!(loaded.get_shape_info(2).unwrap().zone_id(), 2);
        assert_eq!(loaded.shape_positions(1).unwrap(), vec![Vec2::new(5.0, 8.0), Vec2::new(2.0, 2.0)]);
    }

    /// Zones without shapes leave them out of the file, and files without them load with no shapes
    #[test]
    fn no_shapes_omitted() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let json = serde_json::to_string(&objects.save()).unwrap();
        assert!(!json.contains("shape"));

        let saved: SavedObjects = serde_json::from_str(&json).unwrap();
        assert_eq!(saved.validate().unwrap().shapes.len(), 0);
    }

    /// Shapes still being drawn aren't saved, and neither are the vertices only they use
    #[test]
    fn pending_not_saved() {
        let mut objects = sample();
        let vertex = objects.add_shape_vertex(0, Vec2::new(9.0, 9.0)).unwrap();
        objects.create_shape(0, &[vertex, 0], ShapeParams::default(), super::super::shape::SHAPE_PENDING).unwrap();
        assert_eq!(objects.save(), sample().save());
    }

    /// Groups are saved, and shapes from before groups each get a group nobody else has
    #[test]
    fn shape_groups() {
        let mut objects = sample();
        objects.create_shape_in_group(0, &[0], 0, 0).unwrap();
        let saved = objects.save();
        assert_eq!(saved.zones[0].shapes.iter().map(|shape| shape.group).collect::<Vec<_>>(), vec![Some(0), Some(0), Some(1)]);

        let mut old = saved.clone();
        old.zones[0].shapes[0].group = None;
        old.zones[0].shapes[2].group = None;
        let loaded = ObjectsHandle::headless();
        loaded.load(old.validate().unwrap());
        let groups: Vec<u16> = (0..3).map(|shape_id| loaded.get_shape_info(shape_id).unwrap().group()).collect();
        assert_eq!(groups, vec![1, 0, 2]);
    }

    /// Shapes need 1 to 3 different vertices that exist in their zone, and values in range
    #[test]
    fn invalid_shapes() {
        let shape = |vertices: Vec<u16>| SavedShape { vertices, profile: ShapeProfile::Cone, inner_radius: 0.0, outer_radius: 5.0, steepness: 1.0, blend: 1.0, group: None };
        let with_shape = |shape: SavedShape| {
            let mut saved = with_groups(3, vec![]);
            saved.zones[0].shape_vertices = vec![Vec2::ZERO, Vec2::ONE];
            saved.zones[0].shapes = vec![shape];
            saved
        };

        assert!(with_shape(shape(vec![0, 1])).validate().is_ok());
        assert!(with_shape(shape(vec![0, 2])).validate().is_err());
        assert!(with_shape(shape(vec![])).validate().is_err());
        assert!(with_shape(shape(vec![1, 1])).validate().is_err());
        assert!(with_shape(SavedShape { outer_radius: -1.0, ..shape(vec![0]) }).validate().is_err());
        assert!(with_shape(SavedShape { blend: f32::INFINITY, ..shape(vec![0]) }).validate().is_err());
    }
}
