use crate::{TranslateResult, TranslateStreamChunk};
use anyhow::{anyhow, bail, Result};
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;

pub type GetPluginName = unsafe extern fn() -> *mut c_char;
pub type CreateTranslator = unsafe extern fn(*const c_char) -> *mut FfiResult<TranslatorHandle>;
pub type GetSupportedInputLanguages = unsafe extern fn(*mut TranslatorHandle, *mut *mut *const c_char, *mut usize) -> *mut FfiResult<i8>;
pub type IsSupportedInputLanguage = unsafe extern fn(*mut TranslatorHandle, *const c_char) -> *mut FfiResult<i8>;
pub type GetSupportedOutputLanguages = unsafe extern fn(*mut TranslatorHandle, *mut *mut *const c_char, *mut usize) -> *mut FfiResult<i8>;
pub type IsSupportedOutputLanguage = unsafe extern fn(*mut TranslatorHandle, *const c_char) -> *mut FfiResult<i8>;
pub type CallTranslate = unsafe extern fn(*mut TranslatorHandle, *const c_char) -> *mut FfiResult<TranslateResultFFI>;
pub type CallTranslateStream = unsafe extern fn(*mut TranslatorHandle, *const c_char, StreamCallback, *mut c_void) -> *mut FfiResult<i8>;

#[repr(C)]
pub struct TranslatorHandle {
    _private: [u8; 0],
}

#[repr(C)]
pub struct FfiObj {
    _private: [u8; 0],
}

#[repr(C)]
pub struct FfiResult<T> {
    pub ptr: *mut T,
    pub err: *mut c_char,
}

pub trait FfiResultExt<T> {
    fn to_ptr(self) -> *mut T;
}

impl <T> FfiResultExt<FfiResult<T>> for Result<T> {
    fn to_ptr(self) -> *mut FfiResult<T> {
        Box::into_raw(Box::new(self.into()))
    }
}

impl <T> Into<FfiResult<T>> for Result<T> {
    fn into(self) -> FfiResult<T> {
        match self {
            Ok(handle) => {
                FfiResult {
                    ptr: Box::into_raw(Box::new(handle)),
                    err: ptr::null_mut(),
                }
            }
            Err(err) => {
                FfiResult {
                    ptr: ptr::null_mut(),
                    err: CString::new(format!("{:?}", err)).unwrap().into_raw(),
                }
            }
        }
    }
}

pub fn unwrap_handle_result<T>(result: *mut FfiResult<T>) -> Result<*mut T> {
    if result.is_null() {
        return Err(anyhow!("result is null"));
    }

    let result = unsafe { Box::from_raw(result) };

    if !result.err.is_null() {
        return Err(anyhow!("result's error: {:?}", unsafe { CString::from_raw(result.err) }.to_string_lossy().to_owned()));
    }

    if result.ptr.is_null() {
        return Err(anyhow!("result obj is null"));
    }

    Ok(result.ptr)
}

#[repr(C)]
pub struct TranslateResultFFI {
    reasoning: *mut c_char,
    content: *mut c_char,
}

impl TranslateResult {
    pub fn into_ffi_unbox(self) -> TranslateResultFFI {
        let reasoning = self.reasoning
            .map(|s| CString::new(s).unwrap().into_raw())
            .unwrap_or(std::ptr::null_mut());

        let content = self.content
            .map(|s| CString::new(s).unwrap().into_raw())
            .unwrap_or(std::ptr::null_mut());

        TranslateResultFFI { reasoning, content }
    }

    pub fn into_ffi(self) -> *mut TranslateResultFFI {
        let reasoning = self.reasoning
            .map(|s| CString::new(s).unwrap().into_raw())
            .unwrap_or(std::ptr::null_mut());

        let content = self.content
            .map(|s| CString::new(s).unwrap().into_raw())
            .unwrap_or(std::ptr::null_mut());

        Box::into_raw(Box::new(TranslateResultFFI { reasoning, content }))
    }

    pub fn from_ffi(result: *mut TranslateResultFFI) -> Result<TranslateResult> {
        if result.is_null() {
            bail!("null pointer received from ffi");
        }

        let result = unsafe { Box::from_raw(result) };

        Ok(TranslateResult {
            reasoning: if result.reasoning.is_null() {
                None
            } else {
                Some(unsafe { CString::from_raw(result.reasoning).into_string()? })
            },
            content: if result.content.is_null() {
                None
            } else {
                Some(unsafe { CString::from_raw(result.content).into_string()? })
            },
        })
    }
}

