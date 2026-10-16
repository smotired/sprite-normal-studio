use std::{cell::{Ref, RefCell, RefMut}, rc::Rc};

use wgpu::{Buffer, Device, Queue};

use super::{ObjectBuffers, BufferStates, MIN_BUFFER_SIZE};
use super::diff::{ListDiff, ObjectDiff};
use crate::Controller;

pub(super) struct VecWithBuffer<T> where T : bytemuck::Pod + bytemuck::Zeroable {
    /// Items in the vector
    pub(super) items: Vec<T>,

    /// Buffer storing these items. Only None when created without a GPU for tests.
    buffer: Option<Buffer>,

    /// Amount of items the buffer can hold before a resize
    buffer_size: usize,

    /// The label for the buffer
    label: &'static str,

    /// If the items have changed since they were last written to the buffer
    gpu_stale: bool,

    /// If writes are being recorded for a change
    recording: bool,

    /// The items from before the first write of the change being recorded, if anything was written
    before: Option<Vec<T>>,
}

impl<T> VecWithBuffer<T> where T : bytemuck::Pod + bytemuck::Zeroable {
    pub fn new(device: &Device, label: &'static str) -> Self {
        let items = vec![];
        let (buffer, buffer_size) = create_buffer(device, &items, label);
        Self {
            items,
            buffer: Some(buffer),
            buffer_size,
            label,
            gpu_stale: true,
            recording: false,
            before: None,
        }
    }

    /// Create a list with no GPU buffer behind it, for testing the CPU side.
    #[cfg(test)]
    pub fn headless(label: &'static str) -> Self {
        Self { items: vec![], buffer: None, buffer_size: MIN_BUFFER_SIZE, label, gpu_stale: true, recording: false, before: None }
    }

    /// Get the buffer and its size. Caller should keep track of buffer sizes and recreate bind groups if they change.
    pub fn get_buffer(&mut self, device: &Device) -> (Buffer, usize) {
        let required_space = &self.items.len().max(MIN_BUFFER_SIZE);
        let maintain_range = (self.buffer_size >> 2)..=(self.buffer_size);

        // Recreate the buffer if the length has grown too large or too small
        if !(maintain_range).contains(required_space) {
            println!("Need to recreate {} | Buffer size: {} |  Item count: {}", self.label, self.buffer_size, self.items.len());
            let (buffer, buffer_size) = create_buffer(device, &self.items, self.label);
            (self.buffer, self.buffer_size) = (Some(buffer), buffer_size);
            self.gpu_stale = true;
        }

        (self.buffer.clone().expect("Buffer should exist when a device is available"), self.buffer_size)
    }

    /// Write the items to the buffer, if they changed since the last write.
    pub fn write(&mut self, queue: &Queue) {
        if !self.gpu_stale { return; }
        if let Some(buffer) = &self.buffer {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&self.items[..]));
            self.gpu_stale = false;
        }
    }

    /// Note that the items are about to be written to.
    /// If a change is being recorded and this is its first write, copy the items so the change can be found later.
    fn mark_written(&mut self) {
        self.gpu_stale = true;
        if self.recording && self.before.is_none() {
            self.before = Some(self.items.clone());
        }
    }

    /// Start recording a change
    fn begin_change(&mut self) {
        self.recording = true;
        self.before = None;
    }

    /// Stop recording a change, and get what changed if anything
    fn finish_change(&mut self) -> Option<ListDiff<T>> {
        self.recording = false;
        self.before.take().and_then(|before| ListDiff::between(&before, &self.items))
    }

    /// Stop recording a change, and put the items back the way they were before it
    fn rollback_change(&mut self) {
        self.recording = false;
        if let Some(before) = self.before.take() {
            self.items = before;
            self.gpu_stale = true;
        }
    }
}

/// A shared object list. Mutably borrowing it marks the items as written, so changes can be recorded
/// and the buffer is only rewritten when needed.
pub(super) struct ObjectList<T>(Rc<RefCell<VecWithBuffer<T>>>) where T : bytemuck::Pod + bytemuck::Zeroable;

