use caprice::{expr::Params, Engine, Limits, Mode, Outcome};
fn main() {
    if let Err(e) = main_result() {
        eprintln!("{e}");
        std::process::exit(2)
    }
}
fn main_result() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 4 {
        return Err("usage: caprice raw|pe|jit PATTERN INPUT [NAME=VALUE ...]".into());
    }
    let mode = match args[1].as_str() {
        "raw" => Mode::Raw,
        "pe" => Mode::Pe,
        "jit" => Mode::Jit,
        _ => return Err("mode must be raw, pe, or jit".into()),
    };
    let mut params = Params::new();
    for a in &args[4..] {
        let (k, v) = a.split_once('=').ok_or("parameter must be NAME=VALUE")?;
        params.insert(k.into(), v.as_bytes().to_vec());
    }
    let e = Engine::new(&args[2], params, mode)?;
    let result = match e.run(args[3].as_bytes(), Limits::default()) {
        Outcome::NoMatch => serde_json::json!({"status":"no_match"}),
        Outcome::Budget => serde_json::json!({"status":"budget"}),
        Outcome::Match(caps) => {
            let groups:std::collections::BTreeMap<_,_>=e.program().names.iter().map(|(n,i)|(n,caps[*i].map(|(a,z)|serde_json::json!({"start":a,"end":z,"bytes":&args[3].as_bytes()[a..z]})))).collect();
            serde_json::json!({"status":"match","captures":groups})
        }
    };
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
    Ok(())
}
