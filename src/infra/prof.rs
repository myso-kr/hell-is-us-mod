//! Where a thread's time goes: named spans timed as they close, and a summary of each
//! span's calls, mean and worst for the log, every so often. The worker's step is made
//! of readings that differ by orders of magnitude (a pose in microseconds, the guide in
//! hundreds of milliseconds); this says which is which in play.

use std::cell::RefCell;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct Tally {
    name: &'static str,
    calls: u32,
    total: Duration,
    worst: Duration,
}

thread_local! {
    static TALLIES: RefCell<(Option<Instant>, Vec<Tally>)> = const { RefCell::new((None, Vec::new())) };
}

/// A span of the thread's time, counted under `name` when it is dropped.
pub struct Span(&'static str, Instant);

pub fn span(name: &'static str) -> Span {
    Span(name, Instant::now())
}

impl Drop for Span {
    fn drop(&mut self) {
        add(self.0, self.1.elapsed());
    }
}

fn add(name: &'static str, took: Duration) {
    TALLIES.with(|t| {
        let (since, tallies) = &mut *t.borrow_mut();
        since.get_or_insert_with(Instant::now);
        match tallies.iter_mut().find(|x| x.name == name) {
            Some(x) => {
                x.calls += 1;
                x.total += took;
                x.worst = x.worst.max(took);
            }
            None => tallies.push(Tally { name, calls: 1, total: took, worst: took }),
        }
    });
}

/// Once `every` has passed since the first span, the summary headed `what` (and the
/// tallies start over): `name calls×mean/worst ms`, in the order the spans first closed.
pub fn report(what: &str, every: Duration) -> Option<String> {
    TALLIES.with(|t| {
        let (since, tallies) = &mut *t.borrow_mut();
        if since.is_none_or(|s| s.elapsed() < every) {
            return None;
        }
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let line = tallies
            .iter()
            .map(|x| format!("{} {}×{:.1}/{:.0}", x.name, x.calls, ms(x.total) / x.calls as f64, ms(x.worst)))
            .collect::<Vec<_>>()
            .join(", ");
        *since = None;
        tallies.clear();
        Some(format!("{what} time (calls×mean/worst ms): {line}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tallies_and_reports_once_due() {
        add("a", Duration::from_millis(10));
        add("a", Duration::from_millis(30));
        add("b", Duration::from_millis(1));
        assert_eq!(report("worker", Duration::from_secs(3600)), None);
        let line = report("worker", Duration::ZERO).unwrap();
        assert_eq!(line, "worker time (calls×mean/worst ms): a 2×20.0/30, b 1×1.0/1");
        assert_eq!(report("worker", Duration::ZERO), None);
    }
}
