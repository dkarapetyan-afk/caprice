pub mod baseline;
pub mod expr;
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
pub struct Program {
    pub code: Vec<Inst>,
    pub names: BTreeMap<String, usize>,
    pub params: Params,
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
        let program = Box::new(Program {
            code: pattern::compile(&a, mode != Mode::Raw)?,
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
        let mut r = Runtime {
            program: &self.program,
            input,
            pos: 0,
            caps: vec![None; self.program.names.len()],
            stack: vec![],
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
            0 => Outcome::NoMatch,
            1 => Outcome::Match(r.caps),
            _ => Outcome::Budget,
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
    caps: Captures,
}
pub(crate) struct Runtime<'a> {
    program: &'a Program,
    input: &'a [u8],
    pos: usize,
    caps: Captures,
    stack: Vec<Frame>,
    steps: usize,
    limits: Limits,
    exhausted: bool,
}
impl Runtime<'_> {
    fn push(&mut self, pc: usize, pos: usize) -> bool {
        if self.stack.len() >= self.limits.stack {
            self.exhausted = true;
            false
        } else {
            self.stack.push(Frame {
                pc,
                pos,
                caps: self.caps.clone(),
            });
            true
        }
    }
    pub(crate) fn backtrack(&mut self) -> i64 {
        if self.exhausted {
            return -2;
        }
        if let Some(f) = self.stack.pop() {
            self.pos = f.pos;
            self.caps = f.caps;
            f.pc as i64
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
            Inst::Run(c, min, max, greedy) => {
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
                    // Enumeration order agrees with the unfused greedy/lazy loop.
                    if *greedy {
                        for p in low..end {
                            if !self.push(pc + 1, p) {
                                return 2;
                            }
                        }
                        self.pos = end
                    } else {
                        for p in ((low + 1)..=end).rev() {
                            if !self.push(pc + 1, p) {
                                return 2;
                            }
                        }
                        self.pos = low
                    }
                    true
                }
            }
            Inst::SaveStart(id) => {
                self.caps[*id] = Some((self.pos, usize::MAX));
                true
            }
            Inst::SaveEnd(id) => {
                let a = self.caps[*id].expect("compiler pairs capture markers").0;
                self.caps[*id] = Some((a, self.pos));
                true
            }
            Inst::Backref(id) => {
                if let Some((a, z)) =
                    self.caps[*id].filter(|(a, z)| a <= z && *z <= self.input.len())
                {
                    if self.input[self.pos..].starts_with(&self.input[a..z]) {
                        self.pos += z - a;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Inst::Guard(e) | Inst::Emit(e) => {
                match expr::eval(
                    e,
                    self.input,
                    &self.caps,
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
            Inst::Jump(_) => true,
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
