import Std

/- A checked kernel model, not a verification of the Rust parser or Cranelift.
   Primitive operators are arbitrary total typed functions. Fully applied
   lambdas use higher-order abstract syntax and strict Option sequencing. -/
namespace Caprice
inductive Ty where
  | str | nat | bool

def Val : Ty → Type
  | .str => List UInt8
  | .nat => Nat
  | .bool => Bool

inductive Expr : Ty → Type where
  | lit {t : Ty} : Val t → Expr t
  | cap : Nat → Expr .str
  | param : Nat → Expr .str
  | unary {a b : Ty} : (Val a → Val b) → Expr a → Expr b
  | binary {a b c : Ty} : (Val a → Val b → Val c) → Expr a → Expr b → Expr c
  | letE {a b : Ty} : Expr a → (Val a → Expr b) → Expr b

abbrev Captures := Nat → Option (List UInt8)
abbrev Params := Nat → List UInt8
abbrev Static := Nat → Option (List UInt8)

def eval (γ : Captures) (ρ : Params) : Expr t → Option (Val t)
  | .lit v => some v
  | .cap k => γ k
  | .param k => some (ρ k)
  | .unary f e => (eval γ ρ e).map f
  | .binary f e₁ e₂ =>
      (eval γ ρ e₁).bind fun x => (eval γ ρ e₂).map (f x)
  | .letE e body => (eval γ ρ e).bind fun x => eval γ ρ (body x)

def smartUnary (f : Val a → Val b) : Expr a → Expr b
  | .lit v => .lit (f v)
  | e => .unary f e

def smartBinary (f : Val a → Val b → Val c) : Expr a → Expr b → Expr c
  | .lit x, .lit y => .lit (f x y)
  | e₁, e₂ => .binary f e₁ e₂

def smartLet (e : Expr a) (body : Val a → Expr b) : Expr b :=
  match e with
  | .lit v => body v
  | _ => .letE e body

def specialize (σ : Static) : Expr t → Expr t
  | .lit v => .lit v
  | .cap k => .cap k
  | .param k => match σ k with
      | some v => .lit v
      | none => .param k
  | .unary f e => smartUnary f (specialize σ e)
  | .binary f e₁ e₂ => smartBinary f (specialize σ e₁) (specialize σ e₂)
  | .letE e body => smartLet (specialize σ e) (fun x => specialize σ (body x))

theorem eval_smartUnary (γ : Captures) (ρ : Params)
    (f : Val a → Val b) (e : Expr a) :
    eval γ ρ (smartUnary f e) = (eval γ ρ e).map f := by
  cases e <;> rfl

theorem eval_smartBinary (γ : Captures) (ρ : Params)
    (f : Val a → Val b → Val c) (e₁ : Expr a) (e₂ : Expr b) :
    eval γ ρ (smartBinary f e₁ e₂) =
      ((eval γ ρ e₁).bind fun x => (eval γ ρ e₂).map (f x)) := by
  cases e₁ <;> cases e₂ <;> rfl

theorem eval_smartLet (γ : Captures) (ρ : Params)
    (e : Expr a) (body : Val a → Expr b) :
    eval γ ρ (smartLet e body) =
      ((eval γ ρ e).bind fun x => eval γ ρ (body x)) := by
  cases e <;> rfl

def Extends (ρ : Params) (σ : Static) : Prop :=
  ∀ k v, σ k = some v → ρ k = v

theorem specialization_sound (γ : Captures) (ρ : Params) (σ : Static)
    (h : Extends ρ σ) (e : Expr t) :
    eval γ ρ (specialize σ e) = eval γ ρ e := by
  induction e with
  | lit v => rfl
  | cap k => rfl
  | param k =>
      simp only [specialize]
      split
      next v hk => simp [eval, h k v hk]
      next => rfl
  | unary f e ih =>
      simp only [specialize, eval_smartUnary, eval, ih]
  | binary f e₁ e₂ ih₁ ih₂ =>
      simp only [specialize, eval_smartBinary, eval, ih₁, ih₂]
  | letE e body ih ihbody =>
      simp only [specialize, eval_smartLet, eval, ih, ihbody]

theorem staged_specialization (γ : Captures) (ρ : Params) (σ τ : Static)
    (hσ : Extends ρ σ) (hτ : Extends ρ τ) (e : Expr t) :
    eval γ ρ (specialize τ (specialize σ e)) =
      eval γ ρ (specialize τ e) := by
  rw [specialization_sound γ ρ τ hτ, specialization_sound γ ρ σ hσ,
      specialization_sound γ ρ τ hτ]

def accepts (v : Option Bool) : Bool := v == some true

theorem guard_preservation (γ : Captures) (ρ : Params) (σ : Static)
    (h : Extends ρ σ) (e : Expr .bool) :
    accepts (eval γ ρ (specialize σ e)) = accepts (eval γ ρ e) := by
  rw [specialization_sound γ ρ σ h]

-- Regression law: dropping an unused dynamic lambda argument is unsound.
theorem missing_argument_is_strict (γ : Captures) (ρ : Params) (k : Nat)
    (h : γ k = none) :
    eval γ ρ (.letE (.cap k) (fun _ => .lit (t := .bool) true)) = none := by
  simp [eval, h]

#print axioms specialization_sound
#print axioms staged_specialization
#print axioms guard_preservation
#print axioms missing_argument_is_strict
end Caprice
