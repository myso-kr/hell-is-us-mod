//! Log formatting. Everything this tool says goes through these two macros.
//!
//! They never panic. `println!` does when its write fails, and the panel gives up
//! its console when started by double-click — after which a log line is an abort
//! with nothing to say why.

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stdout(), "[hiumod] {}", format!($($t)*));
    }};
}

#[macro_export]
macro_rules! warn {
    ($($t:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), "[hiumod] WARN {}", format!($($t)*));
    }};
}
