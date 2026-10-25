use caprice::{expr::Params, Engine, Limits, MatchStatus, Mode, Outcome, Scratch};

#[test]
fn native_bytes_lengths_and_class_tables() {
    let mut cases = 0;
    for n in [
        0, 1, 2, 3, 4, 7, 8, 9, 15, 16, 17, 63, 64, 65, 127, 128, 129,
    ] {
        let pattern = "a".repeat(n);
        let pe = Engine::new(&pattern, Params::new(), Mode::Pe).unwrap();
        let jit = Engine::new(&pattern, Params::new(), Mode::Jit).unwrap();
        for size in n.saturating_sub(2)..=n + 2 {
            let mut input = vec![b'a'; size];
            assert_eq!(
                pe.run(&input, Limits::default()),
                jit.run(&input, Limits::default())
            );
            cases += 1;
            for at in 0..size {
                input[at] = b'b';
                assert_eq!(
                    pe.run(&input, Limits::default()),
                    jit.run(&input, Limits::default())
                );
                cases += 1;
                input[at] = b'a';
            }
        }
    }
    for p in [
        "[acegikmoqsuwy]+",
        "[^b]+",
        "[a-zA-Z0-9_]",
        "[a-z]*",
        "[a-z]{2,4}",
    ] {
        let pe = Engine::new(p, Params::new(), Mode::Pe).unwrap();
        let jit = Engine::new(p, Params::new(), Mode::Jit).unwrap();
        for b in 0u8..=255 {
            for n in 0..=5 {
                let s = vec![b; n];
                assert_eq!(
                    pe.run(&s, Limits::default()),
                    jit.run(&s, Limits::default()),
                    "{p} {s:?}"
                );
                cases += 1;
            }
        }
    }
    println!("native_boundary_cases={cases}");
}

#[test]
fn ranges_capture_trails_and_scratch_reuse() {
    let patterns = [
        r"(?<a>[ab]*)(?<b>[ab]*)\k<a>",
        r"(?<a>[ab]*?)(?<b>[ab]*?)\k<a>",
        r"(?:(?<a>[ab]+)a|(?<b>[ab]+)b)\k<b>",
        r"(?:(?<a>a)|(?<b>b))+\k<a>",
        r#"(?<x>[ab]*)(?{(eq (cap x) "a")})a?"#,
        r#"(?<x>a)?(?{(call (lambda (s Str) (eq (lower s) "a")) (cap x))})"#,
        r#"(?<x>[ab]*?)-${(upper (cap x))}"#,
    ];
    let mut words = vec![vec![]];
    let mut layer = vec![vec![]];
    for _ in 0..5 {
        let mut next = vec![];
        for s in &layer {
            for b in b"abAB-" {
                let mut s = s.clone();
                s.push(*b);
                next.push(s);
            }
        }
        words.extend(next.clone());
        layer = next;
    }
    let mut cases = 0;
    for p in patterns {
        let raw = Engine::new(p, Params::new(), Mode::Raw).unwrap();
        for m in [Mode::Pe, Mode::Jit] {
            let engine = Engine::new(p, Params::new(), m).unwrap();
            let mut scratch = Scratch::default();
            for s in &words {
                let expected = raw.run(s, Limits::default());
                let status = engine.run_with(s, Limits::default(), &mut scratch);
                match expected {
                    Outcome::Match(ref c) => {
                        assert_eq!(status, MatchStatus::Match);
                        assert_eq!(scratch.captures(), c, "{p} {s:?}");
                    }
                    Outcome::NoMatch => assert_eq!(status, MatchStatus::NoMatch, "{p} {s:?}"),
                    Outcome::Budget => panic!("unexpected bounded test exhaustion"),
                }
                cases += 1;
            }
        }
    }
    println!("range_trail_reuse_comparisons={cases}");
}

#[test]
fn native_and_vm_have_identical_residual_budgets() {
    let ps = [
        "a|b",
        "(?:a|aa)*b",
        "[ab]*a",
        "[ab]*?a",
        r"(?<x>[ab]+)-\k<x>",
        r#"(?<x>[ab]*)(?{(le (len (cap x)) 2)})"#,
        r#"(?<x>[ab]*):${(upper (cap x))}"#,
    ];
    let inputs = ["", "a", "b", "aa", "ab", "a-a", "ab-ab", "ab:AB", "aaaaacb"];
    for p in ps {
        let pe = Engine::new(p, Params::new(), Mode::Pe).unwrap();
        let jit = Engine::new(p, Params::new(), Mode::Jit).unwrap();
        for s in inputs {
            for steps in 0..20 {
                for stack in 0..3 {
                    for expression_bytes in 0..4 {
                        let limits = Limits {
                            steps,
                            stack,
                            expression_bytes,
                        };
                        assert_eq!(
                            pe.run(s.as_bytes(), limits),
                            jit.run(s.as_bytes(), limits),
                            "{p} {s} {limits:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn necessary_literal_rejects_but_does_not_decide_surviving_searches() {
    for m in [Mode::Pe, Mode::Jit] {
        let e = Engine::new("(?:a|aa)*b", Params::new(), m).unwrap();
        assert_eq!(
            e.run(
                &vec![b'a'; 100_000],
                Limits {
                    steps: 1,
                    ..Limits::default()
                }
            ),
            Outcome::NoMatch
        );
        assert_eq!(e.run(b"aaaaab", Limits::default()), Outcome::Match(vec![]));
        assert_eq!(
            e.run(
                b"aaaaaaaaaaaaaaaaaaaacb",
                Limits {
                    steps: 100,
                    ..Limits::default()
                }
            ),
            Outcome::Budget
        );
    }
}
