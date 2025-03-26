use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, LitStr, Token, Type};
use syn::parse::{Parse, ParseStream};

struct BuildFfiInput {
    pub name: String,
    pub translator: Type,
}

impl Parse for BuildFfiInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse::<LitStr>()?;

        input.parse::<Token![,]>()?;

        let typ = input.parse::<Type>()?;

        Ok(BuildFfiInput {
            name: name.value(),
            translator: typ,
        })
    }
}

#[proc_macro]
pub fn build_ffi(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as BuildFfiInput);

    let name = input.name;
    let translator = input.translator;

    TokenStream::from(quote!{
use lib::ffi::{FfiResult, FfiResultExt, StreamCallback, TranslateResultFFI, TranslatorHandle, convert_string_vec_to_c_array};
use lib::{TranslateStreamChunk, TranslateTask, Translator};
use std::ffi::{c_char, c_void, CStr, CString};
use tokio::runtime::Handle;
use tokio::sync::mpsc::channel;

#[allow(dead_code)]
impl #translator {
    pub fn into_ffi(self) -> *mut TranslatorHandle {
        Box::into_raw(Box::new(self)) as *mut TranslatorHandle
    }

    pub fn from_ptr(ptr: *mut TranslatorHandle) -> Option<Box<#translator>> {
        if ptr.is_null() {
            return None;
        }

        Some(unsafe { Box::from_raw(ptr as *mut #translator) })
    }
}

#[no_mangle]
pub extern "C" fn get_plugin_name() -> *mut c_char {
    CString::new(#name).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn create_translator(
    json_str: *const c_char
) -> *mut FfiResult<#translator> {
    let input = unsafe {
        if json_str.is_null() {
            return Err(anyhow::anyhow!("Null pointer received")).to_ptr();
        }
        match CStr::from_ptr(json_str).to_str() {
            Ok(s) => s,
            Err(e) => {
                return Err(anyhow::anyhow!("Invalid UTF-8: {}", e)).to_ptr();
            }
        }
    };

    let value: serde_json::Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => {
            return Err(anyhow::anyhow!("JSON parse error: {}", e)).to_ptr();
        }
    };

    if let Ok(handle) = Handle::try_current() {
        handle.block_on(async {
            match #translator::new(value).await {
                Ok(translator) => {
                    return Ok(translator).to_ptr();
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Creation error: {}", e)).to_ptr();
                }
            }
        })
    } else {
        let handle = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        handle.block_on(async {
            match #translator::new(value).await {
                Ok(translator) => {
                    return Ok(translator).to_ptr();
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Creation error: {}", e)).to_ptr();
                }
            }
        })
    }
}

