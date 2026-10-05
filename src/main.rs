//! Entry point. It reads the arguments and assembles the run order — nothing else.

use hiumod::attr::ANY;
use hiumod::cheats::{self, Active};
use hiumod::cli::{self, Command, Options};
use hiumod::engine::{attach, Engine};
use hiumod::game::locate;
use hiumod::game::process;
use hiumod::{log, warn};
use hiumod::{tr, trf};
use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::time::Duration;

type R = Result<(), String>;

fn main() -> ExitCode {
    // Messages in the game's language, as in the panel (.spec/I18N.md).
    hiumod::i18n::follow_game();
    let opt = match cli::parse(std::env::args().skip(1)) {
        Ok(cli::Parsed::Run(o)) => o,
        Ok(cli::Parsed::Help) => {
            println!("{}", cli::usage());
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            warn!("{e}");
            eprintln!("{}", cli::usage());
            return ExitCode::from(2);
        }
    };
    let result = match &opt.command {
        Command::Ui(launch) => hiumod::ui::run(*launch),
        Command::Doctor => doctor(&opt),
        Command::Probe(args) => probe(args),
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
        Ok(i) => log!("{}", trf!("INSTALL_STEAM_BUILD", ok = ok(true), dir = i.dir.display(), build = i.version)),
        Err(e) => log!("{}", trf!("INSTALL", ok = ok(false), e = e)),
    }

    let a = match attach() {
        Ok(a) => a,
        Err(e) => {
            log!("{} {e}", ok(false));
            return Ok(());
        }
    };
    log!(
        "{}",
        trf!(
            "PROCESS_BASE_0X_IMAGE_0X_BYTES",
            ok = ok(true),
            pid = a.game.pid,
            base = format!("{:X}", a.game.base),
            size = format!("{:X}", a.game.size)
        )
    );
    let an = &a.anchors;
    log!("{}", trf!("NAME_POOL_0X", ok = ok(true), at = format!("{:X}", an.names_rva)));
    log!("{} GEngine +0x{:X}", ok(true), an.gengine_rva);
    let l = an.names.layout;
    log!(
        "{}",
        trf!(
            "FFIELD_LAYOUT_NEXT_0X_NAME_0X",
            ok = ok(true),
            next = format!("{:X}", l.next),
            name = format!("{:X}", l.name),
            size = format!("{:X}", l.size),
            offset = format!("{:X}", l.offset)
        )
    );

    let chain = match a.chain() {
        Ok(c) => c,
        Err(e) => {
            log!("{}", trf!("PLAYER", ok = ok(false), e = e));
            return Ok(());
        }
    };
    log!("{}", trf!("HERO", ok = ok(true), classes = chain.classes.join(" < ")));
    log!(
        "{}",
        trf!(
            "CHAIN_GENGINE_CONTROLLER_0X_PAWN_0X",
            ok = ok(true),
            path = format!("{:X?}", chain.to_controller),
            pawn = format!("{:X}", chain.pawn),
            asc = format!("{:X}", chain.asc),
            sets = format!("{:X}", chain.sets)
        )
    );
    match a.gate() {
        Ok(()) => log!("{}", tr!("OK_HERO_GATE_OPEN_WRITES_ALLOWED")),
        Err(e) => log!("FAIL {e}"),
    }
    match a.pose() {
        Ok((p, yaw)) => log!("{}", trf!("OK_POSE_YAW", x = p[0], y = p[1], z = p[2], yaw = yaw)),
        Err(e) => log!("{}", trf!("FAIL_POSE", e = e)),
    }

    let started = std::time::Instant::now();
    match a.things() {
        Ok(t) => {
            let count = |k: hiumod::actors::Kind| t.iter().filter(|x| x.kind() == k).count();
            let kinds: Vec<String> =
                hiumod::actors::Kind::ALL.iter().map(|&k| format!("{} {}", k.label(), count(k))).collect();
            log!(
                "{}",
                trf!(
                    "OK_MINIMAP_THINGS_IN_MS",
                    n = t.len(),
                    ms = started.elapsed().as_millis(),
                    kinds = kinds.join(", ")
                )
            );
            let subs: Vec<String> = hiumod::actors::Sub::ALL
                .iter()
                .filter_map(|&s| {
                    let n = t.iter().filter(|x| x.sub == s).count();
                    (n > 0).then(|| format!("{} {n}", s.label()))
                })
                .collect();
            println!("        {}", subs.join(", "));
        }
        Err(e) => log!("{}", trf!("FAIL_MINIMAP_THINGS", e = e)),
    }

    let started = std::time::Instant::now();
    match a.goals() {
        Ok((g, k)) => {
            let count = |t: hiumod::goals::Tier| g.iter().filter(|x| x.tier == t).count();
            log!(
                "{}",
                trf!(
                    "OK_GUIDE_KNOWS_FACTS_TAGS_OPEN",
                    facts = k.facts.len(),
                    tags = k.tags.len(),
                    open = k.quest_names.join(", "),
                    places = g.len(),
                    ms = started.elapsed().as_millis(),
                    quest = count(hiumod::goals::Tier::Quest),
                    secret = count(hiumod::goals::Tier::Secret),
                    clue = count(hiumod::goals::Tier::Clue)
                )
            );
            let (p, _) = a.pose().unwrap_or(([0.0; 3], 0.0));
            let mut near: Vec<_> = g
                .iter()
                .map(|x| (((x.at[0] as f64 - p[0]).powi(2) + (x.at[1] as f64 - p[1]).powi(2)).sqrt() / 100.0, x))
                .collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (d, x) in near.iter().take(12) {
                println!("        {:>6.0} m  {:<10} {:<44} {}", d, x.tier.label(), x.label, x.detail);
            }
            // Every goal, for when one points somewhere unexpected: where, what, and why.
            let all: String = near
                .iter()
                .map(|(d, x)| {
                    format!(
                        "{d:>7.0} m  {:<10} {:<40} at {:.0},{:.0},{:.0}  id {:X}  {}  tags {:?}  keys {:?}
",
                        x.tier.label(),
                        x.label,
                        x.at[0],
                        x.at[1],
                        x.at[2],
                        x.id,
                        x.detail,
                        x.tags,
                        x.keys
                    )
                })
                .collect();
            let dir = hiumod::paths::data_dir().join("doctor");
            if std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("goals.txt"), all)).is_ok() {
                println!("        {}", dir.join("goals.txt").display());
            }
        }
        Err(e) => log!("{}", trf!("FAIL_GUIDE", e = e)),
    }
    // The choice puzzles (slots.rs) as the survey groups them: each set's slots, and
    // whether one is right. Answers are not printed.
    for p in hiumod::slots::group(&a.catalogue(), &Default::default()) {
        let sets: Vec<String> = p
            .sets
            .iter()
            .map(|s| format!("{}{}", s.slots.len(), if s.answer().is_some() { "" } else { "*" }))
            .collect();
        println!("        {} · {}: [{}]", p.world, p.items.join(", "), sets.join(" "));
    }
    if let Ok(w) = a.chain().and_then(|c| c.world(&a.game, &a.anchors)) {
        println!("        world {w}");
    }
    // The choice slots loaded near the hero as the panel reads them: the actor's place,
    // its slot's (the groove), and what the slot holds. The quest pass finds them, so the
    // engine runs a few seconds first, only with one within 60 m.
    let near = a.pose().ok().is_some_and(|(h, _)| {
        a.catalogue()
            .iter()
            .any(|(p, _)| p.choice.is_some() && (p.at[0] - h[0] as f32).hypot(p.at[1] - h[1] as f32) < 6_000.0)
    });
    if let Some(mut engine) = near.then(hiumod::engine::Engine::new).and_then(Result::ok) {
        let until = std::time::Instant::now() + Duration::from_secs(12);
        let mut snap = engine.step();
        while std::time::Instant::now() < until && !snap.puzzles.iter().any(|p| p.class.contains("PuzzleCheck")) {
            std::thread::sleep(Duration::from_millis(100));
            snap = engine.step();
        }
        for p in snap.puzzles.iter().filter(|p| p.class.contains("PuzzleCheck")) {
            let short = p.class.split('_').next().unwrap_or("");
            let at = |v: &[f32; 3]| format!("{:.0},{:.0},{:.0}", v[0], v[1], v[2]);
            let slots: Vec<String> = p.slot_at.iter().map(at).collect();
            println!("        {short} at {} slot {:?} holds {:?}", at(&p.at), slots, p.placed);
        }
    }
    // The choice slots loaded near the hero, as the panel reads them: where, and what
    // each of their slots holds.
    if let Ok((h, _)) = a.pose() {
        for p in a
            .puzzles([h[0] as f32, h[1] as f32, h[2] as f32], 4_000.0)
            .iter()
            .filter(|p| p.class.contains("PuzzleCheck"))
        {
            let short = p.class.split('_').next().unwrap_or("");
            println!("        {short} at {:.0},{:.0},{:.0} holds {:?}", p.at[0], p.at[1], p.at[2], p.placed);
        }
    }

    let s = match a.session() {
        Ok(s) => s,
        Err(e) => {
            log!("{}", trf!("DOCTOR_ATTRIBUTES_FAIL", ok = ok(false), e = e));
            return Ok(());
        }
    };
    let count: usize = s.sets.iter().map(|x| x.attributes.len()).sum();
    log!(
        "{}",
        trf!(
            "ATTRIBUTES_SETS_ATTRIBUTES_BY_NAME_VTABLE",
            ok = ok(true),
            sets = s.sets.len(),
            count = count,
            vt = format!("{:X}", s.vtable - a.game.base)
        )
    );
    for set in &s.sets {
        println!("        {:<36} {}", set.class, trf!("ATTRIBUTE_COUNT", n = set.attributes.len()));
    }

    let missing: Vec<String> =
        cheats::attributes().into_iter().filter(|x| !s.has(*x)).map(|x| format!("{}.{}", x.set, x.name)).collect();
    if missing.is_empty() {
        log!("{}", trf!("CHEAT_TABLE_ALL_ATTRIBUTES_IT_NAMES", ok = ok(true), n = cheats::attributes().len()));
    } else {
        log!("{}", trf!("CHEAT_TABLE_NOT_IN_THE_GAME", ok = ok(false), missing = missing.join(", ")));
    }
    let marks = hiumod::verify::load();
    for c in cheats::CHEATS {
        let state = if c.verified { tr!("VERIFIED") } else { tr!("UNVERIFIED") };
        let tried = match marks.get(c.id) {
            Some(true) => tr!("PLAYER_WORKS"),
            Some(false) => tr!("PLAYER_DOES_NOT_WORK"),
            None => "",
        };
        let sets: Vec<String> = c
            .effects()
            .iter()
            .flat_map(|e| match *e {
                cheats::Effect::Fixed(x, _) | cheats::Effect::Chosen(x) => vec![x],
                cheats::Effect::Fill(x, y) => vec![x, y],
                _ => vec![],
            })
            .map(|x| match (x.set == ANY).then(|| s.set_of(x)).flatten() {
                Some(set) => format!("{set}.{}", x.name),
                None => format!("{}.{}", x.set, x.name),
            })
            .collect();
        println!(
            "        {:<16} {:<10} {:<22} {} [{}]",
            c.id,
            state,
            tried,
            hiumod::i18n::tr(c.label),
            sets.join(", ")
        );
    }
    Ok(())
}

