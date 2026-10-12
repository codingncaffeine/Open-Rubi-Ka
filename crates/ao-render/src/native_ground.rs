//! N3 AnarchyGround_t preference callback 100320b2 and draw selection 10034dab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundPath { Fused, ShadowsTwoPass, Simple }
#[derive(Clone, Copy, Debug)]
pub struct NativeGround { mode: i32, enabled: bool, shadows: bool }
impl Default for NativeGround {
    fn default() -> Self { Self { mode: 3, enabled: true, shadows: false } }
}
impl NativeGround {
    pub fn set(&mut self, mode: i32) {
        let mode = mode.clamp(0, 3);
        if self.mode == mode { return; }
        self.mode = mode;
        match mode {
            0 => self.enabled = false,
            1 => { self.enabled = true; self.shadows = true; }
            _ => { self.enabled = true; self.shadows = false; }
        }
    }
    pub fn recreate(&mut self) {
        let mode = self.mode;
        *self = Self::default();
        self.set(mode);
    }
    pub fn path(self) -> GroundPath {
        if !self.shadows { GroundPath::Fused }
        else if self.enabled { GroundPath::ShadowsTwoPass }
        else { GroundPath::Simple }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_mode_histories() {
        for initial in 0..=3 {
            let mut s = NativeGround::default(); s.set(initial);
            assert_eq!(s.path(), if initial == 1 { GroundPath::ShadowsTwoPass } else { GroundPath::Fused });
        }
        let mut s = NativeGround::default();
        for (mode, path) in [(1,GroundPath::ShadowsTwoPass),(0,GroundPath::Simple),(0,GroundPath::Simple),(2,GroundPath::Fused),(0,GroundPath::Fused),(1,GroundPath::ShadowsTwoPass),(3,GroundPath::Fused)] {
            s.set(mode); assert_eq!(s.path(),path);
        }
        s.set(1); s.set(0); s.recreate();
        assert_eq!(s.path(), GroundPath::Fused, "new native ground object fires the saved mode from clean flags");
    }
    #[test]
    #[ignore = "Requires a GPU; native ground stage output regression"]
    fn native_ground_gpu_mode_history() {
        use ao_scene::{ground_detail::{GroundCell, GroundPatch}, Environment, Instance, Mesh, Scene, Submesh, Texture, TextureKey, Vertex, IDENTITY};
        use crate::{Camera, Renderer, Targets, Vec3};
        let tile = TextureKey { rdb_type: 1_010_006, id: 1 };
        let palette = TextureKey { rdb_type: 1_000_007, id: 2 };
        let composite = TextureKey { rdb_type: 0, id: 3 };
        let mut scene = Scene { environment: Some(Environment { sky_color: [0.0; 3], fog_color: [0.0; 3], fog_start: 100.0, fog_end: 200.0, ambient: [0.2f32.powf(2.2); 3], sun_color: [0.0; 3], sun_dir: [0.0, 1.0, 0.0], sun_specular: 0.0 }), ..Default::default() };
        for (key, value) in [(tile, 204), (palette, 64), (composite, 204)] {
            scene.textures.insert(key, Texture { width: 1, height: 1, rgba: vec![value, value, value, 255], source: Default::default() });
        }
        let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, -1.0]].into_iter().map(|pos| Vertex { pos, normal: [0.0, 1.0, 0.0], uv: [0.5; 2], color: [1.0; 4] }).collect();
        let mut fine = Submesh::new(vec![0, 1, 2, 1, 3, 2], Some(tile));
        fine.texture_clamp = true;
        let mut coarse = Submesh::new(vec![0; 6], Some(composite));
        coarse.texture_clamp = true;
        scene.meshes.push(Mesh { vertices, submeshes: vec![fine, coarse] });
        scene.instances.push(Instance { mesh: 0, transform: IDENTITY });
        scene.ground_detail.push(GroundPatch { mesh: 0, cells: 1, cell_size: 1.0, heights: vec![0; 4], coarse_submesh: 1, fine_cells: vec![GroundCell { submesh: 0, vertices: [0, 1, 2, 3] }], shadow_texture: Some(palette), shadow_uv_scale: [1.0; 2], ..Default::default() });
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let mut renderer = Renderer::new(&instance, None).unwrap();
        renderer.upload(&scene);
        let targets = Targets::new(&renderer, 32, 32);
        let out = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: None, size: wgpu::Extent3d { width: 32, height: 32, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: renderer.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[],
        });
        let camera = Camera::look_at(Vec3::new(0.5, 1.0, -0.5), Vec3::new(0.5, 0.0, -0.5));
        let path = std::env::temp_dir().join(format!("ao-ground-history-{}.png", std::process::id()));
        for fog in [false, true] {
            renderer.set_ground_rendering(3);
            renderer.env.fog_start = if fog { 0.0 } else { 100.0 };
            renderer.env.fog_end = if fog { 2.0 } else { 200.0 };
            renderer.env.fog_color = [0.6f32.powf(2.2); 3];
        for (mode, expected_path, expected_pixel, calls) in [(0, GroundPath::Fused, 51, 1), (1, GroundPath::ShadowsTwoPass, 51, 2), (0, GroundPath::Simple, 41, 1), (2, GroundPath::Fused, 51, 1), (3, GroundPath::Fused, 51, 1)] {
            renderer.set_ground_rendering(mode);
            let mut expected_pixel = if fog { match expected_path { GroundPath::Fused => 102, GroundPath::ShadowsTwoPass => 112, GroundPath::Simple => 97 } } else { expected_pixel };
            if mode == 2 {
                let textures = [(TextureKey { rdb_type: 1_010_022, id: 1 }, Texture { width: 1, height: 1, rgba: vec![102,102,102,255], source: Default::default() })].into_iter().collect();
                renderer.refresh_ground_textures(&textures);
                expected_pixel = if fog { 89 } else { 26 };
            } else if mode == 3 { renderer.refresh_ground_textures(&scene.textures); }
            renderer.render(&out.create_view(&Default::default()), &targets, &camera);
            assert_eq!(renderer.ground_rendering_path(), expected_path);
            assert_eq!(renderer.stats.draw_calls, calls);
            crate::texture_to_png(&renderer, &out, 32, 32, &path).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            std::fs::remove_file(&path).unwrap();
            let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().unwrap();
            let mut pixels = vec![0; reader.output_buffer_size()];
            let info = reader.next_frame(&mut pixels).unwrap();
            let pixel = pixels[(16 * 32 + 16) * (info.color_type.samples())] as i32;
            assert!((pixel - expected_pixel).abs() <= 2, "mode {mode}: {pixel} != {expected_pixel}");
        }
        }
    }
}
