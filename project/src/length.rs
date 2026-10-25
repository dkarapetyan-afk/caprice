//! Conservative affine full-match lengths: |subject| = fixed + scale * |capture|.
use crate::{
    fast_expr::FastExpr,
    pattern::{Ast, Class, Inst},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
struct Affine {
    fixed: usize,
    scale: usize,
}
impl Affine {
    fn add(self, rhs: Self) -> Option<Self> {
        Some(Self {
            fixed: self.fixed.checked_add(rhs.fixed)?,
            scale: self.scale.checked_add(rhs.scale)?,
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct GuardBounds {
    min: usize,
    max: Option<usize>,
    constant_bytes: usize,
}
impl GuardBounds {
    fn intersect(self, rhs: Self) -> Self {
        Self {
            min: self.min.max(rhs.min),
            max: match (self.max, rhs.max) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            },
            constant_bytes: self.constant_bytes.max(rhs.constant_bytes),
        }
    }
}
#[derive(Clone, Debug)]
pub struct SizedRun {
    pub class: Class,
    pub min: usize,
    pub max: Option<usize>,
    fixed: usize,
    scale: usize,
    guard: Option<GuardBounds>,
}
impl SizedRun {
    pub fn count(&self, subject_len: usize, expression_limit: usize) -> Option<usize> {
        let remaining = subject_len.checked_sub(self.fixed)?;
        if !remaining.is_multiple_of(self.scale) {
            return None;
        }
        let count = remaining / self.scale;
        if count < self.min || self.max.is_some_and(|max| count > max) {
            return None;
        }
        if let Some(g) = self.guard {
            // Do not turn a mandatory expression-limit error into rejection.
            // At a smaller limit the original guard still executes downstream.
            if expression_limit >= count.max(g.constant_bytes)
                && (count < g.min || g.max.is_some_and(|m| count > m))
            {
                return None;
            }
        }
        Some(count)
    }
}
#[derive(Default)]
struct Analysis {
    captures: BTreeMap<usize, Affine>,
    variable: Option<usize>,
    guard: Option<GuardBounds>,
}
fn single(mut a: &Ast) -> &Ast {
    while let Ast::Seq(v) = a {
        if v.len() != 1 {
            break;
        }
        a = &v[0];
    }
    a
}
impl Analysis {
    fn visit(&mut self, a: &Ast) -> Option<Affine> {
        let constant = |n| Some(Affine { fixed: n, scale: 0 });
        match a {
            Ast::Empty | Ast::Start | Ast::End => constant(0),
            Ast::Lit(s) => constant(s.len()),
            Ast::Class(_) => constant(1),
            Ast::Seq(v) => {
                let mut sum = Affine { fixed: 0, scale: 0 };
                for a in v {
                    sum = sum.add(self.visit(a)?)?;
                }
                Some(sum)
            }
            Ast::Repeat(a, min, Some(max), _) if min == max => {
                // Captures in repeated bodies need an independent multiplicity analysis.
                if !matches!(single(a), Ast::Class(_) | Ast::Lit(_)) {
                    return None;
                }
                let inner = self.visit(a)?;
                constant(inner.fixed.checked_mul(*min)?)
            }
            Ast::Capture(id, body) => {
                let size = if let Ast::Repeat(c, min, max, _) = single(body) {
                    if matches!(single(c), Ast::Class(_)) && *max != Some(*min) {
                        if self.variable.is_some() {
                            return None;
                        }
                        self.variable = Some(*id);
                        Affine { fixed: 0, scale: 1 }
                    } else {
                        self.visit(body)?
                    }
                } else {
                    self.visit(body)?
                };
                self.captures.insert(*id, size);
                Some(size)
            }
            Ast::Backref(id) => self.captures.get(id).copied(),
            Ast::Emit(e) => {
                let f = FastExpr::compile(e)?;
                let id = f.emitted_capture()?;
                self.captures.get(&id).copied()
            }
            Ast::Guard(e) => {
                let f = FastExpr::compile(e)?;
                let variable = self.variable?;
                self.captures.get(&variable)?;
                let (min, max, constant_bytes) = f.length_bounds(variable)?;
                let next = GuardBounds {
                    min,
                    max,
                    constant_bytes,
                };
                self.guard = Some(self.guard.map_or(next, |g| g.intersect(next)));
                constant(0)
            }
            _ => None,
        }
    }
}
pub fn lower(a: &Ast, code: &mut [Inst]) {
    let mut analysis = Analysis::default();
    let Some(size) = analysis.visit(a) else {
        return;
    };
    let Some(id) = analysis.variable else {
        return;
    };
    if size.scale == 0 {
        return;
    }
    for pc in 0..code.len().saturating_sub(2) {
        if !matches!(code[pc],Inst::SaveStart(i) if i==id)
            || !matches!(code[pc+2],Inst::SaveEnd(i) if i==id)
        {
            continue;
        }
        let (class, min, max, ambiguous) = match &code[pc + 1] {
            Inst::Run(c, n, m, _) => (c.clone(), *n, *m, true),
            Inst::AtomicRun(c, n, m) => (c.clone(), *n, *m, false),
            _ => continue,
        };
        if (size.scale > 1 && ambiguous) || analysis.guard.is_some() {
            code[pc + 1] = Inst::SizedRun(Box::new(SizedRun {
                class,
                min,
                max,
                fixed: size.fixed,
                scale: size.scale,
                guard: analysis.guard,
            }));
        }
    }
}