/// The built survey tool: beside the executable (`survey\survey.dll`), or in the source
/// tree when running from `target\release`.
fn survey_tool() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [
        dir.join("survey").join("survey.dll"),
        dir.join("..")
            .join("..")
            .join("tools")
            .join("survey")
            .join("bin")
            .join("Release")
            .join("net8.0")
            .join("survey.dll"),
    ]
    .into_iter()
    .find(|p| p.exists())
}

/// `doctor graph`: the requirement graph from the survey (graph.rs) — how much of the game
/// can be played to from nothing known, what cannot, and what no place gives. Reads only
/// the survey, not the game.
fn graph() -> R {
    let g = hiumod::graph::Graph::load(&hiumod::paths::data_dir().join("survey"));
    let r = g.reach();
    let mut out = format!("nodes {} · doable from nothing known {} · stuck {}\n", r.nodes, r.doable, r.stuck.len());
    let mut by_world: std::collections::BTreeMap<&str, (usize, usize)> = Default::default();
    for (i, n) in g.nodes.iter().enumerate() {
        let e = by_world.entry(n.world.as_str()).or_default();
        e.0 += 1;
        if r.stuck.contains(&i) {
            e.1 += 1;
        }
    }
    for (w, (all, stuck)) in &by_world {
        out += &format!("  {w}: {all} nodes, {stuck} stuck\n");
    }
    let mut missing: Vec<(&String, &usize)> = r.no_giver.iter().collect();
    missing.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    out += &format!("needed, given by no place ({}):\n", missing.len());
    for (what, n) in &missing {
        out += &format!("  {n:3}  {what}\n");
    }
    // Each quest's Datapad facts, and what gives them.
    let facts: Vec<String> = std::fs::read_to_string(hiumod::paths::data_dir().join("locale").join("facts.tsv"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split('\t').nth(1).map(str::to_string))
        .collect();
    let given = facts.iter().filter(|f| g.gives(f)).count();
    out += &format!("Datapad facts given by some node: {given}/{}\n", facts.len());
    // Those no node gives, by their kind (the sixth column: Name, Location, Quest…).
    let kinds: Vec<(String, String)> =
        std::fs::read_to_string(hiumod::paths::data_dir().join("locale").join("facts.tsv"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let c: Vec<&str> = l.split('\t').collect();
                Some((c.get(1)?.to_string(), c.get(5)?.to_string()))
            })
            .collect();
    let mut by_kind: std::collections::BTreeMap<&str, usize> = Default::default();
    for (_, k) in kinds.iter().filter(|(f, _)| !g.gives(f)) {
        *by_kind.entry(k.as_str()).or_default() += 1;
    }
    out += &format!("  not given, by kind: {by_kind:?}\n");
    out += "quests (facts given / all, by kind of giver):\n";
    for (q, all, kinds, none) in g.quest_coverage(&facts) {
        let given: usize = kinds.values().sum();
        let mut k: Vec<String> = kinds.iter().map(|(k, n)| format!("{k} {n}")).collect();
        k.sort();
        out += &format!("  {q}: {given}/{all} ({})\n", k.join(", "));
        for f in none.iter().take(6) {
            out += &format!("      not given: {f}\n");
        }
    }
    out += "stuck:\n";
    for &i in &r.stuck {
        let n = &g.nodes[i];
        out += &format!("  {} · {} · {:?}\n", n.world, n.class, n.needs);
    }
    print!("{out}");
    let dir = hiumod::paths::data_dir().join("doctor");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("graph.txt"), &out).map_err(|e| e.to_string())
}

