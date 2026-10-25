use crate::expr::{self, Expr, Params, Ty, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Class(pub [u64; 4]);
impl Class {
    fn empty() -> Self {
        Self([0; 4])
    }
    fn insert(&mut self, b: u8) {
        self.0[b as usize / 64] |= 1u64 << (b % 64)
    }
    pub fn has(&self, b: u8) -> bool {
        self.0[b as usize / 64] & (1u64 << (b % 64)) != 0
    }
    fn inverse(mut self) -> Self {
        for x in &mut self.0 {
            *x = !*x
        }
        self
    }
}
#[derive(Clone, Debug)]
pub enum Ast {
    Empty,
    Fail,
    Lit(Vec<u8>),
    Class(Class),
    Seq(Vec<Ast>),
    Alt(Vec<Ast>),
    Capture(usize, Box<Ast>),
    Repeat(Box<Ast>, usize, Option<usize>, bool),
    Backref(usize),
    Guard(Expr),
    Emit(Expr),
    Start,
    End,
}
pub fn nullable(a: &Ast) -> bool {
    match a {
        Ast::Fail | Ast::Class(_) => false,
        Ast::Lit(v) => v.is_empty(),
        Ast::Empty | Ast::Start | Ast::End | Ast::Guard(_) | Ast::Emit(_) | Ast::Backref(_) => true,
        Ast::Seq(v) => v.iter().all(nullable),
        Ast::Alt(v) => v.iter().any(nullable),
        Ast::Capture(_, a) => nullable(a),
        Ast::Repeat(a, n, _, _) => *n == 0 || nullable(a),
    }
}
struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    names: BTreeMap<String, usize>,
    depth: usize,
}
impl Parser<'_> {
    fn eat(&mut self, b: u8) -> bool {
        if self.s.get(self.i) == Some(&b) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn require(&mut self, b: u8) -> Result<(), String> {
        if self.eat(b) {
            Ok(())
        } else {
            Err(format!("expected '{}' at byte {}", b as char, self.i))
        }
    }
    fn ident(&mut self, end: u8) -> Result<String, String> {
        let start = self.i;
        while self
            .s
            .get(self.i)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            self.i += 1
        }
        if self.i == start || self.s[start].is_ascii_digit() {
            return Err("expected identifier".into());
        }
        let n = String::from_utf8(self.s[start..self.i].to_vec()).unwrap();
        self.require(end)?;
        Ok(n)
    }
    fn alt(&mut self) -> Result<Ast, String> {
        self.depth += 1;
        if self.depth > 128 {
            return Err("pattern nesting exceeds 128".into());
        }
        let mut v = vec![self.seq()?];
        while self.eat(b'|') {
            v.push(self.seq()?)
        }
        self.depth -= 1;
        Ok(if v.len() == 1 {
            v.pop().unwrap()
        } else {
            Ast::Alt(v)
        })
    }
    fn seq(&mut self) -> Result<Ast, String> {
        let mut v = vec![];
        while self.i < self.s.len() && self.s[self.i] != b')' && self.s[self.i] != b'|' {
            v.push(self.repeat()?)
        }
        Ok(if v.is_empty() {
            Ast::Empty
        } else {
            Ast::Seq(v)
        })
    }
    fn number(&mut self) -> Result<usize, String> {
        let start = self.i;
        while self.s.get(self.i).is_some_and(u8::is_ascii_digit) {
            self.i += 1
        }
        let n = std::str::from_utf8(&self.s[start..self.i])
            .unwrap()
            .parse::<usize>()
            .map_err(|_| "invalid repetition count")?;
        if n > 1000 {
            return Err("repetition count exceeds 1000".into());
        }
        Ok(n)
    }
    fn repeat(&mut self) -> Result<Ast, String> {
        let a = self.atom()?;
        let q = if self.eat(b'*') {
            Some((0, None))
        } else if self.eat(b'+') {
            Some((1, None))
        } else if self.eat(b'?') {
            Some((0, Some(1)))
        } else if self.eat(b'{') {
            let n = self.number()?;
            let m = if self.eat(b',') {
                if self.s.get(self.i) == Some(&b'}') {
                    None
                } else {
                    Some(self.number()?)
                }
            } else {
                Some(n)
            };
            self.require(b'}')?;
            if m.is_some_and(|m| m < n) {
                return Err("reversed repetition bounds".into());
            }
            Some((n, m))
        } else {
            None
        };
        if let Some((n, m)) = q {
            if m.is_none() && nullable(&a) {
                return Err("unbounded repetition has a possibly empty body".into());
            }
            let greedy = !self.eat(b'?');
            Ok(Ast::Repeat(Box::new(a), n, m, greedy))
        } else {
            Ok(a)
        }
    }
    fn expression(&mut self) -> Result<Expr, String> {
        let start = self.i;
        let mut quoted = false;
        let mut escaped = false;
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if quoted {
                if escaped {
                    escaped = false
                } else if c == b'\\' {
                    escaped = true
                } else if c == b'"' {
                    quoted = false
                }
            } else if c == b'"' {
                quoted = true
            } else if c == b'}' {
                break;
            }
            self.i += 1
        }
        let e = expr::parse(
            std::str::from_utf8(&self.s[start..self.i]).map_err(|_| "expression must be UTF-8")?,
            &self.names,
        )?;
        self.require(b'}')?;
        Ok(e)
    }
    fn escaped(&mut self) -> Result<Vec<u8>, String> {
        let c = *self.s.get(self.i).ok_or("trailing escape")?;
        self.i += 1;
        Ok(match c {
            b'd' => (b'0'..=b'9').collect(),
            b'w' => (0u8..=255)
                .filter(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .collect(),
            b's' => vec![b' ', b'\t', b'\r', b'\n', 0x0b, 0x0c],
            b'n' => vec![b'\n'],
            b'r' => vec![b'\r'],
            b't' => vec![b'\t'],
            b'x' => {
                if self.i + 2 > self.s.len() {
                    return Err("short hex escape".into());
                }
                let v = u8::from_str_radix(
                    std::str::from_utf8(&self.s[self.i..self.i + 2]).map_err(|_| "hex escape")?,
                    16,
                )
                .map_err(|_| "invalid hex escape")?;
                self.i += 2;
                vec![v]
            }
            b if b.is_ascii_alphanumeric() => {
                return Err(format!("unsupported escape \\{}", b as char))
            }
            b => vec![b],
        })
    }
    fn class_unit(&mut self) -> Result<Vec<u8>, String> {
        if self.eat(b'\\') {
            self.escaped()
        } else {
            let b = *self.s.get(self.i).ok_or("unclosed class")?;
            self.i += 1;
            Ok(vec![b])
        }
    }
    fn atom(&mut self) -> Result<Ast, String> {
        let c = *self.s.get(self.i).ok_or("expected pattern atom")?;
        self.i += 1;
        match c {
            b'(' => {
                if self.eat(b'?') {
                    if self.eat(b'<') {
                        let n = self.ident(b'>')?;
                        if self.names.contains_key(&n) {
                            return Err("duplicate capture name".into());
                        }
                        let id = self.names.len();
                        if id >= 128 {
                            return Err("capture count exceeds 128".into());
                        }
                        self.names.insert(n, id);
                        let a = self.alt()?;
                        self.require(b')')?;
                        Ok(Ast::Capture(id, Box::new(a)))
                    } else if self.eat(b':') {
                        let a = self.alt()?;
                        self.require(b')')?;
                        Ok(a)
                    } else if self.eat(b'{') {
                        let e = self.expression()?;
                        self.require(b')')?;
                        if expr::type_of(&e, &mut BTreeMap::new())? != Ty::Bool {
                            return Err("guard requires Bool".into());
                        }
                        Ok(Ast::Guard(e))
                    } else {
                        Err("supported group prefixes: (?<name>, (?:, (?{".into())
                    }
                } else {
                    Err("use named capture (?<name>...) or noncapturing (?:...)".into())
                }
            }
            b'[' => {
                let invert = self.eat(b'^');
                let mut set = Class::empty();
                let mut count = 0;
                while self.s.get(self.i) != Some(&b']') {
                    let a = self.class_unit()?;
                    if self.s.get(self.i) == Some(&b'-') && self.s.get(self.i + 1) != Some(&b']') {
                        self.i += 1;
                        let z = self.class_unit()?;
                        if a.len() != 1 || z.len() != 1 || a[0] > z[0] {
                            return Err("invalid class range".into());
                        }
                        for b in a[0]..=z[0] {
                            set.insert(b)
                        }
                    } else {
                        for b in a {
                            set.insert(b)
                        }
                    }
                    count += 1;
                }
                self.require(b']')?;
                if count == 0 {
                    return Err("empty class".into());
                }
                Ok(Ast::Class(if invert { set.inverse() } else { set }))
            }
            b'.' => {
                let mut c = Class([u64::MAX; 4]);
                c.0[0] &= !(1u64 << b'\n');
                Ok(Ast::Class(c))
            }
            b'^' => Ok(Ast::Start),
            b'$' => {
                if self.eat(b'{') {
                    let e = self.expression()?;
                    if expr::type_of(&e, &mut BTreeMap::new())? != Ty::Str {
                        return Err("interpolation requires Str".into());
                    }
                    Ok(Ast::Emit(e))
                } else {
                    Ok(Ast::End)
                }
            }
            b'\\' => {
                if self.eat(b'k') {
                    self.require(b'<')?;
                    let n = self.ident(b'>')?;
                    Ok(Ast::Backref(
                        *self.names.get(&n).ok_or(format!("unknown capture {n}"))?,
                    ))
                } else {
                    let v = self.escaped()?;
                    if v.len() == 1 {
                        Ok(Ast::Lit(v))
                    } else {
                        let mut c = Class::empty();
                        for b in v {
                            c.insert(b)
                        }
                        Ok(Ast::Class(c))
                    }
                }
            }
            b'*' | b'+' | b'?' | b'{' | b'}' | b']' => {
                Err(format!("unescaped metacharacter at byte {}", self.i - 1))
            }
            b => Ok(Ast::Lit(vec![b])),
        }
    }
}
pub fn parse(s: &str) -> Result<(Ast, BTreeMap<String, usize>), String> {
    if s.len() > 8192 {
        return Err("pattern size exceeds 8192 bytes".into());
    }
    let mut p = Parser {
        s: s.as_bytes(),
        i: 0,
        names: BTreeMap::new(),
        depth: 0,
    };
    let a = p.alt()?;
    if p.i != p.s.len() {
        return Err("unmatched closing parenthesis".into());
    }
    Ok((a, p.names))
}
pub fn check_params(a: &Ast, p: &Params) -> Result<(), String> {
    match a {
        Ast::Guard(e) | Ast::Emit(e) => expr::check_params(e, p),
        Ast::Seq(v) | Ast::Alt(v) => {
            for a in v {
                check_params(a, p)?
            }
            Ok(())
        }
        Ast::Capture(_, a) | Ast::Repeat(a, _, _, _) => check_params(a, p),
        _ => Ok(()),
    }
}
pub fn specialize(a: &Ast, p: &Params) -> Ast {
    match a {
        Ast::Guard(e) => {
            let e = expr::specialize(e, p, &mut BTreeMap::new());
            match e {
                Expr::Val(Value::Bool(true)) => Ast::Empty,
                Expr::Val(Value::Bool(false)) => Ast::Fail,
                e => Ast::Guard(e),
            }
        }
        Ast::Emit(e) => {
            let e = expr::specialize(e, p, &mut BTreeMap::new());
            match e {
                Expr::Val(Value::Str(s)) => Ast::Lit(s),
                e => Ast::Emit(e),
            }
        }
        Ast::Seq(v) => {
            let mut out = vec![];
            for a in v {
                let a = specialize(a, p);
                let flat = if let Ast::Seq(v) = a { v } else { vec![a] };
                for a in flat {
                    match a {
                        Ast::Empty => {}
                        Ast::Lit(s) => {
                            if let Some(Ast::Lit(t)) = out.last_mut() {
                                t.extend_from_slice(&s)
                            } else {
                                out.push(Ast::Lit(s))
                            }
                        }
                        a => out.push(a),
                    }
                }
            }
            if out.is_empty() {
                Ast::Empty
            } else {
                Ast::Seq(out)
            }
        }
        Ast::Alt(v) => Ast::Alt(v.iter().map(|a| specialize(a, p)).collect()),
        Ast::Capture(id, a) => Ast::Capture(*id, Box::new(specialize(a, p))),
        Ast::Repeat(a, n, m, g) => Ast::Repeat(Box::new(specialize(a, p)), *n, *m, *g),
        a => a.clone(),
    }
}