impl<T> ObjectList<T> where T : bytemuck::Pod + bytemuck::Zeroable {
    pub(super) fn new(list: VecWithBuffer<T>) -> Self { Self(Rc::new(RefCell::new(list))) }

    pub(super) fn borrow(&self) -> Ref<'_, VecWithBuffer<T>> { self.0.borrow() }

    /// Borrow the list to write to its items
    pub(super) fn borrow_mut(&self) -> RefMut<'_, VecWithBuffer<T>> {
        let mut list = self.0.borrow_mut();
        list.mark_written();
        list
    }

    /// Borrow the list to manage its buffer or change recording, without marking the items as written
    fn borrow_untracked(&self) -> RefMut<'_, VecWithBuffer<T>> { self.0.borrow_mut() }
}

impl<T> Clone for ObjectList<T> where T : bytemuck::Pod + bytemuck::Zeroable {
    fn clone(&self) -> Self { Self(self.0.clone()) }
}

impl ObjectBuffers {
    /// Get references to the buffers and their sizes. Recreates the buffers if needed.
    /// The caller should keep track of the previous buffer sizes and recreate the bind group if they differ.
    pub fn get_buffers(&mut self, device: &Device) -> BufferStates {
        BufferStates {
            zones: self.zones.borrow_untracked().get_buffer(device),
            points: self.points.borrow_untracked().get_buffer(device),
            point_siblings: self.point_siblings.borrow_untracked().get_buffer(device),
            shapes: self.shapes.borrow_untracked().get_buffer(device),
            shape_vertices: self.shape_vertices.borrow_untracked().get_buffer(device),
        }
    }

    /// Add a command to write the lists that changed to the buffers. Assumes the buffers have already been sized.
    pub fn write_buffers(&self, queue: &Queue) {
        self.zones.borrow_untracked().write(queue);
        self.points.borrow_untracked().write(queue);
        self.point_siblings.borrow_untracked().write(queue);
        self.shapes.borrow_untracked().write(queue);
        self.shape_vertices.borrow_untracked().write(queue);
    }
}

impl ObjectBuffers {
    /// Start recording a change to the objects. Lists are only copied the first time they are written to.
    pub fn begin_change(&self) {
        self.zones.borrow_untracked().begin_change();
        self.points.borrow_untracked().begin_change();
        self.point_siblings.borrow_untracked().begin_change();
        self.shapes.borrow_untracked().begin_change();
        self.shape_vertices.borrow_untracked().begin_change();
    }

    /// Stop recording a change, and get what changed
    pub fn finish_change(&self) -> ObjectDiff {
        ObjectDiff {
            zones: self.zones.borrow_untracked().finish_change(),
            points: self.points.borrow_untracked().finish_change(),
            point_siblings: self.point_siblings.borrow_untracked().finish_change(),
            shapes: self.shapes.borrow_untracked().finish_change(),
            shape_vertices: self.shape_vertices.borrow_untracked().finish_change(),
        }
    }

    /// Stop recording a change, and put every object list back the way it was before the change
    pub fn rollback_change(&self) {
        self.zones.borrow_untracked().rollback_change();
        self.points.borrow_untracked().rollback_change();
        self.point_siblings.borrow_untracked().rollback_change();
        self.shapes.borrow_untracked().rollback_change();
        self.shape_vertices.borrow_untracked().rollback_change();
    }
}

