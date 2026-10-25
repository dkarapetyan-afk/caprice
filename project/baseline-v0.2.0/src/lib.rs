pub mod baseline;
pub mod expr;
mod fast_expr;
pub mod jit;
pub mod pattern;
use expr::{Captures, EvalError, Params, Value};
use pattern::Inst;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub steps: usize,
    pub stack: usize,
    pub expression_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            steps: 1_000_000,
            stack: 100_000,
            expression_bytes: 1 << 20,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Match(Captures),
    NoMatch,
    Budget,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Raw,
    Pe,
    Jit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchStatus {
    Match,
    NoMatch,
    Budget,
}
/// Caller-owned reusable match storage, analogous to PCRE2 match data.
#[derive(Default)]
pub struct Scratch {
    caps: Captures,
    stack: Vec<Frame>,
    undo: Vec<(usize, Option<(usize, usize)>)>,
}
impl Scratch {
    pub fn captures(&self) -> &[Option<(usize, usize)>] {
        &self.caps
    }
}
pub struct Program {
    pub code: Vec<Inst>,
    pub names: BTreeMap<String, usize>,
    pub params: Params,
    fast: Vec<Option<fast_expr::FastExpr>>,
    rejection_literal: Option<Vec<u8>>,
}
pub struct Engine {
    program: Box<Program>,
    native: Option<jit::Native>,
    pub mode: Mode,
}
impl Engine {
    pub fn program(&self) -> &Program {
        &self.program
    }
    pub fn native(&self) -> Option<&jit::Native> {
        self.native.as_ref()
    }
    pub fn new(pattern: &str, params: Params, mode: Mode) -> Result<Self, String> {
        if params.values().any(|v| v.len() > 1 << 20) {
            return Err("static parameter exceeds 1 MiB".into());
        }
        let (mut a, names) = pattern::parse(pattern)?;
        pattern::check_params(&a, &params)?;
        if mode != Mode::Raw {
            a = pattern::specialize(&a, &params)
        }
        let code = pattern::compile(&a, mode != Mode::Raw)?;
        let fast = code
            .iter()
            .map(|i| {
                if mode != Mode::Raw {
                    match i {
                        Inst::Guard(e) | Inst::Emit(e) => fast_expr::FastExpr::compile(e),
                        _ => None,
                    }
                } else {
                    None
                }
            })
            .collect();
        let program = Box::new(Program {
            code,
            fast,
            rejection_literal: if mode == Mode::Raw {
                None
            } else {
                pattern::rejection_literal(&a)
            },
            names,
            params,
        });
        let native = if mode == Mode::Jit {
            Some(jit::Native::compile(&program)?)
        } else {
            None
        };
        Ok(Self {
            program,
            native,
            mode,
        })
    }
    pub fn run(&self, input: &[u8], limits: Limits) -> Outcome {
        let mut scratch = Scratch::default();
        match self.run_with(input, limits, &mut scratch) {
            MatchStatus::Match => Outcome::Match(scratch.caps),
            MatchStatus::NoMatch => Outcome::NoMatch,
            MatchStatus::Budget => Outcome::Budget,
        }
    }
    pub fn run_with(&self, input: &[u8], limits: Limits, scratch: &mut Scratch) -> MatchStatus {
        scratch.caps.clear();
        scratch.caps.resize(self.program.names.len(), None);
        scratch.stack.clear();
        scratch.undo.clear();
        if let Some(required) = &self.program.rejection_literal {
            if !input.windows(required.len()).any(|w| w == required) {
                return MatchStatus::NoMatch;
            }
        }
        let mut r = Runtime {
            input_ptr: input.as_ptr(),
            input_len: input.len(),
            step_limit: limits.steps,
            program: &self.program,
            input,
            pos: 0,
            caps: &mut scratch.caps,
            stack: &mut scratch.stack,
            undo: &mut scratch.undo,
            steps: 0,
            limits,
            exhausted: false,
        };
        let status = if let Some(j) = &self.native {
            j.run(&mut r)
        } else {
            interpret(&mut r)
        };
        match status {
            0 => MatchStatus::NoMatch,
            1 => MatchStatus::Match,
            _ => MatchStatus::Budget,
        }
    }
    pub fn is_match_with(&self, input: &[u8], scratch: &mut Scratch) -> Result<bool, String> {
        match self.run_with(input, Limits::default(), scratch) {
            MatchStatus::Match => Ok(true),
            MatchStatus::NoMatch => Ok(false),
            MatchStatus::Budget => Err("resource budget exhausted".into()),
        }
    }
    pub fn is_match(&self, input: &[u8]) -> Result<bool, String> {
        match self.run(input, Limits::default()) {
            Outcome::Match(_) => Ok(true),
            Outcome::NoMatch => Ok(false),
            Outcome::Budget => Err("resource budget exhausted".into()),
        }
    }
}
struct Frame {
    pc: usize,
    pos: usize,
    undo_len: usize,
    range: Option<(usize, bool)>,
}
#[repr(C)]
pub(crate) struct Runtime<'a> {
    // Plain fields explicitly laid out for generated native loads/stores.
    input_ptr: *const u8,
    input_len: usize,
    step_limit: usize,
    program: &'a Program,
    input: &'a [u8],
    pos: usize,
    caps: &'a mut Captures,
    stack: &'a mut Vec<Frame>,
    undo: &'a mut Vec<(usize, Option<(usize, usize)>)>,
    steps: usize,
    limits: Limits,
    exhausted: bool,
}
impl Runtime<'_> {
    fn save_start(&mut self, id: usize) {
        if !self.stack.is_empty() {
            self.undo.push((id, self.caps[id]));
        }
        self.caps[id] = Some((self.pos, usize::MAX));
    }
    fn save_end(&mut self, id: usize) {
        if !self.stack.is_empty() {
            self.undo.push((id, self.caps[id]));
        }
        self.caps[id] = Some((
            self.caps[id].expect("compiler pairs capture markers").0,
            self.pos,
        ));
    }
    fn backref(&mut self, id: usize) -> bool {
        if let Some((a, z)) = self.caps[id].filter(|(a, z)| a <= z && *z <= self.input.len()) {
            if self.input[self.pos..].starts_with(&self.input[a..z]) {
                self.pos += z - a;
                return true;
            }
        }
        false
    }
    fn fast_expression(&mut self, pc: usize) -> i64 {
        match self.program.fast[pc].as_ref().unwrap().test(
            self.input,
            self.caps,
            self.pos,
            self.limits.expression_bytes,
        ) {
            Ok(Some(n)) => {
                self.pos += n;
                0
            }
            Ok(None) | Err(EvalError::Unset) => 1,
            Err(EvalError::Limit) => {
                self.exhausted = true;
                2
            }
        }
    }
    fn push(&mut self, pc: usize, pos: usize) -> bool {
        if self.stack.len() >= self.limits.stack {
            self.exhausted = true;
            false
        } else {
            self.stack.push(Frame {
                pc,
                pos,
                undo_len: self.undo.len(),
                range: None,
            });
            true
        }
    }
    pub(crate) fn backtrack(&mut self) -> i64 {
        if self.exhausted {
            return -2;
        }
        if let Some(mut f) = self.stack.pop() {
            self.pos = f.pos;
            while self.undo.len() > f.undo_len {
                let (id, old) = self.undo.pop().unwrap();
                self.caps[id] = old;
            }
            let pc = f.pc;
            if let Some((last, descending)) = f.range {
                if f.pos != last {
                    f.pos = if descending { f.pos - 1 } else { f.pos + 1 };
                    self.stack.push(f);
                }
            }
            pc as i64
        } else {
            -1
        }
    }
    /// 0 = success, 1 = branch failure, 2 = resource exhaustion.
    pub(crate) fn step(&mut self, pc: usize) -> i64 {
        if self.steps >= self.limits.steps {
            self.exhausted = true;
            return 2;
        }
        self.steps += 1;
        let ok = match &self.program.code[pc] {
            Inst::Lit(s) => {
                if self.input[self.pos..].starts_with(s) {
                    self.pos += s.len();
                    true
                } else {
                    false
                }
            }
            Inst::Class(c) => {
                if self.input.get(self.pos).is_some_and(|b| c.has(*b)) {
                    self.pos += 1;
                    true
                } else {
                    false
                }
            }
            Inst::Run(c, min, max, _) | Inst::AtomicRun(c, min, max) => {
                let start = self.pos;
                let mut end = start;
                let bound = self
                    .input
                    .len()
                    .min(start.saturating_add(max.unwrap_or(usize::MAX)));
                while end < bound && c.has(self.input[end]) {
                    end += 1
                }
                if end - start < *min {
                    false
                } else {
                    let low = start + *min;
                    if let Inst::Run(_, _, _, greedy) = &self.program.code[pc] {
                        let greedy = *greedy;
                        if low < end {
                            let first = if greedy { end - 1 } else { low + 1 };
                            if !self.push(pc + 1, first) {
                                return 2;
                            }
                            self.stack.last_mut().unwrap().range =
                                Some((if greedy { low } else { end }, greedy));
                        }
                        self.pos = if greedy { end } else { low };
                    } else {
                        self.pos = end;
                    }
                    true
                }
            }
            Inst::SaveStart(id) => {
                self.save_start(*id);
                true
            }
            Inst::SaveEnd(id) => {
                self.save_end(*id);
                true
            }
            Inst::Backref(id) => self.backref(*id),
            Inst::Guard(e) | Inst::Emit(e) => {
                if self.program.fast[pc].is_some() {
                    return self.fast_expression(pc);
                }
                match expr::eval(
                    e,
                    self.input,
                    self.caps,
                    &self.program.params,
                    &mut BTreeMap::new(),
                    self.limits.expression_bytes,
                ) {
                    Ok(Value::Bool(b)) => b,
                    Ok(Value::Str(s)) => {
                        if self.input[self.pos..].starts_with(&s) {
                            self.pos += s.len();
                            true
                        } else {
                            false
                        }
                    }
                    Ok(Value::Nat(_)) => unreachable!("root expressions are checked"),
                    Err(EvalError::Unset) => false,
                    Err(EvalError::Limit) => {
                        self.exhausted = true;
                        return 2;
                    }
                }
            }
            Inst::Split(_, other) => {
                if !self.push(*other, self.pos) {
                    return 2;
                }
                true
            }
            Inst::Jump(_) | Inst::Switch(..) => true,
            Inst::Start => self.pos == 0,
            Inst::End | Inst::Accept => self.pos == self.input.len(),
            Inst::Fail => false,
        };
        if ok {
            0
        } else {
            1
        }
    }
}
fn interpret(r: &mut Runtime<'_>) -> i64 {
    let mut pc = 0;
    loop {
        match r.step(pc) {
            2 => return 2,
            1 => {
                let next = r.backtrack();
                if next < 0 {
                    return if next == -2 { 2 } else { 0 };
                }
                pc = next as usize
            }
            _ => match r.program.code[pc] {
                Inst::Accept => return 1,
                Inst::Jump(n) => pc = n,
                Inst::Split(n, _) => pc = n,
                Inst::Switch(ref c, a, b) => {
                    pc = if r.input.get(r.pos).is_some_and(|v| c.has(*v)) {
                        a
                    } else {
                        b
                    }
                }
                _ => pc += 1,
            },
        }
    }
}
// No callback accepts source code or calls into a host interpreter.
pub(crate) unsafe extern "C" fn jit_step(ptr: *mut u8, pc: i64) -> i64 {
    // SAFETY: Native owns executable code; Engine keeps Program alive. The
    // invocation supplies one exclusive Runtime for this complete synchronous call.
    unsafe { (&mut *(ptr as *mut Runtime<'_>)).step(pc as usize) }
}
pub(crate) unsafe extern "C" fn jit_backtrack(ptr: *mut u8) -> i64 {
    unsafe { (&mut *(ptr as *mut Runtime<'_>)).backtrack() }
}
pub(crate) unsafe extern "C" fn jit_save_start(ptr: *mut u8, id: i64) -> i64 {
    unsafe {
        (&mut *(ptr as *mut Runtime<'_>)).save_start(id as usize);
    }
    0
}
pub(crate) unsafe extern "C" fn jit_save_end(ptr: *mut u8, id: i64) -> i64 {
    unsafe {
        (&mut *(ptr as *mut Runtime<'_>)).save_end(id as usize);
    }
    0
}
pub(crate) unsafe extern "C" fn jit_backref(ptr: *mut u8, id: i64) -> i64 {
    if unsafe { (&mut *(ptr as *mut Runtime<'_>)).backref(id as usize) } {
        0
    } else {
        1
    }
}
pub(crate) unsafe extern "C" fn jit_fast_expression(ptr: *mut u8, pc: i64) -> i64 {
    unsafe { (&mut *(ptr as *mut Runtime<'_>)).fast_expression(pc as usize) }
}
