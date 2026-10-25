//! Native compilation of the residual VM control-flow graph. Fast expressions
//! stay checked runtime calls. Generic residual expressions are inlined.
use crate::{jit_backtrack, jit_step, pattern::Inst, Program, Runtime};
use cranelift_codegen::ir::{
    condcodes::IntCC, types, AbiParam, Block, InstBuilder, MemFlags, Value,
};
use cranelift_module::DataDescription;
#[path = "jit_expr.rs"]
mod jit_expr;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Switch};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};

pub struct Native {
    module: Option<JITModule>,
    entry: unsafe extern "C" fn(*mut u8) -> i64,
    pub code_bytes: usize,
    pub clif: String,
}
impl Native {
    pub fn compile(p: &Program) -> Result<Self, String> {
        if !cfg!(target_pointer_width = "64") || !cfg!(target_endian = "little") {
            return Err(
                "this native optimization prototype requires a 64-bit little-endian host".into(),
            );
        }
        let mut builder = JITBuilder::with_flags(
            &[("opt_level", "speed")],
            cranelift_module::default_libcall_names(),
        )
        .map_err(|e| e.to_string())?;
        builder.symbol("caprice_step", jit_step as *const u8);
        builder.symbol("caprice_backtrack", jit_backtrack as *const u8);
        let primitives = [
            ("save_start", crate::jit_save_start as *const u8),
            ("save_end", crate::jit_save_end as *const u8),
            ("backref", crate::jit_backref as *const u8),
            ("fast_expression", crate::jit_fast_expression as *const u8),
            ("arena_reset", crate::jit_arena_reset as *const u8),
            ("arena_alloc", crate::jit_arena_alloc as *const u8),
            ("set_span", crate::jit_set_span as *const u8),
            ("iter_cap", crate::jit_iter_cap as *const u8),
        ];
        for (name, address) in primitives {
            builder.symbol(name, address);
        }
        let mut module = JITModule::new(builder);
        let ptrty = module.target_config().pointer_type();
        let mut step_sig = module.make_signature();
        step_sig.params.push(AbiParam::new(ptrty));
        step_sig.params.push(AbiParam::new(types::I64));
        step_sig.returns.push(AbiParam::new(types::I64));
        let step_id = module
            .declare_function("caprice_step", Linkage::Import, &step_sig)
            .map_err(|e| e.to_string())?;
        let primitive_ids = primitives
            .iter()
            .map(|(name, _)| {
                module
                    .declare_function(name, Linkage::Import, &step_sig)
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptrty));
        sig.returns.push(AbiParam::new(types::I64));
        let back_id = module
            .declare_function("caprice_backtrack", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;
        let id = module
            .declare_function("match_residual", Linkage::Local, &sig)
            .map_err(|e| e.to_string())?;
        let mut ctx = module.make_context();
        ctx.func.signature = sig;
        let pools = pool_globals(&mut module, &mut ctx.func, p)?;
        let mut fbctx = FunctionBuilderContext::new();
        {
            let step = module.declare_func_in_func(step_id, &mut ctx.func);
            let back = module.declare_func_in_func(back_id, &mut ctx.func);
            let helpers: Vec<_> = primitive_ids
                .iter()
                .map(|id| module.declare_func_in_func(*id, &mut ctx.func))
                .collect();
            let mut b = FunctionBuilder::new(&mut ctx.func, &mut fbctx);
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            let blocks: Vec<_> = p.code.iter().map(|_| b.create_block()).collect();
            let fail = b.create_block();
            let yes = b.create_block();
            let no = b.create_block();
            let budget = b.create_block();
            let dispatch = b.create_block();
            b.append_block_param(dispatch, types::I64);
            b.switch_to_block(entry);
            let ptr = b.block_params(entry)[0];
            b.ins().jump(blocks[0], &[]);
            for (pc, inst) in p.code.iter().enumerate() {
                b.switch_to_block(blocks[pc]);
                let next = match inst {
                    Inst::Accept => yes,
                    Inst::Jump(n) | Inst::Split(n, _) => blocks[*n],
                    _ => blocks.get(pc + 1).copied().unwrap_or(no),
                };
                if !p.backtracking && inline_capture(&mut b, ptr, inst, next, budget) {
                    continue;
                }
                if (!p.backtracking || !matches!(inst, Inst::CaptureRun(..)))
                    && inline(&mut b, ptr, inst, next, fail, budget, &blocks)
                {
                    continue;
                }
                if let Some(residual) = p.residual[pc].as_ref() {
                    jit_expr::inline(
                        &mut b,
                        ptr,
                        residual,
                        pools[pc],
                        (helpers[4], helpers[5]),
                        (next, fail, budget),
                    );
                    continue;
                }
                let primitive = match inst {
                    Inst::SaveStart(id) => Some((helpers[0], *id)),
                    Inst::SaveEnd(id) => Some((helpers[1], *id)),
                    Inst::Backref(id) => Some((helpers[2], *id)),
                    Inst::SetSpan { .. } => Some((helpers[6], pc)),
                    Inst::IterCap { .. } => Some((helpers[7], pc)),
                    Inst::Guard(_) | Inst::Emit(_) if p.fast[pc].is_some() => {
                        Some((helpers[3], pc))
                    }
                    _ => None,
                };
                let (function, index) = if let Some((f, i)) = primitive {
                    charge(&mut b, ptr, budget);
                    (f, i)
                } else {
                    (step, pc)
                };
                let idx = b.ins().iconst(types::I64, index as i64);
                let call = b.ins().call(function, &[ptr, idx]);
                let result = b.inst_results(call)[0];
                let check = b.create_block();
                let exhausted = b.ins().icmp_imm(IntCC::Equal, result, 2);
                b.ins().brif(exhausted, budget, &[], check, &[]);
                b.switch_to_block(check);
                let ok = b.ins().icmp_imm(IntCC::Equal, result, 0);
                b.ins().brif(ok, next, &[], fail, &[]);
            }
            b.switch_to_block(fail);
            let call = b.ins().call(back, &[ptr]);
            let pc = b.inst_results(call)[0];
            let exhausted = b.ins().icmp_imm(IntCC::Equal, pc, -2);
            b.ins().brif(exhausted, budget, &[], dispatch, &[pc.into()]);
            b.switch_to_block(dispatch);
            let pc = b.block_params(dispatch)[0];
            let mut sw = Switch::new();
            for (i, block) in blocks.iter().enumerate() {
                sw.set_entry(i as u128, *block)
            }
            sw.emit(&mut b, pc, no);
            for (block, status) in [(yes, 1), (no, 0), (budget, 2)] {
                b.switch_to_block(block);
                let v = b.ins().iconst(types::I64, status);
                b.ins().return_(&[v]);
            }
            b.seal_all_blocks();
            b.finalize();
        }
        let clif = ctx.func.display().to_string();
        module
            .define_function(id, &mut ctx)
            .map_err(|e| e.to_string())?;
        let code_bytes = ctx
            .compiled_code()
            .map(|c| c.code_buffer().len())
            .unwrap_or(0);
        module.clear_context(&mut ctx);
        module.finalize_definitions().map_err(|e| e.to_string())?;
        let code = module.get_finalized_function(id);
        // SAFETY: the declared ABI is exactly extern C fn(*mut u8)->i64. The
        // module owns the executable pages until Drop, and entry never escapes.
        let entry =
            unsafe { std::mem::transmute::<*const u8, unsafe extern "C" fn(*mut u8) -> i64>(code) };
        Ok(Self {
            module: Some(module),
            entry,
            code_bytes,
            clif,
        })
    }
    pub(crate) fn run(&self, r: &mut Runtime<'_>) -> i64 {
        unsafe { (self.entry)(r as *mut Runtime<'_> as *mut u8) }
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        if let Some(m) = self.module.take() {
            unsafe { m.free_memory() }
        }
    }
}

// All subject loads are dominated by explicit length checks. No assumptions
// about Rust slice or Option layout enter the generated code. The Runtime is
// repr(C); offsets come from the exact Rust build hosting this JIT.
const POS: i32 = std::mem::offset_of!(Runtime<'static>, pos) as i32;
const LEN: i32 = std::mem::offset_of!(Runtime<'static>, input_len) as i32;
const INPUT: i32 = std::mem::offset_of!(Runtime<'static>, input_ptr) as i32;
const STEPS: i32 = std::mem::offset_of!(Runtime<'static>, steps) as i32;
const LIMIT: i32 = std::mem::offset_of!(Runtime<'static>, step_limit) as i32;
const CAPS: i32 = std::mem::offset_of!(Runtime<'static>, capture_ptr) as i32;
fn pool_globals(
    module: &mut JITModule,
    func: &mut cranelift_codegen::ir::Function,
    p: &Program,
) -> Result<Vec<Option<cranelift_codegen::ir::GlobalValue>>, String> {
    let mut out = Vec::with_capacity(p.code.len());
    for slot in &p.residual {
        let Some(residual) = slot else {
            out.push(None);
            continue;
        };
        if residual.pool.is_empty() {
            out.push(None);
            continue;
        }
        let id = module
            .declare_anonymous_data(false, false)
            .map_err(|e| e.to_string())?;
        let mut data = DataDescription::new();
        data.define(residual.pool.clone().into_boxed_slice());
        module.define_data(id, &data).map_err(|e| e.to_string())?;
        out.push(Some(module.declare_data_in_func(id, func)));
    }
    Ok(out)
}
fn inline_capture(
    b: &mut FunctionBuilder,
    ptr: Value,
    inst: &Inst,
    next: Block,
    budget: Block,
) -> bool {
    let (id, end) = match inst {
        Inst::SaveStart(id) => (*id, false),
        Inst::SaveEnd(id) => (*id, true),
        _ => return false,
    };
    charge(b, ptr, budget);
    let pos = load(b, ptr, POS);
    let base = load(b, ptr, CAPS);
    let stride = std::mem::size_of::<crate::CaptureSlot>();
    let at = (id * stride) as i32;
    if end {
        b.ins().store(
            MemFlags::new(),
            pos,
            base,
            at + std::mem::offset_of!(crate::CaptureSlot, end) as i32,
        );
    } else {
        b.ins().store(
            MemFlags::new(),
            pos,
            base,
            at + std::mem::offset_of!(crate::CaptureSlot, start) as i32,
        );
        let open = b.ins().iconst(types::I64, -1);
        b.ins().store(
            MemFlags::new(),
            open,
            base,
            at + std::mem::offset_of!(crate::CaptureSlot, end) as i32,
        );
    }
    b.ins().jump(next, &[]);
    true
}
fn load(b: &mut FunctionBuilder, ptr: Value, offset: i32) -> Value {
    b.ins().load(types::I64, MemFlags::new(), ptr, offset)
}
fn class_test(b: &mut FunctionBuilder, c: &crate::pattern::Class, byte: Value) -> Value {
    // A negated small set is cheaper than several disjoint positive ranges.
    if c.0.iter().map(|w| (!w).count_ones()).sum::<u32>() <= 4 {
        let mut ok = b.ins().iconst(types::I8, 1);
        for v in 0u16..256 {
            if !c.has(v as u8) {
                let different = b.ins().icmp_imm(IntCC::NotEqual, byte, v as i64);
                ok = b.ins().band(ok, different);
            }
        }
        return ok;
    }
    let mut ranges = vec![];
    let mut i = 0u16;
    while i < 256 {
        if c.has(i as u8) {
            let lo = i;
            while i + 1 < 256 && c.has((i + 1) as u8) {
                i += 1;
            }
            ranges.push((lo, i));
        }
        i += 1;
    }
    if ranges.len() <= 4 {
        let mut any = b.ins().iconst(types::I8, 0);
        for (lo, hi) in ranges {
            let shifted = b.ins().iadd_imm(byte, -(lo as i64));
            let inside =
                b.ins()
                    .icmp_imm(IntCC::UnsignedLessThanOrEqual, shifted, (hi - lo) as i64);
            any = b.ins().bor(any, inside);
        }
        any
    } else {
        let wide = b.ins().uextend(types::I64, byte);
        let index = b.ins().ushr_imm(wide, 6);
        let mut word = b.ins().iconst(types::I64, c.0[0] as i64);
        for j in 1..4 {
            let is = b.ins().icmp_imm(IntCC::Equal, index, j as i64);
            let w = b.ins().iconst(types::I64, c.0[j] as i64);
            word = b.ins().select(is, w, word);
        }
        let bit = b.ins().ushr(word, wide);
        let bit = b.ins().band_imm(bit, 1);
        b.ins().icmp_imm(IntCC::NotEqual, bit, 0)
    }
}
fn inline(
    b: &mut FunctionBuilder,
    ptr: Value,
    inst: &Inst,
    next: Block,
    fail: Block,
    budget: Block,
    blocks: &[Block],
) -> bool {
    if !matches!(inst,Inst::Lit(s) if s.len()<=128)
        && !matches!(
            inst,
            Inst::Class(_)
                | Inst::AtomicRun(..)
                | Inst::CaptureRun(..)
                | Inst::Start
                | Inst::End
                | Inst::Accept
                | Inst::Jump(_)
                | Inst::Switch(..)
                | Inst::Fail
        )
    {
        return false;
    }
    charge(b, ptr, budget);
    let pos = load(b, ptr, POS);
    let len = load(b, ptr, LEN);
    match inst {
        Inst::Switch(c, left, right) => {
            let enough = b.ins().icmp(IntCC::UnsignedLessThan, pos, len);
            let compare = b.create_block();
            b.ins().brif(enough, compare, &[], blocks[*right], &[]);
            b.switch_to_block(compare);
            let base = load(b, ptr, INPUT);
            let address = b.ins().iadd(base, pos);
            let byte = b.ins().load(types::I8, MemFlags::new(), address, 0);
            let left_match = class_test(b, c, byte);
            b.ins()
                .brif(left_match, blocks[*left], &[], blocks[*right], &[]);
        }
        Inst::Jump(_) => {
            b.ins().jump(next, &[]);
        }
        Inst::Fail => {
            b.ins().jump(fail, &[]);
        }
        Inst::Start => {
            let ok = b.ins().icmp_imm(IntCC::Equal, pos, 0);
            b.ins().brif(ok, next, &[], fail, &[]);
        }
        Inst::End | Inst::Accept => {
            let ok = b.ins().icmp(IntCC::Equal, pos, len);
            b.ins().brif(ok, next, &[], fail, &[]);
        }
        Inst::Lit(s) => {
            let remaining = b.ins().isub(len, pos);
            let enough =
                b.ins()
                    .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, remaining, s.len() as i64);
            let compare = b.create_block();
            b.ins().brif(enough, compare, &[], fail, &[]);
            b.switch_to_block(compare);
            let base = load(b, ptr, INPUT);
            let address = b.ins().iadd(base, pos);
            let mut offset = 0;
            let mut equal = b.ins().iconst(types::I8, 1);
            for (size, ty) in [
                (8, types::I64),
                (4, types::I32),
                (2, types::I16),
                (1, types::I8),
            ] {
                while offset + size <= s.len() {
                    let mut bytes = [0u8; 8];
                    bytes[..size].copy_from_slice(&s[offset..offset + size]);
                    let value = u64::from_le_bytes(bytes);
                    let got = b.ins().load(ty, MemFlags::new(), address, offset as i32);
                    let ok = b.ins().icmp_imm(IntCC::Equal, got, value as i64);
                    equal = b.ins().band(equal, ok);
                    offset += size;
                }
            }
            let advance = b.create_block();
            b.ins().brif(equal, advance, &[], fail, &[]);
            b.switch_to_block(advance);
            let end = b.ins().iadd_imm(pos, s.len() as i64);
            b.ins().store(MemFlags::new(), end, ptr, POS);
            b.ins().jump(next, &[]);
        }
        Inst::Class(c) => {
            let enough = b.ins().icmp(IntCC::UnsignedLessThan, pos, len);
            let compare = b.create_block();
            let advance = b.create_block();
            b.ins().brif(enough, compare, &[], fail, &[]);
            b.switch_to_block(compare);
            let base = load(b, ptr, INPUT);
            let address = b.ins().iadd(base, pos);
            let byte = b.ins().load(types::I8, MemFlags::new(), address, 0);
            let ok = class_test(b, c, byte);
            b.ins().brif(ok, advance, &[], fail, &[]);
            b.switch_to_block(advance);
            let end = b.ins().iadd_imm(pos, 1);
            b.ins().store(MemFlags::new(), end, ptr, POS);
            b.ins().jump(next, &[]);
        }
        Inst::AtomicRun(c, min, max) | Inst::CaptureRun(_, c, min, max) => {
            let base = load(b, ptr, INPUT);
            let remaining = b.ins().isub(len, pos);
            let count = if let Some(max) = max {
                let m = b.ins().iconst(types::I64, *max as i64);
                let less = b.ins().icmp(IntCC::UnsignedLessThan, remaining, m);
                b.ins().select(less, remaining, m)
            } else {
                remaining
            };
            let bound = b.ins().iadd(pos, count);
            let scan = b.create_block();
            let compare = b.create_block();
            let advance = b.create_block();
            let done = b.create_block();
            let commit = b.create_block();
            b.append_block_param(scan, types::I64);
            b.append_block_param(done, types::I64);
            b.ins().jump(scan, &[pos.into()]);
            b.switch_to_block(scan);
            let cursor = b.block_params(scan)[0];
            let more = b.ins().icmp(IntCC::UnsignedLessThan, cursor, bound);
            b.ins().brif(more, compare, &[], done, &[cursor.into()]);
            b.switch_to_block(compare);
            let address = b.ins().iadd(base, cursor);
            let byte = b.ins().load(types::I8, MemFlags::new(), address, 0);
            let ok = class_test(b, c, byte);
            b.ins().brif(ok, advance, &[], done, &[cursor.into()]);
            b.switch_to_block(advance);
            let cursor = b.ins().iadd_imm(cursor, 1);
            b.ins().jump(scan, &[cursor.into()]);
            b.switch_to_block(done);
            let end = b.block_params(done)[0];
            let size = b.ins().isub(end, pos);
            let enough = b
                .ins()
                .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, size, *min as i64);
            b.ins().brif(enough, commit, &[], fail, &[]);
            b.switch_to_block(commit);
            if let Inst::CaptureRun(id, ..) = inst {
                let slots = load(b, ptr, CAPS);
                let at = (*id * std::mem::size_of::<crate::CaptureSlot>()) as i32;
                b.ins().store(
                    MemFlags::new(),
                    pos,
                    slots,
                    at + std::mem::offset_of!(crate::CaptureSlot, start) as i32,
                );
                b.ins().store(
                    MemFlags::new(),
                    end,
                    slots,
                    at + std::mem::offset_of!(crate::CaptureSlot, end) as i32,
                );
            }
            b.ins().store(MemFlags::new(), end, ptr, POS);
            b.ins().jump(next, &[]);
        }
        _ => unreachable!(),
    }
    true
}
fn charge(b: &mut FunctionBuilder, ptr: Value, budget: Block) {
    let count = load(b, ptr, STEPS);
    let limit = load(b, ptr, LIMIT);
    let exhausted = b
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, count, limit);
    let execute = b.create_block();
    b.ins().brif(exhausted, budget, &[], execute, &[]);
    b.switch_to_block(execute);
    let count = b.ins().iadd_imm(count, 1);
    b.ins().store(MemFlags::new(), count, ptr, STEPS);
}
