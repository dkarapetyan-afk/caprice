//! Minimal PCRE2-8 adapter used only for reproducibility and differential tests.
use std::ffi::c_void;
#[link(name = "pcre2-8")]
unsafe extern "C" {
    fn pcre2_compile_8(
        s: *const u8,
        n: usize,
        flags: u32,
        error: *mut i32,
        offset: *mut usize,
        ctx: *mut c_void,
    ) -> *mut c_void;
    fn pcre2_code_free_8(p: *mut c_void);
    fn pcre2_match_data_create_from_pattern_8(p: *const c_void, ctx: *mut c_void) -> *mut c_void;
    fn pcre2_match_data_free_8(p: *mut c_void);
    fn pcre2_match_context_create_8(ctx: *mut c_void) -> *mut c_void;
    fn pcre2_match_context_free_8(ctx: *mut c_void);
    fn pcre2_set_match_limit_8(ctx: *mut c_void, limit: u32) -> i32;
    fn pcre2_jit_compile_8(p: *mut c_void, flags: u32) -> i32;
    fn pcre2_pattern_info_8(p: *const c_void, what: u32, where_: *mut c_void) -> i32;
    fn pcre2_match_8(
        code: *const c_void,
        s: *const u8,
        n: usize,
        start: usize,
        flags: u32,
        data: *mut c_void,
        ctx: *mut c_void,
    ) -> i32;
    fn pcre2_jit_match_8(
        code: *const c_void,
        s: *const u8,
        n: usize,
        start: usize,
        flags: u32,
        data: *mut c_void,
        ctx: *mut c_void,
    ) -> i32;
    fn pcre2_get_ovector_pointer_8(data: *mut c_void) -> *mut usize;
    fn pcre2_config_8(what: u32, where_: *mut c_void) -> i32;
}
pub struct Pcre {
    code: *mut c_void,
    data: *mut c_void,
    ctx: *mut c_void,
    count: usize,
    matched: bool,
    jit_fast: bool,
    pub jit_bytes: usize,
}
impl Pcre {
    pub fn new(p: &str, jit: bool) -> Result<Self, String> {
        unsafe {
            let pattern = format!("\\A(?:{p})\\z");
            let mut error = 0;
            let mut offset = 0;
            let code = pcre2_compile_8(
                pattern.as_ptr(),
                pattern.len(),
                0,
                &mut error,
                &mut offset,
                std::ptr::null_mut(),
            );
            if code.is_null() {
                return Err(format!("PCRE2 error {error} at {offset}"));
            }
            if jit {
                let rc = pcre2_jit_compile_8(code, 1);
                if rc != 0 {
                    pcre2_code_free_8(code);
                    return Err(format!("PCRE2 JIT unavailable: {rc}"));
                }
            }
            let data = pcre2_match_data_create_from_pattern_8(code, std::ptr::null_mut());
            let ctx = pcre2_match_context_create_8(std::ptr::null_mut());
            if data.is_null() || ctx.is_null() {
                if !data.is_null() {
                    pcre2_match_data_free_8(data)
                }
                if !ctx.is_null() {
                    pcre2_match_context_free_8(ctx)
                }
                pcre2_code_free_8(code);
                return Err("PCRE2 allocation failed".into());
            }
            pcre2_set_match_limit_8(ctx, 1_000_000);
            let mut jit_bytes = 0;
            pcre2_pattern_info_8(code, 10, &mut jit_bytes as *mut usize as *mut c_void);
            let mut count = 0u32;
            pcre2_pattern_info_8(code, 4, &mut count as *mut u32 as *mut c_void);
            Ok(Self {
                code,
                data,
                ctx,
                count: count as usize,
                matched: false,
                jit_fast: false,
                jit_bytes,
            })
        }
    }
    pub fn run(&mut self, s: &[u8]) -> Result<Option<crate::expr::Captures>, i32> {
        unsafe {
            let rc = pcre2_match_8(self.code, s.as_ptr(), s.len(), 0, 0, self.data, self.ctx);
            self.matched = rc >= 0;
            if rc == -1 {
                return Ok(None);
            }
            if rc < 0 {
                return Err(rc);
            }
            let ptr = pcre2_get_ovector_pointer_8(self.data);
            Ok(Some(
                (0..=self.count)
                    .map(|i| {
                        let a = *ptr.add(2 * i);
                        let z = *ptr.add(2 * i + 1);
                        if a == usize::MAX {
                            None
                        } else {
                            Some((a, z))
                        }
                    })
                    .collect(),
            ))
        }
    }
    /// No output allocation for timed Boolean queries; caller may inspect spans.
    pub fn set_jit_fast(&mut self, enabled: bool) -> Result<(), String> {
        if enabled && self.jit_bytes == 0 {
            return Err("fast path requires compiled JIT".into());
        }
        self.jit_fast = enabled;
        Ok(())
    }
    pub fn test(&mut self, s: &[u8]) -> Result<bool, i32> {
        unsafe {
            let rc = if self.jit_fast {
                pcre2_jit_match_8(self.code, s.as_ptr(), s.len(), 0, 0, self.data, self.ctx)
            } else {
                pcre2_match_8(self.code, s.as_ptr(), s.len(), 0, 0, self.data, self.ctx)
            };
            self.matched = rc >= 0;
            if rc == -1 {
                Ok(false)
            } else if rc < 0 {
                Err(rc)
            } else {
                Ok(true)
            }
        }
    }
    pub fn capture(&self, id: usize, s: &[u8]) -> Option<Vec<u8>> {
        self.capture_slice(id, s).map(<[u8]>::to_vec)
    }
    pub fn capture_slice<'a>(&self, id: usize, s: &'a [u8]) -> Option<&'a [u8]> {
        if !self.matched || id > self.count {
            return None;
        }
        unsafe {
            let p = pcre2_get_ovector_pointer_8(self.data);
            let a = *p.add(2 * id);
            let z = *p.add(2 * id + 1);
            if a <= z && z <= s.len() {
                Some(&s[a..z])
            } else {
                None
            }
        }
    }
    pub fn version() -> String {
        unsafe {
            let mut out = [0u8; 128];
            pcre2_config_8(11, out.as_mut_ptr() as *mut c_void);
            String::from_utf8_lossy(&out[..out.iter().position(|b| *b == 0).unwrap_or(128)]).into()
        }
    }
}
impl Drop for Pcre {
    fn drop(&mut self) {
        unsafe {
            pcre2_match_context_free_8(self.ctx);
            pcre2_match_data_free_8(self.data);
            pcre2_code_free_8(self.code)
        }
    }
}
