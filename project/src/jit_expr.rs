//! Inline a residual expression into the native matcher.
//! Failures branch to the matcher's backtrack block. Byte-limit failures branch
//! to the budget return. `and`/`or` still run a later argument after a successful
//! earlier one, because only an unset or limit result leaves this sequence.
use crate::residual_expr::{Op, Residual};
use cranelift_codegen::ir::{
    condcodes::IntCC, types, Block, FuncRef, GlobalValue, InstBuilder, MemFlags, Value,
};
use cranelift_frontend::FunctionBuilder;

const ARENA: i32 = std::mem::offset_of!(crate::Runtime, expr_arena_ptr) as i32;
const EXPR_LIMIT: i32 = std::mem::offset_of!(crate::Runtime, limits) as i32
    + std::mem::offset_of!(crate::Limits, expression_bytes) as i32;

#[derive(Clone, Copy)]
enum Fold {
    Id,
    Lower,
    Upper,
}
impl Fold {
    fn apply(self, byte: u8) -> u8 {
        match self {
            Self::Id => byte,
            Self::Lower => byte.to_ascii_lowercase(),
            Self::Upper => byte.to_ascii_uppercase(),
        }
    }
    fn compose(self, outer: Self) -> Self {
        match outer {
            Self::Id => self,
            other => other,
        }
    }
}
#[derive(Clone, Copy)]
enum Place {
    Input,
    Pool,
    Arena,
}
#[derive(Clone, Copy)]
enum Imm {
    C(usize),
    V(Value),
}
#[derive(Clone, Copy)]
enum View {
    Pool {
        fold: Fold,
        start: usize,
        end: usize,
    },
    Dyn {
        place: Place,
        fold: Fold,
        start: Imm,
        end: Imm,
    },
}
#[derive(Clone, Copy)]
enum Ssa {
    Bool(Value),
    Nat(Imm),
    Str(View),
}
struct Env<'a, 'b> {
    b: &'a mut FunctionBuilder<'b>,
    ptr: Value,
    input: Value,
    input_len: Value,
    limit: Value,
    pool: &'a [u8],
    pool_addr: Option<Value>,
    alloc: FuncRef,
    fail: Block,
    budget: Block,
    stack: Vec<Ssa>,
    vars: Vec<Option<Ssa>>,
    saved: Vec<(usize, Option<Ssa>)>,
}
pub(crate) fn inline(
    b: &mut FunctionBuilder,
    ptr: Value,
    residual: &Residual,
    pool_gv: Option<GlobalValue>,
    arena: (FuncRef, FuncRef),
    exits: (Block, Block, Block),
) {
    let (reset, alloc) = arena;
    let (next, fail, budget) = exits;
    super::charge(b, ptr, budget);
    let input = super::load(b, ptr, super::INPUT);
    let input_len = super::load(b, ptr, super::LEN);
    let limit = super::load(b, ptr, EXPR_LIMIT);
    let pool_addr = pool_gv.map(|gv| b.ins().global_value(types::I64, gv));
    let mut env = Env {
        b,
        ptr,
        input,
        input_len,
        limit,
        pool: &residual.pool,
        pool_addr,
        alloc,
        fail,
        budget,
        stack: Vec::new(),
        vars: vec![None; residual.nvars],
        saved: Vec::new(),
    };
    if residual.ops.iter().any(|op| matches!(op, Op::Concat)) {
        let zero = env.iconst64(0);
        env.call(reset, zero);
    }
    for op in &residual.ops {
        if !env.operation(*op) {
            return;
        }
    }
    env.finish(next);
}
impl Env<'_, '_> {
    fn operation(&mut self, op: Op) -> bool {
        match op {
            Op::Bool(v) => {
                let bit = self.iconst8(u8::from(v));
                self.push(Ssa::Bool(bit));
            }
            Op::Nat(n) => self.push(Ssa::Nat(Imm::C(n))),
            Op::Unset => {
                let fail = self.fail;
                self.jump(fail);
                return false;
            }
            Op::Const(start, end) => {
                self.guard_len(end - start);
                self.push(Ssa::Str(View::Pool {
                    fold: Fold::Id,
                    start,
                    end,
                }));
            }
            Op::Cap(id) => {
                let Some(view) = self.capture(id) else {
                    return false;
                };
                self.push(Ssa::Str(view));
            }
            Op::Load(id) => match self.vars[id] {
                Some(cell) => self.push(cell),
                None => {
                    let fail = self.fail;
                    self.jump(fail);
                    return false;
                }
            },
            Op::Bind(id) => {
                let cell = self.pop();
                let old = self.vars[id].replace(cell);
                self.saved.push((id, old));
            }
            Op::Unbind(id) => {
                let (bound, old) = self.saved.pop().expect("lambda binding");
                debug_assert_eq!(bound, id);
                self.vars[id] = old;
            }
            Op::Lower | Op::Upper => {
                let fold = if matches!(op, Op::Lower) {
                    Fold::Lower
                } else {
                    Fold::Upper
                };
                let view = self.pop_str();
                self.push(Ssa::Str(self.refold(view, fold)));
            }
            Op::Trim => {
                let view = self.pop_str();
                let view = self.trim(view);
                self.push(Ssa::Str(view));
            }
            Op::Len => {
                let view = self.pop_str();
                let len = self.len_as_imm(&view);
                self.push(Ssa::Nat(len));
            }
            Op::Not => {
                let value = self.pop_bool();
                let flipped = self.b.ins().icmp_imm(IntCC::Equal, value, 0);
                self.push(Ssa::Bool(flipped));
            }
            Op::Concat => {
                let right = self.pop_str();
                let left = self.pop_str();
                let Some(view) = self.concat(left, right) else {
                    return false;
                };
                self.push(Ssa::Str(view));
            }
            Op::Contains => {
                let needle = self.pop_str();
                let hay = self.pop_str();
                let value = self.contains(hay, needle);
                self.push(Ssa::Bool(value));
            }
            Op::Starts | Op::Ends => {
                let needle = self.pop_str();
                let hay = self.pop_str();
                let value = if matches!(op, Op::Starts) {
                    self.starts(hay, needle)
                } else {
                    self.ends(hay, needle)
                };
                self.push(Ssa::Bool(value));
            }
            Op::Slice => {
                let end = self.pop_nat();
                let start = self.pop_nat();
                let view = self.pop_str();
                let view = self.slice(view, start, end);
                self.push(Ssa::Str(view));
            }
            Op::Eq => {
                let right = self.pop();
                let left = self.pop();
                let value = self.equal(left, right);
                self.push(Ssa::Bool(value));
            }
            Op::Lt | Op::Le => {
                let right = self.pop_nat();
                let left = self.pop_nat();
                let cc = if matches!(op, Op::Lt) {
                    IntCC::UnsignedLessThan
                } else {
                    IntCC::UnsignedLessThanOrEqual
                };
                let value = self.cmp_nat(cc, left, right);
                self.push(Ssa::Bool(value));
            }
            Op::And | Op::Or => {
                let right = self.pop_bool();
                let left = self.pop_bool();
                let value = if matches!(op, Op::And) {
                    self.b.ins().band(left, right)
                } else {
                    self.b.ins().bor(left, right)
                };
                self.push(Ssa::Bool(value));
            }
        }
        true
    }
    fn finish(&mut self, next: Block) {
        match self.pop() {
            Ssa::Bool(value) => {
                let fail = self.fail;
                self.b.ins().brif(value, next, &[], fail, &[]);
            }
            Ssa::Str(view) => self.accept_str(view, next),
            Ssa::Nat(_) => unreachable!("root expressions are Bool or Str"),
        }
    }
    fn capture(&mut self, id: usize) -> Option<View> {
        let slots = super::load(self.b, self.ptr, super::CAPS);
        let stride = std::mem::size_of::<crate::CaptureSlot>() as i32;
        let at = (id as i32) * stride;
        let start = self.b.ins().load(
            types::I64,
            MemFlags::new(),
            slots,
            at + std::mem::offset_of!(crate::CaptureSlot, start) as i32,
        );
        let end = self.b.ins().load(
            types::I64,
            MemFlags::new(),
            slots,
            at + std::mem::offset_of!(crate::CaptureSlot, end) as i32,
        );
        let missing = self.b.ins().icmp_imm(IntCC::Equal, start, -1);
        let unordered = self.b.ins().icmp(IntCC::UnsignedGreaterThan, start, end);
        let past = self
            .b
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, end, self.input_len);
        let either = self.b.ins().bor(unordered, past);
        let bad = self.b.ins().bor(missing, either);
        let fail = self.fail;
        self.divert(bad, fail);
        let len = self.b.ins().isub(end, start);
        if !self.guard_len_val(len) {
            return None;
        }
        Some(View::Dyn {
            place: Place::Input,
            fold: Fold::Id,
            start: Imm::V(start),
            end: Imm::V(end),
        })
    }
    fn concat(&mut self, left: View, right: View) -> Option<View> {
        let left_len = self.len_val(&left);
        let right_len = self.len_val(&right);
        let sum = self.b.ins().iadd(left_len, right_len);
        let wrapped = self.b.ins().icmp(IntCC::UnsignedGreaterThan, left_len, sum);
        let over = self
            .b
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, sum, self.limit);
        let bad = self.b.ins().bor(wrapped, over);
        let budget = self.budget;
        self.divert(bad, budget);
        let start = self.call(self.alloc, sum);
        self.copy_to(start, &left);
        let mid = self.b.ins().iadd(start, left_len);
        self.copy_to(mid, &right);
        let end = self.b.ins().iadd(start, sum);
        Some(View::Dyn {
            place: Place::Arena,
            fold: Fold::Id,
            start: Imm::V(start),
            end: Imm::V(end),
        })
    }
    fn accept_str(&mut self, view: View, next: Block) {
        let n = self.len_val(&view);
        let pos = super::load(self.b, self.ptr, super::POS);
        let remain = self.b.ins().isub(self.input_len, pos);
        let lack = self.b.ins().icmp(IntCC::UnsignedGreaterThan, n, remain);
        let fail = self.fail;
        self.divert(lack, fail);
        let matched = self.prefix_eq(pos, &view);
        let advance = self.b.create_block();
        self.b.ins().brif(matched, advance, &[], fail, &[]);
        self.b.switch_to_block(advance);
        let end = self.b.ins().iadd(pos, n);
        self.b
            .ins()
            .store(MemFlags::new(), end, self.ptr, super::POS);
        self.b.ins().jump(next, &[]);
    }
    fn equal(&mut self, left: Ssa, right: Ssa) -> Value {
        match (left, right) {
            (Ssa::Bool(left), Ssa::Bool(right))
            | (Ssa::Nat(Imm::V(left)), Ssa::Nat(Imm::V(right))) => {
                self.b.ins().icmp(IntCC::Equal, left, right)
            }
            (Ssa::Nat(left), Ssa::Nat(right)) => self.cmp_nat(IntCC::Equal, left, right),
            (Ssa::Str(left), Ssa::Str(right)) => self.eq_str(left, right),
            _ => unreachable!("typed equality"),
        }
    }
    fn eq_str(&mut self, left: View, right: View) -> Value {
        match (self.len_const(&left), self.len_const(&right)) {
            (Some(a), Some(b)) if a != b => self.iconst8(0),
            (Some(a), Some(_b)) => {
                if let (Some(p), Some(q)) = (self.pool_bytes(&left), self.pool_bytes(&right)) {
                    self.iconst8(u8::from(p == q))
                } else if a <= 8 {
                    self.eq_unrolled(&left, &right, a)
                } else {
                    self.eq_loop(&left, &right)
                }
            }
            (Some(n), _) | (_, Some(n)) if n <= 8 => self.eq_unrolled(&left, &right, n),
            _ => self.eq_loop(&left, &right),
        }
    }
    fn eq_unrolled(&mut self, left: &View, right: &View, n: usize) -> Value {
        let left_len = self.len_val(left);
        let right_len = self.len_val(right);
        let same = self.b.ins().icmp(IntCC::Equal, left_len, right_len);
        let body = self.b.create_block();
        let done = self.b.create_block();
        self.b.append_block_param(done, types::I8);
        let zero = self.iconst8(0);
        self.b.ins().brif(same, body, &[], done, &[zero.into()]);
        self.b.switch_to_block(body);
        let mut ok = self.iconst8(1);
        for i in 0..n {
            let left_byte = self.byte(left, Imm::C(i));
            let right_byte = self.byte(right, Imm::C(i));
            let eq = self.b.ins().icmp(IntCC::Equal, left_byte, right_byte);
            ok = self.b.ins().band(ok, eq);
        }
        self.b.ins().jump(done, &[ok.into()]);
        self.b.switch_to_block(done);
        self.b.block_params(done)[0]
    }
    fn eq_loop(&mut self, left: &View, right: &View) -> Value {
        let len = self.len_val(left);
        let other = self.len_val(right);
        let same = self.b.ins().icmp(IntCC::Equal, len, other);
        let head = self.b.create_block();
        let step = self.b.create_block();
        let yes = self.b.create_block();
        let done = self.b.create_block();
        self.b.append_block_param(head, types::I64);
        self.b.append_block_param(done, types::I8);
        let zero = self.iconst8(0);
        let start = self.iconst64(0);
        self.b
            .ins()
            .brif(same, head, &[start.into()], done, &[zero.into()]);
        self.b.switch_to_block(head);
        let index = self.b.block_params(head)[0];
        let more = self.b.ins().icmp(IntCC::UnsignedLessThan, index, len);
        self.b.ins().brif(more, step, &[], yes, &[]);
        self.b.switch_to_block(step);
        let left_byte = self.byte(left, Imm::V(index));
        let right_byte = self.byte(right, Imm::V(index));
        let eq = self.b.ins().icmp(IntCC::Equal, left_byte, right_byte);
        let next = self.b.ins().iadd_imm(index, 1);
        let mismatch = self.iconst8(0);
        self.b
            .ins()
            .brif(eq, head, &[next.into()], done, &[mismatch.into()]);
        self.b.switch_to_block(yes);
        let one = self.iconst8(1);
        self.b.ins().jump(done, &[one.into()]);
        self.b.switch_to_block(done);
        self.b.block_params(done)[0]
    }
    fn contains(&mut self, hay: View, needle: View) -> Value {
        if self.len_const(&needle) == Some(0) {
            return self.iconst8(1);
        }
        let hay_len = self.len_val(&hay);
        let needle_len = self.len_val(&needle);
        let too_long = self
            .b
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, needle_len, hay_len);
        let search = self.b.create_block();
        let head = self.b.create_block();
        let body = self.b.create_block();
        let done = self.b.create_block();
        self.b.append_block_param(head, types::I64);
        self.b.append_block_param(done, types::I8);
        let zero = self.iconst8(0);
        self.b
            .ins()
            .brif(too_long, done, &[zero.into()], search, &[]);
        self.b.switch_to_block(search);
        let last = self.b.ins().isub(hay_len, needle_len);
        let start = self.iconst64(0);
        self.b.ins().jump(head, &[start.into()]);
        self.b.switch_to_block(head);
        let offset = self.b.block_params(head)[0];
        let more = self
            .b
            .ins()
            .icmp(IntCC::UnsignedLessThanOrEqual, offset, last);
        self.b.ins().brif(more, body, &[], done, &[zero.into()]);
        self.b.switch_to_block(body);
        let found = self.region_eq(&hay, offset, &needle);
        let next = self.b.ins().iadd_imm(offset, 1);
        let one = self.iconst8(1);
        self.b
            .ins()
            .brif(found, done, &[one.into()], head, &[next.into()]);
        self.b.switch_to_block(done);
        self.b.block_params(done)[0]
    }
    fn starts(&mut self, hay: View, needle: View) -> Value {
        self.anchored(hay, needle, false)
    }
    fn ends(&mut self, hay: View, needle: View) -> Value {
        self.anchored(hay, needle, true)
    }
    fn anchored(&mut self, hay: View, needle: View, suffix: bool) -> Value {
        if self.len_const(&needle) == Some(0) {
            return self.iconst8(1);
        }
        let hay_len = self.len_val(&hay);
        let needle_len = self.len_val(&needle);
        let lack = self
            .b
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, needle_len, hay_len);
        let body = self.b.create_block();
        let done = self.b.create_block();
        self.b.append_block_param(done, types::I8);
        let zero = self.iconst8(0);
        self.b.ins().brif(lack, done, &[zero.into()], body, &[]);
        self.b.switch_to_block(body);
        let offset = if suffix {
            self.b.ins().isub(hay_len, needle_len)
        } else {
            self.iconst64(0)
        };
        let matched = self.region_eq(&hay, offset, &needle);
        self.b.ins().jump(done, &[matched.into()]);
        self.b.switch_to_block(done);
        self.b.block_params(done)[0]
    }
    fn region_eq(&mut self, hay: &View, offset: Value, needle: &View) -> Value {
        let Some(n) = self.len_const(needle) else {
            return self.region_loop(hay, offset, needle);
        };
        if n > 8 {
            return self.region_loop(hay, offset, needle);
        }
        let mut ok = self.iconst8(1);
        for i in 0..n {
            let at = self.b.ins().iadd_imm(offset, i as i64);
            let got = self.byte(hay, Imm::V(at));
            let expect = self.byte(needle, Imm::C(i));
            let eq = self.b.ins().icmp(IntCC::Equal, got, expect);
            ok = self.b.ins().band(ok, eq);
        }
        ok
    }
    fn region_loop(&mut self, hay: &View, offset: Value, needle: &View) -> Value {
        let n = self.len_val(needle);
        let head = self.b.create_block();
        let step = self.b.create_block();
        let done = self.b.create_block();
        self.b.append_block_param(head, types::I64);
        self.b.append_block_param(done, types::I8);
        let start = self.iconst64(0);
        self.b.ins().jump(head, &[start.into()]);
        self.b.switch_to_block(head);
        let index = self.b.block_params(head)[0];
        let more = self.b.ins().icmp(IntCC::UnsignedLessThan, index, n);
        let one = self.iconst8(1);
        self.b.ins().brif(more, step, &[], done, &[one.into()]);
        self.b.switch_to_block(step);
        let at = self.b.ins().iadd(offset, index);
        let got = self.byte(hay, Imm::V(at));
        let expect = self.byte(needle, Imm::V(index));
        let eq = self.b.ins().icmp(IntCC::Equal, got, expect);
        let next = self.b.ins().iadd_imm(index, 1);
        let zero = self.iconst8(0);
        self.b
            .ins()
            .brif(eq, head, &[next.into()], done, &[zero.into()]);
        self.b.switch_to_block(done);
        self.b.block_params(done)[0]
    }
    fn prefix_eq(&mut self, pos: Value, view: &View) -> Value {
        let window = View::Dyn {
            place: Place::Input,
            fold: Fold::Id,
            start: Imm::V(pos),
            end: Imm::V({
                let len = self.len_val(view);
                self.b.ins().iadd(pos, len)
            }),
        };
        // Lengths match by construction, so a mismatch is a byte mismatch.
        match self.len_const(view) {
            Some(n) if n <= 8 => {
                let mut ok = self.iconst8(1);
                for i in 0..n {
                    let got = self.byte(view, Imm::C(i));
                    let expect = self.byte(&window, Imm::C(i));
                    let eq = self.b.ins().icmp(IntCC::Equal, got, expect);
                    ok = self.b.ins().band(ok, eq);
                }
                ok
            }
            _ => self.eq_loop(view, &window),
        }
    }
    fn trim(&mut self, view: View) -> View {
        if let View::Pool { fold, start, end } = view {
            let mut a = start;
            let mut z = end;
            while a < z && Fold::apply(fold, self.pool[a]).is_ascii_whitespace() {
                a += 1;
            }
            while z > a && Fold::apply(fold, self.pool[z - 1]).is_ascii_whitespace() {
                z -= 1;
            }
            return View::Pool {
                fold,
                start: a,
                end: z,
            };
        }
        let (start, end) = self.trim_dyn(view);
        let (place, fold) = self.place_fold(view);
        View::Dyn {
            place,
            fold,
            start,
            end,
        }
    }
    fn trim_dyn(&mut self, view: View) -> (Imm, Imm) {
        let base = self.val(self.view_start(&view));
        let len = self.len_val(&view);
        let head = self.b.create_block();
        let lead_body = self.b.create_block();
        let lead_done = self.b.create_block();
        self.b.append_block_param(head, types::I64);
        self.b.append_block_param(lead_done, types::I64);
        let start = self.iconst64(0);
        self.b.ins().jump(head, &[start.into()]);
        self.b.switch_to_block(head);
        let index = self.b.block_params(head)[0];
        let more = self.b.ins().icmp(IntCC::UnsignedLessThan, index, len);
        self.b
            .ins()
            .brif(more, lead_body, &[], lead_done, &[index.into()]);
        self.b.switch_to_block(lead_body);
        let blank = self.is_ws(&view, Imm::V(index));
        let next = self.b.ins().iadd_imm(index, 1);
        self.b
            .ins()
            .brif(blank, head, &[next.into()], lead_done, &[index.into()]);
        self.b.switch_to_block(lead_done);
        let lead = self.b.block_params(lead_done)[0];
        let tail = self.b.create_block();
        let tail_body = self.b.create_block();
        let tail_done = self.b.create_block();
        self.b.append_block_param(tail, types::I64);
        self.b.append_block_param(tail_done, types::I64);
        self.b.ins().jump(tail, &[len.into()]);
        self.b.switch_to_block(tail);
        let end = self.b.block_params(tail)[0];
        let more = self.b.ins().icmp(IntCC::UnsignedGreaterThan, end, lead);
        self.b
            .ins()
            .brif(more, tail_body, &[], tail_done, &[end.into()]);
        self.b.switch_to_block(tail_body);
        let prev = self.b.ins().iadd_imm(end, -1);
        let blank = self.is_ws(&view, Imm::V(prev));
        self.b
            .ins()
            .brif(blank, tail, &[prev.into()], tail_done, &[end.into()]);
        self.b.switch_to_block(tail_done);
        let trail = self.b.block_params(tail_done)[0];
        (
            Imm::V(self.b.ins().iadd(base, lead)),
            Imm::V(self.b.ins().iadd(base, trail)),
        )
    }
    fn slice(&mut self, view: View, start: Imm, end: Imm) -> View {
        if let (
            View::Pool {
                fold,
                start: base,
                end: lim,
            },
            Imm::C(a),
            Imm::C(z),
        ) = (view, start, end)
        {
            let n = lim - base;
            let a = a.min(n);
            let z = z.min(n).max(a);
            return View::Pool {
                fold,
                start: base + a,
                end: base + z,
            };
        }
        let base = self.val(self.view_start(&view));
        let len = self.len_val(&view);
        let start = self.val(start);
        let end = self.val(end);
        let a = self.umin(start, len);
        let capped = self.umin(end, len);
        let z = self.umax(a, capped);
        let (place, fold) = self.place_fold(view);
        View::Dyn {
            place,
            fold,
            start: Imm::V(self.b.ins().iadd(base, a)),
            end: Imm::V(self.b.ins().iadd(base, z)),
        }
    }
    fn refold(&self, view: View, outer: Fold) -> View {
        match view {
            View::Pool { fold, start, end } => View::Pool {
                fold: fold.compose(outer),
                start,
                end,
            },
            View::Dyn {
                place,
                fold,
                start,
                end,
            } => View::Dyn {
                place,
                fold: fold.compose(outer),
                start,
                end,
            },
        }
    }
    fn copy_to(&mut self, dst: Value, view: &View) {
        match self.len_const(view) {
            Some(n) if n <= 8 => {
                for i in 0..n {
                    let byte = self.byte(view, Imm::C(i));
                    self.store_byte(dst, Imm::C(i), byte);
                }
            }
            _ => {
                let len = self.len_val(view);
                let head = self.b.create_block();
                let body = self.b.create_block();
                let exit = self.b.create_block();
                self.b.append_block_param(head, types::I64);
                let start = self.iconst64(0);
                self.b.ins().jump(head, &[start.into()]);
                self.b.switch_to_block(head);
                let index = self.b.block_params(head)[0];
                let more = self.b.ins().icmp(IntCC::UnsignedLessThan, index, len);
                self.b.ins().brif(more, body, &[], exit, &[]);
                self.b.switch_to_block(body);
                let byte = self.byte(view, Imm::V(index));
                self.store_byte(dst, Imm::V(index), byte);
                let next = self.b.ins().iadd_imm(index, 1);
                self.b.ins().jump(head, &[next.into()]);
                self.b.switch_to_block(exit);
            }
        }
    }
    fn store_byte(&mut self, dst: Value, index: Imm, byte: Value) {
        let base = self.arena_ptr();
        let off = self.offset(dst, index);
        let at = self.b.ins().iadd(base, off);
        let narrow = self.b.ins().ireduce(types::I8, byte);
        self.b.ins().store(MemFlags::new(), narrow, at, 0);
    }
    fn offset(&mut self, base: Value, index: Imm) -> Value {
        match index {
            Imm::C(0) => base,
            Imm::C(n) => self.b.ins().iadd_imm(base, n as i64),
            Imm::V(index) => self.b.ins().iadd(base, index),
        }
    }
    fn byte(&mut self, view: &View, index: Imm) -> Value {
        if let View::Pool { fold, start, .. } = *view {
            if let Imm::C(i) = index {
                let byte = u64::from(fold.apply(self.pool[start + i]));
                return self.iconst64(byte as i64);
            }
        }
        let (base, fold, start) = match *view {
            View::Pool { fold, start, .. } => {
                (self.pool_addr.expect("pool bytes"), fold, Imm::C(start))
            }
            View::Dyn {
                place, fold, start, ..
            } => (self.place_base(place), fold, start),
        };
        let off = self.add_imm(start, index);
        let off = self.val(off);
        let at = self.b.ins().iadd(base, off);
        let raw = self.b.ins().load(types::I8, MemFlags::new(), at, 0);
        let wide = self.b.ins().uextend(types::I64, raw);
        self.fold_runtime(wide, fold)
    }
    fn add_imm(&mut self, start: Imm, index: Imm) -> Imm {
        match (start, index) {
            (Imm::C(a), Imm::C(b)) => Imm::C(a + b),
            (Imm::C(a), Imm::V(b)) => Imm::V(self.b.ins().iadd_imm(b, a as i64)),
            (Imm::V(a), Imm::C(b)) => Imm::V(self.b.ins().iadd_imm(a, b as i64)),
            (Imm::V(a), Imm::V(b)) => Imm::V(self.b.ins().iadd(a, b)),
        }
    }
    fn place_base(&mut self, place: Place) -> Value {
        match place {
            Place::Input => self.input,
            Place::Pool => self.pool_addr.expect("pool bytes"),
            Place::Arena => self.arena_ptr(),
        }
    }
    fn fold_runtime(&mut self, byte: Value, fold: Fold) -> Value {
        match fold {
            Fold::Id => byte,
            Fold::Lower => self.map_range(byte, b'A', b'Z', 32),
            Fold::Upper => self.map_range(byte, b'a', b'z', -32),
        }
    }
    fn map_range(&mut self, byte: Value, lo: u8, hi: u8, delta: i64) -> Value {
        let above = self
            .b
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, byte, i64::from(lo));
        let below = self
            .b
            .ins()
            .icmp_imm(IntCC::UnsignedLessThanOrEqual, byte, i64::from(hi));
        let inside = self.b.ins().band(above, below);
        let mapped = self.b.ins().iadd_imm(byte, delta);
        self.b.ins().select(inside, mapped, byte)
    }
    fn is_ws(&mut self, view: &View, index: Imm) -> Value {
        let byte = self.byte(view, index);
        let mut any = self.iconst8(0);
        for mark in [0x20u8, 0x09, 0x0a, 0x0d, 0x0c] {
            let hit = self.b.ins().icmp_imm(IntCC::Equal, byte, i64::from(mark));
            any = self.b.ins().bor(any, hit);
        }
        any
    }
    fn guard_len(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let too = self
            .b
            .ins()
            .icmp_imm(IntCC::UnsignedLessThan, self.limit, len as i64);
        let budget = self.budget;
        self.divert(too, budget);
    }
    /// Returns false when this block already diverted entirely to the budget.
    fn guard_len_val(&mut self, len: Value) -> bool {
        let too = self
            .b
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, len, self.limit);
        let budget = self.budget;
        self.divert(too, budget);
        true
    }
    fn divert(&mut self, cond: Value, to: Block) {
        let cont = self.b.create_block();
        self.b.ins().brif(cond, to, &[], cont, &[]);
        self.b.switch_to_block(cont);
    }
    fn jump(&mut self, to: Block) {
        self.b.ins().jump(to, &[]);
    }
    fn call(&mut self, func: FuncRef, arg: Value) -> Value {
        let call = self.b.ins().call(func, &[self.ptr, arg]);
        self.b.inst_results(call)[0]
    }
    fn arena_ptr(&mut self) -> Value {
        super::load(self.b, self.ptr, ARENA)
    }
    fn len_const(&self, view: &View) -> Option<usize> {
        match *view {
            View::Pool { start, end, .. } => Some(end - start),
            View::Dyn {
                start: Imm::C(a),
                end: Imm::C(b),
                ..
            } => Some(b - a),
            _ => None,
        }
    }
    fn len_as_imm(&mut self, view: &View) -> Imm {
        if let Some(n) = self.len_const(view) {
            Imm::C(n)
        } else {
            let View::Dyn { start, end, .. } = *view else {
                unreachable!("pool lengths are constant");
            };
            let left = self.val(start);
            let right = self.val(end);
            Imm::V(self.b.ins().isub(right, left))
        }
    }
    fn len_val(&mut self, view: &View) -> Value {
        let len = self.len_as_imm(view);
        self.val(len)
    }
    fn view_start(&self, view: &View) -> Imm {
        match *view {
            View::Pool { start, .. } => Imm::C(start),
            View::Dyn { start, .. } => start,
        }
    }
    fn place_fold(&self, view: View) -> (Place, Fold) {
        match view {
            View::Pool { fold, .. } => (Place::Pool, fold),
            View::Dyn { place, fold, .. } => (place, fold),
        }
    }
    fn pool_bytes(&self, view: &View) -> Option<Vec<u8>> {
        let View::Pool { fold, start, end } = *view else {
            return None;
        };
        Some(
            self.pool[start..end]
                .iter()
                .map(|byte| fold.apply(*byte))
                .collect(),
        )
    }
    fn cmp_nat(&mut self, cc: IntCC, left: Imm, right: Imm) -> Value {
        if let (Imm::C(a), Imm::C(b)) = (left, right) {
            let ok = match cc {
                IntCC::Equal => a == b,
                IntCC::UnsignedLessThan => a < b,
                IntCC::UnsignedLessThanOrEqual => a <= b,
                _ => unreachable!("nat comparison"),
            };
            return self.iconst8(u8::from(ok));
        }
        let left = self.val(left);
        let right = self.val(right);
        self.b.ins().icmp(cc, left, right)
    }
    fn umin(&mut self, left: Value, right: Value) -> Value {
        let less = self.b.ins().icmp(IntCC::UnsignedLessThan, left, right);
        self.b.ins().select(less, left, right)
    }
    fn umax(&mut self, left: Value, right: Value) -> Value {
        let more = self.b.ins().icmp(IntCC::UnsignedGreaterThan, left, right);
        self.b.ins().select(more, left, right)
    }
    fn val(&mut self, imm: Imm) -> Value {
        match imm {
            Imm::C(n) => self.b.ins().iconst(types::I64, n as i64),
            Imm::V(value) => value,
        }
    }
    fn iconst8(&mut self, n: u8) -> Value {
        self.b.ins().iconst(types::I8, i64::from(n))
    }
    fn iconst64(&mut self, n: i64) -> Value {
        self.b.ins().iconst(types::I64, n)
    }
    fn push(&mut self, cell: Ssa) {
        self.stack.push(cell);
    }
    fn pop(&mut self) -> Ssa {
        self.stack.pop().expect("residual stack")
    }
    fn pop_str(&mut self) -> View {
        match self.pop() {
            Ssa::Str(view) => view,
            _ => unreachable!("typed string"),
        }
    }
    fn pop_nat(&mut self) -> Imm {
        match self.pop() {
            Ssa::Nat(n) => n,
            _ => unreachable!("typed nat"),
        }
    }
    fn pop_bool(&mut self) -> Value {
        match self.pop() {
            Ssa::Bool(v) => v,
            _ => unreachable!("typed bool"),
        }
    }
}