/// `doctor inspect|find|dump|watch|scan|usmap|survey` (probe.rs): reads only.
fn probe(args: &[String]) -> R {
    use hiumod::mem::{self, Memory};
    use hiumod::probe;
    if args.first().map(String::as_str) == Some("profile") {
        return profile(args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30));
    }
    if args.first().map(String::as_str) == Some("graph") {
        return graph();
    }
    let a = attach()?;
    let (m, n) = (&a.game, &a.anchors.names);
    let objects = hiumod::gobjects::discover(m, a.game.base)?;
    let dir = hiumod::paths::data_dir().join("doctor");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let save = |file: &str, text: &str| -> R {
        let path = dir.join(file);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        log!("{}", trf!("WRITTEN_TO", path = path.display()));
        Ok(())
    };
    let arg = |i: usize| args.get(i).map(String::as_str);
    match arg(0) {
        Some("inspect") => {
            let what = arg(1).ok_or("inspect what? e.g. `doctor inspect hero`")?;
            let depth = arg(2).and_then(|d| d.parse().ok()).unwrap_or(0);
            let gaps = args.iter().any(|x| x == "gaps");
            let mut out = String::new();
            for obj in target(&a, &objects, what)? {
                let mut i = probe::Inspect::new(n, m, depth, gaps);
                i.object(obj, 0, depth);
                out.push_str(&i.out);
                out.push('\n');
            }
            print!("{out}");
            save(&format!("inspect-{}.txt", what.replace([':', '\\', '/'], "_")), &out)
        }
        Some("find") => {
            let text = arg(1).ok_or("find what? e.g. `doctor find Quantity`")?;
            let structs = probe::structs(n, m, &objects.all(m), &[]);
            let rows = probe::find(n, m, &structs, text);
            for r in &rows {
                println!("{r}");
            }
            log!("{}", trf!("PROPERTIES_IN_CLASSES_AND_STRUCTS", p = rows.len(), s = structs.len()));
            save(&format!("find-{text}.txt"), &rows.join("\n"))
        }
        Some("dump") => {
            let mut prefixes: Vec<String> = args[1..].to_vec();
            if prefixes.is_empty() {
                prefixes = vec!["Charlie".into(), "Story".into()];
            }
            let structs = probe::structs(n, m, &objects.all(m), &prefixes);
            let text = probe::dump(n, m, &structs);
            log!("{}", trf!("CLASSES_AND_STRUCTS_STARTING_WITH", n = structs.len(), prefix = prefixes.join(", ")));
            save("sdk.txt", &text)
        }
        Some(sub @ ("survey" | "locale" | "tables")) => {
            // Mappings first, then the survey tool over the game's maps (.spec/SURVEY.md),
            // then the game's text in every language (.spec/I18N.md); `locale` only that.
            let (bytes, structs, enums) = hiumod::usmap::build(m, n, &objects);
            std::fs::write(dir.join("HellIsUs.usmap"), &bytes).map_err(|e| e.to_string())?;
            log!("{}", trf!("MAPPINGS_STRUCTS_AND_CLASSES_ENUMS", structs = structs, enums = enums));
            let tool = survey_tool().ok_or(
                "tools/survey is not built: run `dotnet build -c Release` in tools/survey (needs the .NET 8 SDK)",
            )?;
            let game = hiumod::paths::data_dir().parent().ok_or("no game folder")?.to_path_buf();
            let run = |extra: &[&str]| -> Result<(), String> {
                let dotnet = hiumod::runtime::dotnet().ok_or(tr!("THE_SURVEY_NEEDS_THE_NET_8_RUNTIME"))?;
                let mut cmd = std::process::Command::new(dotnet);
                cmd.arg(&tool).arg("--game").arg(&game).args(extra);
                log!("{}", trf!("RUNNING", tool = tool.display(), args = extra.join(" ")));
                let status = cmd.status().map_err(|e| format!("dotnet: {e}"))?;
                status.success().then_some(()).ok_or(format!("the survey tool failed ({status})"))
            };
            if sub == "tables" {
                // Only the game's own tables (spawners, vaults, recipes): seconds, not minutes.
                run(&["--tables"])?;
                log!("{}", trf!("SURVEY_WRITTEN_TO", path = hiumod::paths::data_dir().join("survey").display()));
                return Ok(());
            }
            if sub == "survey" {
                match arg(1) {
                    Some(w) => run(&["--world", w])?,
                    None => run(&[])?,
                }
                log!("{}", trf!("SURVEY_WRITTEN_TO", path = hiumod::paths::data_dir().join("survey").display()));
            }
            run(&["--locale"])?;
            log!("{}", trf!("THE_GAMES_TEXT_WRITTEN_TO", path = hiumod::i18n::names::dir().display()));
            Ok(())
        }
        Some("saves") => {
            // Every CharlieSaveGame in memory, every 5 s: which one the game updates as
            // the hero learns things (.spec/QUESTS.md, live state).
            let secs: u64 = arg(1).and_then(|s| s.parse().ok()).unwrap_or(120);
            let started = std::time::Instant::now();
            while started.elapsed().as_secs() < secs {
                for s in objects.of_class(m, n, "CharlieSaveGame") {
                    let date =
                        n.path(m, s, &["SaveDate"]).and_then(|(at, _)| hiumod::mem::read_u64(m, at)).unwrap_or(0);
                    let k = hiumod::knowledge::read(n, m, s);
                    let deeds = hiumod::quests::deed_states(m, n, s);
                    let done = deeds.iter().filter(|d| (2..=3).contains(&d.1)).count();
                    println!(
                        "{:>4}s 0x{s:X} date {date} facts {} tags {} deeds done {done}/{}",
                        started.elapsed().as_secs(),
                        k.as_ref().map_or(0, |k| k.facts.len()),
                        k.as_ref().map_or(0, |k| k.tags.len()),
                        deeds.len()
                    );
                }
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
            Ok(())
        }
        Some("usmap") => {
            // The survey tool's mappings (.spec/SURVEY.md §3.1).
            let (bytes, structs, enums) = hiumod::usmap::build(m, n, &objects);
            let path = dir.join("HellIsUs.usmap");
            std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            log!(
                "{}",
                trf!(
                    "STRUCTS_AND_CLASSES_ENUMS_KB_WRITTEN",
                    structs = structs,
                    enums = enums,
                    kb = bytes.len() / 1024,
                    path = path.display()
                )
            );
            Ok(())
        }
        Some("watch") => {
            let what = arg(1).ok_or("watch what? e.g. `doctor watch inventory`")?;
            let secs: u64 = arg(2).and_then(|s| s.parse().ok()).unwrap_or(60);
            let objs = target(&a, &objects, what)?;
            let read = |o: u64, len: u64| {
                let mut b = vec![0u8; len as usize];
                m.read(o, &mut b).then_some(b)
            };
            let mut last: Vec<(u64, u64, Vec<u8>)> = objs
                .iter()
                .filter_map(|&o| {
                    let len = probe::extent(n, m, o);
                    read(o, len).map(|b| (o, len, b))
                })
                .collect();
            log!("{}", trf!("WATCHING_OBJECT_S_FOR_S_CTRL", n = last.len(), secs = secs));
            process::catch_ctrl_c();
            let start = std::time::Instant::now();
            let mut log_text = String::new();
            while start.elapsed().as_secs() < secs && !process::STOP.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
                for (o, len, before) in last.iter_mut() {
                    let Some(now) = read(*o, *len) else { continue };
                    for line in probe::diff(n, m, *o, before, &now) {
                        let row = format!("{:>7.1}s {o:#x} {line}", start.elapsed().as_secs_f32());
                        println!("{row}");
                        log_text.push_str(&row);
                        log_text.push('\n');
                    }
                    *before = now;
                }
            }
            save(&format!("watch-{}.txt", what.replace([':', '\\', '/'], "_")), &log_text)
        }
        Some("scan") if arg(1) == Some("next") => {
            let v: f64 = arg(2).and_then(|s| s.parse().ok()).ok_or("scan next <value>")?;
            let path = dir.join("scan.txt");
            let text =
                std::fs::read_to_string(&path).map_err(|_| "no scan yet: run `doctor scan <target> <value>` first")?;
            let kept: Vec<String> = text
                .lines()
                .filter(|l| {
                    let f: Vec<&str> = l.split_whitespace().collect();
                    let (Some(at), Some(kind)) =
                        (f.first().and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok()), f.get(1))
                    else {
                        return false;
                    };
                    probe::read_as(m, at, kind).is_some_and(|x| (x - v).abs() <= 1e-3 * v.abs().max(1.0))
                })
                .map(String::from)
                .collect();
            for k in &kept {
                println!("{k}");
            }
            log!("{}", trf!("OF_PLACES_NOW_HOLD", kept = kept.len(), all = text.lines().count(), v = v));
            save("scan.txt", &kept.join("\n"))
        }
        Some("scan") => {
            let what = arg(1).ok_or("scan what? e.g. `doctor scan items 3`")?;
            let v: f64 = arg(2).and_then(|s| s.parse().ok()).ok_or("scan <target> <value>")?;
            let mut rows = Vec::new();
            for o in target(&a, &objects, what)? {
                let len = probe::extent(n, m, o).max(0x400);
                let mut b = vec![0u8; len as usize];
                if !m.read(o, &mut b) {
                    continue;
                }
                let name = n.object(m, o).unwrap_or_default();
                for (off, kind) in probe::matches(&b, v) {
                    rows.push(format!("{:#x} {kind} +{off:#x} in {name} @{o:#x}", o + off as u64));
                }
            }
            for r in &rows {
                println!("{r}");
            }
            log!("{}", trf!("PLACES_HOLD_CHANGE_IT_IN_GAME", n = rows.len(), v = v));
            let _ = mem::read_u32; // (reads above go through Memory)
            save("scan.txt", &rows.join("\n"))
        }
        _ => {
            Err("doctor takes: inspect, find, dump, watch, scan, usmap, survey, locale, tables (see `hiumod help`)"
                .into())
        }
    }
}

