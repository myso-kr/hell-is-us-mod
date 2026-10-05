// tools/survey — reads the game's cooked World Partition maps and lists what the quest
// guide needs: everything that hands something out (pickups, devices, markers), NPCs
// and their conversations, hand-overs — with where they are and their save GUID
// (.spec/SURVEY.md).
//
// Runs on the player's PC only. The AES key is passed in (hiumod finds it in the
// executable); nothing is written but the survey JSON.
//
//   survey --game <install dir> [--world <name>|all] [--peek <actor name part>]
//   (or each of --paks, --usmap, --oodle, --aes, --out)
//
// A placed actor's components often keep their data in the blueprint's component
// template (`Template`, then its template…), not in the placed copy: each component's
// properties are the template chain's, overridden by the copy's own.

using CUE4Parse.Compression;
using CUE4Parse.Encryption.Aes;
using CUE4Parse.FileProvider;
using CUE4Parse.MappingsProvider;
using CUE4Parse.UE4.Assets.Exports;
using CUE4Parse.UE4.Objects.Core.Misc;
using CUE4Parse.UE4.Versions;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

var opts = Args.Parse(args);
string Need(string k) => opts.TryGetValue(k, out var v) ? v : throw new ArgumentException($"--{k} is required");

// --game <install dir> fills in the rest: the paks, the executable for the key,
// Mods\doctor\HellIsUs.usmap, Mods	ools\oo2core_9_win64.dll and Mods\survey.
if (opts.TryGetValue("game", out var game))
{
    opts.TryAdd("paks", Path.Combine(game, "HellIsUs", "Content", "Paks"));
    opts.TryAdd("usmap", Path.Combine(game, "Mods", "doctor", "HellIsUs.usmap"));
    opts.TryAdd("oodle", Path.Combine(game, "Mods", "tools", "oo2core_9_win64.dll"));
    opts.TryAdd("out", Path.Combine(game, "Mods", "survey"));
    if (!opts.ContainsKey("aes"))
        opts["aes"] = AesKey.Find(Path.Combine(game, "HellIsUs", "Binaries", "Win64", "HellIsUs-Win64-Shipping.exe"),
            Path.Combine(game, "HellIsUs", "Content", "Paks", "pakchunk0-Windows.pak"));
}

var oodle = Need("oodle");
if (!File.Exists(oodle))
{
    Directory.CreateDirectory(Path.GetDirectoryName(oodle)!);
    Console.Error.WriteLine($"fetching Oodle to {oodle}");
    await OodleHelper.DownloadOodleDllAsync(oodle);
}
OodleHelper.Initialize(oodle);
var provider = new DefaultFileProvider(Need("paks"), SearchOption.TopDirectoryOnly, true, new VersionContainer(EGame.GAME_UE5_5));
provider.MappingsContainer = new FileUsmapTypeMappingsProvider(Need("usmap"));
provider.Initialize();
provider.SubmitKey(new FGuid(), new FAesKey(Need("aes")));
provider.PostMount();
Console.Error.WriteLine($"{provider.Files.Count} files mounted");

var worldArg = opts.GetValueOrDefault("world", "all");
var worlds = provider.Files.Keys
    .Select(p => System.Text.RegularExpressions.Regex.Match(p, @"/Maps/([^/]+)/\1_Root_WP\.umap$", System.Text.RegularExpressions.RegexOptions.IgnoreCase))
    .Where(m => m.Success)
    .Select(m => m.Groups[1].Value)
    .Where(w => worldArg == "all" || w.Equals(worldArg, StringComparison.OrdinalIgnoreCase))
    .Distinct()
    .OrderBy(w => w)
    .ToList();
Console.Error.WriteLine($"worlds: {string.Join(", ", worlds)}");

// --ls <regex>: the mounted paths that match. --dump <path>: a package's exports as JSON.
if (opts.TryGetValue("ls", out var ls))
{
    var re = new System.Text.RegularExpressions.Regex(ls, System.Text.RegularExpressions.RegexOptions.IgnoreCase);
    foreach (var p in provider.Files.Keys.Where(p => re.IsMatch(p)).OrderBy(p => p)) Console.WriteLine(p);
    return;
}
if (opts.TryGetValue("dump", out var dumpPath))
{
    Console.WriteLine(JsonConvert.SerializeObject(provider.LoadPackage(dumpPath).GetExports(), Formatting.Indented));
    return;
}

// --locale: every culture's text (the game's locres) to Mods\locale\<culture>.tsv —
// namespace, key, text — names.tsv: which text names each item, NPC and region, and
// facts.tsv: the text of every Datapad fact (.spec/I18N.md).
if (opts.ContainsKey("locale"))
{
    var dir = Path.Combine(Path.GetDirectoryName(Need("out"))!, "locale");
    Directory.CreateDirectory(dir);
    Locale.Write(provider, dir);
    Locale.Names(provider, dir, worlds);
    Locale.Facts(provider, dir);
    return;
}

// --tables: only the game's own tables the guide reads (spawners, vaults) — also
// written at the end of a full survey.
if (opts.ContainsKey("tables"))
{
    Directory.CreateDirectory(Need("out"));
    Tables.Write(provider, Need("out"));
    return;
}

var survey = new Survey(provider);
// --grep <text> [--in <path prefix>]: the packages whose bytes hold `text` (a name in
// their name table): what refers to an item or a fact, wherever it is.
if (opts.TryGetValue("grep", out var grepFor))
{
    var prefix = opts.GetValueOrDefault("in", "HellIsUs/Content/");
    // Several at once, `|` between them: each package read once.
    var needles = grepFor.Split('|').Select(n => (n, System.Text.Encoding.ASCII.GetBytes(n))).ToList();
    foreach (var p in provider.Files.Keys.Where(p => p.StartsWith(prefix, StringComparison.OrdinalIgnoreCase) && (p.EndsWith(".uasset") || p.EndsWith(".umap"))).OrderBy(p => p))
    {
        try
        {
            if (!provider.TrySaveAsset(p, out var bytes)) continue;
            foreach (var (n, b) in needles)
                if (bytes.AsSpan().IndexOf(b) >= 0) Console.WriteLine($"{n}	{p}");
        }
        catch { }
    }
    return;
}
// --terrain <out dir> [--world W] [--cell cm]: each world's landscape as one height grid
// (`<World>.terrain.json`: origin, cell, size, heights as base64 little-endian int16 in
// decimetres, -32768 where there is no ground), read from every landscape component's
// heightmap. Game data: kept on the player's PC (.spec/MAP.md §15).
if (opts.TryGetValue("terrain", out var terrainOut))
{
    Directory.CreateDirectory(terrainOut);
    var cell = opts.TryGetValue("cell", out var cs) ? double.Parse(cs) : 400.0;
    foreach (var w in worlds)
    {
        var started = DateTime.Now;
        var grid = new Dictionary<(int, int), double>();
        var raw = new JArray();
        var comps = 0;
        foreach (var map in survey.Maps(w))
        {
            try { comps += survey.Terrain(map, cell, grid, raw); }
            catch (Exception e) { Console.Error.WriteLine($"  {map}: {e.Message}"); }
        }
        if (grid.Count == 0) { Console.Error.WriteLine($"{w}: no landscape"); continue; }
        // A stray cell far off (a component placed by a transform not read right) would make
        // the grid as large as the gap: the bounds of all but the outermost half percent.
        var xs = grid.Keys.Select(k => k.Item1).OrderBy(v => v).ToList();
        var ys = grid.Keys.Select(k => k.Item2).OrderBy(v => v).ToList();
        int Pick(List<int> l, double q) => l[(int)Math.Clamp(Math.Round(q * (l.Count - 1)), 0, l.Count - 1)];
        int x0 = Pick(xs, 0.005), x1 = Pick(xs, 0.995), y0 = Pick(ys, 0.005), y1 = Pick(ys, 0.995);
        int gw = x1 - x0 + 1, gh = y1 - y0 + 1;
        var bytes = new byte[gw * gh * 2];
        for (var y = 0; y < gh; y++)
            for (var x = 0; x < gw; x++)
            {
                short v = grid.TryGetValue((x + x0, y + y0), out var z) ? (short)Math.Clamp(Math.Round(z / 10.0), -32767, 32767) : short.MinValue;
                BitConverter.TryWriteBytes(bytes.AsSpan((y * gw + x) * 2, 2), v);
            }
        var record = new JObject
        {
            ["world"] = w, ["cell"] = cell, ["x0"] = x0 * cell, ["y0"] = y0 * cell,
            ["w"] = gw, ["h"] = gh, ["components"] = comps, ["heights"] = Convert.ToBase64String(bytes),
            ["transforms"] = opts.ContainsKey("raw") ? raw : null,
        };
        File.WriteAllText(Path.Combine(terrainOut, $"{w}.terrain.json"), record.ToString(Formatting.None));
        Console.Error.WriteLine($"{w}: {comps} components, {gw}x{gh} cells in {(DateTime.Now - started).TotalSeconds:F0} s");
    }
    return;
}
if (opts.TryGetValue("refs", out var refsOf))
{
    foreach (var w in worlds)
        foreach (var map in survey.Maps(w))
            survey.Refs(map, refsOf);
    return;
}
if (opts.TryGetValue("peek", out var peek))
{
    foreach (var w in worlds)
        foreach (var map in survey.Maps(w))
            survey.Peek(map, peek);
    return;
}

