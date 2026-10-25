# Caprice handoff

- Develop the latest implementation in project/, currently v0.3.0.
- Read START_HERE.md and CONTINUE_HERE.md before changing implementation.
- Preserve baseline-v0.1.0, baseline-v0.2.0, original-v0.1.0 and historical raw data.
- Preserve ordered capture observations and backtracking, strict lambda/Boolean
  evaluation, missing versus empty/open captures and explicit Budget outcomes.
- Native capture layout is repr(C); do not rely on Rust Option memory layout.
- Use the bundled offline dependency configuration and PCRE2 build wrapper.
- Run meaningful semantic tests for compiler/runtime changes. Report performance
  only from actual measurements with correctness checks and captured source hashes.
- Keep Boolean and capture-output measurements separate. Record regressions.
- Lean currently checks the expression kernel and affine arithmetic, not the full
  Rust engine. Do not overstate the verification boundary.
- Source/manuscripts use Anonymous author. Keep the original submission record
  historical; check the recorded public endpoint before claiming a current status.
- A new journal upload is a separate action from local implementation and bundling.