#[derive(Clone, Debug)]
pub enum Inst {
    Lit(Vec<u8>),
    Class(Class),
    Run(Class, usize, Option<usize>, bool),
    /// Every shorter endpoint is statically known to fail the continuation.
    AtomicRun(Class, usize, Option<usize>),
    SaveStart(usize),
    SaveEnd(usize),
    Backref(usize),
    Guard(Expr),
    Emit(Expr),
    Split(usize, usize),
    /// Non-nullable alternatives have disjoint first-byte sets.
    Switch(Class, usize, usize),
    Jump(usize),
    Start,
    End,
    Accept,
    Fail,
}
pub fn compile(a: &Ast, pe: bool) -> Result<Vec<Inst>, String> {
    fn emit(a: &Ast, code: &mut Vec<Inst>, pe: bool) -> Result<(), String> {
        if code.len() > 32768 {
            return Err("expanded program exceeds 32768 instructions".into());
        }
        match a {
            Ast::Empty => {}
            Ast::Fail => code.push(Inst::Fail),
            Ast::Lit(s) => code.push(Inst::Lit(s.clone())),
            Ast::Class(c) => code.push(Inst::Class(c.clone())),
            Ast::Start => code.push(Inst::Start),
            Ast::End => code.push(Inst::End),
            Ast::Backref(id) => code.push(Inst::Backref(*id)),
            Ast::Guard(e) => code.push(Inst::Guard(e.clone())),
            Ast::Emit(e) => code.push(Inst::Emit(e.clone())),
            Ast::Capture(id, a) => {
                code.push(Inst::SaveStart(*id));
                emit(a, code, pe)?;
                code.push(Inst::SaveEnd(*id));
            }
            Ast::Seq(v) => {
                for a in v {
                    emit(a, code, pe)?
                }
            }
            Ast::Alt(v) => {
                let mut jumps = vec![];
                for (i, a) in v.iter().enumerate() {
                    if i + 1 == v.len() {
                        emit(a, code, pe)?
                    } else {
                        let split = code.len();
                        code.push(Inst::Fail);
                        emit(a, code, pe)?;
                        jumps.push(code.len());
                        code.push(Inst::Fail);
                        code[split] = Inst::Split(split + 1, code.len());
                    }
                }
                let end = code.len();
                for j in jumps {
                    code[j] = Inst::Jump(end)
                }
            }
            Ast::Repeat(a, n, m, g) => {
                if pe {
                    if let Ast::Class(c) = a.as_ref() {
                        code.push(Inst::Run(c.clone(), *n, *m, *g));
                        return Ok(());
                    }
                }
                for _ in 0..*n {
                    emit(a, code, pe)?
                }
                match m {
                    Some(m) => {
                        let mut exits = Vec::new();
                        for _ in *n..*m {
                            let s = code.len();
                            code.push(Inst::Fail);
                            emit(a, code, pe)?;
                            exits.push(s);
                        }
                        let end = code.len();
                        // Skipping an optional iteration exits the whole repeat.
                        // Independent optionals would enumerate the same count
                        // combinatorially and alter prioritized witness order.
                        for s in exits {
                            code[s] = if *g {
                                Inst::Split(s + 1, end)
                            } else {
                                Inst::Split(end, s + 1)
                            }
                        }
                    }
                    None => {
                        let s = code.len();
                        code.push(Inst::Fail);
                        emit(a, code, pe)?;
                        code.push(Inst::Jump(s));
                        let end = code.len();
                        code[s] = if *g {
                            Inst::Split(s + 1, end)
                        } else {
                            Inst::Split(end, s + 1)
                        }
                    }
                }
            }
        }
        Ok(())
    }
    let mut code = vec![];
    emit(a, &mut code, pe)?;
    if pe {
        // Full-match observation permits End to commute left through a suffix
        // of pure zero-width guards. Preserve edges entering that suffix.
        let at = code
            .iter()
            .rposition(|i| !matches!(i, Inst::Guard(_)))
            .map_or(0, |i| i + 1);
        if at < code.len() {
            for i in &mut code {
                match i {
                    Inst::Split(a, b) => {
                        if *a > at {
                            *a += 1;
                        }
                        if *b > at {
                            *b += 1;
                        }
                    }
                    Inst::Jump(a) if *a > at => {
                        *a += 1;
                    }
                    _ => {}
                }
            }
            code.insert(at, Inst::End);
        }
    }
    code.push(Inst::Accept);
    if pe {
        fn first(code: &[Inst], pc: usize, fuel: usize) -> Option<Class> {
            if fuel == 0 {
                return None;
            }
            match code.get(pc)? {
                Inst::Lit(s) if !s.is_empty() => {
                    let mut c = Class::empty();
                    c.insert(s[0]);
                    Some(c)
                }
                Inst::Class(c) | Inst::Run(c, 1.., _, _) | Inst::AtomicRun(c, 1.., _) => {
                    Some(c.clone())
                }
                Inst::Split(a, b) | Inst::Switch(_, a, b) => {
                    let mut c = first(code, *a, fuel - 1)?;
                    let d = first(code, *b, fuel - 1)?;
                    for (a, b) in c.0.iter_mut().zip(d.0) {
                        *a |= b;
                    }
                    Some(c)
                }
                Inst::Jump(n) => first(code, *n, fuel - 1),
                Inst::SaveStart(_) | Inst::SaveEnd(_) | Inst::Guard(_) | Inst::Start => {
                    first(code, pc + 1, fuel - 1)
                }
                _ => None,
            }
        }
        for pc in (0..code.len()).rev() {
            if let Inst::Split(a, b) = code[pc] {
                if let (Some(c), Some(d)) = (first(&code, a, 64), first(&code, b, 64)) {
                    if c.0.iter().zip(d.0).all(|(a, b)| a & b == 0) {
                        code[pc] = Inst::Switch(c, a, b);
                    }
                }
            }
        }
        // Save/guard instructions consume no input. A disjoint next byte or
        // absolute end makes the maximal class endpoint the only viable one.
        for pc in 0..code.len() {
            if let Inst::Run(c, min, max, _) = &code[pc] {
                let forced = code[pc + 1..].iter().find(|i| {
                    !matches!(
                        i,
                        Inst::SaveStart(_) | Inst::SaveEnd(_) | Inst::Guard(_) | Inst::Start
                    )
                });
                let atomic = match forced {
                    Some(Inst::End | Inst::Accept) => true,
                    Some(Inst::Lit(s)) => s.first().is_some_and(|b| !c.has(*b)),
                    Some(Inst::Class(d)) => c.0.iter().zip(d.0).all(|(a, b)| a & b == 0),
                    _ => false,
                };
                if atomic {
                    code[pc] = Inst::AtomicRun(c.clone(), *min, *max);
                }
            }
        }
    }
    if code.len() > 32768 {
        return Err("expanded program exceeds 32768 instructions".into());
    }
    Ok(code)
}

