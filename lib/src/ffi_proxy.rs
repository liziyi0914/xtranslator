use crate::ffi::{stream_callback, unwrap_handle_result, CallTranslate, CallTranslateStream, CreateTranslator, GetPluginName, TranslateStreamChunkFFI, TranslatorHandle};
use crate::{TranslateResult, TranslateStreamChunk, TranslateTask, Translator};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use libloading::{Library, Symbol};
use serde_json::Value;
use std::collections::HashMap;
use std::ffi::{c_void, CString};
use tokio::sync::mpsc::Sender;
use walkdir::WalkDir;

pub struct ProxyTranslator {
    lib: Library,
    handle: *mut TranslatorHandle,
}

unsafe impl Sync for ProxyTranslator {
}

unsafe impl Send for ProxyTranslator {
}

impl ProxyTranslator {
    pub async fn load(path: String, config: Value) -> Result<Self> {
        let mut cfg = config.clone();
        cfg["_dll_path"] = Value::String(path);

        Self::new(cfg).await
    }
}

#[async_trait]
impl Translator for ProxyTranslator {
    type This = Self;

    async fn new(config: Value) -> Result<Self> {
        let path = config["_dll_path"].as_str().ok_or(anyhow!("missing argument: path"))?;

        unsafe {
            let lib = Library::new(path)?;
            let create_translator: Symbol<CreateTranslator> = lib.get(b"create_translator")?;

            let config_str = serde_json::to_string(&config)?;
            let config_cstr = CString::new(config_str)?.into_raw();

            let handle_result = create_translator(config_cstr);

            let handle = unwrap_handle_result(handle_result)?;

            Ok(ProxyTranslator {
                lib,
                handle,
            })
        }
    }

    async fn translate(&self, task: TranslateTask) -> Result<TranslateResult> {
        let call_translate: Symbol<CallTranslate> = unsafe { self.lib.get(b"call_translate") }?;
        let result = unsafe { call_translate(self.handle, CString::new(serde_json::to_string(&task).unwrap())?.into_raw()) };

        let result = unwrap_handle_result(result)?;

        Ok(TranslateResult::from_ffi(result)?)
    }

    async fn translate_stream(&self, task: TranslateTask, sender: Sender<TranslateStreamChunk>) -> Result<()> {
        let call_translate_stream: Symbol<CallTranslateStream> = unsafe { self.lib.get(b"call_translate_stream") }?;

        let closure: Box<dyn Fn(*mut TranslateStreamChunkFFI)> = Box::new(|x| {
            if let Ok(chunk) = TranslateStreamChunk::from_ffi(x) {
                sender.blocking_send(chunk).unwrap();
            }
        });

        let callback = Box::into_raw(Box::new(closure)) as *mut c_void;

        let result = unsafe {
            call_translate_stream(self.handle, CString::new(serde_json::to_string(&task).unwrap())?.into_raw(), stream_callback, callback)
        };

        unwrap_handle_result(result)?;

        Ok(())
    }
}

impl Drop for ProxyTranslator {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                let _ = Box::from_raw(self.handle);
            }
        }
    }
}

pub fn load_translators(root: String) -> Result<HashMap<String, String>> {
    let extensions = {
        #[cfg(windows)]
        {
            vec!["dll"]
        }

        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            vec!["so"]
        }

        #[cfg(target_os = "macos")]
        {
            vec!["dylib"]
        }

        #[cfg(not(any(
            target_os = "windows",
            target_os = "linux",
            target_os = "android",
            target_os = "macos"
        )))]
        {
            vec![]
        }
    };
    let mut libraries = Vec::new();

    let mut map = HashMap::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                // Windows 不区分大小写，统一转换为小写比较
                let ext_normalized = {
                    #[cfg(windows)]
                    {
                        ext.to_lowercase()
                    }

                    #[cfg(not(windows))]
                    {
                        ext
                    }
                };

                if extensions.iter().any(|&e| e == ext_normalized) {
                    if let Some(path_str) = path.to_str() {
                        libraries.push(path_str.to_string());
                    }
                }
            }
        }
    }


    for lib_path in libraries {
        let library_result = unsafe { Library::new(&lib_path) };
        if library_result.is_err() {
            continue;
        }
        let library = library_result?;

        let get_name_result = unsafe { library.get::<GetPluginName>(b"get_plugin_name") };
        if get_name_result.is_err() {
            continue;
        }
        let get_name = get_name_result?;

        let name_ptr = unsafe { get_name() };
        if name_ptr.is_null() {
            continue;
        }
        let name = unsafe { CString::from_raw(name_ptr).to_string_lossy().into_owned() };

        map.insert(name, lib_path);
    }


    Ok(map)
}