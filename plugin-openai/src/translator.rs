use anyhow::{anyhow, bail, Result};
use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionRequestMessage, CreateChatCompletionRequest, CreateChatCompletionRequestArgs,
};
use async_openai::Client;
use async_trait::async_trait;
use futures_util::StreamExt;
use lib::utils::format_messages;
#[cfg(test)]
use lib::utils::{test_translate, test_translate_stream};
use lib::{TranslateResult, TranslateStreamChunk, TranslateTask, Translator};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc::Sender;

#[repr(C)]
#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAITranslator {
    pub model: String,
    pub system_prompt: Option<String>,
    pub user_prompt: Option<String>,
    pub api_base: String,
    pub api_key: String,
}

impl OpenAITranslator {
    fn build_request(
        &self,
        task: &TranslateTask,
        stream: bool,
    ) -> Result<CreateChatCompletionRequest> {
        let mut request_args = CreateChatCompletionRequestArgs::default();

        let system_prompt = if let Some(system_prompt) = &task.system_prompt {
            format_messages(system_prompt, &task)?
        } else if let Some(system_prompt) = &self.system_prompt {
            format_messages(system_prompt, &task)?
        } else {
            format_messages(&r##"请将以下{{ source_language }}内容精准翻译为{{ target_language }}，确保符合以下要求：
1. 保持专业语气与原文风格
2. 要做到信达雅
3. 保留专业术语及关键数据
4. 只输出译文，不要输出其它内容"##.to_string(), &task)?
        };

        let user_prompt = if let Some(user_prompt) = &task.user_prompt {
            format_messages(user_prompt, &task)?
        } else if let Some(user_prompt) = &self.user_prompt {
            format_messages(user_prompt, &task)?
        } else {
            format_messages(&r##"{{ content }}"##.to_string(), &task)?
        };

        request_args.model(self.model.clone()).messages(vec![
            ChatCompletionRequestMessage::System(system_prompt.into()),
            ChatCompletionRequestMessage::User(user_prompt.into()),
        ]);

        if let Some(extra) = task.extra.clone() {
            if let Value::Number(temperature) = &extra["temperature"] {
                if let Some(temperature) = temperature.as_i64() {
                    request_args.temperature(temperature as f32);
                }
            }

            if let Value::Number(top_p) = &extra["top_p"] {
                if let Some(top_p) = top_p.as_i64() {
                    request_args.top_p(top_p as f32);
                }
            }
        }

        request_args.stream(stream);

        Ok(request_args.build()?)
    }
}

#[async_trait]
impl Translator for OpenAITranslator {
    type This = Self;

    async fn new(config: Value) -> Result<Self> {
        serde_json::from_value(config).map_err(|e| anyhow!(e))
    }

    async fn translate(&self, task: TranslateTask) -> Result<TranslateResult> {
        let client = Client::with_config(
            OpenAIConfig::new()
                .with_api_base(self.api_base.clone())
                .with_api_key(self.api_key.clone()),
        );

        let request = self.build_request(&task, false)?;

        let value: Value = client
            .chat()
            .create_byot(request)
            .await
            .map_err(|e| anyhow!(e))?;

        let reasoning = value["choices"][0]["message"]["reasoning_content"]
            .as_str()
            .map(|s| s.to_string());

        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string());

        Ok(TranslateResult { reasoning, content })
    }

    async fn translate_stream(
        &self,
        task: TranslateTask,
        sender: Sender<TranslateStreamChunk>,
    ) -> Result<()> {
        let client = Client::with_config(
            OpenAIConfig::new()
                .with_api_base(self.api_base.clone())
                .with_api_key(self.api_key.clone()),
        );

        let request = self.build_request(&task, true)?;

        let mut stream = client
            .chat()
            .create_stream_byot::<_, Value>(request)
            .await
            .map_err(|e| anyhow!(e))?;

        sender.send(TranslateStreamChunk::Start).await?;

        while let Some(result) = stream.next().await {
            if let Ok(chunk) = result {
                let reasoning = chunk["choices"][0]["delta"]["reasoning_content"]
                    .as_str()
                    .map(|s| s.to_string());

                let content = chunk["choices"][0]["delta"]["content"]
                    .as_str()
                    .map(|s| s.to_string());

                sender
                    .send(TranslateStreamChunk::Delta(TranslateResult {
                        content,
                        reasoning,
                    }))
                    .await?;
            } else {
                bail!(result.unwrap_err())
            }
        }

        sender.send(TranslateStreamChunk::End).await?;

        Ok(())
    }
}

#[tokio::test]
async fn test_openai() -> Result<()> {
    let translator = OpenAITranslator {
        model: "deepseek-reasoner".to_string(),
        system_prompt: None,
        user_prompt: None,
        api_base: env!("OPENAI_API_BASE").to_string(),
        api_key: env!("OPENAI_API_KEY").to_string(),
    };

    test_translate(translator).await
}

#[tokio::test]
async fn test_openai_stream() -> Result<()> {
    let translator = OpenAITranslator {
        model: "deepseek-reasoner".to_string(),
        system_prompt: None,
        user_prompt: None,
        api_base: env!("OPENAI_API_BASE").to_string(),
        api_key: env!("OPENAI_API_KEY").to_string(),
    };

    test_translate_stream(translator).await
}