var outDir = Need("out");
Directory.CreateDirectory(outDir);
foreach (var w in worlds)
{
    var started = DateTime.Now;
    var actors = new List<JObject>();
    foreach (var map in survey.Maps(w))
    {
        try { actors.AddRange(survey.Actors(map)); }
        catch (Exception e) { Console.Error.WriteLine($"  {map}: {e.Message}"); }
    }
    var file = Path.Combine(outDir, $"{w}.json");
    var record = new JObject { ["world"] = w, ["actors"] = new JArray(actors) };
    // What the world itself gives (CharlieWorldSettings): on first entering it, and on
    // winning its boss fight — the keystones, the keys and the facts no place gives.
    try { if (survey.WorldGives(w) is { } gives) record["gives"] = gives; }
    catch (Exception e) { Console.Error.WriteLine($"  {w} settings: {e.Message}"); }
    File.WriteAllText(file, record.ToString(Formatting.Indented));
    Console.Error.WriteLine($"{w}: {actors.Count} actors in {(DateTime.Now - started).TotalSeconds:F0} s → {file}");
}
// The conversations NPCs run, with what their nodes hand out.
var flows = survey.Flows();
// Kept across runs of single worlds: the new ones over the old.
var flowsFile = Path.Combine(outDir, "flows.json");
if (File.Exists(flowsFile))
{
    var old = JObject.Parse(File.ReadAllText(flowsFile));
    old.Merge(flows);
    flows = old;
}
File.WriteAllText(flowsFile, flows.ToString(Formatting.Indented));
Console.Error.WriteLine($"flows: {flows.Count}");
Tables.Write(provider, outDir);

/// The game's own tables the guide reads as they are (.spec/FEATURES.md §3):
/// spawners.json — every world's spawners (`<World>_Root_WP_Spawner_DT`): the save GUID,
///   the enemies it spawns, where, and the timeloop it belongs to (the "every Hollow"
///   achievement counts them);
/// vaults.json — the Vaults of Forbidden Knowledge (`ResearchCacheData`): GUID, name,
///   region, clue, research entries to unlock, and the four-symbol code.
static class Tables
{
    /// Every identity (a Datapad entry: a person, a place, a thing) with its base facts and
    /// related items: what a payload naming it gives (`identities.json`).
    static void Identities(DefaultFileProvider provider, string outDir)
    {
        var all = new JObject();
        foreach (var path in provider.Files.Keys.Where(p => p.StartsWith("HellIsUs/Content/GameData/StoryUnits/") && p.EndsWith("_Identity_DA.uasset")).OrderBy(p => p))
        {
            try
            {
                foreach (var e in provider.LoadPackage(path).GetExports().Where(e => e.Class?.Name == "IdentityData"))
                {
                    var j = JObject.Parse(JsonConvert.SerializeObject(e.Properties.ToDictionary(x => x.Name.Text, x => x.Tag)));
                    string Last(JToken t) => ((string?)t["ObjectPath"] ?? (string?)t["ObjectName"] ?? "").Split('/').Last().Split('.').First().Split('\'').Last();
                    all[e.Name] = new JObject
                    {
                        ["facts"] = new JArray((j["BaseFacts"] ?? new JArray()).Select(Last).Where(x => x.Length > 0)),
                        ["items"] = new JArray((j["RelatedItems"] ?? new JArray()).Select(Last).Where(x => x.Length > 0)),
                    };
                }
            }
            catch { }
        }
        File.WriteAllText(Path.Combine(outDir, "identities.json"), all.ToString(Formatting.Indented));
        Console.Error.WriteLine($"identities: {all.Count}");
    }

    public static void Write(DefaultFileProvider provider, string outDir)
    {
        Identities(provider, outDir);
        // As the mod's {g:namespace/key}: a string table's namespace is its TableNamespace.
        var nsOf = new Dictionary<string, string?>();
        string Text(JToken? t)
        {
            if (t?["Key"]?.ToString() is not { Length: > 0 } key || t["TableId"]?.ToString() is not { Length: > 0 } table) return "";
            if (!nsOf.TryGetValue(table, out var ns))
            {
                var pkg = table.Split('.')[0];
                pkg = pkg.StartsWith("/Game/") ? "HellIsUs/Content/" + pkg["/Game/".Length..] : pkg.TrimStart('/');
                try { ns = provider.LoadPackage(pkg).GetExports().OfType<CUE4Parse.UE4.Assets.Exports.Internationalization.UStringTable>().First().StringTable.TableNamespace; }
                catch { ns = null; }
                nsOf[table] = ns;
            }
            return ns is null ? "" : $"{ns}/{key}";
        }
        IEnumerable<JObject> Exports(string path)
        {
            try { return JArray.Parse(JsonConvert.SerializeObject(provider.LoadPackage(path).GetExports())).OfType<JObject>(); }
            catch (Exception e) { Console.Error.WriteLine($"  {path}: {e.Message}"); return []; }
        }
        var spawners = new JObject();
        foreach (var path in provider.Files.Keys.Where(p => p.StartsWith("HellIsUs/Content/GameData/Spawner/") && p.EndsWith("_Spawner_DT.uasset")).OrderBy(p => p))
        {
            var world = Path.GetFileName(path).Replace("_Root_WP_Spawner_DT.uasset", "");
            var list = new JArray();
            foreach (var e in Exports(path[..^".uasset".Length]))
                foreach (var (name, row) in e["Rows"] as JObject ?? new JObject())
                {
                    var at = row!["SpawnerLocation"]!;
                    list.Add(new JObject
                    {
                        ["name"] = name.Split('.').Last(),
                        ["guid"] = row["SpawnerSerializeGuid"],
                        ["timeloop"] = row["TimeloopActorID"]?.ToString() is { Length: > 0 } t && t != "None" ? t : null,
                        ["entities"] = row["EntitiesToSpawn"],
                        ["at"] = new JArray(Math.Round((double)at["X"]!), Math.Round((double)at["Y"]!), Math.Round((double)at["Z"]!)),
                    });
                }
            spawners[world] = list;
        }
        File.WriteAllText(Path.Combine(outDir, "spawners.json"), spawners.ToString(Formatting.Indented));
        Console.Error.WriteLine($"spawners: {spawners.Properties().Sum(p => ((JArray)p.Value).Count)} in {spawners.Count} worlds");

        var vaults = new JArray();
        foreach (var path in provider.Files.Keys.Where(p => p.StartsWith("HellIsUs/Content/Gameplay/Research/CacheData/") && p.EndsWith(".uasset")).OrderBy(p => p))
            foreach (var e in Exports(path[..^".uasset".Length]))
            {
                var p = e["Properties"];
                if (p == null) continue;
                vaults.Add(new JObject
                {
                    ["asset"] = e["Name"],
                    ["guid"] = p["Guid"],
                    ["name"] = Text(p["Name"]),
                    ["region"] = Text(p["WorldMapLocation"]),
                    ["clue"] = Text(p["Clue"]),
                    ["entries"] = p["NumberOfLoreEntriesToUnlock"],
                    ["code"] = new JArray((p["Code"] as JArray ?? []).Select(s => int.Parse(s.ToString().Replace("ECacheSymbols::CacheSymbol", "")))),
                });
            }
        File.WriteAllText(Path.Combine(outDir, "vaults.json"), vaults.ToString(Formatting.Indented));
        Console.Error.WriteLine($"vaults: {vaults.Count}");

        // recipes.json — every crafting recipe (`CraftRecipe`): the item it takes, the
        // shards and their counts, what it makes. The shard budget follows them.
        string Asset(JToken? r) => (r?["ObjectPath"]?.ToString() ?? "").Split('/').Last().Split('.')[0];
        var recipes = new JArray();
        foreach (var path in provider.Files.Keys.Where(p => p.StartsWith("HellIsUs/Content/Gameplay/Crafting/") && p.EndsWith("_Recipe_DA.uasset") || p.Contains("/Gameplay/Crafting/") && p.Contains("_Recipe0") && p.EndsWith(".uasset")).OrderBy(p => p))
            foreach (var e in Exports(path[..^".uasset".Length]).Where(x => x["Type"]?.ToString() == "CraftRecipe"))
            {
                var p = e["Properties"];
                if (p == null) continue;
                recipes.Add(new JObject
                {
                    ["recipe"] = e["Name"],
                    ["kind"] = (p["RecipeCategory"]?["TagName"]?.ToString() ?? "").Split('.').Last(),
                    ["from"] = Asset(p["MainIngredient"]?["IngredientBase"]),
                    ["needs"] = new JArray((p["Ingredients"] as JArray ?? []).Select(i => new JArray(Asset(i["IngredientBase"]), i["Quantity"]))),
                    ["to"] = Asset((p["ResultingCrafts"] as JArray)?.FirstOrDefault()),
                });
            }
        File.WriteAllText(Path.Combine(outDir, "recipes.json"), recipes.ToString(Formatting.Indented));
        Console.Error.WriteLine($"recipes: {recipes.Count}");
    }
}

