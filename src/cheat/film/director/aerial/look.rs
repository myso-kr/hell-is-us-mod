//! How a flight looks at a stretch of the land, and from how high: each look's pitch, its turn from
//! the way ahead, its lens and its height over the path, as they go through the stretch (drone
//! moves: the glide, the rising reveal, the pedestal up, the tilt up to the horizon, the skim low
//! over the ground, the side track, the overhead — .spec/FILMING-RESEARCH.md §2).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aerial {
    /// Level with the way, a little down: the land going by.
    Glide,
    /// Starting low and tilted down, the horizon coming up as the camera climbs (a reveal).
    Rise,
    /// Tilted down after the land as it falls away, coming down with it.
    Dive,
    /// Looking into a turn before the path takes it.
    Bank,
    /// Straight down from high over open ground.
    Overhead,
    /// The end: coming down onto it, the lens closing in.
    Arrive,
    /// Low and fast over the ground, wide.
    Skim,
    /// Flying on while looking out to one side: the land tracked past.
    Track,
    /// Straight up as it goes (a pedestal), tilting down to keep the land.
    Ascend,
    /// Looking down at the ground, tilting up until the horizon fills the frame.
    TiltUp,
    /// Coming down from high, levelling out.
    Descend,
    /// Soaring high over open ground, looking about from side to side: an eagle, hunting (what it
    /// sees below, the lens closes in on: the flight's glance).
    Eagle,
    /// Climbing in a corkscrew, the land turning round below.
    Spiral,
    /// Flying on looking back: the way behind falling away.
    LookBack,
    /// A point ahead to the side held as it is passed, the camera turning after it.
    FlyBy,
    /// Climbing as the lens closes in (a dolly zoom from the air): the land below stretching.
    Vertigo,
}

/// A look: pitch, turn from the way ahead (degrees), field of view (degrees), height over the
/// flight's path (cm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub pitch: f32,
    pub yaw: f32,
    pub fov: f32,
    pub lift: f32,
}

impl Look {
    pub fn mix(self, o: Look, k: f32) -> Look {
        let l = |a: f32, b: f32| a + (b - a) * k;
        Look {
            pitch: l(self.pitch, o.pitch),
            yaw: self.yaw + crate::film::wrap(o.yaw - self.yaw) * k,
            fov: l(self.fov, o.fov),
            lift: l(self.lift, o.lift),
        }
    }
}

impl Aerial {
    pub fn label(self) -> &'static str {
        match self {
            Aerial::Glide => tr!("AERIAL_GLIDE"),
            Aerial::Rise => tr!("AERIAL_RISE"),
            Aerial::Dive => tr!("AERIAL_DIVE"),
            Aerial::Bank => tr!("AERIAL_BANK"),
            Aerial::Overhead => tr!("AERIAL_OVERHEAD"),
            Aerial::Arrive => tr!("AERIAL_ARRIVE"),
            Aerial::Skim => tr!("AERIAL_SKIM"),
            Aerial::Track => tr!("AERIAL_TRACK"),
            Aerial::Ascend => tr!("AERIAL_ASCEND"),
            Aerial::TiltUp => tr!("AERIAL_TILT_UP"),
            Aerial::Descend => tr!("AERIAL_DESCEND"),
            Aerial::Eagle => tr!("AERIAL_EAGLE"),
            Aerial::Spiral => tr!("AERIAL_SPIRAL"),
            Aerial::LookBack => tr!("AERIAL_LOOK_BACK"),
            Aerial::FlyBy => tr!("AERIAL_FLY_BY"),
            Aerial::Vertigo => tr!("AERIAL_VERTIGO"),
        }
    }

    /// Its look `u` of the way through it (0–1), `side` the way it turns or looks (+1 left).
    pub fn look(self, u: f32, side: f32) -> Look {
        let u = u.clamp(0.0, 1.0);
        let e = u * u * (3.0 - 2.0 * u);
        let lerp = |a: f32, b: f32| a + (b - a) * e;
        let look = |pitch, yaw, fov, lift| Look { pitch, yaw, fov, lift };
        match self {
            Aerial::Glide => look(-10.0, 0.0, 74.0, 350.0),
            Aerial::Rise => look(lerp(-40.0, -8.0), 0.0, lerp(68.0, 82.0), lerp(150.0, 1400.0)),
            Aerial::Dive => look(lerp(-18.0, -34.0), 0.0, 72.0, lerp(1200.0, 300.0)),
            Aerial::Bank => look(-14.0, 22.0 * side, 76.0, 600.0),
            Aerial::Overhead => look(lerp(-40.0, -62.0), 0.0, lerp(74.0, 80.0), lerp(1500.0, 2200.0)),
            Aerial::Arrive => look(lerp(-10.0, -32.0), 0.0, lerp(76.0, 58.0), lerp(700.0, 60.0)),
            Aerial::Skim => look(-4.0, 0.0, 84.0, 40.0),
            Aerial::Track => look(-14.0, 70.0 * side, 66.0, 500.0),
            Aerial::Ascend => look(lerp(-8.0, -35.0), 0.0, 74.0, lerp(200.0, 1800.0)),
            Aerial::TiltUp => look(lerp(-65.0, -6.0), 0.0, lerp(70.0, 80.0), 900.0),
            Aerial::Descend => look(lerp(-40.0, -10.0), 0.0, 74.0, lerp(1600.0, 250.0)),
            Aerial::Eagle => {
                // up into the sky, then sweeping left and right, slowly
                let sweep = (u * std::f32::consts::TAU * 1.2).sin() * 75.0 * side;
                look(-28.0, sweep * e.min(1.0), 72.0, lerp(1800.0, 3000.0))
            }
            Aerial::Spiral => look(-22.0, 360.0 * e * side, 76.0, lerp(300.0, 1600.0)),
            Aerial::LookBack => look(lerp(-32.0, -12.0), 180.0, lerp(62.0, 80.0), lerp(400.0, 1200.0)),
            Aerial::FlyBy => look(-12.0, lerp(25.0, 160.0) * side, 64.0, 320.0),
            Aerial::Vertigo => look(lerp(-15.0, -45.0), 0.0, lerp(80.0, 40.0), lerp(300.0, 1500.0)),
        }
    }
}
