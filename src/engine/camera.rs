//! Where the camera a take holds keeps its settings, found by name: the three configs' distance
//! and field of view (exploration, combat, APC), and the camera mode's pivot, checks and blend.
//! Shared by the take's record (take_record.rs) and the rescue (rescue.rs).

use super::attached::Attached;

/// An FTransform's size (rotation, translation, scale: doubles, padded).
pub(super) const TRANSFORM: usize = 0x60;

pub(super) struct CameraAt {
    /// Each config's name, and where its distance and its field of view are.
    pub configs: Vec<(&'static str, Option<u64>, Option<u64>)>,
    /// The camera mode object, and where its pivot (the whole transform), its checks' byte and its
    /// two blend times are.
    pub mode: u64,
    pub pivot: Option<u64>,
    pub safety: Option<u64>,
    pub blend: Option<(u64, u64)>,
}

pub(super) const CONFIGS: [&str; 3] = ["ExplorationConfig", "CombatConfig", "APCConfig"];

impl CameraAt {
    pub fn find(a: &Attached) -> Result<CameraAt, String> {
        let (m, n) = (&a.game, &a.anchors.names);
        let pc = a.chain()?.controller(m, &a.anchors)?;
        let pcm = n.follow(m, pc, "PlayerCameraManager")?;
        let at = |o: u64, f: &str| n.field(m, o, f).map(|p| o + p.offset as u64);
        let configs = CONFIGS
            .into_iter()
            .filter_map(|name| {
                let c = n.follow(m, pcm, name).ok()?;
                Some((name, at(c, "DefaultDistanceFromPlayer"), at(c, "FieldOfView")))
            })
            .collect();
        let mode = n.follow(m, pcm, "CameraModeInstance")?;
        Ok(CameraAt {
            configs,
            mode,
            pivot: at(mode, "PivotToViewTarget"),
            safety: at(mode, "bValidateSafeLoc"),
            blend: at(mode, "PenetrationBlendInTime").zip(at(mode, "PenetrationBlendOutTime")),
        })
    }
}