class Survey(DefaultFileProvider provider)
{
    static readonly string[] Wanted =
        ["PayloadRuneComponent", "SaveIdentifierRuneComponent", "FlowComponent", "TradeGiveItemRuneComponent"];

    readonly HashSet<string> flowsToRead = [];

    public IEnumerable<string> Maps(string world) => provider.Files.Keys
        .Where(p => p.EndsWith(".umap", StringComparison.OrdinalIgnoreCase)
                    && (p.Contains($"/Maps/{world}/{world}_Root_WP/_Generated_/", StringComparison.OrdinalIgnoreCase)
                        || p.EndsWith($"/Maps/{world}/{world}_Root_WP.umap", StringComparison.OrdinalIgnoreCase)))
        .OrderBy(p => p);

    /// The properties of an object as JSON (name → value), its template chain's under its own.
    static JObject Props(UObject? o, int depth = 0)
    {
        var merged = new JObject();
        if (o == null || depth > 6) return merged;
        UObject? template = null;
        try { template = o.Template?.Load(); } catch { }
        if (template != null) merged.Merge(Props(template, depth + 1));
        foreach (var p in o.Properties)
            merged[p.Name.Text] = JToken.Parse(JsonConvert.SerializeObject(p.Tag));
        return merged;
    }

    /// Object references under a JSON value: their asset names (`Quest01_PictureC_Item_DA`).
    static IEnumerable<string> Objects(JToken? t) => Paths(t).Select(x => x.Split('/').Last().Split(':').Last());

    /// Object references as full paths (`/Game/Items/Quests/Quest01/Quest01_PictureC_Item_DA`).
    static IEnumerable<string> Paths(JToken? t) => t == null ? [] :
        t.SelectTokens("$..ObjectPath").Concat(t.SelectTokens("$..AssetPathName"))
            .Select(x => (string?)x ?? "")
            .Where(x => x.Length > 0 && x != "None")
            .Select(x => System.Text.RegularExpressions.Regex.Replace(x, @"\.\d+$", ""));

    /// Gameplay tag names under a JSON value.
    static IEnumerable<string> Tags(JToken? t) => t == null ? [] :
        (t is JContainer c ? c.Descendants() : [t]).OfType<JValue>().Where(v => v.Type == JTokenType.String)
            .Select(v => (string)v!)
            .Where(s => s.Contains('.') && !s.Contains('/') && !s.Contains('\'') && !s.Contains(' '));

    /// An object a property refers to, through its package (the map's own, or a
    /// blueprint's): `….X_BP.12` is export 12 of X_BP.
    UObject? Load(JToken? reference)
    {
        var path = (string?)reference?.SelectToken("$..ObjectPath") ?? (string?)reference?["ObjectPath"];
        if (path == null) return null;
        var m = System.Text.RegularExpressions.Regex.Match(path, @"^(.*)\.(\d+)$");
        if (!m.Success) return null;
        var pkg = m.Groups[1].Value;
        pkg = pkg.StartsWith("/Game/") ? "HellIsUs/Content/" + pkg["/Game/".Length..] : pkg.TrimStart('/');
        try
        {
            var exports = provider.LoadPackage(pkg).GetExports().ToList();
            var i = int.Parse(m.Groups[2].Value);
            return i >= 0 && i < exports.Count ? exports[i] : null;
        }
        catch { return null; }
    }

    /// {kind: dial|keypad|placement, dials: [{places, solution}], code, items} of an
    /// actor's puzzle components, or null.
    JObject? PuzzleOf(List<UObject> comps)
    {
        var dials = comps.Where(c => c.Class?.Name == "DialComponent").OrderBy(c => c.Name).Select(c => Props(c)).ToList();
        if (dials.Count > 0 && comps.Any(c => c.Class?.Name?.Contains("DialPuzzleAction") == true))
            return new JObject
            {
                ["kind"] = "dial",
                ["dials"] = new JArray(dials.Select(d => new JObject
                {
                    ["places"] = (int?)d["NbDialState"] ?? 0,
                    ["solution"] = (int?)d["DialSolution"] ?? 0,
                })),
            };
        var keypad = comps.FirstOrDefault(c => c.Class?.Name == "KeypadRuneComponent");
        if (keypad != null && Props(keypad)["Rune"]?["ExpectedCode"]?.ToString() is { Length: > 0 } code)
            return new JObject { ["kind"] = "keypad", ["code"] = code };
        var placement = comps.FirstOrDefault(c => c.Class?.Name?.Contains("ItemPlacementAction") == true);
        if (placement != null)
        {
            var sol = Props(placement)["Solution"];
            var items = Paths(sol).Where(x => x.Contains("/Items/")).ToList();
            if (items.Count == 0 && Load(sol) is { } cond)
                items = Paths(Props(cond)["Solution"]).Where(x => x.Contains("/Items/")).ToList();
            if (items.Count > 0)
                return new JObject { ["kind"] = "placement", ["items"] = new JArray(items.Distinct()) };
        }
        return null;
    }

    /// The item a slot of a choice puzzle (`…_1SlotPlacementPuzzleCheck_…`) counts as right: its
    /// `Item`, set on the placed actor or else by its class default. The slot's placement
    /// `Solution` is only what it accepts (every orb, or a stand-in), so it is not the
    /// answer. A slot that keeps the base's `ItemPlacementValidation_DummyItem` is a
    /// decoy: "" here.
    string Expected(UObject actor)
    {
        try
        {
            var own = Props(actor).Properties().FirstOrDefault(p => p.Name == "Item" || p.Name.StartsWith("Item["));
            if (Paths(own?.Value).FirstOrDefault(x => x.Contains("/Items/")) is { } placed) return placed;
            var path = actor.Class?.GetPathName();
            if (path == null) return "";
            var pkg = path[..path.LastIndexOf('.')];
            pkg = pkg.StartsWith("/Game/") ? "HellIsUs/Content/" + pkg["/Game/".Length..] : pkg.TrimStart('/');
            var cdo = provider.LoadPackage(pkg).GetExports().FirstOrDefault(e => e.Name == "Default__" + actor.Class!.Name);
            if (cdo == null) return "";
            var item = Props(cdo).Properties().FirstOrDefault(p => p.Name == "Item" || p.Name.StartsWith("Item["));
            var expected = Paths(item?.Value).FirstOrDefault(x => x.Contains("/Items/"));
            return expected ?? "";
        }
        catch { return ""; }
    }

    /// A PayloadData as {items, facts, tags}.
    static JObject Payload(JToken? data) => new()
    {
        ["items"] = new JArray(Paths(data?["ItemsToAdd"]).Distinct()),
        ["facts"] = new JArray(Objects(data?["ContainedFacts"]).Distinct()),
        ["tags"] = new JArray(Tags(data?["TagFacts"]).Distinct()),
        // The Datapad entries it gives whole: each with its base facts (identities.json).
        ["identities"] = new JArray(Objects(data?["BaseIdentity"]).Distinct()),
    };

