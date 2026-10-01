//! Entry point. It reads the arguments and assembles the run order — nothing else.

use hiumod::attr::ANY;
use hiumod::cheats::{self, Active};
use hiumod::cli::{self, Command, Options};
use hiumod::engine::{attach, Engine};
use hiumod::game::locate;
use hiumod::game::process;
use hiumod::{log, warn};
use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::time::Duration;

type R = Result<(), String>;

fn main() -> ExitCode {
    let opt = match cli::parse(std::env::args().skip(1)) {
        Ok(cli::Parsed::Run(o)) => o,
        Ok(cli::Parsed::Help) => {
            println!("{}", cli::USAGE);
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            warn!("{e}");
            eprintln!("{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };
    let result = match &opt.command {
        Command::Ui(launch) => hiumod::ui::run(*launch),
        Command::Doctor => doctor(&opt),
        Command::List => read(&[]),
        Command::Get(names) => read(names),
        Command::Set(name, v) => set(name, *v),
        Command::Hold(names) => hold(names),
        Command::Restore => restore(),
        Command::Pose => pose(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            warn!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn doctor(opt: &Options) -> R {
    let ok = |b: bool| if b { "ok  " } else { "FAIL" };
    match locate::find(opt.game_dir.as_deref()) {
        Ok(i) => log!("{} install {} — Steam build {}", ok(true), i.dir.display(), i.version),
        Err(e) => log!("{} install: {e}", ok(false)),
    }

    let a = match attach() {
        Ok(a) => a,
        Err(e) => {
            log!("{} {e}", ok(false));
            return Ok(());
        }
    };
    log!("{} process {} — base 0x{:X}, image 0x{:X} bytes", ok(true), a.game.pid, a.game.base, a.game.size);
    let an = &a.anchors;
    log!("{} name pool +0x{:X}", ok(true), an.names_rva);
    log!("{} GEngine +0x{:X}", ok(true), an.gengine_rva);
    let l = an.names.layout;
    log!(
        "{} FField layout: next +0x{:X}, name +0x{:X}, size +0x{:X}, offset +0x{:X}",
        ok(true),
        l.next,
        l.name,
        l.size,
        l.offset
    );

    let chain = match a.chain() {
        Ok(c) => c,
        Err(e) => {
            log!("{} player: {e}", ok(false));
            return Ok(());
        }
    };
    log!("{} hero: {}", ok(true), chain.classes.join(" < "));
    log!(
        "{} chain: GEngine {:X?} → controller +0x{:X} pawn → +0x{:X} ASC → +0x{:X} SpawnedAttributes",
        ok(true),
        chain.to_controller,
        chain.pawn,
        chain.asc,
        chain.sets
    );
    match a.gate() {
        Ok(()) => log!("ok   hero gate: open — writes allowed"),
        Err(e) => log!("FAIL {e}"),
    }
    match a.pose() {
        Ok((p, yaw)) => log!("ok   pose: ({:.0}, {:.0}, {:.0}) yaw {yaw:.1}", p[0], p[1], p[2]),
        Err(e) => log!("FAIL pose: {e}"),
    }

    let s = match a.session() {
        Ok(s) => s,
        Err(e) => {
            log!("{} attributes: {e}", ok(false));
            return Ok(());
        }
    };
    let count: usize = s.sets.iter().map(|x| x.attributes.len()).sum();
    log!(
        "{} attributes: {} sets, {count} attributes by name, vtable +0x{:X}",
        ok(true),
        s.sets.len(),
        s.vtable - a.game.base
    );
    for set in &s.sets {
        println!("        {:<36} {} attributes", set.class, set.attributes.len());
    }

    let missing: Vec<String> =
        cheats::attributes().into_iter().filter(|x| !s.has(*x)).map(|x| format!("{}.{}", x.set, x.name)).collect();
    if missing.is_empty() {
        log!("{} cheat table: all {} attributes it names are in the game", ok(true), cheats::attributes().len());
    } else {
        log!("{} cheat table: not in the game — {}", ok(false), missing.join(", "));
    }
    let marks = hiumod::verify::load();
    for c in cheats::CHEATS {
        let state = if c.verified { "verified" } else { "unverified" };
        let tried = match marks.get(c.id) {
            Some(true) => "player: works",
            Some(false) => "player: DOES NOT WORK",
            None => "",
        };
        let sets: Vec<String> = c
            .effects()
            .iter()
            .flat_map(|e| match *e {
                cheats::Effect::Fixed(x, _) | cheats::Effect::Chosen(x) => vec![x],
                cheats::Effect::Fill(x, y) => vec![x, y],
            })
            .map(|x| match (x.set == ANY).then(|| s.set_of(x)).flatten() {
                Some(set) => format!("{set}.{}", x.name),
                None => format!("{}.{}", x.set, x.name),
            })
            .collect();
        println!("        {:<16} {:<10} {:<22} {} [{}]", c.id, state, tried, c.label, sets.join(", "));
    }
    Ok(())
}

/// Where the hero is, twice a second until Ctrl+C — what the minimap will draw.
fn pose() -> R {
    let a = attach()?;
    process::catch_ctrl_c();
    while !process::STOP.load(Ordering::SeqCst) {
        match a.pose() {
            Ok((p, yaw)) => log!("x {:>10.0}  y {:>10.0}  z {:>8.0}  yaw {:>7.1}", p[0], p[1], p[2], yaw),
            Err(e) => warn!("{e}"),
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Ok(())
}

/// Every attribute of every set — or only those named by `filter`, as `Name` or
/// `Set.Name`, case ignored.
fn read(filter: &[String]) -> R {
    let a = attach()?;
    let s = a.session()?;
    let wanted = |set: &str, name: &str| {
        filter.is_empty()
            || filter.iter().any(|f| f.eq_ignore_ascii_case(name) || f.eq_ignore_ascii_case(&format!("{set}.{name}")))
    };
    let mut shown = 0;
    for set in &s.sets {
        let rows: Vec<_> = set.attributes.iter().filter(|(n, _)| wanted(&set.class, n)).collect();
        if rows.is_empty() {
            continue;
        }
        println!("{}", set.class);
        for (name, _) in rows {
            shown += 1;
            match s.read_named(&set.class, name) {
                Some((b, c)) if b == c => println!("  {name:<44} {c:>12.3}"),
                Some((b, c)) => println!("  {name:<44} {c:>12.3}   (base {b:.3})"),
                None => println!("  {name:<44} {:>12}", "—"),
            }
        }
    }
    if shown == 0 {
        return Err(format!("no attribute called {}", filter.join(", ")));
    }
    Ok(())
}

fn set(name: &str, v: f32) -> R {
    let a = attach()?;
    a.gate()?;
    let s = a.session()?;
    cheats::set_value(&s, name, v)?;
    log!("{name} = {v}");
    Ok(())
}

fn hold(names: &[String]) -> R {
    let toggles = names.iter().map(|n| Active::parse(n)).collect::<Result<Vec<_>, _>>()?;
    let mut engine = Engine::new()?;
    if engine.pending() > 0 {
        log!("{} original(s) left by an earlier hold — keeping those, not the current values", engine.pending());
    }
    engine.set_active(toggles.clone())?;
    process::catch_ctrl_c();
    let held: Vec<String> = toggles
        .iter()
        .map(|t| format!("{}{}", t.cheat, if t.value != 0.0 { format!("={}", t.value) } else { String::new() }))
        .collect();
    log!("holding {} — Ctrl+C to stop and put things back", held.join(", "));
    log!("if this is killed instead, `hiumod restore` puts things back");

    let mut last = String::new();
    let ended = loop {
        if process::STOP.load(Ordering::SeqCst) {
            break Ok(());
        }
        let snap = engine.step();
        let now = snap.notice.clone().unwrap_or_default();
        if snap.active.is_empty() {
            break Err(if now.is_empty() { "toggles stopped".into() } else { now });
        }
        if now != last {
            if now.is_empty() {
                log!("applying")
            } else {
                warn!("{now}")
            }
            last = now;
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    match engine.stop() {
        Ok(()) => log!("originals restored"),
        Err(e) => warn!("{e}"),
    }
    ended
}

/// Put back what a killed hold left changed.
fn restore() -> R {
    let mut engine = Engine::new()?;
    if engine.pending() == 0 {
        log!("nothing to restore");
        return Ok(());
    }
    engine.stop()?;
    log!("originals restored");
    Ok(())
}
