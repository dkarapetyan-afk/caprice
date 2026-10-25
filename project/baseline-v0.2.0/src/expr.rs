//! Total, first-order, fully applied typed lambdas over byte strings.
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    Str,
    Nat,
    Bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Str(Vec<u8>),
    Nat(usize),
    Bool(bool),
}
impl Value {
    pub fn ty(&self) -> Ty {
        match self {
            Self::Str(_) => Ty::Str,
            Self::Nat(_) => Ty::Nat,
            Self::Bool(_) => Ty::Bool,
        }
    }
}
#[derive(Clone, Debug)]
pub enum Expr {
    Val(Value),
    Cap(usize),
    Param(String),
    Var(usize),
    Op(String, Vec<Expr>),
    Apply {
        id: usize,
        ty: Ty,
        body: Box<Expr>,
        arg: Box<Expr>,
    },
}
pub type Params = BTreeMap<String, Vec<u8>>;
pub type Captures = Vec<Option<(usize, usize)>>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    Unset,
    Limit,
}

#[derive(Debug, PartialEq)]
enum Token {
    L,
    R,
    Atom(String),
    Str(Vec<u8>),
}
fn tokenize(s: &str) -> Result<Vec<Token>, String> {
    let b = s.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        match b[i] {
            b'(' => {
                out.push(Token::L);
                i += 1
            }
            b')' => {
                out.push(Token::R);
                i += 1
            }
            c if c.is_ascii_whitespace() => i += 1,
            b'"' => {
                let start = i;
                i += 1;
                let mut escaped = false;
                while i < b.len() {
                    let c = b[i];
                    i += 1;
                    if escaped {
                        escaped = false
                    } else if c == b'\\' {
                        escaped = true
                    } else if c == b'"' {
                        break;
                    }
                }
                let value: String = serde_json::from_str(&s[start..i])
                    .map_err(|e| format!("string literal: {e}"))?;
                out.push(Token::Str(value.into_bytes()));
            }
            _ => {
                let start = i;
                while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'(' && b[i] != b')' {
                    i += 1
                }
                out.push(Token::Atom(s[start..i].into()));
            }
        }
    }
    Ok(out)
}
struct Parser<'a> {
    t: Vec<Token>,
    i: usize,
    names: &'a BTreeMap<String, usize>,
    vars: Vec<(String, usize, Ty)>,
    next: usize,
    depth: usize,
}
impl Parser<'_> {
    fn atom(&mut self) -> Result<String, String> {
        if let Some(Token::Atom(s)) = self.t.get(self.i) {
            self.i += 1;
            Ok(s.clone())
        } else {
            Err("expected identifier".into())
        }
    }
    fn token(&mut self, t: Token) -> Result<(), String> {
        if self.t.get(self.i) == Some(&t) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected {t:?}"))
        }
    }
    fn expr(&mut self) -> Result<Expr, String> {
        self.depth += 1;
        if self.depth > 128 {
            return Err("expression nesting exceeds 128".into());
        }
        let e = match self.t.get(self.i) {
            Some(Token::Str(s)) => {
                let e = Expr::Val(Value::Str(s.clone()));
                self.i += 1;
                e
            }
            Some(Token::Atom(a)) => {
                let a = a.clone();
                self.i += 1;
                if a == "true" || a == "false" {
                    Expr::Val(Value::Bool(a == "true"))
                } else if let Ok(n) = a.parse::<usize>() {
                    Expr::Val(Value::Nat(n))
                } else {
                    Expr::Var(
                        self.vars
                            .iter()
                            .rev()
                            .find(|v| v.0 == a)
                            .ok_or(format!("unbound variable {a}"))?
                            .1,
                    )
                }
            }
            Some(Token::L) => {
                self.i += 1;
                let op = self.atom()?;
                let e = if op == "cap" {
                    let n = self.atom()?;
                    Expr::Cap(*self.names.get(&n).ok_or(format!("unknown capture {n}"))?)
                } else if op == "param" {
                    Expr::Param(self.atom()?)
                } else if op == "call" {
                    self.token(Token::L)?;
                    if self.atom()? != "lambda" {
                        return Err("call requires a syntactic lambda".into());
                    }
                    self.token(Token::L)?;
                    let n = self.atom()?;
                    let ty = match self.atom()?.as_str() {
                        "Str" => Ty::Str,
                        "Nat" => Ty::Nat,
                        "Bool" => Ty::Bool,
                        _ => return Err("expected Str, Nat, or Bool".into()),
                    };
                    self.token(Token::R)?;
                    let id = self.next;
                    self.next += 1;
                    self.vars.push((n, id, ty.clone()));
                    let body = self.expr()?;
                    self.vars.pop();
                    self.token(Token::R)?;
                    let arg = self.expr()?;
                    Expr::Apply {
                        id,
                        ty,
                        body: Box::new(body),
                        arg: Box::new(arg),
                    }
                } else {
                    let mut args = Vec::new();
                    while self.t.get(self.i) != Some(&Token::R) {
                        if self.i == self.t.len() {
                            return Err("unclosed expression".into());
                        }
                        args.push(self.expr()?)
                    }
                    Expr::Op(op, args)
                };
                self.token(Token::R)?;
                e
            }
            _ => return Err("expected expression".into()),
        };
        self.depth -= 1;
        Ok(e)
    }
}
pub fn parse(s: &str, names: &BTreeMap<String, usize>) -> Result<Expr, String> {
    let mut p = Parser {
        t: tokenize(s)?,
        i: 0,
        names,
        vars: vec![],
        next: 0,
        depth: 0,
    };
    let e = p.expr()?;
    if p.i != p.t.len() {
        return Err("trailing expression tokens".into());
    }
    type_of(&e, &mut BTreeMap::new())?;
    Ok(e)
}
pub fn type_of(e: &Expr, env: &mut BTreeMap<usize, Ty>) -> Result<Ty, String> {
    match e {
        Expr::Val(v) => Ok(v.ty()),
        Expr::Cap(_) | Expr::Param(_) => Ok(Ty::Str),
        Expr::Var(id) => env.get(id).cloned().ok_or("unbound variable id".into()),
        Expr::Apply { id, ty, body, arg } => {
            if type_of(arg, env)? != *ty {
                return Err("lambda argument type mismatch".into());
            }
            let old = env.insert(*id, ty.clone());
            let result = type_of(body, env);
            if let Some(v) = old {
                env.insert(*id, v);
            } else {
                env.remove(id);
            }
            result
        }
        Expr::Op(op, args) => {
            let ts = args
                .iter()
                .map(|a| type_of(a, env))
                .collect::<Result<Vec<_>, _>>()?;
            let str2 = vec![Ty::Str, Ty::Str];
            let nat2 = vec![Ty::Nat, Ty::Nat];
            let bool2 = vec![Ty::Bool, Ty::Bool];
            match (op.as_str(), ts.as_slice()) {
                ("lower" | "upper" | "trim", [Ty::Str]) => Ok(Ty::Str),
                ("len", [Ty::Str]) => Ok(Ty::Nat),
                ("not", [Ty::Bool]) => Ok(Ty::Bool),
                ("concat", _) if ts == str2 => Ok(Ty::Str),
                ("contains" | "starts" | "ends", _) if ts == str2 => Ok(Ty::Bool),
                ("slice", [Ty::Str, Ty::Nat, Ty::Nat]) => Ok(Ty::Str),
                ("eq", [a, b]) if a == b => Ok(Ty::Bool),
                ("lt" | "le", _) if ts == nat2 => Ok(Ty::Bool),
                ("and" | "or", _) if ts == bool2 => Ok(Ty::Bool),
                _ => Err(format!("invalid operator or argument types: {op} {ts:?}")),
            }
        }
    }
}
fn operation(op: &str, vs: &[Value], limit: usize) -> Result<Value, EvalError> {
    use Value::*;
    let value = match (op, vs) {
        ("lower", [Str(s)]) => Str(s.to_ascii_lowercase()),
        ("upper", [Str(s)]) => Str(s.to_ascii_uppercase()),
        ("trim", [Str(s)]) => {
            let a = s
                .iter()
                .position(|c| !c.is_ascii_whitespace())
                .unwrap_or(s.len());
            let z = s
                .iter()
                .rposition(|c| !c.is_ascii_whitespace())
                .map(|i| i + 1)
                .unwrap_or(a);
            Str(s[a..z].to_vec())
        }
        ("len", [Str(s)]) => Nat(s.len()),
        ("not", [Bool(b)]) => Bool(!b),
        ("concat", [Str(a), Str(b)]) => {
            if a.len().saturating_add(b.len()) > limit {
                return Err(EvalError::Limit);
            }
            let mut s = a.clone();
            s.extend_from_slice(b);
            Str(s)
        }
        ("contains", [Str(a), Str(b)]) => Bool(b.is_empty() || a.windows(b.len()).any(|w| w == b)),
        ("starts", [Str(a), Str(b)]) => Bool(a.starts_with(b)),
        ("ends", [Str(a), Str(b)]) => Bool(a.ends_with(b)),
        ("slice", [Str(s), Nat(a), Nat(z)]) => {
            let a = (*a).min(s.len());
            let z = (*z).min(s.len()).max(a);
            Str(s[a..z].to_vec())
        }
        ("eq", [a, b]) => Bool(a == b),
        ("lt", [Nat(a), Nat(b)]) => Bool(a < b),
        ("le", [Nat(a), Nat(b)]) => Bool(a <= b),
        ("and", [Bool(a), Bool(b)]) => Bool(*a && *b),
        ("or", [Bool(a), Bool(b)]) => Bool(*a || *b),
        _ => unreachable!("well-typed internal expression"),
    };
    if matches!(&value,Str(s) if s.len()>limit) {
        Err(EvalError::Limit)
    } else {
        Ok(value)
    }
}
pub fn eval(
    e: &Expr,
    input: &[u8],
    caps: &Captures,
    params: &Params,
    vars: &mut BTreeMap<usize, Value>,
    limit: usize,
) -> Result<Value, EvalError> {
    let v = match e {
        Expr::Val(v) => v.clone(),
        Expr::Var(id) => vars.get(id).cloned().ok_or(EvalError::Unset)?,
        Expr::Cap(id) => {
            let (a, z) = caps[*id]
                .filter(|(a, z)| a <= z && *z <= input.len())
                .ok_or(EvalError::Unset)?;
            if z - a > limit {
                return Err(EvalError::Limit);
            }
            Value::Str(input[a..z].to_vec())
        }
        Expr::Param(n) => Value::Str(params.get(n).ok_or(EvalError::Unset)?.clone()),
        Expr::Op(op, args) => {
            let vs = args
                .iter()
                .map(|a| eval(a, input, caps, params, vars, limit))
                .collect::<Result<Vec<_>, _>>()?;
            operation(op, &vs, limit)?
        }
        Expr::Apply { id, body, arg, .. } => {
            let v = eval(arg, input, caps, params, vars, limit)?;
            let old = vars.insert(*id, v);
            let result = eval(body, input, caps, params, vars, limit);
            if let Some(v) = old {
                vars.insert(*id, v);
            } else {
                vars.remove(id);
            }
            result?
        }
    };
    if matches!(&v,Value::Str(s) if s.len()>limit) {
        Err(EvalError::Limit)
    } else {
        Ok(v)
    }
}
pub fn check_params(e: &Expr, params: &Params) -> Result<(), String> {
    match e {
        Expr::Param(n) if !params.contains_key(n) => Err(format!("missing static parameter {n}")),
        Expr::Op(_, args) => {
            for a in args {
                check_params(a, params)?
            }
            Ok(())
        }
        Expr::Apply { body, arg, .. } => {
            check_params(body, params)?;
            check_params(arg, params)
        }
        _ => Ok(()),
    }
}
/// Strict specialization: a dynamic argument is retained even if unused.
pub fn specialize(e: &Expr, params: &Params, env: &mut BTreeMap<usize, Value>) -> Expr {
    match e {
        Expr::Param(n) => Expr::Val(Value::Str(params[n].clone())),
        Expr::Var(id) => env
            .get(id)
            .cloned()
            .map(Expr::Val)
            .unwrap_or_else(|| e.clone()),
        Expr::Apply { id, ty, body, arg } => {
            let arg = specialize(arg, params, env);
            if let Expr::Val(v) = arg {
                let old = env.insert(*id, v);
                let result = specialize(body, params, env);
                if let Some(v) = old {
                    env.insert(*id, v);
                } else {
                    env.remove(id);
                }
                result
            } else {
                Expr::Apply {
                    id: *id,
                    ty: ty.clone(),
                    body: Box::new(specialize(body, params, env)),
                    arg: Box::new(arg),
                }
            }
        }
        Expr::Op(op, args) => {
            let args: Vec<_> = args.iter().map(|a| specialize(a, params, env)).collect();
            let values: Option<Vec<_>> = args
                .iter()
                .map(|a| {
                    if let Expr::Val(v) = a {
                        Some(v.clone())
                    } else {
                        None
                    }
                })
                .collect();
            if let Some(vs) = values {
                if let Ok(v) = operation(op, &vs, 1 << 20) {
                    return Expr::Val(v);
                }
            }
            Expr::Op(op.clone(), args)
        }
        _ => e.clone(),
    }
}
