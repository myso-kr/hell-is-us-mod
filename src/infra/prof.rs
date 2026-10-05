//! Where a thread's time goes: named spans timed as they close, and a summary of each
//! span's calls, mean and worst for the log, every so often. The worker's step is made
//! of readings that differ by orders of magnitude (a pose in microseconds, the guide in
//! hundreds of milliseconds); this says which is which in play.
//!
//! Each span also counts the reads of the game's memory made on its thread while it was
//! open (`count_read`, called by every ReadProcessMemory): a call costs a trip into the
//! kernel, so how many a span makes says as much as how long it takes.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct Tally {
    name: &'static str,
    calls: u32,
    total: Duration,
    worst: Duration,
    reads: u64,
}

thread_local! {
    /// Reads of the game's memory made on this thread, ever.
    static READS: Cell<u64> = const { Cell::new(0) };
    static TALLIES: RefCell<(Option<Instant>, Vec<Tally>)> = const { RefCell::new((None, Vec::new())) };
}

/// One read of the game's memory on this thread (game/process.rs).
pub fn count_read() {
    READS.with(|r| r.set(r.get() + 1));
}

fn reads() -> u64 {
    READS.with(Cell::get)
}

/// A span of the thread's time, counted under `name` when it is dropped.
pub struct Span(&'static str, Instant, u64);

pub fn span(name: &'static str) -> Span {
    Span(name, Instant::now(), reads())
}

impl Drop for Span {
    fn drop(&mut self) {
        add_reads(self.0, self.1.elapsed(), reads() - self.2);
    }
}

/// `f`, timed as a span: what a lock waits, say.
pub fn timed<T>(name: &'static str, f: impl FnOnce() -> T) -> T {
    let _s = span(name);
    f()
}

#[cfg(test)]
fn add(name: &'static str, took: Duration) {
    add_reads(name, took, 0)
}

fn add_reads(name: &'static str, took: Duration, read: u64) {
    TALLIES.with(|t| {
        let (since, tallies) = &mut *t.borrow_mut();
        since.get_or_insert_with(Instant::now);
        match tallies.iter_mut().find(|x| x.name == name) {
            Some(x) => {
                x.calls += 1;
                x.total += took;
                x.worst = x.worst.max(took);
                x.reads += read;
            }
            None => tallies.push(Tally { name, calls: 1, total: took, worst: took, reads: read }),
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
            .map(|x| {
                let reads = if x.reads > 0 { format!(" [{} r]", x.reads / x.calls as u64) } else { String::new() };
                format!("{} {}×{:.1}/{:.0}{reads}", x.name, x.calls, ms(x.total) / x.calls as f64, ms(x.worst))
            })
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

    #[test]
    fn a_span_counts_the_reads_made_in_it() {
        {
            let _s = span("r");
            count_read();
            count_read();
        }
        let line = report("reads", Duration::ZERO).unwrap();
        assert!(line.contains("r 1×") && line.ends_with("[2 r]"), "{line}");
    }
}
