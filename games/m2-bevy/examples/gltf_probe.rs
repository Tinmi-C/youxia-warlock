// 临时探针：查 gltf crate 解出的 image 数据布局（诊断贴图加载失败）。
fn main() {
    for path in ["assets/Duck.glb", "assets/hero.glb", "assets/monster.glb"] {
        let (doc, _buffers, images) = gltf::import(path).unwrap();
        for (i, img) in images.iter().enumerate() {
            println!(
                "{path} image[{i}]: {}x{} {} bytes (w*h*4={}) format={:?}",
                img.width,
                img.height,
                img.pixels.len(),
                (img.width as usize) * (img.height as usize) * 4,
                img.format
            );
        }
        let _ = &doc;
    }
}
