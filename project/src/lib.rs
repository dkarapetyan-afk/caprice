pub mod baseline;
pub mod cache;
pub mod expr;
mod fast_expr;
pub mod jit;
mod length;
pub mod pattern;
mod residual_expr;
use expr::{Captures, EvalError, Params};
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
    slots: Vec<CaptureSlot>,
    stack: Vec<Frame>,
    undo: Vec<(usize, CaptureSlot)>,
    expr_state: residual_expr::State,
}
/// Explicit native ABI: MAX start = missing; MAX end = open capture.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub(crate) struct CaptureSlot {
    start: usize,
    end: usize,
}
impl CaptureSlot {
    const MISSING: Self = Self {
        start: usize::MAX,
        end: usize::MAX,
    };
    fn span(self) -> Option<(usize, usize)> {
        (self.start != usize::MAX).then_some((self.start, self.end))
    }
}
fn capture_view(slots: &[CaptureSlot], out: &mut Captures) {
    out.clear();
    out.extend(slots.iter().map(|c| c.span()));
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
    residual: Vec<Option<residual_expr::Residual>>,
    rejection_literal: Option<Vec<u8>>,
    backtracking: bool,
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
        let fast: Vec<_> = code
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
        let residual = code
            .iter()
            .zip(&fast)
            .map(|(inst, fast)| match (inst, fast) {
                (Inst::Guard(e) | Inst::Emit(e), None) => Some(residual_expr::compile(e, &params)),
                _ => None,
            })
            .collect();
        let program = Box::new(Program {
            backtracking: code
                .iter()
                .any(|i| matches!(i, Inst::Run(..) | Inst::Split(..))),
            code,
            fast,
            residual,
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
        let status = self.run_internal(input, limits, scratch);
        if status == MatchStatus::Match {
            capture_view(&scratch.slots, &mut scratch.caps);
        }
        status
    }
    fn run_internal(&self, input: &[u8], limits: Limits, scratch: &mut Scratch) -> MatchStatus {
        scratch.caps.clear();
        scratch.slots.clear();
        scratch
            .slots
            .resize(self.program.names.len(), CaptureSlot::MISSING);
        scratch.stack.clear();
        scratch.undo.clear();
        if let Some(required) = &self.program.rejection_literal {
            if !input.windows(required.len()).any(|w| w == required) {
                return MatchStatus::NoMatch;
            }
        }
        let mut r = Runtime {
            capture_ptr: scratch.slots.as_mut_ptr(),
            input_ptr: input.as_ptr(),
            input_len: input.len(),
            step_limit: limits.steps,
            program: &self.program,
            input,
            pos: 0,
            caps: &mut scratch.slots,
            stack: &mut scratch.stack,
            undo: &mut scratch.undo,
            expr_state: &mut scratch.expr_state,
            steps: 0,
            limits,
            exhausted: false,
            expr_arena_ptr: std::ptr::null_mut(),
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
        match self.run_internal(input, Limits::default(), scratch) {
            MatchStatus::Match => Ok(true),
            MatchStatus::NoMatch => Ok(false),
            MatchStatus::Budget => Err("resource budget exhausted".into()),
        }
    }
    pub fn is_match(&self, input: &[u8]) -> Result<bool, String> {
        self.is_match_with(input, &mut Scratch::default())
    }
}
enum Cont {
    Span {
        last: usize,
        descending: bool,
    },
    Iters {
        origin: usize,
        next: usize,
        low: usize,
        high: usize,
        greedy: bool,
        inst: usize,
    },
}
struct Frame {
    pc: usize,
    pos: usize,
    undo_len: usize,
    cont: Option<Cont>,
}
#[repr(C)]
pub(crate) struct Runtime<'a> {
    // Plain fields explicitly laid out for generated native loads/stores.
    input_ptr: *const u8,
    input_len: usize,
    step_limit: usize,
    capture_ptr: *mut CaptureSlot,
    program: &'a Program,
    input: &'a [u8],
    pos: usize,
    caps: &'a mut Vec<CaptureSlot>,
    stack: &'a mut Vec<Frame>,
    undo: &'a mut Vec<(usize, CaptureSlot)>,
    expr_state: &'a mut residual_expr::State,
    steps: usize,
    limits: Limits,
    exhausted: bool,
    /// Arena base published for inlined concatenation. The Vec in `expr_state`
    /// owns the bytes; native code reloads this after every reservation.
    expr_arena_ptr: *mut u8,
}
impl Runtime<'_> {
    fn save_start(&mut self, id: usize) {
        if !self.stack.is_empty() {
            self.undo.push((id, self.caps[id]));
        }
        self.caps[id] = CaptureSlot {
            start: self.pos,
            end: usize::MAX,
        };
    }
    fn save_end(&mut self, id: usize) {
        if !self.stack.is_empty() {
            self.undo.push((id, self.caps[id]));
        }
        debug_assert_ne!(
            self.caps[id].start,
            usize::MAX,
            "compiler pairs capture markers"
        );
        self.caps[id].end = self.pos;
    }
    fn set_span(&mut self, id: usize, start_back: usize, end_back: Option<usize>) -> bool {
        if self.pos < start_back || end_back.is_some_and(|back| self.pos < back) {
            return false;
        }
        if !self.stack.is_empty() {
            self.undo.push((id, self.caps[id]));
        }
        self.caps[id] = CaptureSlot {
            start: self.pos - start_back,
            end: end_back.map(|back| self.pos - back).unwrap_or(usize::MAX),
        };
        true
    }
    fn piece(
        &self,
        pos: usize,
        class: &pattern::Class,
        piece_min: usize,
        piece_max: Option<usize>,
        sep: &[u8],
    ) -> Option<(usize, usize, usize)> {
        let mut end = pos;
        let bound = self
            .input
            .len()
            .min(pos.saturating_add(piece_max.unwrap_or(usize::MAX)));
        while end < bound && class.has(self.input[end]) {
            end += 1;
        }
        if end - pos < piece_min || !self.input[end..].starts_with(sep) {
            return None;
        }
        Some((pos, end, end + sep.len()))
    }
    fn locate_iter(
        &self,
        origin: usize,
        count: usize,
        class: &pattern::Class,
        piece_min: usize,
        piece_max: Option<usize>,
        sep: &[u8],
    ) -> Option<(Option<(usize, usize)>, usize)> {
        if count == 0 {
            return Some((None, origin));
        }
        let mut pos = origin;
        let mut last = None;
        for _ in 0..count {
            let (start, end, next) = self.piece(pos, class, piece_min, piece_max, sep)?;
            last = Some((start, end));
            pos = next;
        }
        Some((last, pos))
    }
    fn write_iter_capture(&mut self, id: usize, span: Option<(usize, usize)>, endpos: usize) {
        if let Some((start, end)) = span {
            if !self.stack.is_empty() {
                self.undo.push((id, self.caps[id]));
            }
            self.caps[id] = CaptureSlot { start, end };
        }
        self.pos = endpos;
    }
    fn iter_cap(&mut self, pc: usize) -> i64 {
        let pattern::Inst::IterCap {
            id,
            class,
            piece_min,
            piece_max,
            sep,
            reps_min,
            reps_max,
            greedy,
        } = self.program.code[pc].clone()
        else {
            return 1;
        };
        let origin = self.pos;
        let mut high = 0;
        let mut pos = origin;
        while high < reps_max {
            let Some((_, _, next)) = self.piece(pos, &class, piece_min, piece_max, &sep) else {
                break;
            };
            high += 1;
            pos = next;
        }
        if high < reps_min {
            return 1;
        }
        let chosen = if greedy { high } else { reps_min };
        let more = if greedy {
            chosen > reps_min
        } else {
            chosen < high
        };
        if more && !self.push(pc + 1, origin) {
            return 2;
        }
        if more {
            let next = if greedy { chosen - 1 } else { chosen + 1 };
            self.stack.last_mut().unwrap().cont = Some(Cont::Iters {
                origin,
                next,
                low: reps_min,
                high,
                greedy,
                inst: pc,
            });
        }
        let Some((span, endpos)) =
            self.locate_iter(origin, chosen, &class, piece_min, piece_max, &sep)
        else {
            return 1;
        };
        self.write_iter_capture(id, span, endpos);
        0
    }
    fn resume_iters(&mut self, pc: usize, cont: Cont) -> i64 {
        let Cont::Iters {
            origin,
            next,
            low,
            high,
            greedy,
            inst,
        } = cont
        else {
            return -1;
        };
        let Some(pattern::Inst::IterCap {
            id,
            class,
            piece_min,
            piece_max,
            sep,
            ..
        }) = self.program.code.get(inst).cloned()
        else {
            return -1;
        };
        let Some((span, endpos)) =
            self.locate_iter(origin, next, &class, piece_min, piece_max, &sep)
        else {
            return -1;
        };
        let following = if greedy {
            next.checked_sub(1)
        } else {
            next.checked_add(1)
        };
        let more = following.is_some_and(|count| (low..=high).contains(&count));
        if more && !self.push(pc, origin) {
            return -2;
        }
        if more {
            self.stack.last_mut().unwrap().cont = Some(Cont::Iters {
                origin,
                next: following.unwrap(),
                low,
                high,
                greedy,
                inst,
            });
        }
        self.write_iter_capture(id, span, endpos);
        pc as i64
    }
    fn backref(&mut self, id: usize) -> bool {
        if let Some((a, z)) = self.caps[id]
            .span()
            .filter(|(a, z)| a <= z && *z <= self.input.len())
        {
            if self.input[self.pos..].starts_with(&self.input[a..z]) {
                self.pos += z - a;
                return true;
            }
        }
        false
    }
    fn fast_expression(&mut self, pc: usize) -> i64 {
        match self.program.fast[pc].as_ref().unwrap().test_slots(
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
    fn residual_expression(&mut self, pc: usize) -> i64 {
        let limit = self.limits.expression_bytes;
        let pos = self.pos;
        let decision = self.program.residual[pc].as_ref().unwrap().run(
            self.input,
            pos,
            self.caps,
            limit,
            self.expr_state,
        );
        match decision {
            Ok(residual_expr::Decision::Success(n)) => {
                self.pos += n;
                0
            }
            Ok(residual_expr::Decision::Fail) | Err(EvalError::Unset) => 1,
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
                cont: None,
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
            if let Some(cont) = f.cont {
                match cont {
                    Cont::Span { last, descending } => {
                        if f.pos != last {
                            f.pos = if descending { f.pos - 1 } else { f.pos + 1 };
                            f.cont = Some(Cont::Span { last, descending });
                            self.stack.push(f);
                        }
                    }
                    Cont::Iters { .. } => return self.resume_iters(pc, cont),
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
        if matches!(&self.program.code[pc], pattern::Inst::IterCap { .. }) {
            return self.iter_cap(pc);
        }
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
                            self.stack.last_mut().unwrap().cont = Some(Cont::Span {
                                last: if greedy { low } else { end },
                                descending: greedy,
                            });
                        }
                        self.pos = if greedy { end } else { low };
                    } else {
                        self.pos = end;
                    }
                    true
                }
            }
            Inst::SizedRun(s) => {
                if let Some(n) = s.count(self.input.len(), self.limits.expression_bytes) {
                    if n <= self.input.len() - self.pos
                        && self.input[self.pos..self.pos + n]
                            .iter()
                            .all(|b| s.class.has(*b))
                    {
                        self.pos += n;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Inst::CaptureRun(id, c, min, max) => {
                let start = self.pos;
                let bound = self
                    .input
                    .len()
                    .min(start.saturating_add(max.unwrap_or(usize::MAX)));
                let mut end = start;
                while end < bound && c.has(self.input[end]) {
                    end += 1;
                }
                if end - start < *min {
                    false
                } else {
                    if !self.stack.is_empty() {
                        self.undo.push((*id, self.caps[*id]));
                    }
                    self.caps[*id] = CaptureSlot { start, end };
                    self.pos = end;
                    true
                }
            }
            Inst::SetSpan {
                id,
                start_back,
                end_back,
            } => self.set_span(*id, *start_back, *end_back),
            Inst::IterCap { .. } => unreachable!("handled before dispatch"),
            Inst::SaveStart(id) => {
                self.save_start(*id);
                true
            }
            Inst::SaveEnd(id) => {
                self.save_end(*id);
                true
            }
            Inst::Backref(id) => self.backref(*id),
            Inst::Guard(_) | Inst::Emit(_) => {
                if self.program.fast[pc].is_some() {
                    return self.fast_expression(pc);
                }
                return self.residual_expression(pc);
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
pub(crate) unsafe extern "C" fn jit_set_span(ptr: *mut u8, pc: i64) -> i64 {
    // SAFETY: the JIT passes the exclusive Runtime and a SetSpan instruction index.
    let rt = unsafe { &mut *(ptr as *mut Runtime<'_>) };
    let pattern::Inst::SetSpan {
        id,
        start_back,
        end_back,
    } = rt.program.code[pc as usize]
    else {
        return 1;
    };
    if rt.set_span(id, start_back, end_back) {
        0
    } else {
        1
    }
}
pub(crate) unsafe extern "C" fn jit_iter_cap(ptr: *mut u8, pc: i64) -> i64 {
    // SAFETY: the JIT passes the exclusive Runtime and an IterCap instruction index.
    let rt = unsafe { &mut *(ptr as *mut Runtime<'_>) };
    rt.iter_cap(pc as usize)
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
pub(crate) unsafe extern "C" fn jit_arena_reset(ptr: *mut u8, _n: i64) -> i64 {
    // SAFETY: the JIT passes the exclusive Runtime for this match.
    unsafe {
        let rt = &mut *(ptr as *mut Runtime<'_>);
        rt.expr_arena_ptr = rt.expr_state.reset_arena();
    }
    0
}
pub(crate) unsafe extern "C" fn jit_arena_alloc(ptr: *mut u8, n: i64) -> i64 {
    // SAFETY: `n` is the logical byte count reserved by an inlined concatenation.
    unsafe {
        let rt = &mut *(ptr as *mut Runtime<'_>);
        let (base, start) = rt.expr_state.reserve_arena(n as usize);
        rt.expr_arena_ptr = base;
        start as i64
    }
}