/// What a probe target names: live objects.
fn target(a: &hiumod::engine::Attached, objects: &hiumod::gobjects::Objects, what: &str) -> Result<Vec<u64>, String> {
    use hiumod::mem;
    let (m, n) = (&a.game, &a.anchors.names);
    let (what, index) = match what.split_once(':') {
        Some((w, i)) => (w, i.parse::<usize>().ok()),
        None => (what, None),
    };
    let pick = |all: Vec<u64>| -> Result<Vec<u64>, String> {
        match index {
            Some(i) => all.get(i).map(|&o| vec![o]).ok_or_else(|| format!("only {} of those", all.len())),
            None => Ok(all),
        }
    };
    let chain = || a.chain();
    let hero = || chain()?.hero(m, &a.anchors);
    match what {
        "hero" => Ok(vec![hero()?]),
        "controller" => Ok(vec![chain()?.controller(m, &a.anchors)?]),
        "asc" => Ok(vec![hiumod::player::find_asc(n, m, hero()?)?.1]),
        "sets" => {
            let arr = chain()?.attribute_sets(m, &a.anchors)?;
            pick(hiumod::actors::array(m, arr, 64))
        }
        "inventory" => Ok(vec![a.inventory()?]),
        "items" => {
            let inv = a.inventory()?;
            let f = n.field(m, inv, "Items").ok_or("inventory without Items")?;
            pick(hiumod::actors::array(m, inv + f.offset as u64, 4096))
        }
        "save" => {
            let saves = objects.of_class(m, n, "CharlieSaveGame");
            Ok(vec![hiumod::knowledge::current(n, m, &saves).ok_or("no save state")?])
        }
        "world" => Ok(vec![a.world_settings()?]),
        "enemy" => {
            a.things()?;
            pick(a.enemies())
        }
        _ if what.starts_with("0x") => {
            let at = u64::from_str_radix(&what[2..], 16).map_err(|_| format!("{what} is not an address"))?;
            mem::plausible(at).then_some(vec![at]).ok_or_else(|| format!("{what} is not a plausible address"))
        }
        class => {
            let live = |all: Vec<u64>| -> Vec<u64> {
                all.into_iter().filter(|&o| !n.object(m, o).unwrap_or_default().starts_with("Default__")).collect()
            };
            let mut all = live(objects.of_class(m, n, class));
            if all.is_empty() {
                // A blueprint's instances are of its generated class (…_C): take any
                // object whose class descends from the one named.
                all = live(objects.all(m).into_iter().filter(|&o| n.is_a(m, o, class)).collect());
            }
            if all.is_empty() {
                return Err(format!("no live {class}. Targets: hero, controller, asc, sets, inventory, items, save, world, enemy[:N], 0xADDRESS, or a class name"));
            }
            pick(if index.is_none() { all.into_iter().take(1).collect() } else { all })
        }
    }
}

