//! How the map is shown: which map is up, how the landscape is drawn, and where a
//! point in the world lands on the map (`View`, the projection).
//!
//! Unreal's axes: X forward, Y right, Z up; yaw is degrees from +X towards +Y, so
//! it turns clockwise seen from above. The map is north-up or heading-up.

use super::minimap::Point;

/// Which map is up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Display {
    Mini,
    Big,
    Off,
}

impl Display {
    /// In the order the map key steps through them.
    pub const ALL: [Display; 3] = [Display::Mini, Display::Big, Display::Off];

    /// As kept in the settings file.
    pub fn key(self) -> &'static str {
        match self {
            Display::Mini => "mini",
            Display::Big => "big",
            Display::Off => "off",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Display::Mini => tr!("미니맵"),
            Display::Big => tr!("큰 지도"),
            Display::Off => tr!("끔"),
        }
    }

    pub fn bit(self) -> u8 {
        1 << Display::ALL.iter().position(|d| *d == self).unwrap()
    }
}

/// How the landscape is drawn on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReliefMode {
    Off,
    /// Hill shading, tinted by height against the hero's.
    Shade,
    /// Contour lines: thin every 2 m, strong every 10 m.
    Contour,
    Both,
}

impl ReliefMode {
    pub const ALL: [ReliefMode; 4] = [ReliefMode::Off, ReliefMode::Shade, ReliefMode::Contour, ReliefMode::Both];

    /// As kept in the settings file.
    pub fn key(self) -> &'static str {
        match self {
            ReliefMode::Off => "off",
            ReliefMode::Shade => "shade",
            ReliefMode::Contour => "contour",
            ReliefMode::Both => "both",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ReliefMode::Off => tr!("끔"),
            ReliefMode::Shade => tr!("음영"),
            ReliefMode::Contour => tr!("등고선"),
            ReliefMode::Both => tr!("둘 다"),
        }
    }

    pub fn shade(self) -> bool {
        matches!(self, ReliefMode::Shade | ReliefMode::Both)
    }

    pub fn contour(self) -> bool {
        matches!(self, ReliefMode::Contour | ReliefMode::Both)
    }
}

/// Where the map stands and faces: the hero's position and the camera's yaw.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub center: Point,
    pub yaw_deg: f32,
    pub heading_up: bool,
    /// Pixels per centimetre.
    pub scale: f32,
    /// The world yaw the game calls north (degrees). In Hell Is Us that is 270: what
    /// the game's own compass shows as north lies along −Y.
    pub north_deg: f32,
    /// Drawn as outlines on a clear background (as Diablo's overlay map): no disc, no
    /// fills — walls, contours and shores as lines, so the game shows through.
    pub outline: bool,
}

impl View {
    /// A world point as an offset in pixels from the map's centre, y down.
    pub fn project(&self, p: Point) -> (f32, f32) {
        let (dx, dy) = (p[0] - self.center[0], p[1] - self.center[1]);
        // Whatever is up on the map: the camera's facing, or north.
        let up = if self.heading_up { self.yaw_deg } else { self.north_deg };
        let (s, c) = up.to_radians().sin_cos();
        let forward = dx * c + dy * s;
        let right = -dx * s + dy * c;
        (right * self.scale, -forward * self.scale)
    }

    /// The world X/Y under a pixel offset from the map's centre — `project` undone.
    pub fn unproject(&self, x: f32, y: f32) -> [f32; 2] {
        let up = if self.heading_up { self.yaw_deg } else { self.north_deg };
        let (s, c) = up.to_radians().sin_cos();
        let (right, forward) = (x / self.scale, -y / self.scale);
        [self.center[0] + forward * c - right * s, self.center[1] + forward * s + right * c]
    }

    /// Which way the hero's arrow points on the map, as a unit vector, y down.
    pub fn heading(&self) -> (f32, f32) {
        if self.heading_up {
            (0.0, -1.0)
        } else {
            let (s, c) = (self.yaw_deg - self.north_deg).to_radians().sin_cos();
            (s, -c)
        }
    }

    /// Which way north is on the map, as a unit vector, y down.
    pub fn north(&self) -> (f32, f32) {
        let (s, c) = self.north_deg.to_radians().sin_cos();
        let (x, y) = self.project([self.center[0] + c, self.center[1] + s, self.center[2]]);
        let len = (x * x + y * y).sqrt().max(f32::EPSILON);
        (x / len, y / len)
    }
}
