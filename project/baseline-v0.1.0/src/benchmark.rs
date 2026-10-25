use caprice::{baseline::Pcre, expr::Params, Engine, Limits, Mode, Outcome};
use serde::Deserialize;
use std::fmt::Write as _;
use std::{collections::BTreeMap, hint::black_box, io::Write, time::Instant};
#[derive(Deserialize, Clone)]
struct Input {
    text: String,
    expected: bool,
}
#[derive(Deserialize, Clone)]
struct Work {
    id: String,
    pattern: String,
    baseline: String,
    regular: bool,
    post: String,
    params: BTreeMap<String, String>,
    inputs: Vec<Input>,
}
enum Backend {
    Ours(Box<Engine>),
    Pcre(Pcre),
    Regex(regex::bytes::Regex),
}
fn build(name: &str, w: &Work) -> Result<Backend, String> {
    Ok(match name {
        "raw" | "pe" | "jit" => Backend::Ours(Box::new(Engine::new(
            &w.pattern,
            w.params
                .iter()
                .map(|(k, v)| (k.clone(), v.as_bytes().to_vec()))
                .collect::<Params>(),
            match name {
                "raw" => Mode::Raw,
                "pe" => Mode::Pe,
                _ => Mode::Jit,
            },
        )?)),
        "pcre2" | "pcre2-jit" => Backend::Pcre(Pcre::new(&w.baseline, name == "pcre2-jit")?),
        "rust-regex" => Backend::Regex(
            regex::bytes::RegexBuilder::new(&format!("\\A(?:{})\\z", w.baseline))
                .unicode(false)
                .build()
                .map_err(|e| e.to_string())?,
        ),
        _ => return Err("unknown engine".into()),
    })
}
fn post(kind: &str, one: Option<Vec<u8>>, two: Option<Vec<u8>>) -> bool {
    match kind {
        "lower_admin" => one.is_some_and(|s| s.eq_ignore_ascii_case(b"admin")),
        "upper_pair" => one
            .zip(two)
            .is_some_and(|(a, b)| a.to_ascii_uppercase() == b),
        "len16" => one.is_some_and(|s| s.len() <= 16),
        "none" => true,
        _ => panic!("unknown postprocessor"),
    }
}
impl Backend {
    fn test(&mut self, s: &[u8], w: &Work) -> Result<bool, String> {
        match self {
            Self::Ours(e) => e.is_match(s),
            Self::Pcre(p) => {
                let ok = p.test(s).map_err(|n| format!("PCRE2 runtime error {n}"))?;
                Ok(ok
                    && (w.post == "none"
                        || post(
                            &w.post,
                            p.capture(1, s),
                            if w.post == "upper_pair" {
                                p.capture(2, s)
                            } else {
                                None
                            },
                        )))
            }
            Self::Regex(r) => {
                if w.post == "none" {
                    Ok(r.is_match(s))
                } else {
                    Ok(r.captures(s).is_some_and(|c| {
                        post(
                            &w.post,
                            c.get(1).map(|m| m.as_bytes().to_vec()),
                            c.get(2).map(|m| m.as_bytes().to_vec()),
                        )
                    }))
                }
            }
        }
    }
    fn bytes(&self) -> usize {
        match self {
            Self::Ours(e) => e.native().map(|j| j.code_bytes).unwrap_or(0),
            Self::Pcre(p) => p.jit_bytes,
            _ => 0,
        }
    }
    fn instructions(&self) -> usize {
        match self {
            Self::Ours(e) => e.program().code.len(),
            _ => 0,
        }
    }
}
struct Job {
    wi: usize,
    name: String,
    engine: Backend,
    rounds: usize,
}
fn run(job: &mut Job, w: &Work, rounds: usize) -> (u128, usize) {
    let begin = Instant::now();
    let mut sum = 0;
    for _ in 0..rounds {
        for i in &w.inputs {
            sum += black_box(
                job.engine
                    .test(black_box(i.text.as_bytes()), w)
                    .expect("benchmark runtime failure"),
            ) as usize
        }
    }
    (begin.elapsed().as_nanos(), black_box(sum))
}
fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let corpus = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("benchmarks/corpus.json");
    let out = args.get(2).map(String::as_str).unwrap_or("benchmarks/run");
    let mut seed = args
        .get(3)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(20261024);
    let work: Vec<Work> = serde_json::from_str(&std::fs::read_to_string(corpus)?)?;
    let mut timings = String::new();
    let mut compile = String::new();
    writeln!(
        timings,
        "workload,engine,sample,rounds,inputs,total_ns,ns_per_match,matches"
    )?;
    writeln!(
        compile,
        "workload,engine,sample,compile_ns,native_bytes,instructions"
    )?;
    let mut jobs = vec![];
    for (wi, w) in work.iter().enumerate() {
        for name in ["raw", "pe", "jit", "pcre2", "pcre2-jit", "rust-regex"] {
            if name == "rust-regex" && !w.regular {
                continue;
            }
            for sample in 0..7 {
                let start = Instant::now();
                let e = build(name, w)?;
                let ns = start.elapsed().as_nanos();
                writeln!(
                    compile,
                    "{},{},{},{},{},{}",
                    w.id,
                    name,
                    sample,
                    ns,
                    e.bytes(),
                    e.instructions()
                )?;
                black_box(e);
            }
            let mut engine = build(name, w)?;
            for i in &w.inputs {
                let got = engine.test(i.text.as_bytes(), w)?;
                assert_eq!(got, i.expected, "{} {} {:?}", w.id, name, i.text)
            }
            let mut job = Job {
                wi,
                name: name.into(),
                engine,
                rounds: 1,
            };
            let _ = run(&mut job, w, 3);
            let (ns, _) = run(&mut job, w, 4);
            job.rounds = ((20_000_000u128 * 4 / ns.max(1)) as usize).clamp(1, 100_000);
            jobs.push(job);
        }
    }
    for sample in 0..9 {
        let mut order: Vec<_> = (0..jobs.len()).collect();
        for i in (1..order.len()).rev() {
            let j = (random(&mut seed) as usize) % (i + 1);
            order.swap(i, j)
        }
        for ji in order {
            let job = &mut jobs[ji];
            let w = &work[job.wi];
            let (ns, sum) = run(job, w, job.rounds);
            let n = job.rounds * w.inputs.len();
            assert_eq!(
                sum,
                job.rounds * w.inputs.iter().filter(|x| x.expected).count()
            );
            writeln!(
                timings,
                "{},{},{},{},{},{},{:.6},{}",
                w.id,
                job.name,
                sample,
                job.rounds,
                n,
                ns,
                ns as f64 / n as f64,
                sum
            )?;
        }
        eprintln!("sample {}/9 complete", sample + 1);
    }
    let mut stress = String::new();
    writeln!(stress, "n,engine,status,elapsed_ns")?;
    for n in [8, 12, 16, 20, 24, 28] {
        let s = vec![b'a'; n];
        for name in ["raw", "pe", "jit", "pcre2", "pcre2-jit", "rust-regex"] {
            let w = Work {
                id: "adversarial".into(),
                pattern: "(?:a|aa)*b".into(),
                baseline: "(?:a|aa)*b".into(),
                regular: true,
                post: "none".into(),
                params: BTreeMap::new(),
                inputs: vec![],
            };
            let mut e = build(name, &w)?;
            let start = Instant::now();
            let status = match &mut e {
                Backend::Ours(e) => match e.run(&s, Limits::default()) {
                    Outcome::Budget => "budget",
                    Outcome::NoMatch => "no_match",
                    _ => "match",
                },
                Backend::Pcre(p) => match p.test(&s) {
                    Ok(false) => "no_match",
                    Ok(true) => "match",
                    Err(_) => "error",
                },
                Backend::Regex(r) => {
                    if r.is_match(&s) {
                        "match"
                    } else {
                        "no_match"
                    }
                }
            };
            writeln!(stress, "{n},{name},{status},{}", start.elapsed().as_nanos())?;
        }
    }
    assert_eq!(compile.lines().count(), 1 + jobs.len() * 7);
    assert_eq!(timings.lines().count(), 1 + jobs.len() * 9);
    save_verified(&format!("{out}-compile.csv"), &compile)?;
    save_verified(&format!("{out}-match.csv"), &timings)?;
    save_verified(&format!("{out}-stress.csv"), &stress)?;
    save_verified(
        &format!("{out}-meta.json"),
        &serde_json::to_string_pretty(
            &serde_json::json!({"pcre2":Pcre::version(),"regex":"1.12.2","cranelift":"0.125.3","samples":9,"compile_samples":7,"target_sample_ms":20,"seed_final":seed,"pid":std::process::id(),"profile":"release","clock":"std::time::Instant","semantics":"full-byte-match; ASCII transforms; host predicates only on unique captures"}),
        )?,
    )?;
    Ok(())
}

fn save_verified(path: &str, content: &str) -> std::io::Result<()> {
    let temporary = format!("{path}.tmp");
    {
        let mut f = std::fs::File::create(&temporary)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&temporary, path)?;
    if std::fs::read_to_string(path)? != content {
        return Err(std::io::Error::other("result write verification failed"));
    }
    Ok(())
}