/// Create a buffer for use with a VecWithBuffer. Return the buffer and its size of the buffer.
fn create_buffer<T>(device: &Device, vector: &[T], label: &str) -> (Buffer, usize) where T : bytemuck::Pod + bytemuck::Zeroable {
    // Make buffer the smallest power of 2 above 32 that will fit
    let item_count = {
        let mut size = MIN_BUFFER_SIZE;
        let target = vector.len();
        while size < target { size <<= 1 }
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
    pub fn write_object_buffers(&self, queue: &Queue) {
        // Write the object buffers
        self.objects.write_buffers(queue);

        // Pack selected points, shape vertices, and points with a selected sibling into the selection texture
        let points = self.tool.selection();
        let mut with_siblings = points.to_vec();
        for &point_id in points {
            with_siblings.extend(self.objects.get_siblings(point_id));
        }
        let packed_bytes = pack_selection(points, self.tool.shape_selection(), &with_siblings);
        queue.write_texture(
            self.selection.as_image_copy(),
            &packed_bytes,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(SELECTION_WIDTH * 4), rows_per_image: Some(SELECTION_HEIGHT) },
            self.selection.size(),
        );
    }
}

/// Texels in each row of the selection texture, which each hold 32 bits. Must match SELECTION_WIDTH in bindings.wgsl.
pub(crate) const SELECTION_WIDTH: u32 = 256;

/// Rows in the selection texture: 65536 bits each for selected points, selected shape vertices, and points with a selected sibling
pub(crate) const SELECTION_HEIGHT: u32 = 3 * 65536 / 32 / SELECTION_WIDTH;

/// Pack selected point IDs, selected shape vertex IDs, and the IDs of points that are selected or have a selected sibling
/// into one bit each, in that order, 65536 bits apart. Must match selection_bit in bindings.wgsl.
fn pack_selection(point_ids: &[u16], vertex_ids: &[u16], with_siblings: &[u16]) -> Vec<u8> {
    let mut packed_bytes = vec![0; (SELECTION_WIDTH * SELECTION_HEIGHT * 4) as usize];
    let bits = point_ids.iter().map(|&id| id as usize)
        .chain(vertex_ids.iter().map(|&id| 65536 + id as usize))
        .chain(with_siblings.iter().map(|&id| 2 * 65536 + id as usize));
    for bit in bits {
        packed_bytes[bit / 8] |= 1 << (bit % 8);
    }
    packed_bytes
}

#[cfg(test)]
mod tests {
    use super::ObjectBuffers;
    use super::super::test_utils::add_square;

    /// Nothing is copied or recorded for lists that are only read
    #[test]
    fn reads_not_recorded() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.begin_change();
        objects.get_point_info(2).unwrap();
        objects.get_siblings(2);
        assert!(objects.points.borrow().before.is_none());
        assert!(objects.finish_change().is_empty());
    }

    /// Lists are only marked for writing to the GPU when they are written to
    #[test]
    fn gpu_stale() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.points.borrow_untracked().gpu_stale = false;
        objects.get_point_info(2).unwrap();
        assert!(!objects.points.borrow().gpu_stale);
        objects.move_points(&[2], studio_math::Vec2::new(1.0, 1.0)).unwrap();
        assert!(objects.points.borrow().gpu_stale);
    }

    /// Points, shape vertices, and points with selected siblings are packed into thirds of the selection
    #[test]
    fn pack_selection() {
        let packed = super::pack_selection(&[0, 9, 65535], &[1, 65535], &[2]);
        assert_eq!(packed.len(), 3 * 8192);
        assert_eq!((packed[0], packed[1], packed[8191]), (0b1, 0b10, 0b1000_0000));
        assert_eq!((packed[8192], packed[16383]), (0b10, 0b1000_0000));
        assert_eq!(packed[16384], 0b100);
        assert_eq!(packed.iter().map(|byte| byte.count_ones()).sum::<u32>(), 6);
    }

    /// Rolling back a change brings back the objects from when it began
    #[test]
    fn rollback_change() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.begin_change();

        add_square(&mut objects, 20.0);
        objects.create_branching_zone(1).unwrap();
        assert_ne!(objects.object_counts(), (1, 4));

        objects.rollback_change();
        assert_eq!(objects.object_counts(), (1, 4));
        assert_eq!(objects.get_point_info(2).unwrap().position(), studio_math::Vec2::new(10.0, 10.0));
        assert_eq!(objects.get_siblings(1), Vec::<u16>::new());
    }
}