pub fn free_translate_result(result: *mut TranslateResultFFI) {
    if result.is_null() {
        return;
    }
    unsafe {
        let result = Box::from_raw(result);
        if !result.reasoning.is_null() {
            let _ = CString::from_raw(result.reasoning);
        }
        if !result.content.is_null() {
            let _ = CString::from_raw(result.content);
        }
    }
}

pub type StreamCallback = extern "C" fn(chunk: *mut TranslateStreamChunkFFI, cb: *mut c_void);

pub extern "C" fn stream_callback(chunk: *mut TranslateStreamChunkFFI, cb: *mut c_void) {
    unsafe {
        let closure = &*(cb as *const Box<dyn Fn(*mut TranslateStreamChunkFFI)>);
        closure(chunk);
    }
}

#[repr(C)]
pub enum TranslateStreamChunkTag {
    Start,
    Delta,
    End,
}

#[repr(C)]
pub union ChunkData {
    delta: *mut TranslateResultFFI,
    _dummy: u8,
}

#[repr(C)]
pub struct TranslateStreamChunkFFI {
    pub tag: TranslateStreamChunkTag,
    pub data: ChunkData,
}

impl TranslateStreamChunk {
    pub fn into_ffi(self) -> *mut TranslateStreamChunkFFI {
        let b = match self {
            TranslateStreamChunk::Start => Box::new(TranslateStreamChunkFFI {
                tag: TranslateStreamChunkTag::Start,
                data: ChunkData { delta: std::ptr::null_mut() },
            }),
            TranslateStreamChunk::Delta(result) => {
                Box::new(TranslateStreamChunkFFI {
                    tag: TranslateStreamChunkTag::Delta,
                    data: ChunkData { delta: result.into_ffi() },
                })
            },
            TranslateStreamChunk::End => Box::new(TranslateStreamChunkFFI {
                tag: TranslateStreamChunkTag::End,
                data: ChunkData { delta: std::ptr::null_mut() },
            }),
        };

        Box::into_raw(b)
    }

    pub fn from_ffi(result: *mut TranslateStreamChunkFFI) -> Result<TranslateStreamChunk> {
        if result.is_null() {
            bail!("null pointer received from ffi");
        }
        let result = unsafe { Box::from_raw(result) };
        match result.tag {
            TranslateStreamChunkTag::Start => {
                Ok(TranslateStreamChunk::Start)
            }
            TranslateStreamChunkTag::Delta => {
                unsafe { Ok(TranslateStreamChunk::Delta(TranslateResult::from_ffi(result.data.delta)?)) }
            }
            TranslateStreamChunkTag::End => {
                Ok(TranslateStreamChunk::End)
            }
        }
    }
}

pub fn wrap_err(error_ptr: *mut c_char) -> Result<()> {
    if !error_ptr.is_null() {
        let msg = unsafe {
            CStr::from_ptr(error_ptr).to_string_lossy().into_owned()
        };
        unsafe {
            let _ = Box::from_raw(error_ptr);
        }
        return Err(anyhow!(msg));
    }

    Ok(())
}

pub fn convert_string_vec_to_c_array(
    strings: Vec<String>,
    output_ptr: *mut *mut *const c_char,
    output_len: *mut usize,
) -> *mut FfiResult<i8> {
    let mut c_strings = Vec::with_capacity(strings.len());
    for s in strings {
        match CString::new(s) {
            Ok(c_str) => c_strings.push(c_str.into_raw()),
            Err(e) => {
                for ptr in &c_strings {
                    unsafe { drop(CString::from_raw(*ptr)) };
                }
                return Err(anyhow!("{}", e)).to_ptr();
            }
        }
    }
    let mut boxed = c_strings.into_boxed_slice();
    let ptr = boxed.as_mut_ptr() as *mut *const c_char;
    let len = boxed.len();
    std::mem::forget(boxed);
    unsafe {
        *output_ptr = ptr;
        *output_len = len;
    }
    Ok(0).to_ptr()
}

pub fn free_supported_languages(array: *mut *const c_char, len: usize) {
    if array.is_null() || len == 0 {
        return;
    }
    let slice = unsafe {
        Box::from_raw(std::slice::from_raw_parts_mut(array as *mut *mut c_char, len))
    };
    for ptr in slice.iter() {
        unsafe {
            if !ptr.is_null() {
                drop(CString::from_raw(*ptr));
            }
        }
    }
}
