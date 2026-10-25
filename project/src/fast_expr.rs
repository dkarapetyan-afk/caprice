//! Conservative expression lowering. Unsupported forms retain the strict evaluator.
use crate::expr::{EvalError, Expr, Value};

#[derive(Clone, Copy, Debug)]
pub enum Transform {
    Identity,
    Lower,
    Upper,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{self, Params};
    use std::collections::BTreeMap;
    #[test]
    fn lowered_expressions_preserve_strict_errors_and_byte_values() {
        let names = BTreeMap::from([("x".to_owned(), 0)]);
        let expressions = [
            "(le (len (cap x)) 1)",
            "(lt (len (cap x)) 2)",
            "(eq (len (cap x)) 1)",
            r#"(eq (lower (cap x)) "a")"#,
            r#"(eq (upper (cap x)) "AB")"#,
            r#"(eq (cap x) "")"#,
            "(upper (cap x))",
            "(lower (cap x))",
            "(cap x)",
            r#"(call (lambda (v Str) (eq (lower v) "a")) (cap x))"#,
        ];
        let mut cases = 0;
        for source in expressions {
            let e = expr::parse(source, &names).unwrap();
            let fast = FastExpr::compile(&e).unwrap();
            for byte in 0u8..=255 {
                let input = [byte, b'a', b'A', byte];
                for slot in [
                    None,
                    Some((0, usize::MAX)),
                    Some((0, 0)),
                    Some((0, 1)),
                    Some((0, 2)),
                ] {
                    let caps = vec![slot];
                    for limit in 0..=3 {
                        let expected = expr::eval(
                            &e,
                            &input,
                            &caps,
                            &Params::new(),
                            &mut BTreeMap::new(),
                            limit,
                        )
                        .map(|v| match v {
                            Value::Bool(ok) => ok.then_some(0),
                            Value::Str(s) => input.starts_with(&s).then_some(s.len()),
                            _ => unreachable!(),
                        });
                        assert_eq!(
                            fast.test(&input, &caps, 0, limit),
                            expected,
                            "{source} {byte} {slot:?} {limit}"
                        );
                        cases += 1;
                    }
                }
            }
        }
        println!("lowered_expression_comparisons={cases}");
        let unused = expr::parse("(call (lambda (v Str) true) (cap x))", &names).unwrap();
        assert!(FastExpr::compile(&unused).is_none());
    }
}
impl Transform {
    fn byte(self, b: u8) -> u8 {
        match self {
            Self::Identity => b,
            Self::Lower => b.to_ascii_lowercase(),
            Self::Upper => b.to_ascii_uppercase(),
        }
    }
}
#[derive(Debug)]
pub enum FastExpr {
    Length {
        id: usize,
        n: usize,
        op: String,
    },
    Equal {
        id: usize,
        transform: Transform,
        value: Vec<u8>,
    },
    Emit {
        id: usize,
        transform: Transform,
    },
}
impl FastExpr {
    pub(crate) fn emitted_capture(&self) -> Option<usize> {
        if let Self::Emit { id, .. } = self {
            Some(*id)
        } else {
            None
        }
    }
    pub(crate) fn length_bounds(&self, capture: usize) -> Option<(usize, Option<usize>, usize)> {
        if self.id() != capture {
            return None;
        }
        match self {
            Self::Equal { value, .. } => Some((value.len(), Some(value.len()), value.len())),
            Self::Length { n, op, .. } => Some(match op.as_str() {
                "le" => (0, Some(*n), 0),
                "lt" if *n == 0 => (1, Some(0), 0),
                "lt" => (0, Some(n - 1), 0),
                _ => (*n, Some(*n), 0),
            }),
            _ => None,
        }
    }
    fn id(&self) -> usize {
        match self {
            Self::Length { id, .. } | Self::Equal { id, .. } | Self::Emit { id, .. } => *id,
        }
    }
    pub fn compile(e: &Expr) -> Option<Self> {
        if let Expr::Apply { id, body, arg, .. } = e {
            if let Expr::Cap(cap) = arg.as_ref() {
                fn subst(e: &Expr, id: usize, cap: usize) -> Expr {
                    match e {
                        Expr::Var(v) if *v == id => Expr::Cap(cap),
                        Expr::Op(op, args) => {
                            Expr::Op(op.clone(), args.iter().map(|e| subst(e, id, cap)).collect())
                        }
                        _ => e.clone(),
                    }
                }
                let f = Self::compile(&subst(body, *id, *cap))?;
                // The lowered expression reads this same capture first, retaining
                // strict evaluation of the dynamic argument (including limits).
                return (f.id() == *cap).then_some(f);
            }
        }
        fn view(e: &Expr) -> Option<(usize, Transform)> {
            match e {
                Expr::Cap(id) => Some((*id, Transform::Identity)),
                Expr::Op(op, a) if a.len() == 1 => match (&a[0], op.as_str()) {
                    (Expr::Cap(id), "lower") => Some((*id, Transform::Lower)),
                    (Expr::Cap(id), "upper") => Some((*id, Transform::Upper)),
                    _ => None,
                },
                _ => None,
            }
        }
        if let Some((id, transform)) = view(e) {
            return Some(Self::Emit { id, transform });
        }
        if let Expr::Op(op, args) = e {
            if let [Expr::Op(len, a), Expr::Val(Value::Nat(n))] = args.as_slice() {
                if len == "len" && matches!(op.as_str(), "le" | "lt" | "eq") {
                    if let [Expr::Cap(id)] = a.as_slice() {
                        return Some(Self::Length {
                            id: *id,
                            n: *n,
                            op: op.clone(),
                        });
                    }
                }
            }
            if op == "eq" {
                if let [lhs, Expr::Val(Value::Str(value))] = args.as_slice() {
                    let (id, transform) = view(lhs)?;
                    return Some(Self::Equal {
                        id,
                        transform,
                        value: value.clone(),
                    });
                }
            }
        }
        None
    }
    /// Return a successful consumed-byte count (zero for a guard), or failure.
    #[cfg(test)]
    pub fn test(
        &self,
        input: &[u8],
        caps: &crate::expr::Captures,
        pos: usize,
        limit: usize,
    ) -> Result<Option<usize>, EvalError> {
        self.test_span(input, caps[self.id()], pos, limit)
    }
    pub(crate) fn test_slots(
        &self,
        input: &[u8],
        caps: &[crate::CaptureSlot],
        pos: usize,
        limit: usize,
    ) -> Result<Option<usize>, EvalError> {
        self.test_span(input, caps[self.id()].span(), pos, limit)
    }
    fn test_span(
        &self,
        input: &[u8],
        span: Option<(usize, usize)>,
        pos: usize,
        limit: usize,
    ) -> Result<Option<usize>, EvalError> {
        let (a, z) = span
            .filter(|(a, z)| a <= z && *z <= input.len())
            .ok_or(EvalError::Unset)?;
        if z - a > limit {
            return Err(EvalError::Limit);
        }
        let s = &input[a..z];
        match self {
            Self::Length { n, op, .. } => Ok(match op.as_str() {
                "le" => s.len() <= *n,
                "lt" => s.len() < *n,
                _ => s.len() == *n,
            }
            .then_some(0)),
            Self::Equal {
                transform, value, ..
            } => {
                if value.len() > limit {
                    return Err(EvalError::Limit);
                }
                Ok((s.len() == value.len()
                    && s.iter().zip(value).all(|(a, b)| transform.byte(*a) == *b))
                .then_some(0))
            }
            Self::Emit { transform, .. } => Ok((input.len() - pos >= s.len()
                && s.iter()
                    .zip(&input[pos..])
                    .all(|(a, b)| transform.byte(*a) == *b))
            .then_some(s.len())),
        }
    }
}
