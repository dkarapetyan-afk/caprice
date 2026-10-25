use caprice::{baseline::Pcre, expr::Params, Engine, Limits, Mode, Outcome};
use caprice::{cache::EngineCache, MatchStatus, Scratch};
use std::rc::Rc;

#[test]
fn cache_keys_lru_and_native_lifetimes() {
    let mut cache = EngineCache::new(2);
    let empty = Params::new();
    let a = cache.get_or_compile("a", &empty, Mode::Jit).unwrap();
    let b = cache.get_or_compile("b", &empty, Mode::Jit).unwrap();
    assert!(Rc::ptr_eq(
        &a,
        &cache.get_or_compile("a", &empty, Mode::Jit).unwrap()
    ));
    cache.get_or_compile("c", &empty, Mode::Jit).unwrap();
    let b2 = cache.get_or_compile("b", &empty, Mode::Jit).unwrap();
    assert!(!Rc::ptr_eq(&b, &b2));
    assert!(b.is_match(b"b").unwrap());
    cache.clear();
    assert!(cache.is_empty());
    assert!(a.is_match(b"a").unwrap());
    let p = Params::from([("x".into(), b"alpha".to_vec())]);
    let q = Params::from([("x".into(), b"beta".to_vec())]);
    let pattern = "${(param x)}";
    let one = cache.get_or_compile(pattern, &p, Mode::Jit).unwrap();
    let two = cache.get_or_compile(pattern, &q, Mode::Jit).unwrap();
    assert!(one.is_match(b"alpha").unwrap());
    assert!(!two.is_match(b"alpha").unwrap());
    let pe = cache.get_or_compile(pattern, &p, Mode::Pe).unwrap();
    assert!(!Rc::ptr_eq(&one, &pe));
    assert!(cache.get_or_compile("[", &empty, Mode::Jit).is_err());
    assert_eq!(cache.len(), 2);
    let mut disabled = EngineCache::new(0);
    let x = disabled.get_or_compile("a", &empty, Mode::Pe).unwrap();
    let y = disabled.get_or_compile("a", &empty, Mode::Pe).unwrap();
    assert!(!Rc::ptr_eq(&x, &y));
    assert!(disabled.is_empty());
}

#[test]
fn stable_capture_slots_restore_through_fallback_and_reuse() {
    let patterns = [
        r#"(?<x>[ab]*)(?{(and (le (len (cap x)) 3) (not (eq (cap x) "b")))})"#,
        r#"(?:(?<x>a)|(?<y>b))+${(concat (cap x) (cap y))}"#,
        r#"(?<outer>(?<x>[ab]*)c)(?{(eq (slice (cap outer) 0 2) "ab")})"#,
    ];
    for pattern in patterns {
        let raw = Engine::new(pattern, Params::new(), Mode::Raw).unwrap();
        let jit = Engine::new(pattern, Params::new(), Mode::Jit).unwrap();
        let mut scratch = Scratch::default();
        for input in words(b"abc", 6) {
            let _ = jit.run_with(
                &input,
                Limits {
                    steps: 1,
                    ..Limits::default()
                },
                &mut scratch,
            );
            let _ = jit.is_match_with(&input, &mut scratch);
            let result = jit.run_with(&input, Limits::default(), &mut scratch);
            match raw.run(&input, Limits::default()) {
                Outcome::Match(c) => {
                    assert_eq!(result, MatchStatus::Match);
                    assert_eq!(scratch.captures(), c);
                }
                Outcome::NoMatch => assert_eq!(result, MatchStatus::NoMatch),
                Outcome::Budget => panic!("bounded case exhausted"),
            }
        }
    }
}
fn words(alphabet: &[u8], max: usize) -> Vec<Vec<u8>> {
    let mut all = vec![vec![]];
    let mut level = all.clone();
    for _ in 0..max {
        let mut next = vec![];
        for s in &level {
            for b in alphabet {
                let mut s = s.clone();
                s.push(*b);
                next.push(s);
            }
        }
        all.extend(next.clone());
        level = next;
    }
    all
}

