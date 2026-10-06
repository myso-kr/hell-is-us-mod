//! Getting the hero out of the ground (`hiumod rescue`): a take that ended without putting things
//! back — the panel closed mid-flight — can leave the hero shrunk and sunk where it was carried.
//! Its mesh is made its own size again, and it is set on the nearest floor it can walk: the
//! navmesh's straight above it (it sank), else about it, else the landscape's. And the camera the
//! take held is put back: the camera mode's pivot, checks and blend from its own default object;
//! the configs' distance and field of view to their values at rest as probed (484 cm, 70°) —
//! the combat and APC configs' own are not known, and a restart of the game brings them back.

use super::{Engine, ABOVE};
use crate::mem::Memory;

/// How far above the hero the floor is looked for, and in what steps (cm); a mesh smaller than this
/// share of its size was shrunk by a flight.
const LOOK_UP: f32 = 4000.0;
const STEP: f32 = 100.0;
const SHRUNK: f64 = 0.01;
/// The exploration camera's distance and field of view at rest (probed on 24045435, .spec/CHEATS.md).
const REST_DISTANCE: f32 = 484.0;
const REST_FOV: f32 = 70.0;
/// An FTransform's size (rotation, translation, scale: doubles, padded).
const TRANSFORM: usize = 0x60;

impl Engine {
    /// The hero out of the ground: where it was put, and whether its size was put back.
    pub fn rescue(&mut self) -> Result<([f64; 3], bool), String> {
        self.refresh()?;
        let a = self.attached.as_ref().unwrap();
        let (m, n) = (&a.game, &a.anchors.names);
        let chain = a.chain()?;
        let hero = chain.hero(m, &a.anchors)?;
        // its size: a flight shrank it out of sight
        let mut resized = false;
        if let Some(at) = n
            .follow(m, hero, "Mesh")
            .ok()
            .and_then(|mesh| n.field(m, mesh, "RelativeScale3D").map(|v| mesh + v.offset as u64))
        {
            let mut b = [0u8; 24];
            if m.read(at, &mut b) {
                let s = [0, 1, 2].map(|i| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap()));
                if s.iter().all(|v| v.is_finite() && *v < SHRUNK) {
                    let one: Vec<u8> = [1.0f64; 3].iter().flat_map(|v| v.to_le_bytes()).collect();
                    resized = m.write(at, &one);
                }
            }
        }
        let (p, _) = a.pose()?;
        let feet = [p[0] as f32, p[1] as f32, p[2] as f32 - crate::film::FEET];
        let nav = a.nav();
        // the walkable floor straight above (it sank), the nearest first; else the landscape
        let up = (0..=(LOOK_UP / STEP) as i32).find_map(|k| {
            let probe = [feet[0], feet[1], feet[2] + k as f32 * STEP];
            nav.locate(probe).map(|(_, q)| q).filter(|q| q[2] >= feet[2] - STEP)
        });
        let ground = up.or_else(|| {
            let t = a.obstacles().terrain.height(feet[0], feet[1])?;
            Some([feet[0], feet[1], t])
        });
        let Some(g) = ground else { return Err(tr!("RESCUE_NO_FLOOR").into()) };
        let at = [g[0] as f64, g[1] as f64, (g[2] + crate::film::FEET) as f64 + ABOVE];
        a.teleport(at)?;
        Ok((at, resized))
    }

    /// The camera as it is at rest: what a take that never ended left held. How many fields were
    /// put back.
    pub fn rescue_camera(&mut self) -> Result<usize, String> {
        self.refresh()?;
        let a = self.attached.as_ref().unwrap();
        let (m, n) = (&a.game, &a.anchors.names);
        let chain = a.chain()?;
        let pc = chain.controller(m, &a.anchors)?;
        let pcm = n.follow(m, pc, "PlayerCameraManager")?;
        let mut put = 0;
        // the configs: distance and field of view at rest
        for name in ["ExplorationConfig", "CombatConfig", "APCConfig"] {
            let Ok(c) = n.follow(m, pcm, name) else { continue };
            for (field, v) in [("DefaultDistanceFromPlayer", REST_DISTANCE), ("FieldOfView", REST_FOV)] {
                if let Some(f) = n.field(m, c, field) {
                    put += m.write(c + f.offset as u64, &v.to_le_bytes()) as usize;
                }
            }
        }
        // the camera mode: from its class's default object
        let mode = n.follow(m, pcm, "CameraModeInstance")?;
        let class = n.class(m, mode).ok_or("the camera mode has no class")?;
        let objects = crate::gobjects::discover(m, a.game.base)?;
        let default = format!("Default__{class}");
        let cdo = objects
            .all(m)
            .into_iter()
            .find(|&o| n.object(m, o).as_deref() == Some(default.as_str()))
            .ok_or_else(|| format!("no {default}"))?;
        for (field, len) in [
            ("PivotToViewTarget", TRANSFORM),
            ("bValidateSafeLoc", 1),
            ("PenetrationBlendInTime", 4),
            ("PenetrationBlendOutTime", 4),
        ] {
            let Some(f) = n.field(m, mode, field) else { continue };
            let mut b = vec![0u8; len];
            if m.read(cdo + f.offset as u64, &mut b) {
                put += m.write(mode + f.offset as u64, &b) as usize;
            }
        }
        Ok(put)
    }
}
