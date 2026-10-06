//! Directing about the hero, shared by the walk and the live take: what the hero does is judged
//! (director/situation.rs) and coverage chooses the shot (director/coverage.rs) — the route's
//! planned shot among the candidates while the hero goes along it.

use super::session::Session;
use crate::film::{judge, openness, Coverage, Ctx, Cue, Motion, Rig};

#[derive(Default)]
pub(super) struct Cover {
    motion: Motion,
    coverage: Coverage,
    /// How open the place is, looked at now and then (it costs eight rays), and when next.
    open: (f32, f32),
}

impl Cover {
    /// The cue for the hero at `hero` going `heading`, the route's shot here if any; `live` when
    /// the player plays (things to interact with left to them, the controls kept while moving).
    pub fn cue(
        &mut self,
        s: &Session,
        hero: [f32; 3],
        heading: f32,
        planned: Option<(Rig, &'static str)>,
        live: bool,
        dt: f32,
    ) -> Cue {
        self.motion.see(hero, dt);
        let subjects = s.plan.subjects.as_ref().map(|f| f.lock().unwrap().clone()).unwrap_or_default();
        let heading = if live { self.motion.heading().unwrap_or(heading) } else { heading };
        self.open.1 -= dt;
        if self.open.1 <= 0.0 {
            self.open = (openness(&s.blocking, hero), 0.5);
        }
        let sight = judge(&self.motion, hero, heading, &subjects, self.open.0, !live);
        let ctx = Ctx {
            hero,
            heading,
            body: s.body_yaw().unwrap_or(heading),
            sight,
            planned,
            live,
            b: &s.blocking,
            cuts: s.plan.cuts,
            dt,
        };
        self.coverage.cue(&ctx)
    }
}
