pub mod translator;

use macros::build_ffi;
use crate::translator::BaiduFanyiTranslator;

build_ffi!("baidu_fanyi", BaiduFanyiTranslator);