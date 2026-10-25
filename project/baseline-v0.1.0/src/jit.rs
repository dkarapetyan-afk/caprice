//! Native compilation of the residual VM control-flow graph. Primitive string
//! operations and expression evaluation remain checked Rust runtime calls.
use crate::{jit_backtrack, jit_step, pattern::Inst, Program, Runtime};
use cranelift_codegen::ir::{condcodes::IntCC, types, AbiParam, InstBuilder};
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
        let mut builder = JITBuilder::with_flags(
            &[("opt_level", "speed")],
            cranelift_module::default_libcall_names(),
        )
        .map_err(|e| e.to_string())?;
        builder.symbol("caprice_step", jit_step as *const u8);
        builder.symbol("caprice_backtrack", jit_backtrack as *const u8);
        let mut module = JITModule::new(builder);
        let ptrty = module.target_config().pointer_type();
        let mut step_sig = module.make_signature();
        step_sig.params.push(AbiParam::new(ptrty));
        step_sig.params.push(AbiParam::new(types::I64));
        step_sig.returns.push(AbiParam::new(types::I64));
        let step_id = module
            .declare_function("caprice_step", Linkage::Import, &step_sig)
            .map_err(|e| e.to_string())?;
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
        let mut fbctx = FunctionBuilderContext::new();
        {
            let step = module.declare_func_in_func(step_id, &mut ctx.func);
            let back = module.declare_func_in_func(back_id, &mut ctx.func);
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
                let idx = b.ins().iconst(types::I64, pc as i64);
                let call = b.ins().call(step, &[ptr, idx]);
                let result = b.inst_results(call)[0];
                let check = b.create_block();
                let exhausted = b.ins().icmp_imm(IntCC::Equal, result, 2);
                b.ins().brif(exhausted, budget, &[], check, &[]);
                b.switch_to_block(check);
                let ok = b.ins().icmp_imm(IntCC::Equal, result, 0);
                let next = match inst {
                    Inst::Accept => yes,
                    Inst::Jump(n) | Inst::Split(n, _) => blocks[*n],
                    _ => blocks.get(pc + 1).copied().unwrap_or(no),
                };
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
