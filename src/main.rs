//! Entry point. It reads the arguments and assembles the run order — nothing else.

use hiumod::{tr, trf};
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
        Ok(i) => log!("{}", trf!("{ok} 설치 {dir} — Steam 빌드 {build}", ok = ok(true), dir = i.dir.display(), build = i.version)),
        Err(e) => log!("{}", trf!("{ok} 설치: {e}", ok = ok(false), e = e)),
    }

    let a = match attach() {
        Ok(a) => a,
        Err(e) => {
            log!("{} {e}", ok(false));
            return Ok(());
        }
    };
    log!("{}", trf!("{ok} 프로세스 {pid} — 베이스 0x{base}, 이미지 0x{size} 바이트", ok = ok(true), pid = a.game.pid, base = format!("{:X}", a.game.base), size = format!("{:X}", a.game.size)));
    let an = &a.anchors;
    log!("{}", trf!("{ok} 이름 풀 +0x{at}", ok = ok(true), at = format!("{:X}", an.names_rva)));
    log!("{} GEngine +0x{:X}", ok(true), an.gengine_rva);
    let l = an.names.layout;
    log!(
        "{}",
        trf!(
            "{ok} FField 레이아웃: next +0x{next}, name +0x{name}, size +0x{size}, offset +0x{offset}",
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
            log!("{}", trf!("{ok} 플레이어: {e}", ok = ok(false), e = e));
            return Ok(());
        }
    };
    log!("{}", trf!("{ok} 주인공: {a0}", ok = ok(true), a0 = chain.classes.join(" < ")));
    log!(
        "{}",
        trf!(
            "{ok} 체인: GEngine {path} → 컨트롤러 +0x{pawn} 폰 → +0x{asc} ASC → +0x{sets} SpawnedAttributes",
            ok = ok(true),
            path = format!("{:X?}", chain.to_controller),
            pawn = format!("{:X}", chain.pawn),
            asc = format!("{:X}", chain.asc),
            sets = format!("{:X}", chain.sets)
        )
    );
    match a.gate() {
        Ok(()) => log!("{}", tr!("ok   주인공 게이트: 열림 — 쓰기 허용")),
        Err(e) => log!("FAIL {e}"),
    }
    match a.pose() {
        Ok((p, yaw)) => log!("{}", trf!("ok   위치: ({x:.0}, {y:.0}, {z:.0}) 방향 {yaw:.1}", x = p[0], y = p[1], z = p[2], yaw = yaw)),
        Err(e) => log!("{}", trf!("FAIL 위치: {e}", e = e)),
    }

    let started = std::time::Instant::now();
    match a.things() {
        Ok(t) => {
            let count = |k: hiumod::actors::Kind| t.iter().filter(|x| x.kind() == k).count();
            let kinds: Vec<String> =
                hiumod::actors::Kind::ALL.iter().map(|&k| format!("{} {}", k.label(), count(k))).collect();
            log!("{}", trf!("ok   미니맵 대상: {n}개, {ms} ms — {kinds}", n = t.len(), ms = started.elapsed().as_millis(), kinds = kinds.join(", ")));
            let subs: Vec<String> = hiumod::actors::Sub::ALL
                .iter()
                .filter_map(|&s| {
                    let n = t.iter().filter(|x| x.sub == s).count();
                    (n > 0).then(|| format!("{} {n}", s.label()))
                })
                .collect();
            println!("        {}", subs.join(", "));
        }
        Err(e) => log!("{}", trf!("FAIL 미니맵 대상: {e}", e = e)),
    }

    let started = std::time::Instant::now();
    match a.goals() {
        Ok((g, k)) => {
            let count = |t: hiumod::goals::Tier| g.iter().filter(|x| x.tier == t).count();
            log!(
                "{}",
                trf!(
                    "ok   안내: 아는 사실 {facts}개, 태그 {tags}개; 진행 중: {open} — 장소 {places}곳, {ms} ms (퀘스트 {quest}, 비밀 {secret}, 단서 {clue})",
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
        }
        Err(e) => log!("{}", trf!("FAIL 안내: {e}", e = e)),
    }

    let s = match a.session() {
        Ok(s) => s,
        Err(e) => {
            log!("{}", trf!("{ok} 속성: {e}", ok = ok(false), e = e));
            return Ok(());
        }
    };
    let count: usize = s.sets.iter().map(|x| x.attributes.len()).sum();
    log!(
        "{}",
        trf!(
            "{ok} 속성: 세트 {sets}개, 이름으로 찾은 속성 {count}개, vtable +0x{vt}",
            ok = ok(true),
            sets = s.sets.len(),
            count = count,
            vt = format!("{:X}", s.vtable - a.game.base)
        )
    );
    for set in &s.sets {
        println!("        {:<36} {}", set.class, trf!("속성 {n}개", n = set.attributes.len()));
    }

    let missing: Vec<String> =
        cheats::attributes().into_iter().filter(|x| !s.has(*x)).map(|x| format!("{}.{}", x.set, x.name)).collect();
    if missing.is_empty() {
        log!("{}", trf!("{ok} 치트 표: 이름이 나오는 속성 {n}개가 모두 게임에 있음", ok = ok(true), n = cheats::attributes().len()));
    } else {
        log!("{}", trf!("{ok} 치트 표: 게임에 없음 — {a0}", ok = ok(false), a0 = missing.join(", ")));
    }
    let marks = hiumod::verify::load();
    for c in cheats::CHEATS {
        let state = if c.verified { tr!("확인됨") } else { tr!("미검증") };
        let tried = match marks.get(c.id) {
            Some(true) => tr!("플레이어: 됨"),
            Some(false) => tr!("플레이어: 안 됨"),
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
        println!("        {:<16} {:<10} {:<22} {} [{}]", c.id, state, tried, hiumod::i18n::tr(c.label), sets.join(", "));
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
        dir.join("..").join("..").join("tools").join("survey").join("bin").join("Release").join("net8.0").join("survey.dll"),
    ]
    .into_iter()
    .find(|p| p.exists())
}

/// `doctor inspect|find|dump|watch|scan|usmap|survey` (probe.rs): reads only.
fn probe(args: &[String]) -> R {
    use hiumod::mem::{self, Memory};
    use hiumod::probe;
    let a = attach()?;
    let (m, n) = (&a.game, &a.anchors.names);
    let objects = hiumod::gobjects::discover(m, a.game.base)?;
    let dir = hiumod::paths::data_dir().join("doctor");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let save = |file: &str, text: &str| -> R {
        let path = dir.join(file);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        log!("{}", trf!("{path} 에 씀", path = path.display()));
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
            log!("{}", trf!("클래스·구조체 {s}개의 속성 {p}개", p = rows.len(), s = structs.len()));
            save(&format!("find-{text}.txt"), &rows.join("\n"))
        }
        Some("dump") => {
            let mut prefixes: Vec<String> = args[1..].to_vec();
            if prefixes.is_empty() {
                prefixes = vec!["Charlie".into(), "Story".into()];
            }
            let structs = probe::structs(n, m, &objects.all(m), &prefixes);
            let text = probe::dump(n, m, &structs);
            log!("{}", trf!("{prefix}(으)로 시작하는 클래스·구조체 {n}개", n = structs.len(), prefix = prefixes.join(", ")));
            save("sdk.txt", &text)
        }
        Some(sub @ ("survey" | "locale")) => {
            // Mappings first, then the survey tool over the game's maps (.spec/ITEMS.md),
            // then the game's text in every language (.spec/I18N.md); `locale` only that.
            let (bytes, structs, enums) = hiumod::usmap::build(m, n, &objects);
            std::fs::write(dir.join("HellIsUs.usmap"), &bytes).map_err(|e| e.to_string())?;
            log!("{}", trf!("매핑: 구조체·클래스 {structs}개, 열거형 {enums}개", structs = structs, enums = enums));
            let tool = survey_tool().ok_or(
                "tools/survey is not built: run `dotnet build -c Release` in tools/survey (needs the .NET 8 SDK)",
            )?;
            let game = hiumod::paths::data_dir().parent().ok_or("no game folder")?.to_path_buf();
            let run = |extra: &[&str]| -> Result<(), String> {
                let mut cmd = std::process::Command::new("dotnet");
                cmd.arg(&tool).arg("--game").arg(&game).args(extra);
                log!("{}", trf!("실행: {tool} {args}", tool = tool.display(), args = extra.join(" ")));
                let status = cmd.status().map_err(|e| format!("dotnet: {e}"))?;
                status.success().then_some(()).ok_or(format!("the survey tool failed ({status})"))
            };
            if sub == "survey" {
                match arg(1) {
                    Some(w) => run(&["--world", w])?,
                    None => run(&[])?,
                }
                log!("{}", trf!("조사 결과: {path}", path = hiumod::paths::data_dir().join("survey").display()));
            }
            run(&["--locale"])?;
            log!("{}", trf!("게임 텍스트: {path}", path = hiumod::i18n::names::dir().display()));
            Ok(())
        }
        Some("saves") => {
            // Every CharlieSaveGame in memory, every 5 s: which one the game updates as
            // the hero learns things (.spec/QUESTS.md, live state).
            let secs: u64 = arg(1).and_then(|s| s.parse().ok()).unwrap_or(120);
            let started = std::time::Instant::now();
            while started.elapsed().as_secs() < secs {
                for s in objects.of_class(m, n, "CharlieSaveGame") {
                    let date = n.path(m, s, &["SaveDate"]).and_then(|(at, _)| hiumod::mem::read_u64(m, at)).unwrap_or(0);
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
            // The survey tool's mappings (.spec/ITEMS.md §3.1).
            let (bytes, structs, enums) = hiumod::usmap::build(m, n, &objects);
            let path = dir.join("HellIsUs.usmap");
            std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            log!("{}", trf!("구조체·클래스 {structs}개, 열거형 {enums}개 — {kb} KB, {path} 에 씀", structs = structs, enums = enums, kb = bytes.len() / 1024, path = path.display()));
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
            log!("{}", trf!("객체 {n}개를 {secs}초 동안 지켜봄 — Ctrl+C 로 멈춤", n = last.len(), secs = secs));
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
                std::fs::read_to_string(&path).map_err(|_| "no scan yet — `doctor scan <target> <value>` first")?;
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
            log!("{}", trf!("{all}곳 중 {kept}곳이 이제 {v}", kept = kept.len(), all = text.lines().count(), v = v));
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
            log!("{}", trf!("{n}곳이 {v} — 게임에서 값을 바꾼 뒤 `doctor scan next <새 값>`", n = rows.len(), v = v));
            let _ = mem::read_u32; // (reads above go through Memory)
            save("scan.txt", &rows.join("\n"))
        }
        _ => Err("doctor takes: inspect, find, dump, watch, scan, usmap, survey, locale — see `hiumod help`".into()),
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
                return Err(format!("no live {class} — targets: hero, controller, asc, sets, inventory, items, save, world, enemy[:N], 0xADDRESS, or a class name"));
            }
            pick(if index.is_none() { all.into_iter().take(1).collect() } else { all })
        }
    }
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
        log!("{}", trf!("이전 hold 가 남긴 원래 값 {n}개 — 지금 값이 아니라 그것을 유지", n = engine.pending()));
    }
    engine.set_active(toggles.clone())?;
    process::catch_ctrl_c();
    let held: Vec<String> = toggles
        .iter()
        .map(|t| format!("{}{}", t.cheat, if t.value != 0.0 { format!("={}", t.value) } else { String::new() }))
        .collect();
    log!("{}", trf!("유지 중: {a0} — Ctrl+C 로 멈추고 되돌림", a0 = held.join(", ")));
    log!("{}", tr!("강제로 종료되면 `hiumod restore` 로 되돌릴 수 있음"));

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
                log!("{}", tr!("적용 중"))
            } else {
                warn!("{now}")
            }
            last = now;
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    match engine.stop() {
        Ok(()) => log!("{}", tr!("원래 값으로 되돌림")),
        Err(e) => warn!("{e}"),
    }
    ended
}

/// Put back what a killed hold left changed.
fn restore() -> R {
    let mut engine = Engine::new()?;
    if engine.pending() == 0 {
        log!("{}", tr!("되돌릴 것이 없음"));
        return Ok(());
    }
    engine.stop()?;
    log!("{}", tr!("원래 값으로 되돌림"));
    Ok(())
}
