//! Stack residual for guards and interpolations that `FastExpr` does not lower.
//!
//! String values are endpoint views plus an ASCII case fold. Concatenation is
//! the only form that copies bytes, into a caller-owned arena. Arguments run
//! strictly left to right: `and` and `or` still evaluate a later argument after
//! an earlier success, and stop only when an earlier argument is unset or over
//! the byte limit. Lambda bindings restore the previous slot, so a nested use
//! of the same id keeps the outer value.
use crate::expr::{EvalError, Expr, Params, Value};
use crate::CaptureSlot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Origin {
    Input,
    Pool,
    Arena,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transform {
    Identity,
    Lower,
    Upper,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StrView {
    origin: Origin,
    transform: Transform,
    start: usize,
    end: usize,
}
impl StrView {
    fn len(self) -> usize {
        self.end - self.start
    }
    fn is_empty(self) -> bool {
        self.start == self.end
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cell {
    Bool(bool),
    Nat(usize),
    Str(StrView),
}
#[derive(Clone, Copy)]
pub(crate) enum Op {
    Bool(bool),
    Nat(usize),
    Const(usize, usize),
    Cap(usize),
    Load(usize),
    Bind(usize),
    Unbind(usize),
    Unset,
    Lower,
    Upper,
    Trim,
    Len,
    Not,
    Concat,
    Contains,
    Starts,
    Ends,
    Slice,
    Eq,
    Lt,
    Le,
    And,
    Or,
}
pub(crate) struct Residual {
    pub(crate) ops: Vec<Op>,
    pub(crate) pool: Vec<u8>,
    pub(crate) nvars: usize,
}
#[derive(Default)]
pub(crate) struct State {
    values: Vec<Cell>,
    vars: Vec<Option<Cell>>,
    arena: Vec<u8>,
    binds: Vec<(usize, Option<Cell>)>,
    copy: Vec<u8>,
}
impl State {
    pub(crate) fn reset_arena(&mut self) -> *mut u8 {
        self.arena.clear();
        self.arena.as_mut_ptr()
    }
    pub(crate) fn reserve_arena(&mut self, extra: usize) -> (*mut u8, usize) {
        let start = self.arena.len();
        self.arena.resize(start.saturating_add(extra), 0);
        (self.arena.as_mut_ptr(), start)
    }
}
pub(crate) enum Decision {
    /// Success consumes this many subject bytes. A Boolean guard consumes none.
    Success(usize),
    Fail,
}
impl Residual {
    pub(crate) fn eval(
        &self,
        input: &[u8],
        caps: &[CaptureSlot],
        limit: usize,
        state: &mut State,
    ) -> Result<Cell, EvalError> {
        state.values.clear();
        state.arena.clear();
        state.binds.clear();
        state.copy.clear();
        state.vars.clear();
        state.values.reserve(self.ops.len());
        state.vars.resize(self.nvars, None);
        for op in &self.ops {
            self.step(op, input, caps, limit, state)?;
        }
        debug_assert!(state.binds.is_empty());
        Ok(state.values.pop().expect("residual result"))
    }
    pub(crate) fn run(
        &self,
        input: &[u8],
        pos: usize,
        caps: &[CaptureSlot],
        limit: usize,
        state: &mut State,
    ) -> Result<Decision, EvalError> {
        let cell = self.eval(input, caps, limit, state)?;
        Ok(self.decide(input, pos, cell, &state.arena))
    }
    pub(crate) fn decide(&self, input: &[u8], pos: usize, cell: Cell, arena: &[u8]) -> Decision {
        match cell {
            Cell::Bool(true) => Decision::Success(0),
            Cell::Bool(false) => Decision::Fail,
            Cell::Str(view) => {
                if has_prefix(input, pos, view, &self.pool, arena) {
                    Decision::Success(view.len())
                } else {
                    Decision::Fail
                }
            }
            Cell::Nat(_) => unreachable!("root expressions are Bool or Str"),
        }
    }
    fn step(
        &self,
        op: &Op,
        input: &[u8],
        caps: &[CaptureSlot],
        limit: usize,
        state: &mut State,
    ) -> Result<(), EvalError> {
        match *op {
            Op::Bool(v) => state.values.push(Cell::Bool(v)),
            Op::Nat(v) => state.values.push(Cell::Nat(v)),
            Op::Unset => return Err(EvalError::Unset),
            Op::Const(start, end) => state.values.push(Cell::Str(check_span(
                Origin::Pool,
                Transform::Identity,
                start,
                end,
                limit,
            )?)),
            Op::Cap(id) => {
                let (start, end) = caps[id]
                    .span()
                    .filter(|(start, end)| start <= end && *end <= input.len())
                    .ok_or(EvalError::Unset)?;
                state.values.push(Cell::Str(check_span(
                    Origin::Input,
                    Transform::Identity,
                    start,
                    end,
                    limit,
                )?));
            }
            Op::Load(id) => {
                let cell = state.vars[id].ok_or(EvalError::Unset)?;
                state.values.push(cell);
            }
            Op::Bind(id) => {
                let value = pop(&mut state.values);
                let old = state.vars[id].replace(value);
                state.binds.push((id, old));
            }
            Op::Unbind(id) => {
                let (bound, old) = state.binds.pop().expect("lambda binding");
                debug_assert_eq!(bound, id);
                state.vars[id] = old;
            }
            Op::Lower | Op::Upper => {
                let view = pop_str(&mut state.values);
                let transform = match op {
                    Op::Lower => Transform::Lower,
                    _ => Transform::Upper,
                };
                state.values.push(Cell::Str(StrView {
                    transform: compose(view.transform, transform),
                    ..view
                }));
            }
            Op::Trim => {
                let view = pop_str(&mut state.values);
                state
                    .values
                    .push(Cell::Str(trim_view(input, &self.pool, &state.arena, view)));
            }
            Op::Len => {
                let view = pop_str(&mut state.values);
                state.values.push(Cell::Nat(view.len()));
            }
            Op::Not => {
                let Cell::Bool(v) = pop(&mut state.values) else {
                    unreachable!("typed not");
                };
                state.values.push(Cell::Bool(!v));
            }
            Op::Concat => {
                let right = pop_str(&mut state.values);
                let left = pop_str(&mut state.values);
                let view = concat_views(input, &self.pool, left, right, limit, state)?;
                state.values.push(Cell::Str(view));
            }
            Op::Contains => {
                let needle = pop_str(&mut state.values);
                let hay = pop_str(&mut state.values);
                state.values.push(Cell::Bool(contains(
                    input,
                    &self.pool,
                    &state.arena,
                    hay,
                    needle,
                )));
            }
            Op::Starts | Op::Ends => {
                let needle = pop_str(&mut state.values);
                let hay = pop_str(&mut state.values);
                let ok = if matches!(op, Op::Starts) {
                    has_prefix_view(input, &self.pool, &state.arena, hay, needle)
                } else {
                    has_suffix_view(input, &self.pool, &state.arena, hay, needle)
                };
                state.values.push(Cell::Bool(ok));
            }
            Op::Slice => {
                let Cell::Nat(end) = pop(&mut state.values) else {
                    unreachable!("typed slice");
                };
                let Cell::Nat(start) = pop(&mut state.values) else {
                    unreachable!("typed slice");
                };
                let view = pop_str(&mut state.values);
                state.values.push(Cell::Str(slice_view(view, start, end)));
            }
            Op::Eq => {
                let right = pop(&mut state.values);
                let left = pop(&mut state.values);
                state.values.push(Cell::Bool(cells_eq(
                    input,
                    &self.pool,
                    &state.arena,
                    left,
                    right,
                )));
            }
            Op::Lt | Op::Le => {
                let Cell::Nat(right) = pop(&mut state.values) else {
                    unreachable!("typed comparison");
                };
                let Cell::Nat(left) = pop(&mut state.values) else {
                    unreachable!("typed comparison");
                };
                let ok = if matches!(op, Op::Lt) {
                    left < right
                } else {
                    left <= right
                };
                state.values.push(Cell::Bool(ok));
            }
            Op::And | Op::Or => {
                let Cell::Bool(right) = pop(&mut state.values) else {
                    unreachable!("typed boolean");
                };
                let Cell::Bool(left) = pop(&mut state.values) else {
                    unreachable!("typed boolean");
                };
                let ok = if matches!(op, Op::And) {
                    left && right
                } else {
                    left || right
                };
                state.values.push(Cell::Bool(ok));
            }
        }
        Ok(())
    }
}
pub(crate) fn compile(e: &Expr, params: &Params) -> Residual {
    let mut ops = Vec::new();
    let mut pool = Vec::new();
    let mut nvars = 0;
    emit(e, params, &mut ops, &mut pool, &mut nvars);
    Residual { ops, pool, nvars }
}
fn emit(e: &Expr, params: &Params, ops: &mut Vec<Op>, pool: &mut Vec<u8>, nvars: &mut usize) {
    match e {
        Expr::Val(Value::Bool(v)) => ops.push(Op::Bool(*v)),
        Expr::Val(Value::Nat(v)) => ops.push(Op::Nat(*v)),
        Expr::Val(Value::Str(s)) => push_const(ops, pool, s),
        Expr::Cap(id) => ops.push(Op::Cap(*id)),
        Expr::Param(name) => match params.get(name) {
            Some(s) => push_const(ops, pool, s),
            None => ops.push(Op::Unset),
        },
        Expr::Var(id) => {
            *nvars = (*nvars).max(id + 1);
            ops.push(Op::Load(*id));
        }
        Expr::Op(op, args) => {
            for arg in args {
                emit(arg, params, ops, pool, nvars);
            }
            ops.push(match op.as_str() {
                "lower" => Op::Lower,
                "upper" => Op::Upper,
                "trim" => Op::Trim,
                "len" => Op::Len,
                "not" => Op::Not,
                "concat" => Op::Concat,
                "contains" => Op::Contains,
                "starts" => Op::Starts,
                "ends" => Op::Ends,
                "slice" => Op::Slice,
                "eq" => Op::Eq,
                "lt" => Op::Lt,
                "le" => Op::Le,
                "and" => Op::And,
                "or" => Op::Or,
                _ => unreachable!("typed operator"),
            });
        }
        Expr::Apply { id, body, arg, .. } => {
            *nvars = (*nvars).max(id + 1);
            emit(arg, params, ops, pool, nvars);
            ops.push(Op::Bind(*id));
            emit(body, params, ops, pool, nvars);
            ops.push(Op::Unbind(*id));
        }
    }
}
fn push_const(ops: &mut Vec<Op>, pool: &mut Vec<u8>, bytes: &[u8]) {
    let start = pool.len();
    pool.extend_from_slice(bytes);
    ops.push(Op::Const(start, pool.len()));
}
fn check_span(
    origin: Origin,
    transform: Transform,
    start: usize,
    end: usize,
    limit: usize,
) -> Result<StrView, EvalError> {
    if end - start > limit {
        Err(EvalError::Limit)
    } else {
        Ok(StrView {
            origin,
            transform,
            start,
            end,
        })
    }
}
fn pop(values: &mut Vec<Cell>) -> Cell {
    values.pop().expect("residual stack")
}
fn pop_str(values: &mut Vec<Cell>) -> StrView {
    match pop(values) {
        Cell::Str(view) => view,
        _ => unreachable!("typed string"),
    }
}
fn compose(inner: Transform, outer: Transform) -> Transform {
    match outer {
        Transform::Identity => inner,
        fold => fold,
    }
}
fn logical_byte(input: &[u8], pool: &[u8], arena: &[u8], view: StrView, index: usize) -> u8 {
    let byte = match view.origin {
        Origin::Input => input[view.start + index],
        Origin::Pool => pool[view.start + index],
        Origin::Arena => arena[view.start + index],
    };
    match view.transform {
        Transform::Identity => byte,
        Transform::Lower => byte.to_ascii_lowercase(),
        Transform::Upper => byte.to_ascii_uppercase(),
    }
}
fn contiguous<'a>(
    input: &'a [u8],
    pool: &'a [u8],
    arena: &'a [u8],
    view: StrView,
) -> Option<&'a [u8]> {
    if view.transform != Transform::Identity {
        return None;
    }
    let bytes = match view.origin {
        Origin::Input => input,
        Origin::Pool => pool,
        Origin::Arena => arena,
    };
    Some(&bytes[view.start..view.end])
}
fn views_eq(input: &[u8], pool: &[u8], arena: &[u8], left: StrView, right: StrView) -> bool {
    if left.len() != right.len() {
        return false;
    }
    if let (Some(left), Some(right)) = (
        contiguous(input, pool, arena, left),
        contiguous(input, pool, arena, right),
    ) {
        return left == right;
    }
    (0..left.len()).all(|i| {
        logical_byte(input, pool, arena, left, i) == logical_byte(input, pool, arena, right, i)
    })
}
fn cells_eq(input: &[u8], pool: &[u8], arena: &[u8], left: Cell, right: Cell) -> bool {
    match (left, right) {
        (Cell::Bool(left), Cell::Bool(right)) => left == right,
        (Cell::Nat(left), Cell::Nat(right)) => left == right,
        (Cell::Str(left), Cell::Str(right)) => views_eq(input, pool, arena, left, right),
        _ => unreachable!("typed equality"),
    }
}
fn has_prefix(input: &[u8], pos: usize, view: StrView, pool: &[u8], arena: &[u8]) -> bool {
    let Some(rest) = input.get(pos..) else {
        return false;
    };
    if rest.len() < view.len() {
        return false;
    }
    if let Some(bytes) = contiguous(input, pool, arena, view) {
        return rest.starts_with(bytes);
    }
    (0..view.len()).all(|i| logical_byte(input, pool, arena, view, i) == rest[i])
}
fn has_prefix_view(input: &[u8], pool: &[u8], arena: &[u8], hay: StrView, needle: StrView) -> bool {
    needle.len() <= hay.len()
        && (0..needle.len()).all(|i| {
            logical_byte(input, pool, arena, hay, i) == logical_byte(input, pool, arena, needle, i)
        })
}
fn has_suffix_view(input: &[u8], pool: &[u8], arena: &[u8], hay: StrView, needle: StrView) -> bool {
    let Some(offset) = hay.len().checked_sub(needle.len()) else {
        return false;
    };
    (0..needle.len()).all(|i| {
        logical_byte(input, pool, arena, hay, offset + i)
            == logical_byte(input, pool, arena, needle, i)
    })
}
fn contains(input: &[u8], pool: &[u8], arena: &[u8], hay: StrView, needle: StrView) -> bool {
    if needle.is_empty() {
        return true;
    }
    if let (Some(hay), Some(needle)) = (
        contiguous(input, pool, arena, hay),
        contiguous(input, pool, arena, needle),
    ) {
        return hay.windows(needle.len()).any(|window| window == needle);
    }
    needle.len() <= hay.len()
        && (0..=hay.len() - needle.len()).any(|offset| {
            (0..needle.len()).all(|i| {
                logical_byte(input, pool, arena, hay, offset + i)
                    == logical_byte(input, pool, arena, needle, i)
            })
        })
}
fn trim_view(input: &[u8], pool: &[u8], arena: &[u8], view: StrView) -> StrView {
    let mut start = 0;
    let mut end = view.len();
    while start < end && logical_byte(input, pool, arena, view, start).is_ascii_whitespace() {
        start += 1;
    }
    while end > start && logical_byte(input, pool, arena, view, end - 1).is_ascii_whitespace() {
        end -= 1;
    }
    StrView {
        start: view.start + start,
        end: view.start + end,
        ..view
    }
}
fn slice_view(view: StrView, start: usize, end: usize) -> StrView {
    let start = start.min(view.len());
    let end = end.min(view.len()).max(start);
    StrView {
        start: view.start + start,
        end: view.start + end,
        ..view
    }
}
fn concat_views(
    input: &[u8],
    pool: &[u8],
    left: StrView,
    right: StrView,
    limit: usize,
    state: &mut State,
) -> Result<StrView, EvalError> {
    let length = left.len().saturating_add(right.len());
    if length > limit {
        return Err(EvalError::Limit);
    }
    state.copy.clear();
    state.copy.reserve(length);
    append_logical(input, pool, &state.arena, &mut state.copy, left);
    append_logical(input, pool, &state.arena, &mut state.copy, right);
    let start = state.arena.len();
    state.arena.extend_from_slice(&state.copy);
    Ok(StrView {
        origin: Origin::Arena,
        transform: Transform::Identity,
        start,
        end: state.arena.len(),
    })
}
fn append_logical(input: &[u8], pool: &[u8], arena: &[u8], copy: &mut Vec<u8>, view: StrView) {
    for index in 0..view.len() {
        copy.push(logical_byte(input, pool, arena, view, index));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{self, Captures};
    use crate::{Engine, Limits, Mode, Outcome};
    use std::collections::BTreeMap;

    fn names() -> BTreeMap<String, usize> {
        BTreeMap::from([("x".to_owned(), 0), ("y".to_owned(), 1)])
    }
    fn slot(span: Option<(usize, usize)>) -> CaptureSlot {
        match span {
            Some((start, end)) => CaptureSlot { start, end },
            None => CaptureSlot::MISSING,
        }
    }
    fn materialize(input: &[u8], pool: &[u8], arena: &[u8], cell: Cell) -> Value {
        match cell {
            Cell::Bool(v) => Value::Bool(v),
            Cell::Nat(v) => Value::Nat(v),
            Cell::Str(view) => Value::Str(
                (0..view.len())
                    .map(|i| logical_byte(input, pool, arena, view, i))
                    .collect(),
            ),
        }
    }
    #[test]
    fn residual_matches_strict_evaluator_including_compound_guards() {
        let sources = [
            r#"(and (le (len (cap x)) 16) (not (eq (cap x) "b")))"#,
            r#"(call (lambda (v Str) (and (le (len v) 2) (not (eq v "b")))) (cap x))"#,
            r#"(or true (eq (cap x) ""))"#,
            r#"(or true (eq "toolong" (cap x)))"#,
            r#"(and false (eq (cap x) "toolong"))"#,
            r#"(eq (lower (cap x)) "ab")"#,
            r#"(eq (upper (cap y)) "AB")"#,
            r#"(eq (trim "  A\n") "A")"#,
            r#"(eq (slice (concat (cap x) "cd") 1 3) "bc")"#,
            r#"(eq (concat (lower (cap x)) (upper (cap y))) "abAB")"#,
            r#"(contains (cap x) "b")"#,
            r#"(contains (upper (cap x)) "")"#,
            r#"(starts (upper (cap x)) "AB")"#,
            r#"(ends (lower (cap x)) "b")"#,
            r#"(call (lambda (x Bool) (call (lambda (x Str) (eq x "A")) "A")) false)"#,
            r#"(eq (len (cap x)) (len (cap y)))"#,
            "(le (len (cap x)) 0)",
            r#"(not (eq (cap x) "ab"))"#,
        ];
        let spans = [
            None,
            Some((0, usize::MAX)),
            Some((0, 0)),
            Some((0, 1)),
            Some((1, 2)),
            Some((0, 2)),
            Some((0, 5)),
        ];
        let mut state = State::default();
        let mut cases = 0;
        for source in sources {
            let expr = expr::parse(source, &names()).unwrap();
            let residual = compile(&expr, &Params::new());
            for x in spans {
                for y in spans {
                    let slots = [slot(x), slot(y)];
                    let public: Captures = slots.iter().map(|slot| slot.span()).collect();
                    let input = b"abAB \n";
                    for limit in [0usize, 1, 2, 4, 8] {
                        let expected = expr::eval(
                            &expr,
                            input,
                            &public,
                            &Params::new(),
                            &mut BTreeMap::new(),
                            limit,
                        );
                        let got = residual
                            .eval(input, &slots, limit, &mut state)
                            .map(|cell| materialize(input, &residual.pool, &state.arena, cell));
                        assert_eq!(got, expected, "{source} {x:?} {y:?} limit {limit}");
                        cases += 1;
                    }
                }
            }
        }
        let mut params = Params::new();
        params.insert("p".into(), b"ab".to_vec());
        let expr = expr::parse(r#"(eq (len (param p)) 2)"#, &names()).unwrap();
        let residual = compile(&expr, &params);
        let got = residual
            .eval(b"", &[], 2, &mut state)
            .map(|cell| materialize(b"", &residual.pool, &state.arena, cell));
        assert_eq!(got, Ok(Value::Bool(true)));
        let same_id = Expr::Apply {
            id: 0,
            ty: expr::Ty::Str,
            arg: Box::new(Expr::Val(Value::Str(b"B".to_vec()))),
            body: Box::new(Expr::Op(
                "concat".into(),
                vec![
                    Expr::Apply {
                        id: 0,
                        ty: expr::Ty::Str,
                        arg: Box::new(Expr::Val(Value::Str(b"A".to_vec()))),
                        body: Box::new(Expr::Var(0)),
                    },
                    Expr::Var(0),
                ],
            )),
        };
        let residual = compile(&same_id, &Params::new());
        let got = residual
            .eval(b"", &[], 8, &mut state)
            .map(|cell| materialize(b"", &residual.pool, &state.arena, cell))
            .unwrap();
        let expected = expr::eval(
            &same_id,
            b"",
            &Captures::new(),
            &Params::new(),
            &mut BTreeMap::new(),
            8,
        )
        .unwrap();
        assert_eq!(got, expected);
        assert_eq!(got, Value::Str(b"AB".to_vec()));
        println!("residual_expression_cases={cases}");
    }
    #[test]
    fn compound_guard_fallback_agrees_across_modes_and_limits() {
        let pattern = r#"(?<x>[ab]{0,32})(?{(and (le (len (cap x)) 16) (not (eq (cap x) "b")))})"#;
        let engines = [Mode::Raw, Mode::Pe, Mode::Jit]
            .map(|mode| Engine::new(pattern, Params::new(), mode).unwrap());
        for n in 0..=32 {
            for byte in *b"ab" {
                let text = vec![byte; n];
                let raw = engines[0].run(&text, Limits::default());
                for engine in &engines[1..] {
                    assert_eq!(engine.run(&text, Limits::default()), raw, "{n} {byte}");
                }
                let matched = matches!(raw, Outcome::Match(_));
                assert_eq!(matched, n <= 16 && text.as_slice() != b"b");
            }
        }
        for mode in [Mode::Raw, Mode::Pe, Mode::Jit] {
            let engine = Engine::new(pattern, Params::new(), mode).unwrap();
            assert_eq!(
                engine.run(
                    b"aaaaa",
                    Limits {
                        expression_bytes: 4,
                        ..Limits::default()
                    }
                ),
                Outcome::Budget,
                "{mode:?}"
            );
        }
        let clif = engines[2].native().unwrap().clif.clone();
        assert!(
            clif.contains("iconst.i64 98") && !clif.contains("call fn0"),
            "compound guard should compare 'b' inline\n{clif}"
        );
    }
    #[test]
    fn inlined_residual_agrees_with_the_interpreter() {
        let patterns = [
            r#"(?<x>[ab]*)(?{(contains (cap x) "ba")})"#,
            r#"(?<x>[ab]*)(?{(starts (upper (cap x)) "AB")})"#,
            r#"(?<x>[ab]*)(?{(ends (lower (cap x)) "b")})"#,
            r#"(?<x>[ A-Za-z]*)(?{(eq (trim (cap x)) "A")})"#,
            r#"(?<x>abcd)(?{(eq (slice (cap x) 1 3) "bc")})"#,
            r#"(?<x>ab)${(slice (concat (upper (cap x)) "cd") 1 3)}"#,
            r#"(?<x>a)?(?{(or true (eq (cap x) ""))})"#,
            r#"(?<x>a)?(?{(not (eq (cap x) "b"))})"#,
            r#"(?<x>[ab]*)(?{(call (lambda (v Str) (and (le (len v) 2) (not (eq v "b")))) (cap x))})"#,
        ];
        let inputs: [&[u8]; 8] = [b"", b"a", b"b", b"ba", b"  A", b"abBc", b"abcd", b"ab"];
        for pattern in patterns {
            let engines = [Mode::Raw, Mode::Pe, Mode::Jit]
                .map(|mode| Engine::new(pattern, Params::new(), mode).unwrap());
            for input in inputs {
                let limits = Limits::default();
                let expected = engines[0].run(input, limits);
                for engine in &engines[1..] {
                    assert_eq!(
                        engine.run(input, limits),
                        expected,
                        "{pattern} {input:?} {:?}",
                        engine.mode
                    );
                }
                for expression_bytes in [0usize, 1, 2, 8] {
                    let limits = Limits {
                        expression_bytes,
                        ..Limits::default()
                    };
                    let expected = engines[1].run(input, limits);
                    assert_eq!(
                        engines[2].run(input, limits),
                        expected,
                        "JIT byte limit {pattern} {input:?} {limits:?}"
                    );
                }
                for steps in [0usize, 1, 2, 4, 8] {
                    let limits = Limits {
                        steps,
                        ..Limits::default()
                    };
                    let expected = engines[1].run(input, limits);
                    assert_eq!(
                        engines[2].run(input, limits),
                        expected,
                        "JIT step budget {pattern} {input:?} {limits:?}"
                    );
                }
            }
        }
    }
}
