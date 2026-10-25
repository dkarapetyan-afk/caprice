import Std

/- Arithmetic obligations for inferred capture lengths. This proves neither the
   Rust AST analyzer nor extraction of a length equation from regex semantics. -/
namespace Caprice.Lengths

theorem affine_unique {fixed scale n x y : Nat} (positive : 0 < scale)
    (hx : fixed + scale * x = n) (hy : fixed + scale * y = n) : x = y := by
  exact Nat.eq_of_mul_eq_mul_left positive (Nat.add_left_cancel (hx.trans hy.symm))

theorem affine_recover {fixed scale n length : Nat} (positive : 0 < scale)
    (equation : fixed + scale * length = n) : (n - fixed) / scale = length := by
  subst n
  rw [Nat.add_sub_cancel_left, Nat.mul_div_cancel_left _ positive]

theorem affine_divisibility {fixed scale n length : Nat}
    (equation : fixed + scale * length = n) : (n - fixed) % scale = 0 := by
  subst n
  rw [Nat.add_sub_cancel_left, Nat.mul_mod_right]

theorem reject_bad_remainder {fixed scale n : Nat}
    (bad : (n - fixed) % scale ≠ 0) : ¬ ∃ length, fixed + scale * length = n := by
  rintro ⟨length, equation⟩
  exact bad (affine_divisibility equation)

theorem reject_upper_bound {fixed scale n maximum : Nat} (positive : 0 < scale)
    (large : maximum < (n - fixed) / scale) :
    ¬ ∃ length, fixed + scale * length = n ∧ length ≤ maximum := by
  rintro ⟨length, equation, bound⟩
  rw [affine_recover positive equation] at large
  exact (Nat.not_lt_of_ge bound) large

#print axioms affine_unique
#print axioms affine_recover
#print axioms affine_divisibility
#print axioms reject_bad_remainder
#print axioms reject_upper_bound
end Caprice.Lengths
