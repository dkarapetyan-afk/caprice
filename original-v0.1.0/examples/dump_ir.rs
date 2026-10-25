use caprice::{expr::Params, Engine, Mode};
fn main() {
    let pattern = r#"(?<x>[a-z]{1,8}):${(upper (cap x))}(?{(le (len (cap x)) 8)})"#;
    let engine = Engine::new(pattern, Params::new(), Mode::Jit).unwrap();
    println!(
        "; Caprice v0.1.0 residual native control-flow IR\n; pattern: {pattern}\n{}",
        engine.native().unwrap().clif
    );
}
