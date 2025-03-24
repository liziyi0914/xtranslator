use crate::{TranslateResult, TranslateStreamChunk, TranslateTask, Translator};
use anyhow::{anyhow, Result};
use handlebars::{
    Context, Handlebars, Helper, HelperResult, Output, RenderContext, RenderErrorReason,
};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
#[cfg(test)]
use crate::TranslatedItem;
#[cfg(test)]
use serde_json::json;

pub fn format_messages(template: &String, task: &TranslateTask) -> Result<String> {
    let mut reg = Handlebars::new();
    reg.register_helper(
        "json",
        Box::new(
            |h: &Helper,
             _: &Handlebars,
             _: &Context,
             _: &mut RenderContext,
             out: &mut dyn Output|
             -> HelperResult {
                let param = h
                    .param(0)
                    .ok_or(RenderErrorReason::ParamNotFoundForIndex("json", 0))?;

                out.write(serde_json::to_string(param.value()).unwrap().as_str())?;
                Ok(())
            },
        ),
    );
    reg.render_template(template.as_str(), &task)
        .map_err(|e| anyhow!(e))
}

pub async fn stream2normal(
    translator: &impl Translator,
    task: TranslateTask
) -> Result<TranslateResult> {
    let (tx, mut rx) = mpsc::channel(64);

    translator.translate_stream(task, tx).await?;

    let mut result = vec![];

    while let Some(chunk) = rx.recv().await {
        if let TranslateStreamChunk::Delta(res) = chunk {
            if let Some(s) = res.content {
                result.push(s);
            }
        }
    }

    Ok(TranslateResult {
        reasoning: None,
        content: Some(result.join("")),
    })
}

pub async fn normal2stream(
    translator: &impl Translator,
    task: TranslateTask,
    sender: Sender<TranslateStreamChunk>,
) -> Result<()> {
    sender.send(TranslateStreamChunk::Start).await?;

    let data = translator.translate(task).await?;

    sender.send(TranslateStreamChunk::Delta(data)).await?;

    sender.send(TranslateStreamChunk::End).await?;

    Ok(())
}

#[test]
fn test_format_messages() -> Result<()> {
    let task = TranslateTask {
        id: "123456".to_string(),
        content: "Hello World!".to_string(),
        source_language: Some("en-US".parse()?),
        target_language: Some("zh-CN".parse()?),
        user_prompt: None,
        system_prompt: None,
        field: None,
        terms: vec![],
        references: vec![],
        extra: None,
    };

    let template =
        r##"请将以下{{ source_language }}内容精准翻译为{{ target_language }}，确保符合以下要求：
1. 保持专业语气与原文风格
2. 要做到信达雅
3. 保留专业术语及关键数据
4. 只输出译文，不要输出其它内容

# 原文
{{ content }}"##
            .to_string();

    let formatted = format_messages(&template, &task)?;

    println!("{}", formatted);

    Ok(())
}

#[test]
fn test_format_messages2() -> Result<()> {
    let task = TranslateTask {
        id: "123456".to_string(),
        content: "Hello World!".to_string(),
        source_language: Some("en-US".parse()?),
        target_language: Some("zh-CN".parse()?),
        user_prompt: None,
        system_prompt: None,
        field: None,
        terms: vec![
            TranslatedItem {
                source: "hello".to_string(),
                target: "你好".to_string(),
            },
            TranslatedItem {
                source: "test".to_string(),
                target: "测试".to_string(),
            },
        ],
        references: vec![],
        extra: Some(json!({
            "translated": [
                {
                    "id": "123456",
                    "content": "Hello World!",
                },
                {
                    "id": "233",
                    "content": "foo!",
                },
            ]
        })),
    };

    let template = r###"## 领域描述
{{ field }}

## 术语表
{{ json terms }}

## 参考译文
{{ json references }}

## 待翻译文本
{{ content }}

## 译文
{{#each extra.translated}}
### {{ this.id }}
{{ this.content }}

{{/each}}"###
        .to_string();

    let formatted = format_messages(&template, &task)?;

    println!("{}", formatted);

    Ok(())
}

pub async fn test_translate<T: Translator>(translator: T) -> Result<()> {
    let task = TranslateTask {
        id: "123456".to_string(),
        content: "落霞与孤鹜齐飞，秋水共长天一色。".to_string(),
        source_language: Some("zh-CN".parse()?),
        target_language: Some("en-US".parse()?),
        user_prompt: None,
        system_prompt: None,
        field: None,
        terms: vec![],
        references: vec![],
        extra: None,
    };

    let result = translator.translate(task).await?;

    println!("{:?}", result);

    Ok(())
}

pub async fn test_translate_stream<T: Translator>(translator: T) -> Result<()> {
    let task = TranslateTask {
        id: "123456".to_string(),
        content: "落霞与孤鹜齐飞，秋水共长天一色。".to_string(),
        source_language: Some("zh-CN".parse()?),
        target_language: Some("en-US".parse()?),
        user_prompt: None,
        system_prompt: None,
        field: None,
        terms: vec![],
        references: vec![],
        extra: None,
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);

    let a = tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            println!("{:?}", chunk);
        }
    });

    translator.translate_stream(task, tx).await?;

    a.await?;

    Ok(())
}
