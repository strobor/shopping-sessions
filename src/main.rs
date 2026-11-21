use std::{collections::{HashMap as StdHashMap, HashSet as StdHashSet}, env, error::Error, fs::{self, File}, io::{BufRead, BufReader}, path::{Path, PathBuf}, sync::Arc};
use arrow_array::{ArrayRef, Date32Array, Int64Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::ArrowWriter;
use shopping_sessions::{csv::parse_line, links::Links, timestamp::parse_ms};
use rayon::prelude::*;

type HashMap<K, V> = StdHashMap<K, V, ahash::RandomState>;
type HashSet<K> = StdHashSet<K, ahash::RandomState>;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const DAY: i64 = 86_400_000;
const WEEK: i64 = 7 * DAY;

fn files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|s| s == "csv") && path.is_file() { paths.push(path); }
    }
    paths.sort();
    Ok(paths)
}
fn read_rows(path: &Path, count: usize, mut f: impl FnMut(Vec<&str>, &str) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    reader.read_line(&mut line)?; // header
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 { break; }
        if line.ends_with('\n') { line.pop(); }
        if let Some(row) = parse_line(&line, count) { f(row, &line)?; }
    }
    Ok(())
}
fn dimensions(path: &Path) -> Result<HashMap<String, String>> {
    let mut result = HashMap::default();
    read_rows(path, 2, |r, _line| { result.insert(r[0].into(), r[1].into()); Ok(()) })?;
    Ok(result)
}
#[derive(Clone)]
struct RawEvent { line: String, ingest: i64 }
struct Event { ts: i64, revenue: i64, category: Option<String> }
struct Click { ts: i64, id: String, campaign: String }
#[derive(Default)]
struct UserDay { sessions: i64, events: i64, revenue: i64, categories: HashMap<String, i64> }
#[derive(Default)]
struct CampaignDay { sessions: i64, purchasing: i64, revenue: i64, assists: i64 }
fn price(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) { return None; }
    let value = s.bytes().try_fold(0_i64, |v, c| v.checked_mul(10)?.checked_add((c-b'0') as i64))?;
    (1..=10_000_000).contains(&value).then_some(value)
}
fn write_table(path: &Path, fields: Vec<Field>, columns: Vec<ArrayRef>) -> Result<()> {
    fs::create_dir_all(path)?;
    let schema = Arc::new(Schema::new(fields));
    let batch = RecordBatch::try_new(schema.clone(), columns)?;
    let mut writer = ArrowWriter::try_new(File::create(path.join("part-00000.parquet"))?, schema, None)?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(())
}
fn string_field(name: &str, nullable: bool) -> Field { Field::new(name, DataType::Utf8, nullable) }
fn int_field(name: &str) -> Field { Field::new(name, DataType::Int64, false) }
fn main() { if let Err(e) = run() { eprintln!("shopping-sessions: {e}"); std::process::exit(1); } }
fn run() -> Result<()> {
    let threads = env::var("NCPU").ok().and_then(|v| v.parse::<usize>().ok()).filter(|n| *n > 0).unwrap_or(8);
    rayon::ThreadPoolBuilder::new().num_threads(threads).build_global()?;
    let mut timer = std::time::Instant::now();
    let timing = env::var_os("SHOPPING_TIMING").is_some();
    macro_rules! mark { ($label:expr) => { if timing { eprintln!("{}: {:.3}s", $label, timer.elapsed().as_secs_f64()); timer = std::time::Instant::now(); } }; }
    let data = PathBuf::from(env::var("DATA_DIR").unwrap_or_else(|_| "/data".into()));
    let output = PathBuf::from(env::var("OUTPUT_DIR").unwrap_or_else(|_| "/output".into()));
    let manifest: serde_json::Value = serde_json::from_reader(File::open(data.join("manifest.json"))?)?;
    let start = parse_ms(manifest["window_start"].as_str().ok_or("missing window_start")?).ok_or("bad window_start")?;
    let end = parse_ms(manifest["window_end"].as_str().ok_or("missing window_end")?).ok_or("bad window_end")?;
    let products = dimensions(&data.join("products.csv"))?;
    let campaigns = dimensions(&data.join("campaigns.csv"))?;
    let mut links = Links::default();
    for path in files(&data.join("links"))? { read_rows(&path, 2, |r, _line| { links.join(r[0], r[1]); Ok(()) })?; }
    mark!("dimensions and links");
    // Deduplicate within each file in parallel, then reconcile IDs across files.
    let event_files = files(&data.join("events"))?;
    let partials: Vec<Result<HashMap<String, RawEvent>>> = event_files.par_iter().map(|path| {
        let mut local = HashMap::<String, RawEvent>::default();
        read_rows(path, 7, |r, line| {
            if !r[0].is_empty() {
                let ingest = parse_ms(r[3]).ok_or("bad ingest timestamp")?;
                let replace = local.get(r[0]).is_none_or(|old| ingest < old.ingest);
                if replace { local.insert(r[0].into(), RawEvent { line: line.to_owned(), ingest }); }
            }
            Ok(())
        })?;
        Ok(local)
    }).collect();
    let mut raw_events = HashMap::<String, RawEvent>::default();
    for partial in partials {
        for (id, event) in partial? {
            use std::collections::hash_map::Entry;
            match raw_events.entry(id) {
                Entry::Vacant(slot) => { slot.insert(event); }
                Entry::Occupied(mut slot) if event.ingest < slot.get().ingest => { slot.insert(event); }
                _ => {}
            }
        }
    }
    mark!("read and deduplicate events");
    // Validate retained rows concurrently, then group them by resolved person.
    let validated: Vec<(String, Event)> = raw_events.into_par_iter().filter_map(|(_, e)| {
        let r = parse_line(&e.line, 7).expect("stored row was parsed already");
        if r[1].is_empty() || r[5].is_empty() { return None; }
        let ts = parse_ms(r[2])?;
        if ts < start || ts >= end || e.ingest - ts > DAY { return None; }
        let (revenue, category) = match r[4] {
            "view" | "cart" => (0, None),
            "purchase" => (price(r[6])?, Some(products.get(r[5]).cloned().unwrap_or_else(|| "unknown".into()))),
            _ => return None,
        };
        Some((links.name_shared(r[1]).to_owned(), Event { ts, revenue, category }))
    }).collect();
    let mut events = HashMap::<String, Vec<Event>>::default();
    for (person, event) in validated { events.entry(person).or_default().push(event); }
    mark!("validate and group events");
    let mut seen = HashSet::default();
    let mut clicks = HashMap::<String, Vec<Click>>::default();
    let click_files = files(&data.join("clicks"))?;
    // Valid copies of a click have identical values. Read and validate files in
    // bounded batches, then deduplicate globally before grouping by person.
    for batch in click_files.chunks(threads) {
        let partials: Vec<Result<Vec<(String, Click)>>> = batch.par_iter().map(|path| {
            let mut local = Vec::new();
            read_rows(path, 4, |r, _line| {
                if r[0].is_empty() || r[1].is_empty() || !campaigns.contains_key(r[2]) { return Ok(()); }
                let Some(ts) = parse_ms(r[3]) else { return Ok(()); };
                if ts < start - WEEK || ts >= end { return Ok(()); }
                local.push((links.name_shared(r[1]).to_owned(), Click { ts, id:r[0].into(), campaign:r[2].into() }));
                Ok(())
            })?;
            Ok(local)
        }).collect();
        for partial in partials {
            for (person, click) in partial? {
                if seen.insert(click.id.clone()) { clicks.entry(person).or_default().push(click); }
            }
        }
    }
    clicks.par_iter_mut().for_each(|(_, list)| list.sort_unstable_by(|a,b| a.ts.cmp(&b.ts).then_with(|| a.id.cmp(&b.id))));
    mark!("read and sort clicks");
    // Each person's sessions are independent. Workers accumulate private daily
    // totals, which avoids synchronization while traversing events and clicks.
    let (users, totals) = events.into_par_iter().fold(
        || (Vec::<((String, i32), UserDay)>::default(), HashMap::<(String, i32), CampaignDay>::default()),
        |(mut users, mut totals), (person, mut list)| {
            list.sort_unstable_by_key(|e| e.ts);
            let person_clicks = clicks.get(&person).map(Vec::as_slice).unwrap_or(&[]);
            let mut person_days = HashMap::<i32, UserDay>::default();
            let mut i = 0;
            while i < list.len() {
                let session_start = list[i].ts;
                let mut j = i + 1;
                while j < list.len() && list[j].ts - list[j-1].ts <= 1_800_000 { j += 1; }
                let day = session_start.div_euclid(DAY) as i32;
                let user = person_days.entry(day).or_default();
                user.sessions += 1;
                user.events += (j - i) as i64;
                let mut revenue = 0;
                let mut purchasing = false;
                for e in &list[i..j] {
                    revenue += e.revenue;
                    if let Some(category) = &e.category { purchasing = true; *user.categories.entry(category.clone()).or_default() += e.revenue; }
                }
                user.revenue += revenue;
                let lo = person_clicks.partition_point(|c| c.ts < session_start - WEEK);
                let hi = person_clicks.partition_point(|c| c.ts <= session_start);
                let eligible = &person_clicks[lo..hi];
                let credited = eligible.last().map(|last| {
                    let first = eligible.partition_point(|c| c.ts < last.ts);
                    eligible[first].campaign.as_str()
                }).unwrap_or("organic");
                let total = totals.entry((credited.to_owned(), day)).or_default();
                total.sessions += 1; total.revenue += revenue; total.purchasing += i64::from(purchasing);
                let assists: HashSet<&str> = eligible.iter().map(|c| c.campaign.as_str()).collect();
                for campaign in assists { totals.entry((campaign.to_owned(), day)).or_default().assists += 1; }
                i = j;
            }
            users.extend(person_days.into_iter().map(|(day, user)| ((person.clone(), day), user)));
            (users, totals)
        }
    ).reduce(
        || (Vec::new(), HashMap::default()),
        |(mut users, mut totals), (other_users, other_totals)| {
            users.extend(other_users);
            for (key, other) in other_totals {
                let total = totals.entry(key).or_default();
                total.sessions += other.sessions; total.purchasing += other.purchasing;
                total.revenue += other.revenue; total.assists += other.assists;
            }
            (users, totals)
        }
    );
    mark!("sessions and attribution");
    let mut user_ids = Vec::new(); let mut user_days = Vec::new(); let mut sessions = Vec::new(); let mut event_counts = Vec::new(); let mut revenues = Vec::new(); let mut categories = Vec::new();
    for ((id, day), u) in users {
        user_ids.push(id); user_days.push(day); sessions.push(u.sessions); event_counts.push(u.events); revenues.push(u.revenue);
        categories.push(u.categories.into_iter().min_by(|a,b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0))).map(|x| x.0));
    }
    write_table(&output.join("user_daily"), vec![string_field("user_id",false), Field::new("day",DataType::Date32,false), int_field("sessions"), int_field("events"), int_field("revenue_cents"), string_field("top_category",true)], vec![Arc::new(StringArray::from(user_ids)), Arc::new(Date32Array::from(user_days)), Arc::new(Int64Array::from(sessions)), Arc::new(Int64Array::from(event_counts)), Arc::new(Int64Array::from(revenues)), Arc::new(StringArray::from(categories))])?;
    let mut ids = Vec::new(); let mut channels = Vec::new(); let mut days = Vec::new(); let mut sessions = Vec::new(); let mut purchasing = Vec::new(); let mut revenue = Vec::new(); let mut assists = Vec::new();
    for ((id, day), c) in totals {
        channels.push(if id == "organic" { "organic".to_owned() } else { campaigns[&id].clone() }); ids.push(id); days.push(day); sessions.push(c.sessions); purchasing.push(c.purchasing); revenue.push(c.revenue); assists.push(c.assists);
    }
    write_table(&output.join("campaign_daily"), vec![string_field("campaign_id",false), string_field("channel",false), Field::new("day",DataType::Date32,false), int_field("sessions"), int_field("purchasing_sessions"), int_field("revenue_cents"), int_field("assisted_sessions")], vec![Arc::new(StringArray::from(ids)), Arc::new(StringArray::from(channels)), Arc::new(Date32Array::from(days)), Arc::new(Int64Array::from(sessions)), Arc::new(Int64Array::from(purchasing)), Arc::new(Int64Array::from(revenue)), Arc::new(Int64Array::from(assists))])?;
    if timing { eprintln!("write output: {:.3}s", timer.elapsed().as_secs_f64()); }
    Ok(())
}