/// Where the hero is, twice a second until Ctrl+C — what the minimap will draw.
/// `doctor profile [seconds]`: the panel's worker steps, as the panel runs them (ten a
/// second, no toggles: nothing is written), and where their time goes (prof.rs).
fn profile(seconds: u64) -> R {
    let mut engine = hiumod::engine::Engine::new()?;
    process::catch_ctrl_c();
    let until = std::time::Instant::now() + Duration::from_secs(seconds);
    // The first steps read everything once (the journal's first pass, the survey):
    // timed apart, so the rest shows the steady state.
    let mut warm = Some(std::time::Instant::now() + Duration::from_secs(8));
    while !process::STOP.load(Ordering::SeqCst) && std::time::Instant::now() < until {
        let started = std::time::Instant::now();
        {
            let _t = hiumod::prof::span("step");
            engine.step();
        }
        if warm.is_some_and(|w| std::time::Instant::now() >= w) {
            warm = None;
            if let Some(line) = hiumod::prof::report("warm-up", Duration::ZERO) {
                println!(
                    "{line}
"
                );
            }
        }
        std::thread::sleep(Duration::from_millis(100).saturating_sub(started.elapsed()));
    }
    if let Some(line) = hiumod::prof::report("worker", Duration::ZERO) {
        for part in line.split(", ") {
            println!("{part}");
        }
    }
    Ok(())
}

