use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use studio_math::{Vec2, Vec3};

use super::{ObjectBuffers, ControlPoint, ControlPointMode, Zone, MAX_OBJECT_ID};

/// Every zone and control point, as stored in a workspace file.
/// Point IDs are not stored. They are numbered in order across all zones, starting from the first point of the first zone.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedObjects {
    pub(crate) zones: Vec<SavedZone>,

    /// Groups of sibling points, each in the order of its sibling ring. Points with no siblings are left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) sibling_groups: Vec<Vec<u16>>,
}

/// A zone and its points, as stored in a workspace file
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedZone {
    pub(crate) normal: Vec3,
    pub(crate) points: Vec<SavedPoint>,
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
}

impl SavedObjects {
    /// How many zones there are
    pub fn zone_count(&self) -> usize { self.zones.len() }

    /// Check the saved objects and build the object lists from them, without touching any current objects.
    pub fn validate(&self) -> anyhow::Result<LoadedObjects> {
        let zone_count = self.zones.len();
        let point_count: usize = self.zones.iter().map(|zone| zone.points.len()).sum();
        if zone_count > MAX_OBJECT_ID as usize { anyhow::bail!("Too many zones: {} (max {})", zone_count, MAX_OBJECT_ID); }
        if point_count > MAX_OBJECT_ID as usize { anyhow::bail!("Too many control points: {} (max {})", point_count, MAX_OBJECT_ID); }

        // Build the zones and points, numbering points in order
        let mut zones = Vec::with_capacity(zone_count);
        let mut points = Vec::with_capacity(point_count);
        for (zone_id, saved_zone) in self.zones.iter().enumerate() {
            let mut zone = Zone::new(points.len() as u16, saved_zone.normal);
            for saved_point in &saved_zone.points {
                let point_id = zone.add_point();
                for sync in [saved_point.left_sync, saved_point.right_sync].into_iter().flatten() {
                    if sync.point as usize >= point_count {
                        anyhow::bail!("Point {} syncs a handle to point {}, which does not exist!", point_id, sync.point);
                    }
                }
                points.push(ControlPoint::from_saved(point_id, zone_id as u16, saved_point));
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
        point_siblings.resize(65536, 0); // match ObjectBuffers::new

        Ok(LoadedObjects { zones, points, point_siblings })
    }
}

impl ObjectBuffers {
    /// Get every zone and control point in the form they are saved to a file
    pub fn save(&self) -> SavedObjects {
        let zone_ids: Vec<u16> = (0..self.zone_count()).collect();
        saved_zones(&zone_ids, &self.zones.borrow().items, &self.points.borrow().items, &self.point_siblings.borrow().items)
    }

    /// Replace every zone and control point with ones that have been loaded from a file
    pub fn load(&self, loaded: LoadedObjects) {
        self.zones.borrow_mut().items = loaded.zones;
        self.points.borrow_mut().items = loaded.points;
        self.point_siblings.borrow_mut().items = loaded.point_siblings;
    }
}

/// Get some zones and their points in the form they are saved to a file, numbering points in order across the zones.
/// Every handle sync and sibling of the zones' points should be in the zones too, or the saved objects won't load.
pub(super) fn saved_zones(zone_ids: &[u16], zones: &[Zone], points: &[ControlPoint], siblings: &[u16]) -> SavedObjects {
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
        SavedZone {
            normal: zone.normal(),
            points: (start..start + count)
                .map(|point_id| points[point_id as usize].saved(point_id, file_id))
                .collect(),
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
    fn raw(objects: &ObjectBuffers) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        (
            bytemuck::cast_slice(&objects.zones.borrow().items).to_vec(),
            bytemuck::cast_slice(&objects.points.borrow().items).to_vec(),
            bytemuck::cast_slice(&objects.point_siblings.borrow().items).to_vec(),
        )
    }

    /// Build objects with a branching zone, curved points, and a custom normal
    fn sample() -> ObjectBuffers {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.create_branching_zone(1).unwrap();
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (25.0, 10.0)]);
        objects.set_point_mode(0, ControlPointMode::Continuous).unwrap();
        objects.zones.borrow_mut().items[2].set_normal(Vec3::new(0.5, 0.0, 0.5));
        objects
    }

    /// Saving, writing to JSON, reading it back and loading gives identical objects
    #[test]
    fn round_trip() {
        let objects = sample();
        let json = serde_json::to_string(&objects.save()).unwrap();
        let saved: SavedObjects = serde_json::from_str(&json).unwrap();

        let loaded = ObjectBuffers::headless();
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
        let mut objects = ObjectBuffers::headless();
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
                { "normal": { "x": 0.0, "y": 0.0, "z": 1.0 }, "points": [point(2.0)] },
            ],
            "sibling_groups": [[1, 2]],
        })).unwrap();

        let objects = ObjectBuffers::headless();
        objects.load(saved.validate().unwrap());
        assert_eq!(objects.object_counts(), (2, 3));
        assert_eq!(objects.get_zone_info(1).unwrap().range(), (2, 1));
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
        SavedObjects { zones: vec![SavedZone { normal: Vec3::FORWARD, points }], sibling_groups }
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

    /// Too many points is an error
    #[test]
    fn too_many_points() {
        let points = vec![serde_json::from_value(point(0.0)).unwrap(); MAX_OBJECT_ID as usize + 1];
        let saved = SavedObjects { zones: vec![SavedZone { normal: Vec3::FORWARD, points }], sibling_groups: vec![] };
        assert!(saved.validate().is_err());
    }
}