/// Necessary-literal filter for complex unbounded repeats only. It proves
/// rejection, never acceptance, and does not remove surviving backtracking.
pub fn rejection_literal(a: &Ast) -> Option<Vec<u8>> {
    fn complex(a: &Ast) -> bool {
        match a {
            Ast::Repeat(a, _, None, _) if !matches!(a.as_ref(), Ast::Class(_) | Ast::Lit(_)) => {
                true
            }
            Ast::Seq(v) | Ast::Alt(v) => v.iter().any(complex),
            Ast::Repeat(a, ..) | Ast::Capture(_, a) => complex(a),
            _ => false,
        }
    }
    fn required(a: &Ast) -> Vec<u8> {
        match a {
            Ast::Lit(s) => s.clone(),
            Ast::Seq(v) => v
                .iter()
                .map(required)
                .max_by_key(Vec::len)
                .unwrap_or_default(),
            Ast::Capture(_, a) | Ast::Repeat(a, 1.., _, _) => required(a),
            Ast::Alt(v) => {
                let all: Vec<_> = v.iter().map(required).collect();
                all.iter()
                    .filter(|s| {
                        !s.is_empty()
                            && all
                                .iter()
                                .all(|v| v.windows(s.len()).any(|w| w == s.as_slice()))
                    })
                    .max_by_key(|s| s.len())
                    .cloned()
                    .unwrap_or_default()
            }
            _ => vec![],
        }
    }
    if !complex(a) {
        return None;
    }
    let s = required(a);
    (!s.is_empty()).then_some(s)
}