#[no_mangle]
pub extern "C" fn get_supported_input_languages(
    translator_ptr: *mut TranslatorHandle,
    array: *mut *mut *const c_char,
    len: *mut usize,
) -> *mut FfiResult<i8> {
    if translator_ptr.is_null() {
        return Err(anyhow::anyhow!("Null pointer received")).to_ptr();
    }

    let translator = unsafe { &*(translator_ptr as *mut #translator) };

    let list = translator.get_supported_input_languages();
    if let Err(e) = list {
        return Err(anyhow::anyhow!("{}", e)).to_ptr();
    }
    convert_string_vec_to_c_array(list.unwrap(), array, len)
}

#[no_mangle]
pub extern "C" fn get_supported_output_languages(
    translator_ptr: *mut TranslatorHandle,
    array: *mut *mut *const c_char,
    len: *mut usize,
) -> *mut FfiResult<i8> {
    if translator_ptr.is_null() {
        return Err(anyhow::anyhow!("Null pointer received")).to_ptr();
    }

    let translator = unsafe { &*(translator_ptr as *mut #translator) };

    let list = translator.get_supported_output_languages();
    if let Err(e) = list {
        return Err(anyhow::anyhow!("{}", e)).to_ptr();
    }
    convert_string_vec_to_c_array(list.unwrap(), array, len)
}

#[no_mangle]
pub extern "C" fn is_supported_input_language(
    translator_ptr: *mut TranslatorHandle,
    lang: *const c_char
) -> *mut FfiResult<i8> {
    let lang = unsafe {
        if lang.is_null() {
            return Err(anyhow::anyhow!("Null pointer received")).to_ptr();
        }
        match CStr::from_ptr(lang).to_str() {
            Ok(s) => s,
            Err(e) => {
                return Err(anyhow::anyhow!("Invalid UTF-8: {}", e)).to_ptr();
            }
        }
    };

    if translator_ptr.is_null() {
        return Err(anyhow::anyhow!("Null pointer received")).to_ptr();
    }

    let translator = unsafe { &*(translator_ptr as *mut #translator) };

    let res = translator.is_supported_input_language(lang.to_string());
    match res {
        Ok(true) => {
            Ok(0).to_ptr()
        }
        Ok(false) => {
            Ok(1).to_ptr()
        }
        Err(e) => {
            Err(anyhow::anyhow!("{}", e)).to_ptr()
        }
    }
}

#[no_mangle]
pub extern "C" fn is_supported_output_language(
    translator_ptr: *mut TranslatorHandle,
    lang: *const c_char
) -> *mut FfiResult<i8> {
    let lang = unsafe {
        if lang.is_null() {
            return Err(anyhow::anyhow!("Null pointer received")).to_ptr();
        }
        match CStr::from_ptr(lang).to_str() {
            Ok(s) => s,
            Err(e) => {
                return Err(anyhow::anyhow!("Invalid UTF-8: {}", e)).to_ptr();
            }
        }
    };

    if translator_ptr.is_null() {
        return Err(anyhow::anyhow!("Null pointer received")).to_ptr();
    }

    let translator = unsafe { &*(translator_ptr as *mut #translator) };

    let res = translator.is_supported_output_language(lang.to_string());
    match res {
        Ok(true) => {
            Ok(0).to_ptr()
        }
        Ok(false) => {
            Ok(1).to_ptr()
        }
        Err(e) => {
            Err(anyhow::anyhow!("{}", e)).to_ptr()
        }
    }
}

#[no_mangle]
pub extern "C" fn call_translate(
    translator_ptr: *mut TranslatorHandle,
    json_str: *const c_char
) -> *mut FfiResult<TranslateResultFFI> {
    let input = unsafe {
        if json_str.is_null() {
            return Err(anyhow::anyhow!("Null pointer received")).to_ptr();
        }
        match CStr::from_ptr(json_str).to_str() {
            Ok(s) => s,
            Err(e) => {
                return Err(anyhow::anyhow!("Invalid UTF-8: {}", e)).to_ptr();
            }
        }
    };

    let task: TranslateTask = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => {
            return Err(anyhow::anyhow!("JSON parse error: {}", e)).to_ptr();
        }
    };

    if translator_ptr.is_null() {
        return Err(anyhow::anyhow!("Null pointer received")).to_ptr();
    }

    let translator = unsafe { &*(translator_ptr as *mut #translator) };

    if let Ok(handle) = Handle::try_current() {
        handle.block_on(async {
            let result = match translator.translate(task).await {
                Ok(v) => v,
                Err(e) => {
                    return Err(anyhow::anyhow!("JSON parse error: {}", e)).to_ptr();
                }
            };

            Ok(result.into_ffi_unbox()).to_ptr()
        })
    } else {
        let handle = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        handle.block_on(async {
            let result = match translator.translate(task).await {
                Ok(v) => v,
                Err(e) => {
                    return Err(anyhow::anyhow!("{}", e)).to_ptr();
                }
            };

            Ok(result.into_ffi_unbox()).to_ptr()
        })
    }
}

#[no_mangle]
pub extern "C" fn call_translate_stream(
    translator_ptr: *mut TranslatorHandle,
    json_str: *const c_char,
    callback_wrapper: StreamCallback,
    callback: *mut c_void
) -> *mut FfiResult<i8> {
    let input = unsafe {
        if json_str.is_null() {
            return Err(anyhow::anyhow!("Null pointer received")).to_ptr();
        }
        match CStr::from_ptr(json_str).to_str() {
            Ok(s) => s,
            Err(e) => {
                return Err(anyhow::anyhow!("Invalid UTF-8: {}", e)).to_ptr();
            }
        }
    };

    let task: TranslateTask = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => {
            return Err(anyhow::anyhow!("JSON parse error: {}", e)).to_ptr();
        }
    };

    if translator_ptr.is_null() {
        return Err(anyhow::anyhow!("Null pointer received")).to_ptr();
    }

    let translator = unsafe { &*(translator_ptr as *mut #translator) };

    let (tx, mut rx) = channel::<TranslateStreamChunk>(256);

    let cb = callback as usize;

    if let Ok(h) = tokio::runtime::Handle::try_current() {
        h.block_on(async {
            let handle = tokio::spawn(async move {
                while let Some(chunk) = rx.recv().await {
                    callback_wrapper(chunk.into_ffi(), cb as *mut c_void);
                }
            });

            match translator.translate_stream(task, tx).await {
                Err(e) => {
                    return Err(anyhow::anyhow!("{}", e)).to_ptr();
                }
                _ => {}
            };

            let _ = handle.await;

            Ok(0i8).to_ptr()
        })
    } else {
        let handle = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let r = handle.block_on(async {
            let handle = tokio::spawn(async move {
                while let Some(chunk) = rx.recv().await {
                    callback_wrapper(chunk.into_ffi(), cb as *mut c_void);
                }
            });

            match translator.translate_stream(task, tx).await {
                Err(e) => {
                    return Err(anyhow::anyhow!("{}", e)).into();
                }
                _ => {}
            };

            let _ = handle.await;

            Ok(0i8)
        });

        r.to_ptr()
    }
}

#[no_mangle]
pub extern "C" fn free_translate_result(result: *mut TranslateResultFFI) {
    lib::ffi::free_translate_result(result)
}

#[no_mangle]
pub extern "C" fn free_supported_languages(array: *mut *const c_char, len: usize) {
    lib::ffi::free_supported_languages(array, len)
}

    })
}