    static bool Empty(JObject p) => p.Properties().All(x => !((JArray)x.Value).Any());

    /// An object reference into this package: `….Root_WP.8663` is export 8663.
    static UObject? Resolve(List<UObject> exports, JToken? reference)
    {
        var path = (string?)reference?.SelectToken("$..ObjectPath") ?? (string?)reference?["ObjectPath"];
        if (path == null) return null;
        var m = System.Text.RegularExpressions.Regex.Match(path, @"\.(\d+)$");
        if (!m.Success) return null;
        var i = int.Parse(m.Groups[1].Value);
        return i >= 0 && i < exports.Count ? exports[i] : null;
    }

    /// A condition as the graph reads it: its class; the interactable it watches and the
    /// state it wants; the tags, facts and items it names; and its sub-conditions (an
    /// all-of or any-of), recursively.
    static JObject Condition(List<UObject> exports, UObject cond, int depth)
    {
        var p = Props(cond);
        var j = new JObject { ["type"] = cond.Class?.Name ?? "" };
        // Every level actor it names, whatever the property (`InteractableActor`, the single-use
        // interactable it watches, a trigger): `Class'Map:PersistentLevel.<actor>'`, a
        // sub-object of an actor (a component) left out.
        var actors = p.SelectTokens("$..ObjectName")
            .Select(t => ((string?)t ?? "").TrimEnd('\''))
            .Select(n => n.Split(":PersistentLevel.", 2))
            .Where(s => s.Length == 2 && !s[1].Contains('.'))
            .Select(s => s[1])
            .Distinct().ToList();
        if (actors.Count == 1) j["actor"] = actors[0];
        else if (actors.Count > 1) j["actors"] = new JArray(actors);
        if (p["ExpectedState"] is { } st) j["state"] = st;
        var tags = p.SelectTokens("$..TagName").Select(t => (string?)t).Where(t => !string.IsNullOrEmpty(t)).Distinct().ToList();
        if (tags.Count > 0) j["tags"] = new JArray(tags);
        var refs = Paths(p).ToList();
        var facts = refs.Where(r => r.Contains("Fact")).Select(r => r.Split('/').Last()).Distinct().ToList();
        if (facts.Count > 0) j["facts"] = new JArray(facts);
        var items = refs.Where(r => r.Contains("/Items/")).Select(r => r.Split('/').Last()).Distinct().ToList();
        if (items.Count > 0) j["items"] = new JArray(items);
        foreach (var flag in new[] { "bNegate", "bInvert", "Operator", "LogicalOperator" })
            if (p[flag] is { } v) j[flag] = v;
        if (depth < 6)
        {
            var subs = new JArray();
            foreach (var r in p.SelectTokens("$..ObjectPath"))
                if (Resolve(exports, r.Parent?.Parent) is { } sub && sub != cond && (sub.Class?.Name ?? "").Contains("Condition"))
                    subs.Add(Condition(exports, sub, depth + 1));
            if (subs.Count > 0) j["all"] = subs;
        }
        return j;
    }

    /// A component's world transform: its relative one under its attach parent's.
    static Xf World(List<UObject> exports, UObject comp, int depth)
    {
        var p = Props(comp);
        var local = Xf.From(p["RelativeLocation"], p["RelativeRotation"], p["RelativeScale3D"]);
        var parent = depth < 16 ? Resolve(exports, p["AttachParent"]) : null;
        return parent == null || parent == comp ? local : World(exports, parent, depth + 1).Apply(local);
    }

