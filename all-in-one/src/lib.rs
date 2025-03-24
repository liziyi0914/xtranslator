#![allow(unused_imports, unused_variables)]
use anyhow::{bail, Result};
use serde_json::Value;
use tokio::sync::mpsc::Sender;
pub use lib::*;

pub async fn translate(name: String, config: Value, task: TranslateTask) -> Result<TranslateResult> {
    match name.as_str() {
        #[cfg(feature = "plugin-openai")]
        "openai" => {
            use plugin_openai::translator::OpenAITranslator;
            let trans = OpenAITranslator::new(config).await?;
            trans.translate(task).await
        },
        #[cfg(feature = "plugin-hunyuan")]
        "hunyuan" => {
            use plugin_hunyuan::translator::HunyuanTranslator;
            let trans = HunyuanTranslator::new(config).await?;
            trans.translate(task).await
        },
        #[cfg(feature = "plugin-qwen")]
        "qwen" => {
            use plugin_qwen::translator::QwenMtTranslator;
            let trans = QwenMtTranslator::new(config).await?;
            trans.translate(task).await
        },
        #[cfg(feature = "plugin-youdao-llm")]
        "youdao_llm" => {
            use plugin_youdao_llm::translator::YoudaoLLMTranslator;
            let trans = YoudaoLLMTranslator::new(config).await?;
            trans.translate(task).await
        },
        #[cfg(feature = "plugin-baidu-fanyi")]
        "baidu_fanyi" => {
            use plugin_baidu_fanyi::translator::BaiduFanyiTranslator;
            let trans = BaiduFanyiTranslator::new(config).await?;
            trans.translate(task).await
        },
        _ => bail!("Translator not found"),
    }
}

pub async fn translate_stream(name: String, config: Value, task: TranslateTask, sender: Sender<TranslateStreamChunk>) -> Result<()> {
    match name.as_str() {
        #[cfg(feature = "plugin-openai")]
        "openai" => {
            let trans = plugin_openai::translator::OpenAITranslator::new(config).await?;
            trans.translate_stream(task, sender).await
        },
        #[cfg(feature = "plugin-hunyuan")]
        "hunyuan" => {
            use plugin_hunyuan::translator::HunyuanTranslator;
            let trans = HunyuanTranslator::new(config).await?;
            trans.translate_stream(task, sender).await
        },
        #[cfg(feature = "plugin-qwen")]
        "qwen" => {
            use plugin_qwen::translator::QwenMtTranslator;
            let trans = QwenMtTranslator::new(config).await?;
            trans.translate_stream(task, sender).await
        },
        #[cfg(feature = "plugin-youdao-llm")]
        "youdao_llm" => {
            use plugin_youdao_llm::translator::YoudaoLLMTranslator;
            let trans = YoudaoLLMTranslator::new(config).await?;
            trans.translate_stream(task, sender).await
        },
        #[cfg(feature = "plugin-baidu-fanyi")]
        "baidu_fanyi" => {
            use plugin_baidu_fanyi::translator::BaiduFanyiTranslator;
            let trans = BaiduFanyiTranslator::new(config).await?;
            trans.translate_stream(task, sender).await
        },
        _ => bail!("Translator not found"),
    }
}
