//! Quick Export and Save for Web write PNG-24 through the web optimizer, not `photocraft_io::export`:
//! they give fully transparent pixels the edge colour too, unless `pngEdgeFill` is false.

use super::*;

/// A red square on a transparent canvas, exported by `cmd` with `params`; the colour and alpha of
/// a transparent pixel far from the square.
fn far_pixel(cmd: &str, params: Value) -> [u8; 4] {
    let dir = std::env::temp_dir().join(format!("photocraft-web-edge-fill-{}-{cmd}-{}", std::process::id(), params));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("star.png").to_string_lossy().into_owned();
    let mut s = crate::session();
    s.execute("file.new", json!({"width": 16, "height": 4, "background": "transparent"})).unwrap();
    s.execute("select.rect", json!({"x": 0, "y": 0, "width": 4, "height": 4})).unwrap();
    s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
    s.execute("select.deselect", json!({})).unwrap();
    let mut p = params;
    p["path"] = json!(path);
    s.execute(cmd, p).unwrap();
    let img = photocraft_codecs::decode(&std::fs::read(&path).unwrap()).unwrap();
    let img = img.convert(photocraft_codecs::ChannelLayout::Rgba, photocraft_codecs::SampleType::U8);
    img.data()[12 * 4..13 * 4].try_into().unwrap()
}

#[test]
fn quick_export_png_fills_transparent_pixels_with_the_edge_colour() {
    assert_eq!(far_pixel("file.export.quickExport", json!({})), [255, 0, 0, 0]);
}

#[test]
fn save_for_web_png_fills_unless_asked_not_to() {
    assert_eq!(far_pixel("file.export.saveForWebLegacy", json!({"format": "png24"})), [255, 0, 0, 0]);
    let plain = far_pixel("file.export.saveForWebLegacy", json!({"format": "png24", "pngEdgeFill": false}));
    assert_eq!(plain[3], 0);
    assert_ne!(plain[..3], [255, 0, 0]);
}