    public IEnumerable<JObject> Actors(string map)
    {
        var pkg = provider.LoadPackage(map);
        var exports = pkg.GetExports().ToList();
        var byOuter = exports.Where(e => e.Outer != null).GroupBy(e => e.Outer!.Name).ToDictionary(g => g.Key, g => g.ToList());
        var cell = Path.GetFileNameWithoutExtension(map);
        var found = new List<JObject>();
        foreach (var actor in exports.Where(e => e.Outer?.Name == "PersistentLevel"))
        {
            if (!byOuter.TryGetValue(actor.Name, out var comps)) continue;
            // The ways out of a region: the APC's door, and the save points that take the
            // hero to it (all but the `…NoTravel…` ones), wherever they are.
            var cls = actor.Class?.Name ?? "";
            string? travel = cls.Contains("APC_Enter") ? "apc"
                : cls.Contains("SavePoint") ? (cls.Contains("NoTravel") ? "save.local" : "save")
                : null;
            // What turns it on (a receiver's `Activators`, on its action components): the
            // edges of the requirement graph (.spec/GRAPH.md). Activators and receivers are
            // kept whatever else they hold, so both ends of an edge are in the survey.
            // `Class'Map:PersistentLevel.<actor>'`: the actor's full name.
            var activators = comps
                .SelectMany(c => Props(c)["Activators"]?.SelectTokens("$..InteractableActor.ObjectName") ?? [])
                .Select(t => ((string?)t ?? "").TrimEnd('\'').Split('.').Last())
                .Where(n => n.Length > 0)
                .Distinct().ToList();
            // What can be an end of an edge: whatever a receiver names as its activator —
            // levers, slots, area triggers (`TriggerNoActions`), doors, first-generation
            // Lymbic activators, cinematic callers, drone translations.
            var linked = activators.Count > 0
                || new[] { "Activator", "Receiver", "Trigger", "Door", "Caller", "DroneTranslation", "_Interact_BP", "QuestListener" }
                    .Any(k => cls.Contains(k));
            // A spawner: its enemies defeated give its guaranteed drop (a fight's outcome:
            // `Quest.Facts.MarastanMarketCleared`, a wave stopped), and it wakes on tags.
            var spawner = cls.EndsWith("_Spawner_C") ? Props(actor) : null;
            var drop = spawner == null ? null : Payload(spawner["GuaranteedDropSpawnerPayload"]);
            var wakes = Tags(spawner?["ActivationRequiredTags"]).ToList();
            var sleeps = Tags(spawner?["ActivationBlockedTags"]).ToList();
            var fight = drop != null && (!Empty(drop) || wakes.Count > 0);
            if (travel == null && !linked && !fight && !comps.Any(c => Wanted.Contains(c.Class?.Name))) continue;
            var rec = new JObject { ["name"] = actor.Name, ["class"] = actor.Class?.Name ?? "", ["cell"] = cell };
            // Where: the root component in world space — through what it is attached to.
            var actorProps = Props(actor);
            var root = Resolve(exports, actorProps["RootComponent"])
                       ?? comps.FirstOrDefault(c => Props(c)["RelativeLocation"] != null);
            var at = root == null ? new Xf() : World(exports, root, 0);
            rec["at"] = new JArray(Math.Round(at.T.X), Math.Round(at.T.Y), Math.Round(at.T.Z));
            var layers = actorProps.Properties().Where(p => p.Name.Contains("DataLayer")).SelectMany(p => Objects(p.Value)).Distinct().ToList();
            if (layers.Count > 0) rec["layers"] = new JArray(layers);
            // A puzzle: its dials (in name order), its keypad code, or the items it takes.
            var puzzle = PuzzleOf(comps);
            if (puzzle != null && (actor.Class?.Name ?? "").Contains("PuzzleCheck"))
                puzzle["expects"] = Expected(actor);
            if (puzzle != null) rec["puzzle"] = puzzle;
            foreach (var c in comps)
            {
                if (!Wanted.Contains(c.Class?.Name)) continue;
                var props = Props(c);
                switch (c.Class?.Name)
                {
                    case "SaveIdentifierRuneComponent":
                        rec["guid"] = (string?)props["Rune"]?["Identifier"];
                        break;
                    case "PayloadRuneComponent":
                        var p = Payload(props["Rune"]?["PayloadData"]);
                        foreach (var f in Objects(props["Rune"]?["ContainedFacts"])) ((JArray)p["facts"]!).Add(f);
                        if (!Empty(p)) rec["payload"] = p;
                        break;
                    case "FlowComponent":
                        var flow = Paths(props["RootFlow"]).FirstOrDefault();
                        if (flow != null)
                        {
                            rec["flow"] = flow;
                            flowsToRead.Add(flow);
                        }
                        break;
                    case "TradeGiveItemRuneComponent":
                        var trades = new JArray();
                        foreach (var t in props["Rune"]?["ValidTrades"] ?? new JArray())
                            trades.Add(new JObject { ["item"] = Paths(t["Item"]).FirstOrDefault(), ["payload"] = Payload(t["Payload"]) });
                        if (trades.Count > 0) rec["trades"] = trades;
                        break;
                }
            }
            // An NPC whose trades or conversation are its blueprint's (the component templates,
            // which the placed actor does not repeat): read from the class's own package.
            if (rec["trades"] == null || rec["flow"] == null)
            {
                foreach (var t in ClassTemplates(actor))
                {
                    var tp = Props(t);
                    if (rec["trades"] == null && (t.Class?.Name ?? "") == "TradeGiveItemRuneComponent")
                    {
                        var trades = new JArray();
                        foreach (var tr in tp["Rune"]?["ValidTrades"] ?? new JArray())
                            trades.Add(new JObject { ["item"] = Paths(tr["Item"]).FirstOrDefault(), ["payload"] = Payload(tr["Payload"]) });
                        if (trades.Count > 0) rec["trades"] = trades;
                    }
                    if (rec["flow"] == null && (t.Class?.Name ?? "") == "FlowComponent" && Paths(tp["RootFlow"]).FirstOrDefault() is { } flow)
                    {
                        rec["flow"] = flow;
                        flowsToRead.Add(flow);
                    }
                }
            }
            // A Vault of Forbidden Knowledge's dial door: where the vault notebook guides to.
            var vault = rec["class"]!.ToString().StartsWith("VOFK_") && rec["class"]!.ToString().Contains("DialPuzzle");
            if (vault) rec["vault"] = true;
            if (travel != null) rec["travel"] = travel;

            // A door that opens from one side only ("locked from the other side"): where the
            // hero must stand to open it, its `TriggerUnlockable` box — the placed copy's, or
            // the blueprint's under the actor's root.
            if (cls.Contains("OneSidedLock"))
            {
                var from = comps.FirstOrDefault(c => c.Name == "TriggerUnlockable") is { } box && Props(box)["RelativeLocation"] != null
                    ? World(exports, box, 0)
                    : ClassTemplates(actor).FirstOrDefault(t => t.Name.StartsWith("TriggerUnlockable")) is { } tb
                        ? at.Apply(Xf.From(Props(tb)["RelativeLocation"], Props(tb)["RelativeRotation"], Props(tb)["RelativeScale3D"]))
                        : (Xf?)null;
                if (from is { } f)
                    rec["opens_from"] = new JArray(Math.Round(f.T.X), Math.Round(f.T.Y), Math.Round(f.T.Z));
            }
            if (activators.Count > 0) rec["activators"] = new JArray(activators);
            // A quest listener sets facts by its blueprint's logic (a boss killed, a photo
            // taken): the tags it names (`Tag_…`) are what it can give, as the story goes.
            if (cls.Contains("QuestListener"))
            {
                var lp = Props(actor);
                var tags = lp.Properties().Where(x => x.Name.StartsWith("Tag_"))
                    .SelectMany(x => x.Value.SelectTokens("$..TagName")).Select(t => (string?)t)
                    .Where(t => !string.IsNullOrEmpty(t)).Distinct().ToList();
                if (tags.Count > 0) rec["script_tags"] = new JArray(tags);
            }
            // How the activators must be used (`MultiActivatorsActivationAction`: in the order
            // listed, all of them, within a time; `MultiActivatorsStateAction`: each turned to
            // its position): the answer of an order or position puzzle.
            foreach (var c in comps)
            {
                var cp = Props(c);
                var logic = new JObject();
                if (cp["HasOrder"] is { } ord) logic["order"] = ord;
                if (cp["Wait for All Activators"] is { } all) logic["all"] = all;
                if (cp["HasTimer"]?.Value<bool>() == true && cp["TimerDuration"] is { } secs) logic["timer"] = secs;
                if (cp["ActivatorSolution"] is JArray sol && sol.Count > 0) logic["solution"] = sol;
                if (logic.Count > 0) rec["logic"] = logic;
            }
            // When it can be used (`ActionCondition` on its actions): the other end of the
            // graph's state edges — another interactable used, a fact or tag known.
            var conds = new JArray();
            foreach (var c in comps)
                if (Resolve(exports, Props(c)["ActionCondition"]?["Condition"]) is { } cond)
                    conds.Add(Condition(exports, cond, 0));
            if (fight)
            {
                if (!Empty(drop!)) rec["payload"] = drop;
                if (wakes.Count > 0) conds.Add(new JObject { ["type"] = "DoesHeroHasFactCondition_BP_C", ["tags"] = new JArray(wakes) });
                if (sleeps.Count > 0) conds.Add(new JObject { ["type"] = "DoesHeroHasNotFactCondition_BP_C", ["tags"] = new JArray(sleeps) });
                rec["fight"] = true;
            }
            if (conds.Count > 0) rec["conditions"] = conds;
            if (rec["payload"] != null || rec["flow"] != null || rec["trades"] != null || vault || puzzle != null || travel != null || linked || fight) found.Add(rec);
        }
        return found;
    }

    /// Every conversation NPCs run, following topic and sub-graphs: what each hands out.
    public JObject Flows()
    {
        var done = new JObject();
        var queue = new Queue<string>(flowsToRead);
        // A person's introduction, said before the topics, beside the root the actor names
        // (`X_ConvoRoot_FA` → `X_ConvoIntro_FA`, `…RootV2…` → `…IntroV2…`).
        foreach (var root in flowsToRead.ToList())
        {
            var intro = root.Replace("_ConvoRoot", "_ConvoIntro");
            if (intro != root && provider.Files.Keys.Any(k => k.EndsWith(intro.Replace("/Game/", "Content/") + ".uasset", StringComparison.OrdinalIgnoreCase)))
                queue.Enqueue(intro);
        }
        while (queue.Count > 0)
        {
            var path = queue.Dequeue();
            if (done[path] != null) continue;
            var payloads = new JArray();
            var subs = new List<string>();
            var gated = new JArray();
            var gatedSubs = new JArray();
            try
            {
                var exports = provider.LoadPackage(path).GetExports().ToList();
                var main = path.Split('/').Last();
                // What must hold to reach each node of the conversation from its start (GRAPH.md §13).
                var reach = Reach(exports.Where(e => e.Outer?.Name == main).ToList(), path);
                foreach (var e in exports.Where(e => e.Outer?.Name == main))
                {
                    var cls = e.Class?.Name ?? "";
                    var guid = (string?)Props(e)["NodeGuid"] ?? "";
                    if (cls == "FlowNode_Payload")
                    {
                        var p = Payload(Props(e)["PayloadData"]);
                        if (Empty(p)) continue;
                        payloads.Add(p);
                        gated.Add(new JObject { ["payload"] = p, ["need"] = reach.TryGetValue(guid, out var n) ? n : null, ["reached"] = reach.ContainsKey(guid) });
                    }
                    else if (cls is "FlowNode_TopicSubGraph" or "FlowNode_SubGraph" or "FlowNode_SubGraphInstanced")
                    {
                        var props = Props(e);
                        foreach (var a in Paths(props["TopicAsset"]).Concat(Paths(props["Asset"])).Distinct())
                        {
                            var asset = a.Contains('.') ? a[..a.LastIndexOf('.')] : a;
                            subs.Add(asset);
                            gatedSubs.Add(new JObject { ["asset"] = asset, ["need"] = reach.TryGetValue(guid, out var n) ? n : null, ["reached"] = reach.ContainsKey(guid) });
                            if (done[asset] == null) queue.Enqueue(asset);
                        }
                    }
                }
            }
            catch (Exception ex) { Console.Error.WriteLine($"  flow {path}: {ex.Message}"); }
            done[path] = new JObject
            {
                ["payloads"] = payloads,
                ["subgraphs"] = new JArray(subs.Distinct()),
                ["gated"] = gated,
                ["gated_subgraphs"] = gatedSubs,
            };
        }
        return done;
    }

    // A condition, as the graph reads it (graph.rs `need_of_flow`): {"all":[…]}, {"any":[…]},
    // {"fact": name, "has": bool} (a fact or a tag), {"identity": name}; null: none.
    static JObject? And(JObject? a, JObject? b)
    {
        if (a == null) return b;
        if (b == null) return a;
        var all = new JArray();
        foreach (var x in new[] { a, b })
            if (x["all"] is JArray xs) foreach (var y in xs) all.Add(y.DeepClone());
            else all.Add(x.DeepClone());
        return new JObject { ["all"] = all };
    }

