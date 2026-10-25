//! Interleave v0.2, v0.3 and PCRE2 in the same process and randomized sample.
use caprice::{
    baseline::Pcre, cache::EngineCache, expr::Params, Engine, Limits, MatchStatus, Mode, Scratch,
};
use serde::Deserialize;
use std::{collections::BTreeMap, fmt::Write as _, hint::black_box, io::Write, time::Instant};
#[derive(Deserialize)]
struct Input {
    text: String,
    expected: bool,
}
#[derive(Deserialize)]
struct Work {
    id: String,
    pattern: String,
    baseline: String,
    post: String,
    params: BTreeMap<String, String>,
    inputs: Vec<Input>,
}
enum Backend {
    Old(Box<caprice_v02::Engine>, caprice_v02::Scratch),
    New(Box<Engine>, Scratch),
    OldOutput(Box<caprice_v02::Engine>, caprice_v02::Scratch),
    NewOutput(Box<Engine>, Scratch),
    Pcre(Pcre),
}
fn params(w: &Work) -> Params {
    w.params
        .iter()
        .map(|(k, v)| (k.clone(), v.as_bytes().to_vec()))
        .collect()
}
fn build(name: &str, w: &Work) -> Result<Backend, String> {
    Ok(match name {
        "v02-output" => Backend::OldOutput(
            Box::new(caprice_v02::Engine::new(
                &w.pattern,
                params(w),
                caprice_v02::Mode::Jit,
            )?),
            caprice_v02::Scratch::default(),
        ),
        "v03-output" => Backend::NewOutput(
            Box::new(Engine::new(&w.pattern, params(w), Mode::Jit)?),
            Scratch::default(),
        ),
        "v02-jit" | "v02-pe" => Backend::Old(
            Box::new(caprice_v02::Engine::new(
                &w.pattern,
                params(w),
                if name == "v02-jit" {
                    caprice_v02::Mode::Jit
                } else {
                    caprice_v02::Mode::Pe
                },
            )?),
            caprice_v02::Scratch::default(),
        ),
        "v03-jit" | "v03-pe" => Backend::New(
            Box::new(Engine::new(
                &w.pattern,
                params(w),
                if name == "v03-jit" {
                    Mode::Jit
                } else {
                    Mode::Pe
                },
            )?),
            Scratch::default(),
        ),
        "pcre2-fast" | "pcre2-jit" => {
            let mut p = Pcre::new(&w.baseline, true)?;
            p.set_jit_fast(name == "pcre2-fast")?;
            Backend::Pcre(p)
        }
        _ => return Err("unknown backend".into()),
    })
}
fn post(w: &Work, p: &Pcre, s: &[u8]) -> bool {
    match w.post.as_str() {
        "none" => true,
        "len16" => p.capture_slice(1, s).is_some_and(|s| s.len() <= 16),
        "len16_not_b" => p
            .capture_slice(1, s)
            .is_some_and(|s| s.len() <= 16 && s != b"b"),
        "lower_admin" => p
            .capture_slice(1, s)
            .is_some_and(|s| s.eq_ignore_ascii_case(b"admin")),
        "upper_pair" => p
            .capture_slice(1, s)
            .zip(p.capture_slice(2, s))
            .is_some_and(|(a, b)| {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.to_ascii_uppercase() == *b)
            }),
        _ => panic!("unknown postcondition"),
    }
}
impl Backend {
    fn test(&mut self, s: &[u8], w: &Work) -> bool {
        match self {
            Self::Old(e, scratch) => e.is_match_with(s, scratch).unwrap(),
            Self::New(e, scratch) => e.is_match_with(s, scratch).unwrap(),
            Self::OldOutput(e, scratch) => {
                let status = e.run_with(s, caprice_v02::Limits::default(), scratch);
                assert_ne!(status, caprice_v02::MatchStatus::Budget);
                if status == caprice_v02::MatchStatus::Match {
                    black_box(scratch.captures());
                    true
                } else {
                    false
                }
            }
            Self::NewOutput(e, scratch) => {
                let status = e.run_with(s, Limits::default(), scratch);
                assert_ne!(status, MatchStatus::Budget);
                if status == MatchStatus::Match {
                    black_box(scratch.captures());
                    true
                } else {
                    false
                }
            }
            Self::Pcre(p) => p.test(s).unwrap() && post(w, p, s),
        }
    }
    fn bytes(&self) -> usize {
        match self {
            Self::Old(e, _) | Self::OldOutput(e, _) => e.native().map_or(0, |j| j.code_bytes),
            Self::New(e, _) | Self::NewOutput(e, _) => e.native().map_or(0, |j| j.code_bytes),
            Self::Pcre(p) => p.jit_bytes,
        }
    }
    fn validate(&mut self, w: &Work) {
        let reference = if w.post == "none" && matches!(self, Self::New(..) | Self::NewOutput(..)) {
            Some(caprice_v02::Engine::new(&w.pattern, params(w), caprice_v02::Mode::Pe).unwrap())
        } else {
            None
        };
        for i in &w.inputs {
            assert_eq!(
                self.test(i.text.as_bytes(), w),
                i.expected,
                "{} {:?}",
                w.id,
                i.text
            );
            // Also compare every requested capture span for shared patterns.
            if w.post == "none" {
                if let Self::New(e, scratch) | Self::NewOutput(e, scratch) = self {
                    let old = reference.as_ref().unwrap();
                    let before = old.run(i.text.as_bytes(), caprice_v02::Limits::default());
                    let status = e.run_with(i.text.as_bytes(), Limits::default(), scratch);
                    match before {
                        caprice_v02::Outcome::Match(c) => {
                            assert_eq!(status, MatchStatus::Match);
                            assert_eq!(scratch.captures(), c);
                        }
                        caprice_v02::Outcome::NoMatch => assert_eq!(status, MatchStatus::NoMatch),
                        _ => panic!("validation budget"),
                    }
                }
            }
        }
    }
}
struct Job {
    work: usize,
    name: &'static str,
    engine: Backend,
    rounds: usize,
}
fn run(job: &mut Job, w: &Work, rounds: usize) -> (u128, usize) {
    let start = Instant::now();
    let mut count = 0;
    for _ in 0..rounds {
        for i in &w.inputs {
            count += black_box(job.engine.test(black_box(i.text.as_bytes()), w)) as usize;
        }
    }
    (start.elapsed().as_nanos(), black_box(count))
}
fn random(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}
fn save(path: &str, content: &str) -> std::io::Result<()> {
    let temp = format!("{path}.tmp");
    let mut f = std::fs::File::create(&temp)?;
    f.write_all(content.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(temp, path)?;
    assert_eq!(std::fs::read_to_string(path)?, content);
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let corpus = &args[1];
    let prefix = &args[2];
    let mut seed = args
        .get(3)
        .map(|s| s.parse().unwrap())
        .unwrap_or(1039102u64);
    let work: Vec<Work> = serde_json::from_str(&std::fs::read_to_string(corpus)?)?;
    let mut timing =
        String::from("workload,engine,sample,rounds,inputs,total_ns,ns_per_match,matches\n");
    let mut compile = String::from("workload,engine,sample,compile_ns,native_bytes\n");
    let mut jobs = vec![];
    for (wi, w) in work.iter().enumerate() {
        for name in [
            "v02-jit",
            "v03-jit",
            "v02-pe",
            "v03-pe",
            "pcre2-fast",
            "pcre2-jit",
            "v02-output",
            "v03-output",
        ] {
            for sample in 0..7 {
                let start = Instant::now();
                let e = build(name, w)?;
                let ns = start.elapsed().as_nanos();
                writeln!(compile, "{},{},{},{},{}", w.id, name, sample, ns, e.bytes())?;
                black_box(e);
            }
            let mut engine = build(name, w)?;
            engine.validate(w);
            let mut job = Job {
                work: wi,
                name,
                engine,
                rounds: 1,
            };
            run(&mut job, w, 3);
            let (ns, _) = run(&mut job, w, 4);
            job.rounds = ((20_000_000 * 4 / ns.max(1)) as usize).clamp(1, 100_000);
            jobs.push(job);
        }
    }
    for sample in 0..9 {
        let mut order: Vec<_> = (0..jobs.len()).collect();
        for i in (1..order.len()).rev() {
            let j = random(&mut seed) as usize % (i + 1);
            order.swap(i, j);
        }
        for i in order {
            let job = &mut jobs[i];
            let w = &work[job.work];
            let (ns, count) = run(job, w, job.rounds);
            let n = job.rounds * w.inputs.len();
            assert_eq!(
                count,
                job.rounds * w.inputs.iter().filter(|i| i.expected).count()
            );
            writeln!(
                timing,
                "{},{},{},{},{},{},{:.6},{}",
                w.id,
                job.name,
                sample,
                job.rounds,
                n,
                ns,
                ns as f64 / n as f64,
                count
            )?;
        }
        eprintln!("{}: sample {}/9", prefix, sample + 1);
    }
    assert_eq!(timing.lines().count(), 1 + jobs.len() * 9);
    assert_eq!(compile.lines().count(), 1 + jobs.len() * 7);
    save(&format!("{prefix}-match.csv"), &timing)?;
    save(&format!("{prefix}-compile.csv"), &compile)?;
    // Warm cache-hit lookup is timed separately from matching and misses.
    let mut cache_output = String::from("workload,sample,lookups,total_ns,ns_per_lookup\n");
    for w in &work {
        let mut cache = EngineCache::new(16);
        let p = params(w);
        cache.get_or_compile(&w.pattern, &p, Mode::Jit)?;
        for sample in 0..9 {
            let start = Instant::now();
            for _ in 0..50_000 {
                black_box(cache.get_or_compile(black_box(&w.pattern), black_box(&p), Mode::Jit)?);
            }
            let ns = start.elapsed().as_nanos();
            writeln!(
                cache_output,
                "{},{},50000,{},{:.6}",
                w.id,
                sample,
                ns,
                ns as f64 / 50_000.
            )?;
        }
        assert_eq!(cache.stats().compilations, 1);
        assert_eq!(cache.stats().hits, 450_000);
    }
    save(&format!("{prefix}-cache.csv"), &cache_output)?;
    save(
        &format!("{prefix}-meta.json"),
        &serde_json::to_string_pretty(
            &serde_json::json!({"pcre2":Pcre::version(),"seed_final":seed,"workloads":work.len(),"samples":9,"compile_samples":7,"target_sample_ns":20_000_000,"validation":"acceptance; v0.2/v0.3 capture spans for shared patterns","clock":"Instant"}),
        )?,
    )?;
    Ok(())
}