#[test]
fn fused_capture_runs_preserve_restoration_and_all_byte_classes() {
    for pattern in [
        r"(?:(?<x>[ab]{1,3})-){1,5}\k<x>",
        r"(?:(?<x>[ab]*)-){1,4}b\k<x>",
    ] {
        let raw = Engine::new(pattern, Params::new(), Mode::Raw).unwrap();
        let jit = Engine::new(pattern, Params::new(), Mode::Jit).unwrap();
        let mut pc = Pcre::new(pattern, true).unwrap();
        for s in words(b"ab-", 7) {
            let before = raw.run(&s, Limits::default());
            assert_eq!(jit.run(&s, Limits::default()), before);
            match (before, pc.run(&s).unwrap()) {
                (Outcome::NoMatch, None) => {}
                (Outcome::Match(c), Some(p)) => assert_eq!(c.as_slice(), &p[1..]),
                (a, b) => panic!("{pattern} {s:?} {a:?} {b:?}"),
            }
        }
    }
    for pattern in [r"(?<x>[^;\n]{0,4})", r"(?<x>[A-Za-z0-9_]{0,4})"] {
        let raw = Engine::new(pattern, Params::new(), Mode::Raw).unwrap();
        let jit = Engine::new(pattern, Params::new(), Mode::Jit).unwrap();
        for b in 0u8..=255 {
            for n in 0..6 {
                let s = vec![b; n];
                assert_eq!(
                    jit.run(&s, Limits::default()),
                    raw.run(&s, Limits::default())
                );
            }
        }
    }
}
#[test]
fn affine_lengths_and_guards_preserve_captures() {
    let patterns = [
        r"(?<x>[ab]{0,4})\k<x>",
        r"(?<x>[ab]{0,4}?)\k<x>",
        r"a(?<x>[ab]{0,4})a\k<x>b",
        r"(?<x>[ab]*)\k<x>\k<x>",
        r"(?<fixed>a)(?<x>[ab]{0,4})\k<fixed>\k<x>",
        r"(?<outer>(?<x>[ab]{0,4})a)\k<outer>",
        r"(?<x>[ab]{0,4})-(?<y>a{2})\k<x>\k<y>",
        r"(?<x>[ab]*)(?<y>[ab]*)\k<x>",
        r#"(?<x>[ab]*)-${(upper (cap x))}"#,
        r#"(?<x>[ab]*)(?{(le (len (cap x)) 2)})"#,
        r#"(?<x>[ab]*)(?{(lt (len (cap x)) 0)})"#,
        r#"(?<x>[ab]*)(?{(eq (len (cap x)) 2)})"#,
        r#"(?<x>[ab]*)(?{(eq (upper (cap x)) "AB")})"#,
        r#"(?<x>[ab]*)(?{(call (lambda (s Str) (eq (lower s) "ab")) (cap x))})"#,
        r#"(?<x>[ab]*)(?{(le (len (cap x)) 2)})(?{(eq (cap x) "ab")})"#,
        r#"(?<x>[ab]*)(?{(call (lambda (unused Str) true) (cap x))})"#,
    ];
    let inputs = words(b"abAB-", 5);
    let mut cases = 0;
    for p in patterns {
        let raw = Engine::new(p, Params::new(), Mode::Raw).unwrap();
        let pe = Engine::new(p, Params::new(), Mode::Pe).unwrap();
        let jit = Engine::new(p, Params::new(), Mode::Jit).unwrap();
        for s in &inputs {
            let expected = raw.run(s, Limits::default());
            assert_eq!(pe.run(s, Limits::default()), expected, "PE {p} {s:?}");
            assert_eq!(jit.run(s, Limits::default()), expected, "JIT {p} {s:?}");
            cases += 1;
        }
    }
    println!("affine_pattern_input_cases={cases}");
}
#[test]
fn length_bounds_do_not_hide_mandatory_expression_limits() {
    for p in [
        r#"(?<x>[a-z]+)(?{(le (len (cap x)) 2)})"#,
        r#"(?<x>[a-z]+)(?{(eq (lower (cap x)) "admin")})"#,
    ] {
        for mode in [Mode::Raw, Mode::Pe, Mode::Jit] {
            let e = Engine::new(p, Params::new(), mode).unwrap();
            assert_eq!(
                e.run(
                    b"abcdef",
                    Limits {
                        expression_bytes: 3,
                        ..Limits::default()
                    }
                ),
                Outcome::Budget,
                "{p} {mode:?}"
            );
            assert_eq!(e.run(b"abcdef", Limits::default()), Outcome::NoMatch);
        }
    }
}
#[test]
fn prefix_factoring_retains_priority_and_capture_side_effects() {
    let patterns = [
        "foo|foobar|foobaz",
        "(?<x>foo|foobar|foobaz)",
        "(?:a|ab|ac|b|ba):[ab]*",
        "(?:ab|abc|a|ac|b|ba)",
        r"(?:(?<x>ab)c|(?<y>ab)d)\k<y>",
        r#"(?<x>foo|foobar|foobaz)(?{(eq (cap x) "foobar")})"#,
        r#"(?<x>ab|abc|a)(?{(not (eq (cap x) "ab"))})c?"#,
    ];
    let mut inputs = words(b"abc:", 5);
    inputs.extend([
        b"foo".to_vec(),
        b"foobar".to_vec(),
        b"foobaz".to_vec(),
        b"abdab".to_vec(),
    ]);
    let mut cases = 0;
    for p in patterns {
        let engines =
            [Mode::Raw, Mode::Pe, Mode::Jit].map(|m| Engine::new(p, Params::new(), m).unwrap());
        let mut pcre = if p.contains("(?{") {
            None
        } else {
            Some(Pcre::new(p, true).unwrap())
        };
        for s in &inputs {
            let expected = engines[0].run(s, Limits::default());
            for e in &engines[1..] {
                assert_eq!(e.run(s, Limits::default()), expected, "{p} {s:?}");
            }
            if let Some(ref mut pc) = pcre {
                match (&expected, pc.run(s).unwrap()) {
                    (Outcome::NoMatch, None) => {}
                    (Outcome::Match(c), Some(p)) => assert_eq!(c.as_slice(), &p[1..], "{s:?}"),
                    (a, b) => panic!("PCRE disagreement {p} {s:?}: {a:?} {b:?}"),
                }
            }
            cases += 1;
        }
    }
    println!("prefix_pattern_input_cases={cases}");
}