    static JObject? Or(IEnumerable<JObject?> alternatives)
    {
        var list = alternatives.ToList();
        if (list.Count == 0 || list.Any(x => x == null)) return null;
        var distinct = list.Select(x => x!.ToString(Formatting.None)).Distinct().Select(JObject.Parse).ToList();
        return distinct.Count == 1 ? distinct[0] : new JObject { ["any"] = new JArray(distinct) };
    }

    static JObject Fact(string name, bool has) => new() { ["fact"] = name, ["has"] = has };

    /// What leaving `node` by `pin` says holds: a fact check passed or failed, a condition
    /// sub-graph finished, a topic's identity known.
    JObject? EdgeCondition(UObject node, string pin, string package)
    {
        var cls = node.Class?.Name ?? "";
        var p = Props(node);
        var passed = pin is "Passed" or "True" or "Success" or "Finish" or "Out";
        switch (cls)
        {
            case "FlowNode_HasFact":
            case "FlowNode_DoesNotHaveFact":
            {
                var f = Objects(p["Fact"]).FirstOrDefault();
                if (f == null || !(pin is "Passed" or "Failed")) return null;
                return Fact(f, (cls == "FlowNode_HasFact") == (pin == "Passed"));
            }
            case "FlowNode_HasTagFact":
            case "FlowNode_DoesNotHaveTagFact":
            {
                var t = Tags(p["TagFact"]).FirstOrDefault() ?? (string?)p["TagFact"]?["TagName"];
                if (t == null || !(pin is "Passed" or "Failed")) return null;
                return Fact(t, (cls == "FlowNode_HasTagFact") == (pin == "Passed"));
            }
            case "FlowNode_ConditionSubGraph":
            {
                if (!passed) return null;
                var asset = (string?)p["Asset"]?["AssetPathName"] ?? "";
                var sub = (string?)p["Asset"]?["SubPathString"] ?? "";
                var pkg = asset.Contains('.') ? asset[..asset.LastIndexOf('.')] : asset;
                return ConditionOf(pkg.Length > 0 ? pkg : package, sub);
            }
            case "FlowNode_QuestionSelector":
            {
                var id = Objects(p["TopicIdentity"]).FirstOrDefault();
                return id == null ? null : new JObject { ["identity"] = id };
            }
        }
        return null;
    }

    /// Forward from a graph's start nodes: for each node, what must hold on some way to it
    /// (up to four ways), by its NodeGuid.
    Dictionary<string, JObject?> Reach(List<UObject> graph, string package)
    {
        var byGuid = graph.Where(e => Props(e)["NodeGuid"] != null).ToDictionary(e => (string)Props(e)["NodeGuid"]!, e => e);
        var ways = new Dictionary<string, List<JObject?>>();
        var queue = new Queue<string>();
        foreach (var (g, e) in byGuid)
            if ((e.Class?.Name ?? "") == "FlowNode_Start" && Props(e)["Connections"] is JArray { Count: > 0 })
            {
                ways[g] = [null];
                queue.Enqueue(g);
            }
        var steps = 0;
        while (queue.Count > 0 && steps++ < 20000)
        {
            var g = queue.Dequeue();
            var node = byGuid[g];
            foreach (var c in Props(node)["Connections"] as JArray ?? [])
            {
                var pin = (string?)c["Key"] ?? "";
                var to = (string?)c["Value"]?["NodeGuid"];
                if (to == null || !byGuid.ContainsKey(to)) continue;
                var edge = EdgeCondition(node, pin, package);
                var list = ways.TryGetValue(to, out var l) ? l : ways[to] = [];
                var added = false;
                foreach (var w in ways[g].ToList())
                {
                    var next = And(w, edge);
                    var key = next?.ToString(Formatting.None) ?? "";
                    if (list.Count >= 4 || list.Any(x => (x?.ToString(Formatting.None) ?? "") == key)) continue;
                    list.Add(next);
                    added = true;
                }
                if (added) queue.Enqueue(to);
            }
        }
        return ways.ToDictionary(kv => kv.Key, kv => Or(kv.Value));
    }

    /// A condition flow (`Condition_N` of a package) as an expression: back from its finish,
    /// each fact check on the way, an AND node all of its inputs.
    readonly Dictionary<string, JObject?> conditions = new();
    JObject? ConditionOf(string package, string sub)
    {
        var key = package + ":" + sub;
        if (conditions.TryGetValue(key, out var known)) return known;
        conditions[key] = null;
        JObject? result = null;
        try
        {
            var exports = provider.LoadPackage(package).GetExports().ToList();
            var owner = sub.Length > 0 ? sub : package.Split('/').Last();
            var nodes = exports.Where(e => e.Outer?.Name == owner && Props(e)["NodeGuid"] != null).ToList();
            var byGuid = nodes.ToDictionary(e => (string)Props(e)["NodeGuid"]!, e => e);
            var into = new Dictionary<string, List<(UObject from, string pin, string toPin)>>();
            foreach (var e in nodes)
                foreach (var c in Props(e)["Connections"] as JArray ?? [])
                {
                    var to = (string?)c["Value"]?["NodeGuid"];
                    if (to == null) continue;
                    (into.TryGetValue(to, out var l) ? l : into[to] = []).Add((e, (string?)c["Key"] ?? "", (string?)c["Value"]?["PinName"] ?? ""));
                }
            var memo = new Dictionary<string, JObject?>();
            JObject? In(string g, int depth)
            {
                if (depth > 40) return null;
                if (memo.TryGetValue(g, out var m)) return m;
                memo[g] = null;
                var node = byGuid[g];
                var preds = into.TryGetValue(g, out var l) ? l : [];
                JObject? r;
                if ((node.Class?.Name ?? "") == "FlowNode_LogicalAND")
                    r = preds.GroupBy(x => x.toPin).Select(pin => Or(pin.Select(x => Out(x.from, x.pin, depth + 1)))).Aggregate((JObject?)null, And);
                else
                    r = preds.Count == 0 ? null : Or(preds.Select(x => Out(x.from, x.pin, depth + 1)));
                memo[g] = r;
                return r;
            }
            JObject? Out(UObject from, string pin, int depth) => And(In((string)Props(from)["NodeGuid"]!, depth), EdgeCondition(from, pin, package));
            var finish = byGuid.Where(kv => (kv.Value.Class?.Name ?? "") == "FlowNode_Finish").Select(kv => kv.Key).ToList();
            result = Or(finish.Select(f => In(f, 0)));
        }
        catch (Exception ex) { Console.Error.WriteLine($"  condition {key}: {ex.Message}"); }
        conditions[key] = result;
        return result;
    }

    /// The world settings' payloads: `enter` (FirstEnterWorldPayloadData) and `boss`
    /// (BossFightRoomCompletedPayload, with where the fight's room is).
    public JObject? WorldGives(string world)
    {
        var root = Maps(world).FirstOrDefault(m => m.EndsWith($"{world}_Root_WP.umap", StringComparison.OrdinalIgnoreCase));
        if (root == null) return null;
        var exports = provider.LoadPackage(root).GetExports().ToList();
        var settings = exports.FirstOrDefault(e => (e.Class?.Name ?? "").EndsWith("WorldSettings"));
        if (settings == null) return null;
        var p = Props(settings);
        var gives = new JObject();
        var enter = Payload(p["FirstEnterWorldPayloadData"]);
        if (!Empty(enter)) gives["enter"] = enter;
        var boss = Payload(p["BossFightRoomCompletedPayload"]);
        if (!Empty(boss))
        {
            if (Resolve(exports, p["BossFightRoomTeleportPlayerLocation"]) is { } spot
                && Resolve(exports, Props(spot)["RootComponent"]) is { } rc)
            {
                var at = World(exports, rc, 0);
                boss["at"] = new JArray(Math.Round(at.T.X), Math.Round(at.T.Y), Math.Round(at.T.Z));
            }
            gives["boss"] = boss;
        }
        return gives.Count > 0 ? gives : null;
    }

