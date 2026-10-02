//! The minimap's icons: SVGs in `assets/icons/`, built into the binary and
//! rasterised once, at the size the map draws them, by resvg.
//!
//! Each becomes a small premultiplied `0xAARRGGBB` bitmap — the canvas's own pixel
//! format — so drawing one is a blend per pixel and nothing more.

use crate::actors::Kind;

/// A rasterised icon, `size` px square.
pub struct Icon {
    pub size: usize,
    pub px: Vec<u32>,
}

fn source(k: Kind) -> &'static str {
    match k {
        Kind::Enemy => include_str!("../assets/icons/enemy.svg"),
        Kind::Item => include_str!("../assets/icons/item.svg"),
        Kind::Loot => include_str!("../assets/icons/loot.svg"),
        Kind::Npc => include_str!("../assets/icons/npc.svg"),
        Kind::Interact => include_str!("../assets/icons/interact.svg"),
    }
}

pub fn render(svg: &str, size: usize) -> Result<Icon, String> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).map_err(|e| format!("icon: {e}"))?;
    let mut pixmap = tiny_skia::Pixmap::new(size as u32, size as u32).ok_or("icon: empty size")?;
    let s = tree.size();
    let scale = size as f32 / s.width().max(s.height());
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    // tiny-skia keeps premultiplied RGBA bytes; the canvas wants premultiplied ARGB words.
    let px = pixmap
        .data()
        .chunks_exact(4)
        .map(|c| (c[3] as u32) << 24 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32)
        .collect();
    Ok(Icon { size, px })
}

/// One icon per kind, in `Kind::ALL` order.
pub struct Icons(pub Vec<Icon>);

impl Icons {
    pub fn new(size: usize) -> Result<Icons, String> {
        Kind::ALL.iter().map(|&k| render(source(k), size)).collect::<Result<_, _>>().map(Icons)
    }

    pub fn get(&self, k: Kind) -> &Icon {
        &self.0[Kind::ALL.iter().position(|&x| x == k).unwrap()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_renders_something_opaque_in_the_middle() {
        let icons = Icons::new(16).unwrap();
        for k in Kind::ALL {
            let i = icons.get(k);
            assert_eq!(i.px.len(), 16 * 16);
            let centre = (5..11).flat_map(|y| (5..11).map(move |x| y * 16 + x)).map(|j| i.px[j] >> 24).max().unwrap();
            assert!(centre > 200, "{k:?}: most opaque centre pixel {centre}");
            assert_eq!(i.px[0] >> 24, 0, "{k:?}: corner should be clear");
        }
    }
}
