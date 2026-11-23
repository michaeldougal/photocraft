//! Edge colour fill for PNG export: every fully transparent pixel takes the colour of the nearest
//! pixel that is not fully transparent, and stays fully transparent.
//!
//! A fully transparent pixel's colour is invisible in the file, but a renderer that filters
//! (bilinear sampling when a texture is scaled, rotated or mipmapped) blends it into the visible
//! neighbours before it applies alpha. Transparent pixels stored as black, as most editors leave
//! them, then show as a dark fringe around every shape; game engines that sample with straight
//! (not premultiplied) alpha, such as Roblox's image and decal rendering, show it at every edge.
//! Giving those pixels the colour of the nearest visible pixel makes the blend come out in the
//! shape's own colour. The pixels, and the file's transparency, are otherwise unchanged.

use photocraft_codecs::{Image, SampleType};

/// Alpha below which a float sample counts as fully transparent: it stores as 0 in a 16-bit PNG.
const FLOAT_CLEAR: f32 = 0.5 / 65535.0;

/// Gives every fully transparent pixel of `img` the colour channels of the nearest pixel (by
/// straight-line distance) whose alpha is above zero, keeping its alpha. `false`, and `img`
/// unchanged, when there is nothing to fill: no alpha channel, no fully transparent pixel, no
/// visible pixel to take a colour from, or too many pixels to index.
pub(crate) fn fill_transparent(img: &mut Image) -> bool {
    let layout = img.layout();
    if !layout.has_alpha() {
        return false;
    }
    let (w, h) = (img.width() as usize, img.height() as usize);
    let ch = layout.channels();
    let float = matches!(img.sample_type(), SampleType::F16 | SampleType::F32);
    // Integer samples are 0 only when the stored value is 0; NaN alpha counts as visible.
    let visible: Vec<bool> = (0..w * h)
        .map(|p| {
            let a = img.sample_normalized(p * ch + ch - 1);
            if float { a >= FLOAT_CLEAR || a.is_nan() } else { a > 0.0 }
        })
        .collect();
    if visible.iter().all(|&v| v) {
        return false;
    }
    let Some(nearest) = photocraft_algo::nearest_seed::nearest_seed(&visible, w, h) else { return false };
    let bytes = img.sample_type().bytes();
    let (pixel, color) = (ch * bytes, layout.color_channels() * bytes);
    let data = img.data_mut();
    for (p, (&v, &src)) in visible.iter().zip(&nearest).enumerate() {
        if !v {
            let src = src as usize * pixel;
            data.copy_within(src..src + color, p * pixel);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_codecs::ChannelLayout;

    fn rgba8(w: u32, h: u32, px: &[[u8; 4]]) -> Image {
        Image::from_u8(w, h, ChannelLayout::Rgba, px.iter().flatten().copied().collect()).unwrap()
    }

    fn at(img: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width() + x) * 4) as usize;
        img.data()[i..i + 4].try_into().unwrap()
    }

    /// Transparent pixels take the colour of the nearer of two shapes and stay transparent; the
    /// visible pixels, translucent ones included, keep theirs.
    #[test]
    fn transparent_pixels_take_the_nearest_visible_colour() {
        const RED: [u8; 4] = [255, 0, 0, 255];
        const SOFT_BLUE: [u8; 4] = [0, 0, 255, 1];
        const CLEAR: [u8; 4] = [0, 0, 0, 0];
        let mut row = [CLEAR; 7];
        row[0] = RED;
        row[6] = SOFT_BLUE;
        let mut img = rgba8(7, 1, &row);
        assert!(fill_transparent(&mut img));
        assert_eq!(at(&img, 0, 0), RED);
        assert_eq!(at(&img, 6, 0), SOFT_BLUE);
        for x in 1..3 {
            assert_eq!(at(&img, x, 0), [255, 0, 0, 0], "x = {x}, nearer red");
        }
        for x in 4..6 {
            assert_eq!(at(&img, x, 0), [0, 0, 255, 0], "x = {x}, nearer blue");
        }
    }

    /// Nearest is by straight-line distance, not by steps along rows and columns: a pixel three
    /// right and three down of A (distance 4.24) is nearer to it than to B five right (5), though
    /// B is fewer steps away (5 against 6).
    #[test]
    fn nearest_is_by_straight_line_distance() {
        let (a, b) = ([10, 20, 30, 255], [40, 50, 60, 255]);
        let mut px = vec![[0u8; 4]; 9 * 4];
        px[0] = a;
        px[3 * 9 + 8] = b;
        let mut img = rgba8(9, 4, &px);
        fill_transparent(&mut img);
        assert_eq!(at(&img, 3, 3), [10, 20, 30, 0]);
    }

    /// Sixteen-bit and float images keep every byte of a sample; gray with alpha works too.
    #[test]
    fn wide_samples_and_gray_are_copied_whole() {
        let img = Image::from_u16(2, 1, ChannelLayout::GrayA, &[0x1234, 0xffff, 0x0000, 0x0000]).unwrap();
        let mut filled = img.clone();
        assert!(fill_transparent(&mut filled));
        assert_eq!(filled.to_u16_samples().unwrap(), vec![0x1234, 0xffff, 0x1234, 0]);

        let mut f = Image::from_f32(2, 1, ChannelLayout::Rgba, &[0.25, 0.5, 0.75, 0.5, 0.0, 0.0, 0.0, 0.0]).unwrap();
        assert!(fill_transparent(&mut f));
        assert_eq!(f.to_f32_samples().unwrap()[4..], [0.25, 0.5, 0.75, 0.0]);
    }

    /// Nothing to do without alpha, when every pixel is clear, or when none is.
    #[test]
    fn leaves_images_with_nothing_to_fill() {
        let opaque = rgba8(2, 1, &[[1, 2, 3, 255], [4, 5, 6, 255]]);
        let clear = rgba8(2, 1, &[[1, 2, 3, 0], [4, 5, 6, 0]]);
        for img in [opaque, clear] {
            let mut copy = img.clone();
            assert!(!fill_transparent(&mut copy));
            assert_eq!(copy.data(), img.data());
        }
        let mut rgb = Image::from_u8(1, 1, ChannelLayout::Rgb, vec![0, 0, 0]).unwrap();
        assert!(!fill_transparent(&mut rgb));
    }
}