    /// The component templates of an actor's blueprint (its `_GEN_VARIABLE`s), read once a
    /// class.
    readonly Dictionary<string, List<UObject>> templates = new();
    List<UObject> ClassTemplates(UObject actor)
    {
        var cls = actor.Class?.GetPathName() ?? "";
        var dot = cls.LastIndexOf('.');
        if (dot < 0 || !cls.StartsWith("/Game/")) return [];
        var pkg = cls[..dot];
        if (templates.TryGetValue(pkg, out var cached)) return cached;
        var found = new List<UObject>();
        try
        {
            found = provider.LoadPackage(pkg).GetExports()
                .Where(e => e.Name.EndsWith("_GEN_VARIABLE")
                    && (e.Class?.Name is "TradeGiveItemRuneComponent" or "FlowComponent" || e.Name.StartsWith("TriggerUnlockable")))
                .ToList();
        }
        catch { }
        templates[pkg] = found;
        return found;
    }

    /// The landscape components of `map` sampled into `grid` (cell → highest ground, cm):
    /// each component's heightmap (8-bit R high, G low; 32768 is 0, 128 a unit) through the
    /// component's world transform. Returns how many components were read.
    public int Terrain(string map, double cell, Dictionary<(int, int), double> grid, JArray? raw = null)
    {
        var exports = provider.LoadPackage(map).GetExports().ToList();
        var n = 0;
        foreach (var comp in exports.Where(e => e.Class?.Name == "LandscapeComponent"))
        {
            var p = Props(comp);
            var size = (int?)p["ComponentSizeQuads"] ?? 0;
            if (size <= 0 || Resolve(exports, p["HeightmapTexture"]) is not CUE4Parse.UE4.Assets.Exports.Texture.UTexture2D tex) continue;
            var mip = tex.GetFirstMip();
            var data = mip?.BulkData?.Data;
            if (mip == null || data == null) continue;
            int tw = mip.SizeX, th = mip.SizeY;
            if (data.Length < tw * th * 4) continue;
            var bias = p["HeightmapScaleBias"];
            int ox = (int)Math.Round(((double?)bias?["Z"] ?? 0) * tw), oy = (int)Math.Round(((double?)bias?["W"] ?? 0) * th);
            // The component sits in its proxy's landscape space at its section base (quads), less
            // the proxy's section offset; the proxy's root carries the landscape's scale and turn.
            var proxy = exports.FirstOrDefault(e => e.Name == comp.Outer?.Name && e.Outer?.Name == "PersistentLevel");
            var pp = Props(proxy);
            var root = Resolve(exports, pp["RootComponent"]);
            if (root == null) continue;
            var xf = World(exports, root, 0);
            double bx = (double?)p["SectionBaseX"] ?? 0, by = (double?)p["SectionBaseY"] ?? 0;
            double sx = (double?)pp["LandscapeSectionOffset"]?["X"] ?? 0, sy = (double?)pp["LandscapeSectionOffset"]?["Y"] ?? 0;
            var step = Math.Max(1, (int)Math.Round(cell / Math.Max(1.0, xf.S.X) / 2));
            var samples = new JArray();
            for (var j = 0; j <= size; j += step)
                for (var i = 0; i <= size; i += step)
                {
                    int tx = ox + i, ty = oy + j;
                    if (tx >= tw || ty >= th) continue;
                    var o = (ty * tw + tx) * 4;
                    // B8G8R8A8: B, G, R, A in memory; R high byte, G low.
                    var h = data[o + 2] << 8 | data[o + 1];
                    var local = new Xf(System.Numerics.Quaternion.Identity, new System.Numerics.Vector3((float)(bx - sx + i), (float)(by - sy + j), (h - 32768) / 128f), System.Numerics.Vector3.One);
                    var at = xf.Apply(local).T;
                    // A transform not read right puts samples nowhere: only those on the map.
                    if (!float.IsFinite(at.X) || !float.IsFinite(at.Y) || !float.IsFinite(at.Z) || Math.Abs(at.X) > 5e6 || Math.Abs(at.Y) > 5e6) continue;
                    var key = ((int)Math.Floor(at.X / cell), (int)Math.Floor(at.Y / cell));
                    if (!grid.TryGetValue(key, out var z) || at.Z > z) grid[key] = at.Z;
                }
            raw?.Add(new JObject { ["base"] = new JArray(bx, by), ["offset"] = new JArray(sx, sy), ["t"] = new JArray(xf.T.X, xf.T.Y, xf.T.Z), ["s"] = new JArray(xf.S.X, xf.S.Y, xf.S.Z), ["q"] = new JArray(xf.Q.X, xf.Q.Y, xf.Q.Z, xf.Q.W), ["size"] = size });
            n++;
        }
        return n;
    }

    /// Every export whose properties name `want`: what points at an actor (`--refs`).
    public void Refs(string map, string want)
    {
        var pkg = provider.LoadPackage(map);
        foreach (var e in pkg.GetExports())
        {
            if (e.GetPathName().Contains(want)) continue;
            var text = Props(e).ToString(Formatting.None);
            if (!text.Contains(want)) continue;
            Console.WriteLine($"== {map} {e.Outer?.Name} {e.Name} [{e.Class?.Name}]");
            Console.WriteLine(Props(e).ToString(Formatting.Indented));
        }
    }

    public void Peek(string map, string want)
    {
        var pkg = provider.LoadPackage(map);
        foreach (var e in pkg.GetExports())
        {
            // By its path, so a sub-object (an action's condition) of the actor is shown too.
            if (!e.GetPathName().Contains(want)) continue;
            Console.WriteLine($"== {map} {e.Outer?.Name} {e.Name} [{e.Class?.Name}]");
            Console.WriteLine(Props(e).ToString(Formatting.Indented));
        }
    }
}

/// A transform: rotation (quaternion), translation, scale — Unreal's conventions.
record struct Xf(System.Numerics.Quaternion Q, System.Numerics.Vector3 T, System.Numerics.Vector3 S)
{
    public Xf() : this(System.Numerics.Quaternion.Identity, System.Numerics.Vector3.Zero, System.Numerics.Vector3.One) { }

    static float F(JToken? t, string k, float d) => t?[k] is { } v && v.Type is JTokenType.Float or JTokenType.Integer ? (float)v : d;

    /// From a location, an FRotator (pitch, yaw, roll in degrees) and a scale.
    public static Xf From(JToken? loc, JToken? rot, JToken? scale)
    {
        var (p, y, r) = (F(rot, "Pitch", 0) * MathF.PI / 360, F(rot, "Yaw", 0) * MathF.PI / 360, F(rot, "Roll", 0) * MathF.PI / 360);
        var (sp, cp, sy, cy, sr, cr) = (MathF.Sin(p), MathF.Cos(p), MathF.Sin(y), MathF.Cos(y), MathF.Sin(r), MathF.Cos(r));
        var q = new System.Numerics.Quaternion(
            cr * sp * sy - sr * cp * cy,
            -cr * sp * cy - sr * cp * sy,
            cr * cp * sy - sr * sp * cy,
            cr * cp * cy + sr * sp * sy);
        return new Xf(q, new(F(loc, "X", 0), F(loc, "Y", 0), F(loc, "Z", 0)), new(F(scale, "X", 1), F(scale, "Y", 1), F(scale, "Z", 1)));
    }

    /// `child` (relative to this) in this one's space.
    public Xf Apply(Xf child) => new(
        System.Numerics.Quaternion.Normalize(Q * child.Q),
        T + System.Numerics.Vector3.Transform(S * child.T, Q),
        S * child.S);
}

static class Args
{
    public static Dictionary<string, string> Parse(string[] a)
    {
        var d = new Dictionary<string, string>();
        for (var i = 0; i < a.Length; i++)
        {
            if (!a[i].StartsWith("--")) continue;
            var k = a[i][2..];
            d[k] = i + 1 < a.Length && !a[i + 1].StartsWith("--") ? a[++i] : "true";
        }
        return d;
    }
}

static class Locale
{
    public static string Esc(string s) => s.Replace("\\", "\\\\").Replace("\t", "\\t").Replace("\r", "\\r").Replace("\n", "\\n");

    public static void Write(DefaultFileProvider provider, string dir)
    {
        var byCulture = provider.Files.Keys
            .Select(p => System.Text.RegularExpressions.Regex.Match(p, @"^HellIsUs/Content/Localization/[^/]+/([^/]+)/[^/]+\.locres$", System.Text.RegularExpressions.RegexOptions.IgnoreCase))
            .Where(m => m.Success)
            .GroupBy(m => m.Groups[1].Value, m => m.Value);
        foreach (var g in byCulture)
        {
            var lines = new List<string>();
            foreach (var path in g.OrderBy(p => p))
            {
                if (!provider.TryCreateReader(path, out var ar)) continue;
                var res = new CUE4Parse.UE4.Localization.FTextLocalizationResource(ar);
                foreach (var (ns, entries) in res.Entries)
                    foreach (var (key, entry) in entries)
                        lines.Add($"{Esc(ns.Str)}\t{Esc(key.Str)}\t{Esc(entry.LocalizedString)}");
            }
            lines.Sort(StringComparer.Ordinal);
            var file = Path.Combine(dir, $"{g.Key}.tsv");
            File.WriteAllLines(file, lines);
            Console.Error.WriteLine($"{g.Key}: {lines.Count} → {file}");
        }
    }

