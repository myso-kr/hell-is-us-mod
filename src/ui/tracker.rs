//! The quest tracker at the right of the screen, as MMORPGs have it: the quests
//! under way, the followed one first and opened up — its description, the leads
//! the hero is chasing, how many of its clues are found — the others one line each.

use super::pen::Pen;
use crate::quests::{Kind, Quest};
use crate::raster::{Canvas, Rgba};

pub const W: i32 = 340;
pub const H: i32 = 560;
const PAD: i32 = 12;
/// Quests shown besides the followed one.
const OTHERS: usize = 5;

const FOLLOWED: Rgba = Rgba(255, 210, 90, 255);
const MAIN: Rgba = Rgba(255, 120, 210, 255);
const DEED: Rgba = Rgba(120, 220, 150, 255);
const TEXT: Rgba = Rgba(235, 235, 235, 255);
const DIM: Rgba = Rgba(185, 185, 185, 255);

fn accent(q: &Quest, followed: bool) -> Rgba {
    match (followed, q.kind) {
        (true, _) => FOLLOWED,
        (_, Kind::Main(_)) => MAIN,
        (_, Kind::GoodDeed) => DEED,
    }
}

fn kind_label(q: &Quest) -> String {
    match q.kind {
        Kind::Main(n) => format!("메인 {n}"),
        Kind::GoodDeed => "선행".into(),
    }
}

/// Draw the tracker into `cv` (cleared first); the height used, 0 when there is
/// nothing to show.
pub fn draw(cv: &mut Canvas, pen: &mut Pen, journal: &[Quest], followed: Option<&Quest>, near: bool) -> i32 {
    cv.clear();
    let mut list: Vec<&Quest> = Vec::new();
    if let Some(f) = followed {
        list.push(f);
    }
    let others = journal.iter().filter(|q| q.active() && Some(q.key.as_str()) != followed.map(|f| f.key.as_str()));
    list.extend(others.take(OTHERS));
    if list.is_empty() {
        return 0;
    }
    let width = W - 2 * PAD - 8;
    let x = PAD + 8;
    let mut y = PAD;
    y += pen.write(cv, PAD, y, W - 2 * PAD, "퀘스트", 12, true, DIM, 1) + 6;
    for (i, q) in list.iter().enumerate() {
        let open = i == 0 && followed.is_some();
        let c = accent(q, open);
        let top = y;
        y += pen.write(cv, x, y, width, &q.name, if open { 16 } else { 14 }, true, if open { c } else { TEXT }, 2);
        let mut sub = kind_label(q);
        if let Some((got, all)) = q.progress.filter(|(_, all)| *all > 0) {
            sub += &format!(" · 단서 {got}/{all}");
        }
        if open {
            sub += " · 안내 중";
        }
        y += pen.write(cv, x, y, width, &sub, 11, false, DIM, 1) + 2;
        if open {
            if !near {
                let why = match q.kind {
                    Kind::Main(_) => "이 지역엔 이 퀘스트의 목표가 없음 — 가까운 다른 퀘스트 목표로 안내",
                    Kind::GoodDeed => "이 지역엔 이 선행의 목표가 없음 — 다른 지역에서 진행",
                };
                y += pen.write(cv, x, y, width, why, 12, true, Rgba(255, 190, 90, 255), 2) + 2;
            }
            if !q.detail.is_empty() {
                y += pen.write(cv, x, y, width, &q.detail, 13, false, TEXT, 4) + 4;
            }
            for (lead, text) in q.leads.iter().take(3) {
                let line = match text {
                    Some(t) => format!("· {lead} — {t}"),
                    None => format!("· {lead}"),
                };
                y += pen.write(cv, x + 4, y, width - 4, &line, 12, false, DIM, 2) + 2;
            }
        }
        // The quest's bar along its left edge.
        cv.rect(PAD, top + 3, PAD + 3, y - 2, c);
        y += if open { 12 } else { 8 };
        if y > H - 40 {
            break;
        }
    }
    let used = (y + PAD - 8).min(H);
    under(cv, used, Rgba(8, 10, 14, 120));
    used
}

/// Put `bg` under what is drawn, over the rows `0..h` — so the box is as tall as its
/// content without drawing the content twice.
fn under(cv: &mut Canvas, h: i32, bg: Rgba) {
    let (a, r, g, b) = (bg.3 as u32, bg.0 as u32 * bg.3 as u32 / 255, bg.1 as u32 * bg.3 as u32 / 255, bg.2 as u32 * bg.3 as u32 / 255);
    let rows = (h.max(0) as usize).min(cv.h);
    for p in &mut cv.px[..rows * cv.w] {
        let k = 255 - (*p >> 24);
        let ch = |shift: u32, v: u32| ((((*p >> shift) & 0xFF) + v * k / 255).min(255)) << shift;
        *p = ch(24, a) | ch(16, r) | ch(8, g) | ch(0, b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_box_goes_under_the_text() {
        let mut cv = Canvas::new(2, 1);
        cv.px[0] = 0xFF_FF_FF_FF;
        under(&mut cv, 1, Rgba(0, 0, 0, 128));
        assert_eq!(cv.px[0], 0xFF_FF_FF_FF, "opaque text stays");
        assert_eq!(cv.px[1] >> 24, 128, "empty pixels get the box");
    }
}
