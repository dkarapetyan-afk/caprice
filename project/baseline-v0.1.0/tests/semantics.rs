use caprice::{baseline::Pcre, expr::Params, Engine, Limits, Mode, Outcome};
fn check(pattern: &str, s: &str, expected: bool) {
    for mode in [Mode::Raw, Mode::Pe, Mode::Jit] {
        let e = Engine::new(pattern, Params::new(), mode).unwrap();
        assert_eq!(
            e.is_match(s.as_bytes()).unwrap(),
            expected,
            "{mode:?} {pattern} on {s:?}"
        );
    }
}
#[test]
fn captures_backrefs_and_rollback() {
    check(r"(?<x>[a-z]+)-\k<x>", "abc-abc", true);
    check(r"(?<x>[a-z]+)-\k<x>", "abc-abd", false);
    check(r#"(?<x>a|aa)a?(?{(eq (cap x) "aa")})"#, "aa", true);
    check(r"(?:(?<x>a)b|(?<y>a)c)\k<y>", "aca", true);
    check(r"(?:(?<x>a)b|(?<y>a)c)\k<x>", "aca", false);
}
#[test]
fn typed_lambdas_and_interpolation() {
    check(
        r#"(?<x>[A-Za-z]+):${(upper (cap x))}(?{(call (lambda (s Str) (le (len s) 5)) (cap x))})"#,
        "Abc:ABC",
        true,
    );
    check(
        r#"(?<x>[A-Za-z]+):${(upper (cap x))}(?{(call (lambda (s Str) (le (len s) 5)) (cap x))})"#,
        "Abc:abc",
        false,
    );
    check(
        r#"(?<x>ab)${(slice (concat (cap x) "cd") 1 3)}"#,
        "abbc",
        true,
    );
    check(
        r#"(?{(call (lambda (x Bool) (call (lambda (x Str) (eq x "A")) "A")) false)})"#,
        "",
        true,
    );
    check(r#"(?{(eq (trim "  A\n") "A")})"#, "", true);
}
#[test]
fn missing_is_not_empty_and_application_is_strict() {
    check(
        r#"(?<x>a)?(?{(call (lambda (unused Str) true) (cap x))})"#,
        "",
        false,
    );
    check(r#"(?<x>a)?(?{(or true (eq (cap x) ""))})"#, "", false);
    check(r"(?<x>)\k<x>", "", true);
    check(r"(?<x>a)?\k<x>", "", false);
}
#[test]
fn syntax_and_type_rejections() {
    for p in [
        "(a)",
        "(?<x>a)(?<x>b)",
        r"\k<z>",
        "(?:a?)*",
        r"\q",
        "[z-a]",
        "a{4,2}",
        "[",
        r#"(?{(eq (len "x") "1")})"#,
        r#"${true}"#,
        r#"(?{(call (lambda (x Nat) true) "bad")})"#,
    ] {
        assert!(Engine::new(p, Params::new(), Mode::Pe).is_err(), "{p}");
    }
}
#[test]
fn parameters_are_literal_and_specialization_agrees() {
    let mut p = Params::new();
    p.insert("tag".into(), b".*".to_vec());
    for mode in [Mode::Raw, Mode::Pe, Mode::Jit] {
        let e = Engine::new(
            r#"${(param tag)}(?{(call (lambda (x Str) (eq (len x) 2)) (param tag))})"#,
            p.clone(),
            mode,
        )
        .unwrap();
        assert!(e.is_match(b".*").unwrap());
        assert!(!e.is_match(b"anything").unwrap());
    }
}
#[test]
fn budget_is_a_separate_outcome() {
    for mode in [Mode::Raw, Mode::Pe, Mode::Jit] {
        let e = Engine::new("(?:a|aa)*b", Params::new(), mode).unwrap();
        assert_eq!(
            e.run(
                b"aaaaaaaaaaaaaaaaaaaa",
                Limits {
                    steps: 100,
                    ..Limits::default()
                }
            ),
            Outcome::Budget
        )
    }
}
#[test]
fn bounded_repeat_does_not_enumerate_duplicate_counts() {
    for mode in [Mode::Raw, Mode::Pe, Mode::Jit] {
        let e = Engine::new("[a-z]{0,31}", Params::new(), mode).unwrap();
        assert_eq!(
            e.run(
                b"abcdefghijklmnopqrstuvwxy#",
                Limits {
                    steps: 1000,
                    ..Limits::default()
                }
            ),
            Outcome::NoMatch
        );
    }
}
fn words(alphabet: &[u8], max: usize) -> Vec<Vec<u8>> {
    let mut all = vec![vec![]];
    let mut level = vec![vec![]];
    for _ in 0..max {
        let mut next = vec![];
        for p in &level {
            for c in alphabet {
                let mut v = p.clone();
                v.push(*c);
                next.push(v)
            }
        }
        all.extend(next.clone());
        level = next
    }
    all
}
#[test]
fn exhaustive_differential_capture_semantics() {
    let patterns = [
        "",
        "a",
        "ab",
        "a|ab",
        "(?:a|b)*",
        "a*?b?",
        "[ab]{1,3}",
        "[^b]*",
        "a{0,3}?a",
        r"(?<x>a*)b\k<x>",
        r"(?<x>a|ab)\k<x>",
        r"(?<x>[ab]+)-\k<x>",
        "(?<x>a)?b",
        "(?:(?<x>a)|b)+",
        "(?<x>a*)(?<y>a*)",
        "(?<x>a*?)(?<y>a*)",
        "(?:a|aa)*b",
        "^a?$",
        r"(?<x>)\k<x>",
    ];
    let inputs = words(b"ab-", 6);
    let mut cases = 0;
    for pat in patterns {
        let engines: [Engine; 3] =
            [Mode::Raw, Mode::Pe, Mode::Jit].map(|m| Engine::new(pat, Params::new(), m).unwrap());
        let mut pcre = Pcre::new(pat, false).unwrap();
        for s in &inputs {
            let expected = pcre.run(s).unwrap();
            let raw = engines[0].run(s, Limits::default());
            for e in &engines[1..] {
                assert_eq!(
                    raw,
                    e.run(s, Limits::default()),
                    "{} {:?} {:?}",
                    pat,
                    s,
                    e.mode
                )
            }
            match (raw, expected) {
                (Outcome::NoMatch, None) => {}
                (Outcome::Match(c), Some(pc)) => {
                    for (i, v) in c.iter().enumerate() {
                        assert_eq!(
                            *v,
                            pc.get(i + 1).copied().flatten(),
                            "capture {} on {} {:?}",
                            i,
                            pat,
                            s
                        )
                    }
                }
                (a, b) => panic!("{pat} {s:?}: {a:?} != {b:?}"),
            };
            cases += 1;
        }
    }
    println!("exhaustive_shared_cases={cases}");
}
#[test]
fn exhaustive_dynamic_specialization_and_jit() {
    let patterns = [
        r#"(?<x>[ab]*)-${(upper (cap x))}"#,
        r#"(?<x>a|aa)a?(?{(eq (cap x) "aa")})"#,
        r#"(?<x>[ab]*)(?{(call (lambda (v Str) (and (le (len v) 2) (not (eq v "b")))) (cap x))})"#,
        r#"(?<x>a)?(?{(or true (eq (cap x) ""))})"#,
    ];
    let inputs = words(b"abAB-", 5);
    let mut cases = 0;
    for pat in patterns {
        let engines: [Engine; 3] =
            [Mode::Raw, Mode::Pe, Mode::Jit].map(|m| Engine::new(pat, Params::new(), m).unwrap());
        for s in &inputs {
            let a = engines[0].run(s, Limits::default());
            for e in &engines[1..] {
                assert_eq!(a, e.run(s, Limits::default()), "{pat} {s:?}")
            }
            cases += 1;
        }
    }
    println!("exhaustive_extended_cases={cases}");
}