    /// names.tsv — the text that names a thing, as `kind  id  namespace  key  [more]`:
    ///   item    <data asset name, lower case>                ns key
    ///   region  <world>                                      ns key
    ///   npc     <blueprint class, lower case>                ns key <story unit or ->
    ///   name    <name fact asset>                            ns key <story unit>
    /// An NPC's row is the name it shows when met; a `name` row of its story unit the hero
    /// knows (the real name, learned later) wins over it.
    public static void Names(DefaultFileProvider provider, string dir, List<string> worlds)
    {
        var rows = new SortedSet<string>(StringComparer.Ordinal);
        var tableNs = new Dictionary<string, string?>();

        string FilePath(string objectPath)
        {
            var p = objectPath.Split('.')[0];
            return p.StartsWith("/Game/") ? "HellIsUs/Content/" + p["/Game/".Length..] : p.TrimStart('/');
        }
        string? Namespace(string tableId)
        {
            if (tableNs.TryGetValue(tableId, out var ns)) return ns;
            try
            {
                var st = provider.LoadPackage(FilePath(tableId)).GetExports().OfType<CUE4Parse.UE4.Assets.Exports.Internationalization.UStringTable>().First();
                ns = st.StringTable.TableNamespace;
            }
            catch { ns = null; }
            return tableNs[tableId] = ns;
        }
        (string, string)? Text(JToken? t)
        {
            if (t is not JObject o || o["Key"]?.ToString() is not { Length: > 0 } key) return null;
            if (o["TableId"]?.ToString() is { Length: > 0 } table)
                return Namespace(table) is { } ns ? (ns, key) : null;
            return o["Namespace"] is { } n ? (n.ToString(), key) : null;
        }
        IEnumerable<JObject> Exports(string path)
        {
            List<JObject> list;
            try { list = JArray.Parse(JsonConvert.SerializeObject(provider.LoadPackage(path).GetExports())).OfType<JObject>().ToList(); }
            catch (Exception e) { Console.Error.WriteLine($"  {path}: {e.Message}"); list = []; }
            return list;
        }
        string Esc(string s) => Locale.Esc(s);
        string UnitOf(string objectPath)
        {
            var m = System.Text.RegularExpressions.Regex.Match(objectPath, @"/StoryUnits/([^/]+)/");
            return m.Success ? m.Groups[1].Value : "-";
        }
        var files = provider.Files.Keys.ToList();

        // Items: every data asset under Items/ with a Name.
        foreach (var path in files.Where(p => p.StartsWith("HellIsUs/Content/Items/") && p.EndsWith("_DA.uasset")))
            foreach (var e in Exports(path[..^".uasset".Length]))
                if (Text(e["Properties"]?["Name"]) is var (ns, key))
                    rows.Add($"item\t{Esc(e["Name"]!.ToString().ToLowerInvariant())}\t{Esc(ns)}\t{Esc(key)}");

        // Regions: the worlds' location names.
        foreach (var w in worlds)
            rows.Add($"region\t{w}\tFacts_Shared\tUniversal_Location_{w}");

        // Name facts of the story units.
        foreach (var path in files.Where(p => p.StartsWith("HellIsUs/Content/GameData/StoryUnits/") && p.EndsWith("TextFact_DA.uasset")))
            foreach (var e in Exports(path[..^".uasset".Length]))
                if (e["Type"]?.ToString() == "StringFactData" && e["Properties"]?["Track"]?.ToString() == "Name"
                    && Text(e["Properties"]?["Description"]) is var (ns, key))
                    rows.Add($"name\t{Esc(e["Name"]!.ToString())}\t{Esc(ns)}\t{Esc(key)}\t{UnitOf(path)}");

        // NPCs: a quick chat's speaker, or a conversation's name fact.
        var factText = new Dictionary<string, (string, string)?>();
        foreach (var path in files.Where(p => p.StartsWith("HellIsUs/Content/Gameplay/DynamicInteract/NPCs/") && p.EndsWith("_BP.uasset")))
        {
            var cls = Path.GetFileNameWithoutExtension(path).ToLowerInvariant() + "_c";
            foreach (var e in Exports(path[..^".uasset".Length]))
            {
                var rune = e["Properties"]?["Rune"];
                if (Text(rune?["SpeakerInfos"]?["SpeakerName"]) is var (ns, key))
                {
                    rows.Add($"npc\t{cls}\t{Esc(ns)}\t{Esc(key)}\t-");
                    break;
                }
                if (rune?["NamePayload"]?["ContainedFacts"]?.FirstOrDefault()?["ObjectPath"]?.ToString() is { } fact)
                {
                    if (!factText.TryGetValue(fact, out var t))
                        factText[fact] = t = Exports(FilePath(fact)).Select(x => Text(x["Properties"]?["Description"])).FirstOrDefault(x => x != null);
                    if (t is var (fns, fkey))
                    {
                        rows.Add($"npc\t{cls}\t{Esc(fns)}\t{Esc(fkey)}\t{UnitOf(fact)}");
                        break;
                    }
                }
            }
        }
        var file = Path.Combine(dir, "names.tsv");
        File.WriteAllLines(file, rows);
        Console.Error.WriteLine($"names: {rows.Count} → {file}");
    }

    /// facts.tsv — the Datapad's facts that have text, as
    /// `fact  <asset name>  ns  key  <story unit or quest>  <track>  <category>`:
    /// what the clue board shows of a known fact, and what it groups the fact under.
    public static void Facts(DefaultFileProvider provider, string dir)
    {
        var rows = new SortedSet<string>(StringComparer.Ordinal);
        var tableNs = new Dictionary<string, string?>();
        string FilePath(string objectPath)
        {
            var p = objectPath.Split('.')[0];
            return p.StartsWith("/Game/") ? "HellIsUs/Content/" + p["/Game/".Length..] : p.TrimStart('/');
        }
        string? Namespace(string tableId)
        {
            if (tableNs.TryGetValue(tableId, out var ns)) return ns;
            try
            {
                var st = provider.LoadPackage(FilePath(tableId)).GetExports().OfType<CUE4Parse.UE4.Assets.Exports.Internationalization.UStringTable>().First();
                ns = st.StringTable.TableNamespace;
            }
            catch { ns = null; }
            return tableNs[tableId] = ns;
        }
        var re = new System.Text.RegularExpressions.Regex(@"^HellIsUs/Content/GameData/(?:StoryUnits|Quests)/([^/]+)/.*TextFact_DA\.uasset$");
        foreach (var path in provider.Files.Keys)
        {
            var m = re.Match(path);
            if (!m.Success) continue;
            List<JObject> exports;
            try { exports = JArray.Parse(JsonConvert.SerializeObject(provider.LoadPackage(path[..^".uasset".Length]).GetExports())).OfType<JObject>().ToList(); }
            catch (Exception e) { Console.Error.WriteLine($"  {path}: {e.Message}"); continue; }
            foreach (var e in exports.Where(x => x["Type"]?.ToString() == "StringFactData"))
            {
                var p = e["Properties"];
                if (p?["Description"] is not JObject d || d["Key"]?.ToString() is not { Length: > 0 } key) continue;
                var ns = d["TableId"]?.ToString() is { Length: > 0 } table ? Namespace(table) : d["Namespace"]?.ToString();
                if (ns == null) continue;
                var category = (p["UICategory"]?["TagName"]?.ToString() ?? "").Split('.').Last();
                rows.Add(string.Join("	", "fact", Esc(e["Name"]!.ToString()), Esc(ns), Esc(key), m.Groups[1].Value,
                    Esc(p["Track"]?.ToString() ?? ""), Esc(category)));
            }
        }
        var file = Path.Combine(dir, "facts.tsv");
        File.WriteAllLines(file, rows);
        Console.Error.WriteLine($"facts: {rows.Count} → {file}");
    }
}