fn pose() -> R {
    let a = attach()?;
    process::catch_ctrl_c();
    while !process::STOP.load(Ordering::SeqCst) {
        match a.pose() {
            Ok((p, yaw)) => log!("x {:>10.0}  y {:>10.0}  z {:>8.0}  yaw {:>7.1}", p[0], p[1], p[2], yaw),
            Err(e) => warn!("{e}"),
        }
        // And the camera the frame was drawn from (the overlay's marks in the world).
        if let Some(c) =
            a.chain().ok().and_then(|c| c.pose_source(&a.game, &a.anchors).ok()).and_then(|s| s.camera(&a.game))
        {
            log!(
                "camera x {:>10.0}  y {:>10.0}  z {:>8.0}  pitch {:>6.1}  yaw {:>7.1}  fov {:>5.1}",
                c.at[0],
                c.at[1],
                c.at[2],
                c.rotation[0],
                c.rotation[1],
                c.fov
            );
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
        log!("{}", trf!("ORIGINAL_S_LEFT_BY_AN_EARLIER", n = engine.pending()));
    }
    engine.set_active(toggles.clone())?;
    process::catch_ctrl_c();
    let held: Vec<String> = toggles
        .iter()
        .map(|t| format!("{}{}", t.cheat, if t.value != 0.0 { format!("={}", t.value) } else { String::new() }))
        .collect();
    log!("{}", trf!("HOLDING_CTRL_C_TO_STOP_AND", cheats = held.join(", ")));
    log!("{}", tr!("IF_THIS_IS_KILLED_INSTEAD_HIUMOD"));

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
                log!("{}", tr!("APPLYING"))
            } else {
                warn!("{now}")
            }
            last = now;
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    match engine.stop() {
        Ok(()) => log!("{}", tr!("ORIGINAL_VALUES_RESTORED")),
        Err(e) => warn!("{e}"),
    }
    ended
}

/// Put back what a killed hold left changed.
fn restore() -> R {
    let mut engine = Engine::new()?;
    if engine.pending() == 0 {
        log!("{}", tr!("NOTHING_TO_RESTORE"));
        return Ok(());
    }
    engine.stop()?;
    log!("{}", tr!("ORIGINAL_VALUES_RESTORED"));
    Ok(())
}
