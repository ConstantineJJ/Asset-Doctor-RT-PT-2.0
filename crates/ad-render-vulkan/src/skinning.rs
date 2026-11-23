use super::Renderer;
use crate::{context::GpuResult, resources::Buffer};
use ad_document::GeometrySnapshot;
use ash::vk;

pub(super) struct PoseBuffers {
    vertices: Buffer,
    normals: Buffer,
    tangents: Buffer,
}

impl Renderer {
    /// Publish a pose for the current read-only scene. Indices, draws and
    /// materials are shared with the base snapshot.
    pub fn set_scene_pose(&mut self, snapshot: &GeometrySnapshot) -> GpuResult<()> {
        let scene = self.scene.as_ref().ok_or("No scene for animation")?;
        let positions = bytes(&snapshot.positions);
        let normals = bytes(&snapshot.normals);
        let tangents = bytes(&snapshot.tangents);
        if snapshot.document_id != scene.document_id
            || snapshot.revision != scene.revision
            || positions.len() != scene.vertices.size()
            || normals.len() != scene.normals.size()
            || tangents.len() != scene.tangents.size()
        {
            return Err("Animation pose does not match the current scene".into());
        }
        let next = match self.pose_buffers.take() {
            Some(buffers)
                if buffers.vertices.size() == positions.len()
                    && buffers.normals.size() == normals.len()
                    && buffers.tangents.size() == tangents.len() =>
            {
                // SAFETY: These buffers were retired before being moved out
                // of the live scene. No submission can access them.
                unsafe {
                    buffers.vertices.overwrite(positions)?;
                    buffers.normals.overwrite(normals)?;
                    buffers.tangents.overwrite(tangents)?;
                }
                buffers
            }
            _ => {
                let upload = |values| {
                    Buffer::upload(self.gpu.clone(), values, vk::BufferUsageFlags::VERTEX_BUFFER)
                };
                PoseBuffers {
                    vertices: upload(positions)?,
                    normals: upload(normals)?,
                    tangents: upload(tangents)?,
                }
            }
        };
        self.wait_idle()?;
        let scene = self.scene.as_mut().expect("Scene checked before upload");
        self.pose_buffers = Some(PoseBuffers {
            vertices: std::mem::replace(&mut scene.vertices, next.vertices),
            normals: std::mem::replace(&mut scene.normals, next.normals),
            tangents: std::mem::replace(&mut scene.tangents, next.tangents),
        });
        Ok(())
    }
}

fn bytes<const N: usize>(values: &[[f32; N]]) -> &[u8] {
    // SAFETY: These arrays consist of initialized f32 values without padding.
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast(), std::mem::size_of_val(values))
    }
}
