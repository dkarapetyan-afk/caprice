use caprice::{expr::Params, Engine, Limits, Mode, Outcome};
fn agree(pattern: &str, inputs: &[&[u8]]) {
    let engines = [Mode::Raw, Mode::Pe, Mode::Jit]
        .map(|mode| Engine::new(pattern, Params::new(), mode).unwrap());
    for input in inputs {
        let expected = engines[0].run(input, Limits::default());
        for engine in &engines[1..] {
            assert_eq!(
                engine.run(input, Limits::default()),
                expected,
                "{pattern} {input:?} {:?}",
                engine.mode
            );
        }
        for steps in [0usize, 1, 2, 4, 8, 30] {
            let limits = Limits {
                steps,
                ..Limits::default()
            };
            assert_eq!(
                engines[2].run(input, limits),
                engines[1].run(input, limits),
                "step budget {pattern} {input:?} {steps}"
            );
        }
    }
}
#[test]
fn captured_prefixes_keep_their_spans() {
    let pattern = r"(?:(?<a>foo)a|(?<b>foo)b|(?<c>foobar)c)";
    agree(
        pattern,
        &[b"fooa", b"foob", b"foobarc", b"foo", b"foobar", b"foobaz"],
    );
    let engine = Engine::new(pattern, Params::new(), Mode::Pe).unwrap();
    assert_eq!(
        engine.run(b"foobarc", Limits::default()),
        Outcome::Match(vec![None, None, Some((0, 6))])
    );
    assert_eq!(
        engine.run(b"foob", Limits::default()),
        Outcome::Match(vec![None, Some((0, 3)), None])
    );
}
#[test]
fn repeated_captures_restore_the_iteration_that_matched() {
    agree(
        r"(?:(?<x>[a-z]{1,8}):){1,16}\k<x>",
        &[b"ab:ab", b"ab:cd:cd", b"ab:ab:ab:ab", b"a:b", b"abcdefghi:"],
    );
    agree(
        r"(?:(?<x>[ab]{0,3})-){1,8}\k<x>",
        &[b"-", b"---", b"ab-", b"ab-ab-", b"a-b-a", b"-a-"],
    );
    let engine = Engine::new(
        r"(?:(?<x>[a-z]{1,8}):){1,16}\k<x>",
        Params::new(),
        Mode::Jit,
    )
    .unwrap();
    assert_eq!(
        engine.run(b"ab:cd:cd", Limits::default()),
        Outcome::Match(vec![Some((3, 5))])
    );
}
