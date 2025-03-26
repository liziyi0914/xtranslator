pub mod utils;
pub mod ffi;
pub mod ffi_proxy;

use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use language_tags::LanguageTag;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc::Sender;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslatedItem {
    /// 原文
    pub source: String,
    /// 译文
    pub target: String,
}

#[derive(Debug, Clone, Builder, Serialize, Deserialize)]
#[builder(setter(into))]
pub struct TranslateTask {
    /// ID
    pub id: String,
    /// 原文
    pub content: String,
    /// 源语言
    pub source_language: Option<LanguageTag>,
    /// 目标语言
    pub target_language: Option<LanguageTag>,
    /// 用户提示词模板
    pub user_prompt: Option<String>,
    /// 系统提示词模板
    pub system_prompt: Option<String>,
    /// 领域描述
    pub field: Option<String>,
    /// 术语表
    pub terms: Vec<TranslatedItem>,
    /// 参考译文
    pub references: Vec<TranslatedItem>,
    /// 扩展数据
    pub extra: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranslateResult {
    pub reasoning: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TranslateStreamChunk {
    Start,
    Delta(TranslateResult),
    End,
}

#[async_trait]
pub trait Translator {
    type This;

    /// 创建翻译实例
    async fn new(config: Value) -> Result<Self::This>;

    /// 获取支持的源语言列表
    fn get_supported_input_languages(&self) -> Result<Vec<String>>;

    /// 获取支持的目标语言列表
    fn get_supported_output_languages(&self) -> Result<Vec<String>>;

    /// 是否支持该语言作为源语言
    fn is_supported_input_language(&self, lang: String) -> Result<bool>;

    /// 是否支持该语言作为目标语言
    fn is_supported_output_language(&self, lang: String) -> Result<bool>;

    /// 翻译
    async fn translate(&self, task: TranslateTask) -> Result<TranslateResult>;

    /// 流式翻译
    async fn translate_stream(
        &self,
        task: TranslateTask,
        sender: Sender<TranslateStreamChunk>,
    ) -> Result<()>;
}
